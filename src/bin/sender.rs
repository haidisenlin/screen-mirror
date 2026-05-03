use anyhow::Result;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

use screen_mirror::audio::AudioConfig;
use screen_mirror::audio::opus_encoder::OpusEncoder;
use screen_mirror::capture::CaptureConfig;
use screen_mirror::capture::macos::{MacOsCapture, native_resolution};
use screen_mirror::encode::EncoderConfig;
use screen_mirror::encode::videotoolbox::VTEncoder;
use screen_mirror::transport::fec::FecEncoder;
use screen_mirror::transport::rtp::{H264Packetizer, RtpHeader, RtpPacket};
use screen_mirror::transport::udp::UdpSender;

const VIDEO_PT: u8 = 96;
const AUDIO_PT: u8 = 111;
const VIDEO_SSRC: u32 = 0x12345678;
const AUDIO_SSRC: u32 = 0x87654321;
const FEC_GROUP_SIZE: usize = 6;

fn extract_audio_pcm(sample: &screencapturekit::cm::CMSampleBuffer) -> Option<Vec<f32>> {
    let buffer_list = sample.audio_buffer_list()?;
    let mut pcm = Vec::new();
    for buf in buffer_list.iter() {
        let data = buf.data();
        if data.len() >= 4 {
            let floats: &[f32] = unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
            };
            pcm.extend_from_slice(floats);
        }
    }
    if pcm.is_empty() { None } else { Some(pcm) }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let target: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:5004".to_string())
        .parse()?;

    tracing::info!("sender starting, target={target}");

    let (cap_w, cap_h) = native_resolution();
    let pixels = cap_w as u64 * cap_h as u64;
    // Scale bitrate proportionally: 15 Mbps baseline for 1920×1080, cap at 40 Mbps
    let bitrate = ((pixels * 15_000_000 / (1920 * 1080)) as u32).min(40_000_000);
    // High-res (>1080p) → 30fps for encoder throughput; otherwise 60fps
    let fps: u32 = if pixels > 1920 * 1200 { 30 } else { 60 };
    tracing::info!("capturing at {cap_w}x{cap_h}, {fps}fps, bitrate={}Mbps", bitrate / 1_000_000);

    let capture = MacOsCapture::new(&CaptureConfig {
        fps,
        width: cap_w,
        height: cap_h,
        capture_audio: true,
    })?;

    let mut encoder = VTEncoder::new(&EncoderConfig {
        width: cap_w,
        height: cap_h,
        fps,
        bitrate,
    })?;

    let mut packetizer = H264Packetizer::new(VIDEO_PT, VIDEO_SSRC, 1400);
    let udp = UdpSender::new(target)?;

    // Video FEC
    let mut video_fec = FecEncoder::new(FEC_GROUP_SIZE, VIDEO_PT);

    // Audio encoding
    let audio_config = AudioConfig {
        sample_rate: 48000,
        channels: 2,
        bitrate: 128_000,
        frame_duration_ms: 10,
    };
    let mut opus_encoder = OpusEncoder::new(&audio_config)?;
    let frame_size_samples = opus_encoder.frame_size_samples(); // 960
    let mut audio_pcm_buf: Vec<f32> = Vec::with_capacity(frame_size_samples * 2);
    let mut audio_seq: u16 = 0;
    let mut audio_ts: u32 = 0;
    let mut audio_fec = FecEncoder::new(FEC_GROUP_SIZE, AUDIO_PT);

    let mut frame_count: u64 = 0;

    tracing::info!("audio: Opus 128kbps, 48kHz stereo, 10ms frames, FEC group={FEC_GROUP_SIZE}");

    loop {
        // 1. Block on video frame
        let Some(sample) = capture.next_frame() else {
            continue;
        };

        let Some(pixel_buffer) = sample.image_buffer() else {
            continue;
        };

        let ts_ns = sample.display_time().unwrap_or_else(|| {
            let t = sample.presentation_timestamp();
            (t.value as u64 * 1_000_000_000) / t.timescale as u64
        });

        encoder.encode_pixel_buffer(pixel_buffer.as_ptr(), ts_ns)?;
        frame_count += 1;

        if frame_count.is_multiple_of(fps as u64) {
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
                    // Push to video FEC
                    if let Some(fec_pkt) = video_fec.push(pkt) {
                        udp.send(&fec_pkt.serialize())?;
                    }
                }
                // Flush FEC at frame boundary (after last NAL of this encoded frame)
                if i == nal_count - 1
                    && let Some(fec_pkt) = video_fec.flush()
                {
                    udp.send(&fec_pkt.serialize())?;
                }
            }
        }

        // 2. Drain available audio samples non-blocking
        while let Some(audio_sample) = capture.try_next_audio() {
            let Some(pcm) = extract_audio_pcm(&audio_sample) else {
                continue;
            };
            audio_pcm_buf.extend_from_slice(&pcm);

            // 3. Encode when we have enough samples for a 10ms frame
            while audio_pcm_buf.len() >= frame_size_samples {
                let frame: Vec<f32> =
                    audio_pcm_buf.drain(..frame_size_samples).collect();
                let opus_data = opus_encoder.encode(&frame)?;

                // Build audio RTP packet
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
                // 48kHz, 10ms frame = 480 samples per channel
                audio_ts = audio_ts.wrapping_add(480);

                // 4. Send audio RTP + FEC
                udp.send(&audio_pkt.serialize())?;
                if let Some(fec_pkt) = audio_fec.push(&audio_pkt) {
                    udp.send(&fec_pkt.serialize())?;
                }
            }
        }
    }
}
