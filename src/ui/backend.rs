use std::io::Write;
use std::net::{TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::audio::opus_encoder::OpusEncoder;
use crate::audio::AudioConfig;
use crate::capture::CaptureConfig;
#[cfg(target_os = "macos")]
use crate::capture::macos::{native_resolution, MacOsCapture};
use crate::capture::VideoCapture;
use crate::discovery::browser;
use crate::encode::{EncoderConfig, VideoEncoder};
use crate::protocol::negotiate::*;
use crate::protocol::session::SecureChannel;
use crate::security::cipher::Cipher;
use crate::security::pairing;
use crate::transport::fec::FecEncoder;
use crate::transport::rtp::{H264Packetizer, RtpHeader, RtpPacket};
use crate::ui::messages::{BackendEvent, CaptureMode, StreamStats, UiCommand};

#[cfg(target_os = "windows")]
use crate::capture::windows::{DxgiCapture, WasapiCapture};
#[cfg(target_os = "windows")]
use crate::capture::AudioCapture;

const VIDEO_PT: u8 = 96;
const AUDIO_PT: u8 = 111;
const VIDEO_SSRC: u32 = 0x12345678;
const AUDIO_SSRC: u32 = 0x87654321;

/// Spawns a background thread that continuously browses for mDNS receivers
/// and sends `DevicesUpdated` events every ~5 seconds (3s browse + 2s sleep).
pub fn spawn_mdns_browser(
    event_tx: Sender<BackendEvent>,
    shared_devices: Arc<Mutex<Vec<browser::DiscoveredReceiver>>>,
) -> JoinHandle<()> {
    thread::spawn(move || loop {
        match browser::browse(Duration::from_secs(3)) {
            Ok(devices) => {
                let mut seen = std::collections::HashSet::new();
                let deduped: Vec<_> = devices
                    .into_iter()
                    .filter(|d| seen.insert(d.name.clone()))
                    .collect();
                *shared_devices.lock().unwrap() = deduped.clone();
                if event_tx.send(BackendEvent::DevicesUpdated(deduped)).is_err() {
                    break;
                }
            }
            Err(e) => {
                tracing::warn!("mDNS browse error: {e}");
            }
        }
        thread::sleep(Duration::from_secs(2));
    })
}

/// Spawns the command handler thread.
///
/// State machine:
/// 1. Wait for `Connect` → perform TCP + SPAKE2 pairing
/// 2. Wait for `StartStreaming` → spawn streaming sub-thread
/// 3. While streaming: relay Pause/Resume/Disconnect to atomics
/// 4. When streaming ends: send `Disconnected`, go back to step 1
pub fn spawn_command_handler(
    cmd_rx: Receiver<UiCommand>,
    event_tx: Sender<BackendEvent>,
    shared_devices: Arc<Mutex<Vec<browser::DiscoveredReceiver>>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        'outer: loop {
            // ── Phase 1: wait for Connect ─────────────────────────────────
            let (addr, pin) = loop {
                match cmd_rx.recv() {
                    Ok(UiCommand::Connect { addr, pin }) => break (addr, pin),
                    Ok(UiCommand::VerifyPin { pin }) => {
                        let devices = shared_devices.lock().unwrap().clone();
                        handle_verify_pin(&pin, &devices, &event_tx);
                    }
                    Ok(_) => {}
                    Err(_) => return,
                }
            };

            // ── Phase 2: TCP connect + SPAKE2 pairing ────────────────────
            let pairing_result = (|| -> anyhow::Result<(SecureChannel, [u8; 32], [u8; 32])> {
                let mut stream =
                    TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;

                let (msg_a, state) = pairing::sender_start(&pin);
                stream.write_all(&(msg_a.len() as u32).to_be_bytes())?;
                stream.write_all(&msg_a)?;

                let mut len_buf = [0u8; 4];
                std::io::Read::read_exact(&mut stream, &mut len_buf)?;
                let pb_len = u32::from_be_bytes(len_buf) as usize;
                if pb_len > 256 {
                    anyhow::bail!("pairing message too large: {pb_len}");
                }
                let mut msg_b = vec![0u8; pb_len];
                std::io::Read::read_exact(&mut stream, &mut msg_b)?;

                let keys = pairing::sender_finish(state, &msg_b)
                    .map_err(|e| anyhow::anyhow!("pairing failed: {e:?}"))?;

                let channel = SecureChannel::new(stream, &keys.control_key, true);
                Ok((channel, keys.control_key, keys.media_key))
            })();

            let (mut channel, _control_key, media_key) = match pairing_result {
                Ok(v) => {
                    let _ = event_tx.send(BackendEvent::PairingSuccess);
                    v
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("timed out") || msg.contains("timeout") {
                        let _ = event_tx.send(BackendEvent::PairingTimeout);
                    } else {
                        let _ = event_tx.send(BackendEvent::PairingFailed(msg));
                    }
                    continue; // back to waiting for Connect
                }
            };

            // ── Phase 3: wait for StartStreaming ─────────────────────────
            loop {
                match cmd_rx.recv() {
                    Ok(UiCommand::StartStreaming { .. }) => break,
                    Ok(UiCommand::Disconnect) => {
                        let _ = channel.shutdown();
                        let _ = event_tx.send(BackendEvent::Disconnected(String::new()));
                        continue 'outer;
                    }
                    Ok(_) => {} // ignore Pause/Resume before streaming
                    Err(_) => return,
                }
            }

            // Perform negotiation before starting the streaming sub-thread.
            // If negotiation fails, send Disconnected and restart.
            #[cfg(target_os = "macos")]
            let negotiate_result = negotiate_macos(&mut channel);

            #[cfg(target_os = "windows")]
            let negotiate_result = negotiate_windows(&mut channel);

            let (params, udp_socket) = match negotiate_result {
                Ok(v) => v,
                Err(e) => {
                    let _ = channel.shutdown();
                    let _ = event_tx
                        .send(BackendEvent::Disconnected(format!("negotiation failed: {e}")));
                    continue;
                }
            };

            let receiver_media_addr =
                std::net::SocketAddr::new(addr.ip(), params.receiver_udp_port);
            if let Err(e) = udp_socket.connect(receiver_media_addr) {
                let _ = channel.shutdown();
                let _ = event_tx.send(BackendEvent::Disconnected(format!("UDP connect failed: {e}")));
                continue;
            }

            // ── Phase 4: streaming sub-thread ────────────────────────────
            let active = Arc::new(AtomicBool::new(true));
            let paused = Arc::new(AtomicBool::new(false));

            let active_sub = Arc::clone(&active);
            let paused_sub = Arc::clone(&paused);
            let event_tx_sub = event_tx.clone();

            let streaming_handle = thread::spawn(move || {
                run_streaming_loop(
                    params,
                    udp_socket,
                    media_key,
                    active_sub,
                    paused_sub,
                    event_tx_sub,
                );
            });

            let _ = event_tx.send(BackendEvent::StreamingStarted);

            // ── Phase 5: relay Pause/Resume/Disconnect while streaming ───
            loop {
                match cmd_rx.recv() {
                    Ok(UiCommand::Pause) => {
                        paused.store(true, Ordering::Relaxed);
                    }
                    Ok(UiCommand::Resume) => {
                        paused.store(false, Ordering::Relaxed);
                    }
                    Ok(UiCommand::Disconnect) => {
                        active.store(false, Ordering::Relaxed);
                        break;
                    }
                    Ok(UiCommand::Connect { .. }) => {
                        // New Connect while streaming → stop current stream first
                        active.store(false, Ordering::Relaxed);
                        break;
                    }
                    Ok(UiCommand::StartStreaming { .. } | UiCommand::VerifyPin { .. } | UiCommand::ListWindows) => {}
                    Err(_) => {
                        active.store(false, Ordering::Relaxed);
                        break;
                    }
                }

                // If the streaming thread finished on its own, stop waiting.
                if streaming_handle.is_finished() {
                    break;
                }
            }

            // Wait for the streaming sub-thread to finish.
            let _ = streaming_handle.join();
            let _ = channel.shutdown();
            let _ = event_tx.send(BackendEvent::Disconnected("streaming ended".into()));
        }
    })
}

