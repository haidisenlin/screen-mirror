use std::ffi::c_void;
use std::sync::mpsc::{self, Receiver, SyncSender};

use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows::core::PCSTR;

use super::{EncodedPacket, EncoderConfig};
use crate::capture::CapturedFrame;

pub struct NvencEncoder {
    _module: HMODULE,
    tx: SyncSender<EncodedPacket>,
    rx: Receiver<EncodedPacket>,
}

unsafe impl Send for NvencEncoder {}

impl NvencEncoder {
    pub fn new(_config: &EncoderConfig) -> anyhow::Result<Self> {
        unsafe {
            let module = LoadLibraryA(PCSTR(b"nvEncodeAPI64.dll\0".as_ptr()))
                .map_err(|_| anyhow::anyhow!("nvEncodeAPI64.dll not found — no NVIDIA GPU?"))?;

            let _create_fn = GetProcAddress(module, PCSTR(b"NvEncodeAPICreateInstance\0".as_ptr()))
                .ok_or_else(|| anyhow::anyhow!("NvEncodeAPICreateInstance not found"))?;

            // Full NVENC session init requires NVIDIA Video Codec SDK struct layout.
            // This probe confirms the DLL exists but doesn't complete initialization.
            // TODO: Implement full NvEncodeAPICreateInstance → OpenEncodeSession flow.
            anyhow::bail!("NVENC probe succeeded but full init not yet implemented")
        }
    }

    pub fn encode(&mut self, _frame: &CapturedFrame) -> anyhow::Result<()> {
        anyhow::bail!("NVENC not initialized")
    }

    pub fn next_encoded(&self) -> Option<EncodedPacket> {
        self.rx.try_recv().ok()
    }
}
