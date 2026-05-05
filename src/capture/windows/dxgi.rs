use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_OUTDUPL_FRAME_INFO, IDXGIFactory1, IDXGIOutput1,
    IDXGIOutputDuplication,
};
use windows::core::Interface;

use crate::capture::{CaptureConfig, CaptureError, CapturedFrame, NativeFrame, VideoCapture};

struct DxgiState {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    width: u32,
    height: u32,
}

pub struct DxgiCapture {
    rx: Receiver<CapturedFrame>,
    width: u32,
    height: u32,
    device: ID3D11Device,
}

impl VideoCapture for DxgiCapture {
    fn new(config: &CaptureConfig) -> anyhow::Result<Self> {
        let state = init_dxgi()?;
        let width = state.width;
        let height = state.height;
        let device = state.device.clone();

        let fps = if config.fps == 0 {
            let pixels = width as u64 * height as u64;
            if pixels > 1920 * 1200 { 30u32 } else { 60 }
        } else {
            config.fps
        };

        let (tx, rx) = mpsc::sync_channel::<CapturedFrame>(2);

        thread::Builder::new()
            .name("dxgi-capture".into())
            .spawn(move || capture_loop(state, tx, fps))?;

        Ok(Self {
            rx,
            width,
            height,
            device,
        })
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn next_frame(&self) -> Result<Option<CapturedFrame>, CaptureError> {
        match self.rx.recv() {
            Ok(frame) => Ok(Some(frame)),
            Err(_) => Ok(None),
        }
    }
}

impl DxgiCapture {
    pub fn device(&self) -> &ID3D11Device {
        &self.device
    }
}

fn init_dxgi() -> anyhow::Result<DxgiState> {
    unsafe {
        let mut device = None;
        let mut context = None;

        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            None,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            7,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;

        let device = device.ok_or_else(|| anyhow::anyhow!("D3D11CreateDevice returned null"))?;
        let context = context.ok_or_else(|| anyhow::anyhow!("D3D11 device context is null"))?;

        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let adapter = factory.EnumAdapters(0)?;
        let output = adapter.EnumOutputs(0)?;
        let output1: IDXGIOutput1 = output.cast()?;
        let duplication = output1.DuplicateOutput(&device)?;

        let mut desc = std::mem::zeroed();
        duplication.GetDesc(&mut desc);

        let width = desc.ModeDesc.Width;
        let height = desc.ModeDesc.Height;

        tracing::info!("DXGI: initialized {width}x{height}");

        Ok(DxgiState {
            device,
            context,
            duplication,
            width,
            height,
        })
    }
}

fn capture_loop(state: DxgiState, tx: SyncSender<CapturedFrame>, fps: u32) {
    let frame_interval = Duration::from_nanos(1_000_000_000 / fps as u64);
    let timeout_ms = frame_interval.as_millis() as u32;

    let mut cursor_shape: Vec<u8> = Vec::new();
    let mut cursor_width: u32 = 0;
    let mut cursor_height: u32 = 0;
    let mut cursor_visible = true;
    let mut cursor_x: i32 = 0;
    let mut cursor_y: i32 = 0;

    loop {
        unsafe {
            let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut desktop_resource = None;

            let hr = state.duplication.AcquireNextFrame(
                timeout_ms,
                &mut frame_info,
                &mut desktop_resource,
            );

            if hr.is_err() {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }

            if frame_info.LastMouseUpdateTime != 0 {
                cursor_visible = frame_info.PointerPosition.Visible.as_bool();
                cursor_x = frame_info.PointerPosition.Position.x;
                cursor_y = frame_info.PointerPosition.Position.y;
            }

            if frame_info.PointerShapeBufferSize > 0 {
                cursor_shape.resize(frame_info.PointerShapeBufferSize as usize, 0);
                let mut shape_info = std::mem::zeroed();
                let mut required_size = 0u32;
                let _ = state.duplication.GetFramePointerShape(
                    cursor_shape.len() as u32,
                    cursor_shape.as_mut_ptr() as *mut _,
                    &mut required_size,
                    &mut shape_info,
                );
                cursor_width = shape_info.Width;
                cursor_height = shape_info.Height;
            }

            let resource = match desktop_resource {
                Some(r) => r,
                None => {
                    let _ = state.duplication.ReleaseFrame();
                    continue;
                }
            };

            let texture: ID3D11Texture2D = resource.cast().unwrap();
            let timestamp_ns = frame_info.LastPresentTime as u64 * 100;

            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            desc.Usage = windows::Win32::Graphics::Direct3D11::D3D11_USAGE_DEFAULT;
            desc.BindFlags = windows::Win32::Graphics::Direct3D11::D3D11_BIND_SHADER_RESOURCE;
            desc.CPUAccessFlags = windows::Win32::Graphics::Direct3D11::D3D11_CPU_ACCESS_FLAG(0);
            desc.MiscFlags = windows::Win32::Graphics::Direct3D11::D3D11_RESOURCE_MISC_FLAG(0);

            let mut copy_texture = None;
            let hr = state
                .device
                .CreateTexture2D(&desc, None, Some(&mut copy_texture));
            if hr.is_err() {
                let _ = state.duplication.ReleaseFrame();
                continue;
            }

            let copy_tex = copy_texture.unwrap();
            state.context.CopyResource(&copy_tex, &texture);
            let _ = state.duplication.ReleaseFrame();

            if cursor_visible && !cursor_shape.is_empty() && cursor_width > 0 {
                composite_cursor_onto_texture(
                    &state.device,
                    &state.context,
                    &copy_tex,
                    &cursor_shape,
                    cursor_x,
                    cursor_y,
                    cursor_width,
                    cursor_height,
                    desc.Width,
                    desc.Height,
                );
            }

            let native = Box::into_raw(Box::new(copy_tex)) as NativeFrame;

            let frame = CapturedFrame {
                native,
                timestamp_ns,
            };

            if let Err(e) = tx.try_send(frame) {
                let leaked = e.into_inner();
                let _ = Box::from_raw(leaked.native as *mut ID3D11Texture2D);
            }
        }
    }
}

unsafe fn composite_cursor_onto_texture(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    target: &ID3D11Texture2D,
    cursor_data: &[u8],
    cx: i32,
    cy: i32,
    cw: u32,
    ch: u32,
    frame_w: u32,
    frame_h: u32,
) {
    if cx >= frame_w as i32 || cy >= frame_h as i32 || cx + cw as i32 <= 0 || cy + ch as i32 <= 0 {
        return;
    }

    let mut desc = D3D11_TEXTURE2D_DESC::default();
    target.GetDesc(&mut desc);
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = windows::Win32::Graphics::Direct3D11::D3D11_BIND_FLAG(0);
    desc.CPUAccessFlags =
        D3D11_CPU_ACCESS_READ | windows::Win32::Graphics::Direct3D11::D3D11_CPU_ACCESS_WRITE;

    let mut staging = None;
    if device
        .CreateTexture2D(&desc, None, Some(&mut staging))
        .is_err()
    {
        return;
    }
    let staging = staging.unwrap();
    context.CopyResource(&staging, target);

    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    if context
        .Map(
            &staging,
            0,
            windows::Win32::Graphics::Direct3D11::D3D11_MAP_READ_WRITE,
            0,
            Some(&mut mapped),
        )
        .is_err()
    {
        return;
    }

    let row_pitch = mapped.RowPitch as usize;
    let dst = std::slice::from_raw_parts_mut(mapped.pData as *mut u8, row_pitch * frame_h as usize);

    for row in 0..ch {
        let dy = cy + row as i32;
        if dy < 0 || dy >= frame_h as i32 {
            continue;
        }
        for col in 0..cw {
            let dx = cx + col as i32;
            if dx < 0 || dx >= frame_w as i32 {
                continue;
            }
            let src_offset = (row * cw + col) as usize * 4;
            if src_offset + 3 >= cursor_data.len() {
                break;
            }
            let alpha = cursor_data[src_offset + 3] as u32;
            if alpha == 0 {
                continue;
            }
            let dst_offset = dy as usize * row_pitch + dx as usize * 4;
            if alpha == 255 {
                dst[dst_offset..dst_offset + 4]
                    .copy_from_slice(&cursor_data[src_offset..src_offset + 4]);
            } else {
                let inv = 255 - alpha;
                for c in 0..3 {
                    let s = cursor_data[src_offset + c] as u32;
                    let d = dst[dst_offset + c] as u32;
                    dst[dst_offset + c] = ((s * alpha + d * inv) / 255) as u8;
                }
            }
        }
    }

    context.Unmap(&staging, 0);
    context.CopyResource(target, &staging);
}
