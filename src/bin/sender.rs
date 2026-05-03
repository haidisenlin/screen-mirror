use anyhow::Result;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

use screen_mirror::capture::CaptureConfig;
use screen_mirror::capture::macos::MacOsCapture;
use screen_mirror::encode::EncoderConfig;
use screen_mirror::encode::videotoolbox::VTEncoder;
use screen_mirror::transport::rtp::H264Packetizer;
use screen_mirror::transport::udp::UdpSender;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let target: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:5004".to_string())
        .parse()?;

    tracing::info!("sender starting, target={target}");

    // width=0, height=0 → auto-detect native display resolution
    let capture = MacOsCapture::new(&CaptureConfig {
        fps: 60,
        width: 0,
        height: 0,
        capture_audio: true,
    })?;

    let cap_w = capture.width();
    let cap_h = capture.height();
    let pixels = cap_w as u64 * cap_h as u64;
    // Scale bitrate proportionally: 10 Mbps baseline for 1920×1080
    let bitrate = (pixels * 10_000_000 / (1920 * 1080)) as u32;
    tracing::info!("capturing at {cap_w}x{cap_h}, bitrate={bitrate}");

    let mut encoder = VTEncoder::new(&EncoderConfig {
        width: cap_w,
        height: cap_h,
        fps: 60,
        bitrate,
    })?;

    let mut packetizer = H264Packetizer::new(96, 0x12345678, 1400);
    let udp = UdpSender::new(target)?;

    let mut frame_count: u64 = 0;

    loop {
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

        if frame_count.is_multiple_of(60) {
            tracing::info!("encoded {frame_count} frames");
        }

        while let Some(encoded) = encoder.next_encoded() {
            let rtp_ts = (encoded.timestamp / 11_111) as u32;
            for nal in &encoded.nal_units {
                let rtp_packets = packetizer.packetize(nal, rtp_ts);
                for pkt in &rtp_packets {
                    udp.send(&pkt.serialize())?;
                }
            }
        }
    }
}
