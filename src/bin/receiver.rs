use std::io::{Read as IoRead, Write as IoWrite};
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use tracing_subscriber::EnvFilter;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::EventLoop;
use winit::window::Window;

use screen_mirror::audio::AudioConfig;
use screen_mirror::audio::opus_decoder::OpusDecoder;
use screen_mirror::audio::output::AudioOutput;
use screen_mirror::decode::DecoderConfig;
use screen_mirror::decode::videotoolbox::VTDecoder;
use screen_mirror::discovery::advertiser::Advertiser;
use screen_mirror::protocol::negotiate::*;
use screen_mirror::protocol::session::SecureChannel;
use screen_mirror::render::metal::MetalRenderer;
use screen_mirror::security::cipher::Cipher;
use screen_mirror::security::pairing;
use screen_mirror::security::replay::ReplayWindow;
use screen_mirror::sync::SyncState;
use screen_mirror::transport::fec::{FecDecoder, FecHeader};
use screen_mirror::transport::jitter::AudioJitterBuffer;
use screen_mirror::transport::rtp::{H264Depacketizer, RtpPacket};

const PT_VIDEO: u8 = 96;
const PT_AUDIO: u8 = 111;
const PT_FEC: u8 = 127;

struct App {
    window: Option<Window>,
    renderer: Option<MetalRenderer>,
    audio_output: Option<AudioOutput>,
    udp: UdpSocket,
    media_cipher: Cipher,
    replay_window: ReplayWindow,
    depacketizer: H264Depacketizer,
    decoder: VTDecoder,
    video_fec: FecDecoder,
    last_video_seq: Option<u16>,
    opus_decoder: OpusDecoder,
    jitter_buffer: Arc<Mutex<AudioJitterBuffer>>,
    audio_fec: FecDecoder,
    last_audio_seq: Option<u16>,
    sync_state: SyncState,
    recv_buf: Vec<u8>,
    frames_rendered: u64,
    total_packets: u64,
}