fn handle_verify_pin(
    pin: &str,
    devices: &[browser::DiscoveredReceiver],
    event_tx: &Sender<BackendEvent>,
) {
    for device in devices {
        let port = device.http_port.unwrap_or(device.addr.port());
        let url = format!("http://{}:{}/verify-pin", device.addr.ip(), port);
        let result = ureq::post(&url)
            .timeout(Duration::from_millis(500))
            .send_json(ureq::json!({ "pin": pin }));
        match result {
            Ok(response) => {
                if let Ok(json) = response.into_json::<serde_json::Value>() {
                    let device_name = json["device_name"]
                        .as_str()
                        .unwrap_or(&device.name)
                        .to_string();
                    let _ = event_tx.send(BackendEvent::PinMatched {
                        device_name,
                        addr: device.addr,
                    });
                    return;
                }
            }
            Err(_) => continue,
        }
    }
    let _ = event_tx.send(BackendEvent::PinNotFound);
}

// ── Negotiation helpers ───────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn negotiate_macos(
    channel: &mut SecureChannel,
) -> anyhow::Result<(NegotiatedParams, UdpSocket)> {
    let (cap_w, cap_h) = native_resolution();

    let pixels = cap_w as u64 * cap_h as u64;
    let bitrate = ((pixels * 15_000_000 / (1920 * 1080)) as u32).min(40_000_000);
    let fps: u32 = if pixels > 1920 * 1200 { 30 } else { 60 };

    let udp_socket = UdpSocket::bind("0.0.0.0:0")?;
    let sender_udp_port = udp_socket.local_addr()?.port();

    let offer = Offer {
        video: OfferVideo {
            codec: "h264".to_string(),
            width: cap_w,
            height: cap_h,
            fps,
            bitrate,
        },
        audio: OfferAudio {
            codec: "opus".to_string(),
            sample_rate: 48000,
            channels: 2,
            bitrate: 128_000,
        },
        transport: OfferTransport {
            udp_port: sender_udp_port,
            fec_group_size: 6,
        },
    };

    do_negotiate(channel, offer, udp_socket)
}

