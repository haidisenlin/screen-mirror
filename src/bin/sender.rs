use std::io::Write;
use std::net::{TcpStream, UdpSocket};
use std::time::Duration;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

use screen_mirror::audio::AudioConfig;
use screen_mirror::audio::opus_encoder::OpusEncoder;
use screen_mirror::capture::CaptureConfig;
#[cfg(target_os = "macos")]
use screen_mirror::capture::macos::{MacOsCapture, native_resolution};
use screen_mirror::capture::VideoCapture;
use screen_mirror::discovery::browser;
use screen_mirror::encode::{EncoderConfig, VideoEncoder};
use screen_mirror::protocol::negotiate::*;
use screen_mirror::protocol::session::SecureChannel;
use screen_mirror::security::cipher::Cipher;
use screen_mirror::security::pairing;
use screen_mirror::transport::fec::FecEncoder;
use screen_mirror::transport::rtp::{H264Packetizer, RtpHeader, RtpPacket};

#[cfg(target_os = "windows")]
use screen_mirror::capture::windows::{DxgiCapture, WasapiCapture};
#[cfg(target_os = "windows")]
use screen_mirror::capture::AudioCapture;

const VIDEO_PT: u8 = 96;
const AUDIO_PT: u8 = 111;
const VIDEO_SSRC: u32 = 0x12345678;
const AUDIO_SSRC: u32 = 0x87654321;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Step 1: Discover receivers via mDNS
    tracing::info!("searching for receivers...");
    let receivers = browser::browse(Duration::from_secs(3))?;

    if receivers.is_empty() {
        anyhow::bail!("no receivers found on the network");
    }

    let target = &receivers[0];
    tracing::info!("found receiver: {} at {}", target.name, target.addr);

    // Step 2: Get PIN
    let pin = std::env::args()
        .nth(1)
        .unwrap_or_else(|| {
            eprint!("Enter 6-digit PIN: ");
            std::io::stderr().flush().unwrap();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            input.trim().to_string()
        });

    // Step 3: TCP connect and SPAKE2 pairing
    tracing::info!("connecting to {}...", target.addr);
    let mut stream = TcpStream::connect_timeout(&target.addr, Duration::from_secs(5))?;

    let (msg_a, state) = pairing::sender_start(&pin);
    stream.write_all(&(msg_a.len() as u32).to_be_bytes())?;
    stream.write_all(&msg_a)?;

    let mut len_buf = [0u8; 4];
    std::io::Read::read_exact(&mut stream, &mut len_buf)?;
    let pb_len = u32::from_be_bytes(len_buf) as usize;
    let mut msg_b = vec![0u8; pb_len];
    std::io::Read::read_exact(&mut stream, &mut msg_b)?;

    let keys = pairing::sender_finish(state, &msg_b)?;
    tracing::info!("pairing successful, session keys derived");

    // Step 4: Secure channel + negotiation
    let mut channel = SecureChannel::new(stream, &keys.control_key);

    #[cfg(target_os = "macos")]
    let (cap_w, cap_h, capture) = {
        let (w, h) = native_resolution();
        let capture = MacOsCapture::new(&CaptureConfig { fps: 0, width: w, height: h })?;
        (capture.width(), capture.height(), capture)
    };

    #[cfg(target_os = "windows")]
    let (cap_w, cap_h, capture, audio_capture) = {
        let capture = DxgiCapture::new(&CaptureConfig { fps: 0, width: 0, height: 0 })?;
        let w = capture.width();
        let h = capture.height();
        let audio_capture = WasapiCapture::new(48000, 2)?;
        (w, h, capture, audio_capture)
    };

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

    channel.send(&NegotiateMessage::Offer(offer.clone()).to_bytes())?;

    channel.set_read_timeout(Some(Duration::from_secs(5)))?;
    let answer_bytes = channel
        .recv()?
        .ok_or_else(|| anyhow::anyhow!("connection closed during negotiation"))?;
    let answer = match NegotiateMessage::from_bytes(&answer_bytes)? {
        NegotiateMessage::Answer(a) => a,
        _ => anyhow::bail!("expected Answer, got something else"),
    };

    let params = NegotiatedParams::resolve(&offer, &answer);
    tracing::info!(
        "negotiated: {}x{} @ {}fps, receiver UDP port {}",
        params.video_width, params.video_height, params.video_fps, params.receiver_udp_port
    );
    channel.set_read_timeout(None)?;

    // Step 5: Set up encrypted media
    let receiver_media_addr = std::net::SocketAddr::new(target.addr.ip(), params.receiver_udp_port);
    udp_socket.connect(receiver_media_addr)?;
    let mut media_cipher = Cipher::new(&keys.media_key, [0, 0, 0, 1]);

    // Step 6: Encode and send
    #[cfg(target_os = "macos")]
    let mut encoder = VideoEncoder::new(&EncoderConfig {
        width: params.video_width,
        height: params.video_height,
        fps: params.video_fps,
        bitrate: params.video_bitrate,
    })?;

    #[cfg(target_os = "windows")]
    let mut encoder = VideoEncoder::new_with_device(&EncoderConfig {
        width: params.video_width,
        height: params.video_height,
        fps: params.video_fps,
        bitrate: params.video_bitrate,
    }, capture.device())?;

    let mut packetizer = H264Packetizer::new(VIDEO_PT, VIDEO_SSRC, 1400);
    let mut video_fec = FecEncoder::new(params.fec_group_size, VIDEO_PT);

    let audio_config = AudioConfig {
        sample_rate: 48000,
        channels: 2,
        bitrate: 128_000,
        frame_duration_ms: 10,
    };
    let mut opus_encoder = OpusEncoder::new(&audio_config)?;
    let frame_size_samples = opus_encoder.frame_size_samples();
    let mut audio_pcm_buf: Vec<f32> = Vec::with_capacity(frame_size_samples * 2);
    let mut audio_seq: u16 = 0;
    let mut audio_ts: u32 = 0;
    let mut audio_fec = FecEncoder::new(params.fec_group_size, AUDIO_PT);

    let mut frame_count: u64 = 0;

    tracing::info!("streaming started");

    loop {
        let Some(frame) = capture.next_frame() else {
            continue;
        };

        encoder.encode(frame)?;
        frame_count += 1;

        if frame_count % fps as u64 == 0 {
            tracing::info!("encoded {frame_count} frames");
        }

        while let Some(encoded) = encoder.next_encoded() {
            let rtp_ts = (encoded.timestamp / 11_111) as u32;
            let nal_count = encoded.nal_units.len();
            for (i, nal) in encoded.nal_units.iter().enumerate() {
                let rtp_packets = packetizer.packetize(nal, rtp_ts);
                for pkt in &rtp_packets {
                    let encrypted = media_cipher.seal(&pkt.serialize());
                    udp_socket.send(&encrypted)?;
                    if let Some(fec_pkt) = video_fec.push(pkt) {
                        let encrypted_fec = media_cipher.seal(&fec_pkt.serialize());
                        udp_socket.send(&encrypted_fec)?;
                    }
                }
                if i == nal_count - 1 {
                    if let Some(fec_pkt) = video_fec.flush() {
                        let encrypted_fec = media_cipher.seal(&fec_pkt.serialize());
                        udp_socket.send(&encrypted_fec)?;
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        let audio_iter = std::iter::from_fn(|| capture.try_next_audio());
        #[cfg(target_os = "windows")]
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
                udp_socket.send(&encrypted)?;
                if let Some(fec_pkt) = audio_fec.push(&audio_pkt) {
                    let encrypted_fec = media_cipher.seal(&fec_pkt.serialize());
                    udp_socket.send(&encrypted_fec)?;
                }
            }
        }
    }
}
