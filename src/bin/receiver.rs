use anyhow::Result;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::EventLoop;
use winit::window::Window;

use screen_mirror::decode::videotoolbox::VTDecoder;
use screen_mirror::decode::DecoderConfig;
use screen_mirror::render::metal::MetalRenderer;
use screen_mirror::transport::rtp::{H264Depacketizer, RtpPacket};
use screen_mirror::transport::udp::UdpReceiver;

struct App {
    window: Option<Window>,
    renderer: Option<MetalRenderer>,
    udp: UdpReceiver,
    depacketizer: H264Depacketizer,
    decoder: VTDecoder,
    recv_buf: Vec<u8>,
    last_seq: Option<u16>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes()
                .with_title("Screen Mirror Receiver")
                .with_inner_size(winit::dpi::LogicalSize::new(1920u32, 1080u32));
            let window = event_loop.create_window(attrs).unwrap();
            self.renderer = Some(MetalRenderer::new(&window).unwrap());
            self.window = Some(window);
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
                // Poll UDP packets
                loop {
                    match self.udp.recv(&mut self.recv_buf) {
                        Ok(n) => {
                            if let Some(pkt) = RtpPacket::parse(&self.recv_buf[..n]) {
                                if let Some(prev) = self.last_seq {
                                    let expected = prev.wrapping_add(1);
                                    if pkt.header.sequence_number != expected {
                                        tracing::warn!(
                                            "packet loss: expected {expected}, got {}",
                                            pkt.header.sequence_number
                                        );
                                    }
                                }
                                self.last_seq = Some(pkt.header.sequence_number);

                                let marker = pkt.header.marker;
                                let ts = pkt.header.timestamp as u64;
                                self.depacketizer.push(&pkt);

                                if marker {
                                    if let Some(nal) = self.depacketizer.pop_nal() {
                                        let _ = self.decoder.decode_nal(&nal, ts);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            match e.downcast_ref::<std::io::Error>() {
                                Some(io_err)
                                    if io_err.kind() == std::io::ErrorKind::WouldBlock =>
                                {
                                    break;
                                }
                                _ => {
                                    tracing::error!("recv error: {e}");
                                    break;
                                }
                            }
                        }
                    }
                }

                // Render latest decoded frame (Drop handles CFRelease)
                if let Some(frame) = self.decoder.next_frame() {
                    if let Some(renderer) = &mut self.renderer {
                        let _ = renderer.render_pixel_buffer(frame.pixel_buffer);
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

    let mut app = App {
        window: None,
        renderer: None,
        udp,
        depacketizer: H264Depacketizer::new(),
        decoder,
        recv_buf: vec![0u8; 2000],
        last_seq: None,
    };

    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    Ok(())
}
