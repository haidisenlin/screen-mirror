use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tracing_subscriber::EnvFilter;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::EventLoop;
use winit::window::Window;

use screen_mirror::audio::opus_decoder::OpusDecoder;
use screen_mirror::audio::output::AudioOutput;
use screen_mirror::audio::AudioConfig;
use screen_mirror::decode::videotoolbox::VTDecoder;
use screen_mirror::decode::DecoderConfig;
use screen_mirror::render::metal::MetalRenderer;
use screen_mirror::sync::SyncState;
use screen_mirror::transport::fec::{FecDecoder, FecHeader};
use screen_mirror::transport::jitter::AudioJitterBuffer;
use screen_mirror::transport::rtp::{H264Depacketizer, RtpPacket};
use screen_mirror::transport::udp::UdpReceiver;

const PT_VIDEO: u8 = 96;
const PT_AUDIO: u8 = 111;
const PT_FEC: u8 = 127;

struct App {
    window: Option<Window>,
    renderer: Option<MetalRenderer>,
    audio_output: Option<AudioOutput>,
    udp: UdpReceiver,
    // Video pipeline
    depacketizer: H264Depacketizer,
    decoder: VTDecoder,
    video_fec: FecDecoder,
    last_video_seq: Option<u16>,
    // Audio pipeline
    opus_decoder: OpusDecoder,
    jitter_buffer: Arc<Mutex<AudioJitterBuffer>>,
    audio_fec: FecDecoder,
    last_audio_seq: Option<u16>,
    // Sync
    sync_state: SyncState,
    // Shared state
    recv_buf: Vec<u8>,
    frames_rendered: u64,
    total_packets: u64,
}

impl App {
    fn poll_packets(&mut self) {
        loop {
            match self.udp.recv(&mut self.recv_buf) {
                Ok(n) => {
                    let Some(pkt) = RtpPacket::parse(&self.recv_buf[..n]) else {
                        continue;
                    };
                    self.total_packets += 1;

                    // Periodic FEC cleanup
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
                        pt => {
                            tracing::debug!("unknown payload type: {pt}");
                        }
                    }
                }
                Err(e) => {
                    match e.downcast_ref::<std::io::Error>() {
                        Some(io_err) if io_err.kind() == std::io::ErrorKind::WouldBlock => break,
                        _ => {
                            tracing::error!("recv error: {e}");
                            break;
                        }
                    }
                }
            }
        }
    }

    fn handle_video_packet(&mut self, pkt: &RtpPacket) {
        let seq = pkt.header.sequence_number;
        self.video_fec.push_media(pkt);

        // Loss detection
        if let Some(prev) = self.last_video_seq {
            let expected = prev.wrapping_add(1);
            if seq != expected {
                let gap = seq.wrapping_sub(expected);
                tracing::warn!("video loss: expected {expected}, got {seq} (gap={gap})");
                // Try FEC recovery for each missing seq
                let mut s = expected;
                while s != seq {
                    if let Some(recovered) = self.video_fec.recover(s) {
                        tracing::debug!("video FEC recovered seq {s}");
                        // Reconstruct a fake RtpPacket to depacketize
                        let recovered_pkt = RtpPacket {
                            header: pkt.header.clone(),
                            payload: recovered,
                        };
                        self.depacketizer.push(&recovered_pkt);
                    } else {
                        tracing::warn!("video seq {s} unrecoverable");
                    }
                    s = s.wrapping_add(1);
                }
            }
        }
        self.last_video_seq = Some(seq);

        let marker = pkt.header.marker;
        let ts = pkt.header.timestamp as u64;
        self.depacketizer.push(pkt);

        if marker
            && let Some(nal) = self.depacketizer.pop_nal()
            && let Err(e) = self.decoder.decode_nal(&nal, ts)
        {
            tracing::warn!("decode error: {e}");
        }
    }

    fn handle_audio_packet(&mut self, pkt: &RtpPacket) {
        let seq = pkt.header.sequence_number;
        self.audio_fec.push_media(pkt);

        // Loss detection
        if let Some(prev) = self.last_audio_seq {
            let expected = prev.wrapping_add(1);
            if seq != expected {
                let gap = seq.wrapping_sub(expected);
                tracing::warn!("audio loss: expected {expected}, got {seq} (gap={gap})");
                // Try FEC recovery or PLC for each missing seq
                let mut s = expected;
                while s != seq {
                    if let Some(recovered) = self.audio_fec.recover(s) {
                        tracing::debug!("audio FEC recovered seq {s}");
                        match self.opus_decoder.decode(&recovered) {
                            Ok(pcm) => {
                                if let Ok(mut jb) = self.jitter_buffer.lock() {
                                    jb.push_frame(pcm);
                                }
                            }
                            Err(e) => {
                                tracing::warn!("opus decode of recovered packet failed: {e}");
                                self.do_plc();
                            }
                        }
                    } else {
                        tracing::debug!("audio seq {s} unrecoverable, using PLC");
                        self.do_plc();
                    }
                    s = s.wrapping_add(1);
                }
            }
        }
        self.last_audio_seq = Some(seq);

        // Decode current packet
        match self.opus_decoder.decode(&pkt.payload) {
            Ok(pcm) => {
                if let Ok(mut jb) = self.jitter_buffer.lock() {
                    jb.push_frame(pcm);
                }
            }
            Err(e) => {
                tracing::warn!("opus decode error: {e}");
                self.do_plc();
            }
        }
    }

    fn do_plc(&mut self) {
        match self.opus_decoder.decode_plc() {
            Ok(pcm) => {
                if let Ok(mut jb) = self.jitter_buffer.lock() {
                    jb.push_frame(pcm);
                }
            }
            Err(e) => {
                tracing::warn!("PLC failed: {e}");
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
            pt => {
                tracing::debug!("FEC for unknown media_pt: {pt}");
            }
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

            // Start audio output after event loop is active
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

                // Update sync state with audio progress
                if let Some(ref audio_output) = self.audio_output {
                    self.sync_state
                        .report_audio_played(audio_output.samples_played());
                }

                // Render latest decoded frame
                if let Some(frame) = self.decoder.next_frame()
                    && let Some(renderer) = &mut self.renderer
                {
                    let _ = unsafe { renderer.render_pixel_buffer(frame.pixel_buffer) };
                    self.frames_rendered += 1;
                    self.sync_state
                        .report_video_rendered(frame.timestamp as u32);
                    if self.frames_rendered.is_multiple_of(60) {
                        tracing::info!(
                            "rendered {} frames, rate_adj={:.3}",
                            self.frames_rendered,
                            self.sync_state.rate_adjustment()
                        );
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

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let bind: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:5004".to_string())
        .parse()?;

    tracing::info!("receiver starting, listening on {bind}");

    let udp = UdpReceiver::new(bind)?;
    udp.set_nonblocking(true)?;

    let decoder = VTDecoder::new(DecoderConfig {
        width: 1920,
        height: 1080,
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
        udp,
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