#[cfg(target_os = "windows")]
fn negotiate_windows(
    channel: &mut SecureChannel,
) -> anyhow::Result<(NegotiatedParams, UdpSocket)> {
    // Probe resolution via a temporary capture object.
    let tmp_capture = DxgiCapture::new(&CaptureConfig { mode: CaptureMode::FullScreen, fps: 0, width: 0, height: 0 })?;
    let cap_w = tmp_capture.width();
    let cap_h = tmp_capture.height();

    let pixels = cap_w as u64 * cap_h as u64;
    let bitrate = ((pixels * 15_000_000 / (1920 * 1080)) as u32).min(40_000_000);
    let fps: u32 = if pixels > 1920 * 1200 { 30 } else { 60 };

    let udp_socket = UdpSocket::bind("0.0.0.0:0")?;
    let sender_udp_port = udp_socket.local_addr()?.port();

    let offer = Offer {
        video: OfferVideo {
            codec: "h264".to_string(),
            width: cap_w,
            height: cap_h,
            fps,
            bitrate,
        },
        audio: OfferAudio {
            codec: "opus".to_string(),
            sample_rate: 48000,
            channels: 2,
            bitrate: 128_000,
        },
        transport: OfferTransport {
            udp_port: sender_udp_port,
            fec_group_size: 6,
        },
    };

    do_negotiate(channel, offer, udp_socket)
}

