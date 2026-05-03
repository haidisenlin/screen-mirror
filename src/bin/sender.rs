// src/bin/sender.rs

use anyhow::Result;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

use screen_mirror::audio::AudioConfig;
use screen_mirror::audio::opus_encoder::OpusEncoder;
use screen_mirror::capture::CaptureConfig;
#[cfg(target_os = "macos")]
use screen_mirror::capture::macos::{MacOsCapture, native_resolution};
use screen_mirror::capture::VideoCapture;
use screen_mirror::encode::{EncoderConfig, VideoEncoder};
use screen_mirror::transport::fec::FecEncoder;
use screen_mirror::transport::rtp::{H264Packetizer, RtpHeader, RtpPacket};
use screen_mirror::transport::udp::UdpSender;

#[cfg(target_os = "windows")]
use screen_mirror::capture::windows::{DxgiCapture, WasapiCapture};
#[cfg(target_os = "windows")]
use screen_mirror::capture::AudioCapture;

const VIDEO_PT: u8 = 96;
const AUDIO_PT: u8 = 111;
const VIDEO_SSRC: u32 = 0x12345678;
const AUDIO_SSRC: u32 = 0x87654321;
const FEC_GROUP_SIZE: usize = 6;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let target: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:5004".to_string())
        .parse()?;

    tracing::info!("sender starting, target={target}");

    // --- Platform-specific capture setup ---
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
    tracing::info!("capturing at {cap_w}x{cap_h}, {fps}fps, bitrate={}Mbps", bitrate / 1_000_000);

    #[cfg(target_os = "macos")]
    let mut encoder = VideoEncoder::new(&EncoderConfig {
        width: cap_w,
        height: cap_h,
        fps,
        bitrate,
    })?;

    #[cfg(target_os = "windows")]
    let mut encoder = VideoEncoder::new_with_device(&EncoderConfig {
        width: cap_w,
        height: cap_h,
        fps,
        bitrate,
    }, capture.device())?;

    let mut packetizer = H264Packetizer::new(VIDEO_PT, VIDEO_SSRC, 1400);
    let udp = UdpSender::new(target)?;

    let mut video_fec = FecEncoder::new(FEC_GROUP_SIZE, VIDEO_PT);

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
    let mut audio_fec = FecEncoder::new(FEC_GROUP_SIZE, AUDIO_PT);

    let mut frame_count: u64 = 0;

    tracing::info!("audio: Opus 128kbps, 48kHz stereo, 10ms frames, FEC group={FEC_GROUP_SIZE}");

    loop {
        // 1. Block on video frame
        let Some(frame) = capture.next_frame() else {
            continue;
        };

        encoder.encode(&frame)?;
        frame_count += 1;

        if frame_count % fps as u64 == 0 {
            tracing::info!("encoded {frame_count} frames");
        }

        // Process encoded video NALs with FEC
        while let Some(encoded) = encoder.next_encoded() {
            let rtp_ts = (encoded.timestamp / 11_111) as u32;
            let nal_count = encoded.nal_units.len();
            for (i, nal) in encoded.nal_units.iter().enumerate() {
                let rtp_packets = packetizer.packetize(nal, rtp_ts);
                for pkt in &rtp_packets {
                    udp.send(&pkt.serialize())?;
                    if let Some(fec_pkt) = video_fec.push(pkt) {
                        udp.send(&fec_pkt.serialize())?;
                    }
                }
                if i == nal_count - 1 {
                    if let Some(fec_pkt) = video_fec.flush() {
                        udp.send(&fec_pkt.serialize())?;
                    }
                }
            }
        }

        // 2. Drain audio samples
        #[cfg(target_os = "macos")]
        let audio_iter = std::iter::from_fn(|| capture.try_next_audio());
        #[cfg(target_os = "windows")]
        let audio_iter = std::iter::from_fn(|| audio_capture.try_next_audio());

        for pcm in audio_iter {
            audio_pcm_buf.extend_from_slice(&pcm);

            while audio_pcm_buf.len() >= frame_size_samples {
                let frame_pcm: Vec<f32> =
                    audio_pcm_buf.drain(..frame_size_samples).collect();
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

                udp.send(&audio_pkt.serialize())?;
                if let Some(fec_pkt) = audio_fec.push(&audio_pkt) {
                    udp.send(&fec_pkt.serialize())?;
                }
            }
        }
    }
}