impl App {
    fn poll_packets(&mut self) {
        loop {
            match self.udp.recv_from(&mut self.recv_buf) {
                Ok((n, _)) => {
                    let encrypted = &self.recv_buf[..n];
                    let (counter, plaintext) = match self.media_cipher.open(encrypted) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };
                    if !self.replay_window.check_and_mark(counter) {
                        continue;
                    }
                    let Some(pkt) = RtpPacket::parse(&plaintext) else {
                        continue;
                    };
                    self.total_packets += 1;

                    if self.total_packets.is_multiple_of(1000) {
                        if let Some(seq) = self.last_video_seq {
                            self.video_fec.remove_old_groups(seq.wrapping_sub(100));
                        }
                        if let Some(seq) = self.last_audio_seq {
                            self.audio_fec.remove_old_groups(seq.wrapping_sub(100));
                        }
                    }

                    match pkt.header.payload_type {
                        PT_VIDEO => self.handle_video_packet(&pkt),
                        PT_AUDIO => self.handle_audio_packet(&pkt),
                        PT_FEC => self.handle_fec_packet(&pkt),
                        _ => {}
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }

    fn handle_video_packet(&mut self, pkt: &RtpPacket) {
        let seq = pkt.header.sequence_number;
        self.video_fec.push_media(pkt);

        if let Some(prev) = self.last_video_seq {
            let expected = prev.wrapping_add(1);
            if seq != expected {
                let mut s = expected;
                while s != seq {
                    if let Some(recovered) = self.video_fec.recover(s) {
                        let recovered_pkt = RtpPacket {
                            header: pkt.header.clone(),
                            payload: recovered,
                        };
                        self.depacketizer.push(&recovered_pkt);
                    }
                    s = s.wrapping_add(1);
                }
            }
        }
        self.last_video_seq = Some(seq);

        let marker = pkt.header.marker;
        let ts = pkt.header.timestamp as u64;
        self.depacketizer.push(pkt);

        if marker {
            if let Some(nal) = self.depacketizer.pop_nal() {
                let _ = self.decoder.decode_nal(&nal, ts);
            }
        }
    }

    fn handle_audio_packet(&mut self, pkt: &RtpPacket) {
        let seq = pkt.header.sequence_number;
        self.audio_fec.push_media(pkt);

        if let Some(prev) = self.last_audio_seq {
            let expected = prev.wrapping_add(1);
            if seq != expected {
                let mut s = expected;
                while s != seq {
                    if let Some(recovered) = self.audio_fec.recover(s) {
                        if let Ok(pcm) = self.opus_decoder.decode(&recovered) {
                            if let Ok(mut jb) = self.jitter_buffer.lock() {
                                jb.push_frame(pcm);
                            }
                        } else {
                            self.do_plc();
                        }
                    } else {
                        self.do_plc();
                    }
                    s = s.wrapping_add(1);
                }
            }
        }
        self.last_audio_seq = Some(seq);

        match self.opus_decoder.decode(&pkt.payload) {
            Ok(pcm) => {
                if let Ok(mut jb) = self.jitter_buffer.lock() {
                    jb.push_frame(pcm);
                }
            }
            Err(_) => self.do_plc(),
        }
    }

    fn do_plc(&mut self) {
        if let Ok(pcm) = self.opus_decoder.decode_plc() {
            if let Ok(mut jb) = self.jitter_buffer.lock() {
                jb.push_frame(pcm);
            }
        }
    }

    fn handle_fec_packet(&mut self, pkt: &RtpPacket) {
        let Some(fec_header) = FecHeader::parse(&pkt.payload) else {
            return;
        };
        match fec_header.media_pt {
            PT_VIDEO => self.video_fec.push_fec(pkt),
            PT_AUDIO => self.audio_fec.push_fec(pkt),
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes()
                .with_title("Screen Mirror Receiver")
                .with_inner_size(winit::dpi::LogicalSize::new(1920u32, 1080u32));
            let window = match event_loop.create_window(attrs) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!("failed to create window: {e}");
                    event_loop.exit();
                    return;
                }
            };
            match MetalRenderer::new(&window) {
                Ok(r) => self.renderer = Some(r),
                Err(e) => {
                    tracing::error!("failed to create Metal renderer: {e}");
                    event_loop.exit();
                    return;
                }
            }
            self.window = Some(window);

            match AudioOutput::new(Arc::clone(&self.jitter_buffer)) {
                Ok(output) => {
                    tracing::info!("audio output started");
                    self.audio_output = Some(output);
                }
                Err(e) => {
                    tracing::error!("failed to start audio output: {e}");
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                self.poll_packets();

                if let Some(ref audio_output) = self.audio_output {
                    self.sync_state
                        .report_audio_played(audio_output.samples_played());
                }

                if let Some(frame) = self.decoder.next_frame() {
                    if let Some(renderer) = &mut self.renderer {
                        let _ = unsafe { renderer.render_pixel_buffer(frame.pixel_buffer) };
                        self.frames_rendered += 1;
                        self.sync_state
                            .report_video_rendered(frame.timestamp as u32);
                        if self.frames_rendered.is_multiple_of(60) {
                            tracing::info!("rendered {} frames", self.frames_rendered);
                        }
                    }
                }

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn spawn_verify_pin_server(
    pin: Arc<Mutex<String>>,
    device_name: String,
) -> (u16, thread::JoinHandle<()>) {
    let server = tiny_http::Server::http("0.0.0.0:0").expect("failed to bind HTTP server");
    let port = server.server_addr().to_ip().unwrap().port();
    let handle = thread::spawn(move || {
        for mut request in server.incoming_requests() {
            if request.method() != &tiny_http::Method::Post || request.url() != "/verify-pin" {
                let resp = tiny_http::Response::from_string("{}").with_status_code(404);
                let _ = request.respond(resp);
                continue;
            }
            let mut body = String::new();
            if request.as_reader().read_to_string(&mut body).is_err() {
                let resp = tiny_http::Response::from_string("{}").with_status_code(400);
                let _ = request.respond(resp);
                continue;
            }
            let received_pin = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["pin"].as_str().map(|s| s.to_string()));
            let current_pin = pin.lock().unwrap().clone();
            if received_pin.as_deref() == Some(current_pin.as_str()) {
                let json = serde_json::json!({ "device_name": device_name });
                let resp = tiny_http::Response::from_string(json.to_string())
                    .with_header(
                        "Content-Type: application/json"
                            .parse::<tiny_http::Header>()
                            .unwrap(),
                    )
                    .with_status_code(200);
                let _ = request.respond(resp);
            } else {
                let resp = tiny_http::Response::from_string("{}").with_status_code(403);
                let _ = request.respond(resp);
            }
        }
    });
    (port, handle)
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let device_name = hostname::get()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Spawn HTTP verify-pin server
    let pin = pairing::generate_pin();
    let shared_pin = Arc::new(Mutex::new(pin.clone()));
    let (http_port, _http_handle) =
        spawn_verify_pin_server(shared_pin.clone(), device_name.clone());
    tracing::info!("HTTP verify-pin server on port {http_port}");

    let (advertiser, listener) = Advertiser::new(&device_name, http_port)?;

    // Loop until we successfully pair and negotiate
    let (media_cipher, params, udp_socket) = loop {
        tracing::info!("========================================");
        tracing::info!("  PIN: {pin}");
        tracing::info!("========================================");
        tracing::info!("waiting for connection on port {}...", advertiser.port());

        listener.set_nonblocking(false)?;
        let (mut tcp_stream, peer_addr) = listener.accept()?;
        tracing::info!("connection from {peer_addr}");
        let _ = advertiser.unregister();

        tcp_stream.set_read_timeout(Some(Duration::from_secs(10)))?;

        // Read pA
        let mut len_buf = [0u8; 4];
        if tcp_stream.read_exact(&mut len_buf).is_err() {
            tracing::warn!("pairing timeout, restarting");
            let _ = advertiser.reregister(&device_name);
            continue;
        }
        let pa_len = u32::from_be_bytes(len_buf) as usize;
        if pa_len > 256 {
            tracing::warn!("pairing message too large: {pa_len}, restarting");
            let _ = advertiser.reregister(&device_name);
            continue;
        }
        let mut msg_a = vec![0u8; pa_len];
        if tcp_stream.read_exact(&mut msg_a).is_err() {
            tracing::warn!("failed to read pairing message, restarting");
            let _ = advertiser.reregister(&device_name);
            continue;
        }

        let (msg_b, state) = pairing::receiver_start(&pin);
        if tcp_stream
            .write_all(&(msg_b.len() as u32).to_be_bytes())
            .is_err()
            || tcp_stream.write_all(&msg_b).is_err()
        {
            tracing::warn!("failed to send pairing response, restarting");
            let _ = advertiser.reregister(&device_name);
            continue;
        }

        let keys = match pairing::receiver_finish(state, &msg_a) {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!("pairing failed: {e}, restarting");
                let _ = advertiser.reregister(&device_name);
                continue;
            }
        };
        tracing::info!("pairing successful");
        tcp_stream.set_read_timeout(None)?;

        // Negotiate
        let mut channel = SecureChannel::new(tcp_stream, &keys.control_key, false);
        channel.set_read_timeout(Some(Duration::from_secs(5)))?;

        let offer_bytes = match channel.recv() {
            Ok(Some(b)) => b,
            Ok(None) => {
                tracing::warn!("connection closed (wrong PIN?), restarting");
                let _ = advertiser.reregister(&device_name);
                continue;
            }
            Err(e) => {
                tracing::warn!("decryption failed (wrong PIN?): {e}, restarting");
                let _ = advertiser.reregister(&device_name);
                continue;
            }
        };
        let offer = match NegotiateMessage::from_bytes(&offer_bytes) {
            Ok(NegotiateMessage::Offer(o)) => o,
            _ => {
                tracing::warn!("expected Offer, restarting");
                let _ = advertiser.reregister(&device_name);
                continue;
            }
        };

        let udp_socket = UdpSocket::bind("0.0.0.0:0")?;
        let receiver_udp_port = udp_socket.local_addr()?.port();
        udp_socket.set_nonblocking(true)?;

        let answer = Answer {
            video: AnswerVideo {
                codec: "h264".to_string(),
                max_width: 1920,
                max_height: 1080,
                max_fps: 60,
            },
            audio: AnswerAudio {
                codec: "opus".to_string(),
                sample_rate: 48000,
                channels: 2,
            },
            transport: AnswerTransport {
                udp_port: receiver_udp_port,
            },
        };
        if channel
            .send(&NegotiateMessage::Answer(answer.clone()).to_bytes())
            .is_err()
        {
            tracing::warn!("failed to send answer, restarting");
            let _ = advertiser.reregister(&device_name);
            continue;
        }

        let params = NegotiatedParams::resolve(&offer, &answer);
        tracing::info!(
            "negotiated: {}x{} @ {}fps",
            params.video_width,
            params.video_height,
            params.video_fps
        );
        channel.set_read_timeout(None)?;

        let media_cipher = Cipher::new(&keys.media_key, [0, 0, 0, 1]);
        break (media_cipher, params, udp_socket);
    };

    // Once pairing succeeds, set up decoder and run event loop (one-shot)
    let decoder = VTDecoder::new(DecoderConfig {
        width: params.video_width,
        height: params.video_height,
    })?;

    let audio_config = AudioConfig {
        sample_rate: 48000,
        channels: 2,
        bitrate: 128000,
        frame_duration_ms: 10,
    };
    let opus_decoder = OpusDecoder::new(&audio_config)?;
    let jitter_buffer = Arc::new(Mutex::new(AudioJitterBuffer::new(4)));

    let mut app = App {
        window: None,
        renderer: None,
        audio_output: None,
        udp: udp_socket,
        media_cipher,
        replay_window: ReplayWindow::new(),
        depacketizer: H264Depacketizer::new(),
        decoder,
        video_fec: FecDecoder::new(),
        last_video_seq: None,
        opus_decoder,
        jitter_buffer,
        audio_fec: FecDecoder::new(),
        last_audio_seq: None,
        sync_state: SyncState::new(),
        recv_buf: vec![0u8; 65536],
        frames_rendered: 0,
        total_packets: 0,
    };

    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;
    Ok(())
}