fn do_negotiate(
    channel: &mut SecureChannel,
    offer: Offer,
    udp_socket: UdpSocket,
) -> anyhow::Result<(NegotiatedParams, UdpSocket)> {
    channel.send(&NegotiateMessage::Offer(offer.clone()).to_bytes())?;

    channel.set_read_timeout(Some(Duration::from_secs(5)))?;
    let answer_bytes = channel
        .recv()
        .map_err(|e| anyhow::anyhow!("recv failed (wrong PIN?): {e}"))?
        .ok_or_else(|| anyhow::anyhow!("connection closed during negotiation"))?;
    let answer = match NegotiateMessage::from_bytes(&answer_bytes)? {
        NegotiateMessage::Answer(a) => a,
        _ => anyhow::bail!("expected Answer, got something else"),
    };
    channel.set_read_timeout(None)?;

    let params = NegotiatedParams::resolve(&offer, &answer);
    tracing::info!(
        "negotiated: {}x{} @ {}fps, receiver UDP port {}",
        params.video_width,
        params.video_height,
        params.video_fps,
        params.receiver_udp_port
    );

    Ok((params, udp_socket))
}

// ── Streaming loop ────────────────────────────────────────────────────────────

fn run_streaming_loop(
    params: NegotiatedParams,
    udp_socket: UdpSocket,
    media_key: [u8; 32],
    active: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    event_tx: Sender<BackendEvent>,
) {
    if let Err(e) = run_streaming_loop_inner(params, udp_socket, media_key, active, paused, &event_tx) {
        tracing::error!("streaming error: {e}");
        let _ = event_tx.send(BackendEvent::Disconnected(e.to_string()));
    }
}

