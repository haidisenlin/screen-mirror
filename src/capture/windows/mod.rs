mod dxgi;
mod wasapi;

pub use dxgi::DxgiCapture;
pub use wasapi::WasapiCapture;

pub fn list_windows_windows() -> Vec<crate::ui::messages::WindowInfo> {
    // TODO: implement Windows window enumeration
    vec![]
}