#[cfg(target_os = "macos")]
fn run_streaming_loop_inner(
    params: NegotiatedParams,
    udp_socket: UdpSocket,
    media_key: [u8; 32],
    active: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    event_tx: &Sender<BackendEvent>,
) -> anyhow::Result<()> {
    let capture = MacOsCapture::new(&CaptureConfig {
        mode: CaptureMode::FullScreen,
        fps: 0,
        width: params.video_width,
        height: params.video_height,
    })?;

    let mut encoder = VideoEncoder::new(&EncoderConfig {
        width: params.video_width,
        height: params.video_height,
        fps: params.video_fps,
        bitrate: params.video_bitrate,
    })?;

    let mut media_cipher = Cipher::new(&media_key, [0, 0, 0, 1]);
    let mut packetizer = H264Packetizer::new(VIDEO_PT, VIDEO_SSRC, 1400);
    let mut video_fec = FecEncoder::new(params.fec_group_size, VIDEO_PT);

    let audio_config = AudioConfig {
        sample_rate: params.audio_sample_rate,
        channels: params.audio_channels,
        bitrate: params.audio_bitrate,
        frame_duration_ms: 10,
    };
    let mut opus_encoder = OpusEncoder::new(&audio_config)?;
    let frame_size_samples = opus_encoder.frame_size_samples();
    let mut audio_pcm_buf: Vec<f32> = Vec::with_capacity(frame_size_samples * 2);
    let mut audio_seq: u16 = 0;
    let mut audio_ts: u32 = 0;
    let mut audio_fec = FecEncoder::new(params.fec_group_size, AUDIO_PT);

    let mut frame_count: u64 = 0;
    let mut bytes_sent: u64 = 0;
    let mut last_stats = Instant::now();

    tracing::info!("streaming started");

    loop {
        if !active.load(Ordering::Relaxed) {
            break;
        }

        if paused.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(16));
            continue;
        }

        let frame = match capture.next_frame() {
            Ok(Some(f)) => f,
            Ok(None) => continue,
            Err(_e) => break,
        };

        encoder.encode(frame)?;
        frame_count += 1;

        while let Some(encoded) = encoder.next_encoded() {
            let rtp_ts = (encoded.timestamp / 11_111) as u32;
            let nal_count = encoded.nal_units.len();
            for (i, nal) in encoded.nal_units.iter().enumerate() {
                let rtp_packets = packetizer.packetize(nal, rtp_ts);
                for pkt in &rtp_packets {
                    let encrypted = media_cipher.seal(&pkt.serialize());
                    bytes_sent += encrypted.len() as u64;
                    udp_socket.send(&encrypted)?;
                    if let Some(fec_pkt) = video_fec.push(pkt) {
                        let encrypted_fec = media_cipher.seal(&fec_pkt.serialize());
                        bytes_sent += encrypted_fec.len() as u64;
                        udp_socket.send(&encrypted_fec)?;
                    }
                }
                if i == nal_count - 1
                    && let Some(fec_pkt) = video_fec.flush()
                {
                    let encrypted_fec = media_cipher.seal(&fec_pkt.serialize());
                    bytes_sent += encrypted_fec.len() as u64;
                    udp_socket.send(&encrypted_fec)?;
                }
            }
        }

        let audio_iter = std::iter::from_fn(|| capture.try_next_audio());
        for pcm in audio_iter {
            audio_pcm_buf.extend_from_slice(&pcm);
            while audio_pcm_buf.len() >= frame_size_samples {
                let frame_pcm: Vec<f32> = audio_pcm_buf.drain(..frame_size_samples).collect();
                let opus_data = opus_encoder.encode(&frame_pcm)?;

                let audio_pkt = RtpPacket {
                    header: RtpHeader {
                        version: 2,
                        padding: false,
                        extension: false,
                        marker: false,
                        payload_type: AUDIO_PT,
                        sequence_number: audio_seq,
                        timestamp: audio_ts,
                        ssrc: AUDIO_SSRC,
                    },
                    payload: opus_data.to_vec(),
                };
                audio_seq = audio_seq.wrapping_add(1);
                audio_ts = audio_ts.wrapping_add(480);

                let encrypted = media_cipher.seal(&audio_pkt.serialize());
                bytes_sent += encrypted.len() as u64;
                udp_socket.send(&encrypted)?;
                if let Some(fec_pkt) = audio_fec.push(&audio_pkt) {
                    let encrypted_fec = media_cipher.seal(&fec_pkt.serialize());
                    bytes_sent += encrypted_fec.len() as u64;
                    udp_socket.send(&encrypted_fec)?;
                }
            }
        }

        // Stats every 500ms
        let elapsed = last_stats.elapsed();
        if elapsed >= Duration::from_millis(500) {
            let secs = elapsed.as_secs_f32();
            let fps_actual = frame_count as f32 / secs;
            let bitrate_bps = (bytes_sent as f32 * 8.0 / secs) as u64;

            let _ = event_tx.send(BackendEvent::StatsUpdate(StreamStats {
                resolution_w: params.video_width,
                resolution_h: params.video_height,
                fps: fps_actual,
                bitrate_bps,
                latency_ms: 0.0,
                packet_loss_pct: 0.0,
            }));

            frame_count = 0;
            bytes_sent = 0;
            last_stats = Instant::now();
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn run_streaming_loop_inner(
    params: NegotiatedParams,
    udp_socket: UdpSocket,
    media_key: [u8; 32],
    active: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    event_tx: &Sender<BackendEvent>,
) -> anyhow::Result<()> {
    let capture = DxgiCapture::new(&CaptureConfig { mode: CaptureMode::FullScreen, fps: 0, width: 0, height: 0 })?;
    let audio_capture = WasapiCapture::new(48000, 2)?;

    let mut encoder = VideoEncoder::new_with_device(
        &EncoderConfig {
            width: params.video_width,
            height: params.video_height,
            fps: params.video_fps,
            bitrate: params.video_bitrate,
        },
        capture.device(),
    )?;

    let mut media_cipher = Cipher::new(&media_key, [0, 0, 0, 1]);
    let mut packetizer = H264Packetizer::new(VIDEO_PT, VIDEO_SSRC, 1400);
    let mut video_fec = FecEncoder::new(params.fec_group_size, VIDEO_PT);

    let audio_config = AudioConfig {
        sample_rate: params.audio_sample_rate,
        channels: params.audio_channels,
        bitrate: params.audio_bitrate,
        frame_duration_ms: 10,
    };
    let mut opus_encoder = OpusEncoder::new(&audio_config)?;
    let frame_size_samples = opus_encoder.frame_size_samples();
    let mut audio_pcm_buf: Vec<f32> = Vec::with_capacity(frame_size_samples * 2);
    let mut audio_seq: u16 = 0;
    let mut audio_ts: u32 = 0;
    let mut audio_fec = FecEncoder::new(params.fec_group_size, AUDIO_PT);

    let mut frame_count: u64 = 0;
    let mut bytes_sent: u64 = 0;
    let mut last_stats = Instant::now();

    tracing::info!("streaming started");

    loop {
        if !active.load(Ordering::Relaxed) {
            break;
        }

        if paused.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(16));
            continue;
        }

        let frame = match capture.next_frame() {
            Ok(Some(f)) => f,
            Ok(None) => continue,
            Err(_e) => break,
        };

        encoder.encode(frame)?;
        frame_count += 1;

        while let Some(encoded) = encoder.next_encoded() {
            let rtp_ts = (encoded.timestamp / 11_111) as u32;
            let nal_count = encoded.nal_units.len();
            for (i, nal) in encoded.nal_units.iter().enumerate() {
                let rtp_packets = packetizer.packetize(nal, rtp_ts);
                for pkt in &rtp_packets {
                    let encrypted = media_cipher.seal(&pkt.serialize());
                    bytes_sent += encrypted.len() as u64;
                    udp_socket.send(&encrypted)?;
                    if let Some(fec_pkt) = video_fec.push(pkt) {
                        let encrypted_fec = media_cipher.seal(&fec_pkt.serialize());
                        bytes_sent += encrypted_fec.len() as u64;
                        udp_socket.send(&encrypted_fec)?;
                    }
                }
                if i == nal_count - 1
                    && let Some(fec_pkt) = video_fec.flush()
                {
                    let encrypted_fec = media_cipher.seal(&fec_pkt.serialize());
                    bytes_sent += encrypted_fec.len() as u64;
                    udp_socket.send(&encrypted_fec)?;
                }
            }
        }

        let audio_iter = std::iter::from_fn(|| audio_capture.try_next_audio());
        for pcm in audio_iter {
            audio_pcm_buf.extend_from_slice(&pcm);
            while audio_pcm_buf.len() >= frame_size_samples {
                let frame_pcm: Vec<f32> = audio_pcm_buf.drain(..frame_size_samples).collect();
                let opus_data = opus_encoder.encode(&frame_pcm)?;

                let audio_pkt = RtpPacket {
                    header: RtpHeader {
                        version: 2,
                        padding: false,
                        extension: false,
                        marker: false,
                        payload_type: AUDIO_PT,
                        sequence_number: audio_seq,
                        timestamp: audio_ts,
                        ssrc: AUDIO_SSRC,
                    },
                    payload: opus_data.to_vec(),
                };
                audio_seq = audio_seq.wrapping_add(1);
                audio_ts = audio_ts.wrapping_add(480);

                let encrypted = media_cipher.seal(&audio_pkt.serialize());
                bytes_sent += encrypted.len() as u64;
                udp_socket.send(&encrypted)?;
                if let Some(fec_pkt) = audio_fec.push(&audio_pkt) {
                    let encrypted_fec = media_cipher.seal(&fec_pkt.serialize());
                    bytes_sent += encrypted_fec.len() as u64;
                    udp_socket.send(&encrypted_fec)?;
                }
            }
        }

        // Stats every 500ms
        let elapsed = last_stats.elapsed();
        if elapsed >= Duration::from_millis(500) {
            let secs = elapsed.as_secs_f32();
            let fps_actual = frame_count as f32 / secs;
            let bitrate_bps = (bytes_sent as f32 * 8.0 / secs) as u64;

            let _ = event_tx.send(BackendEvent::StatsUpdate(StreamStats {
                resolution_w: params.video_width,
                resolution_h: params.video_height,
                fps: fps_actual,
                bitrate_bps,
                latency_ms: 0.0,
                packet_loss_pct: 0.0,
            }));

            frame_count = 0;
            bytes_sent = 0;
            last_stats = Instant::now();
        }
    }

    Ok(())
}
