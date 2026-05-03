use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11ComputeShader, ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView,
    ID3D11Texture2D, ID3D11UnorderedAccessView, D3D11_BIND_SHADER_RESOURCE,
    D3D11_BIND_UNORDERED_ACCESS, D3D11_SRV_DIMENSION_TEXTURE2D, D3D11_TEX2D_SRV,
    D3D11_TEX2D_UAV, D3D11_TEXTURE2D_DESC, D3D11_UAV_DIMENSION_TEXTURE2D,
    D3D11_UNORDERED_ACCESS_VIEW_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_FORMAT_R8G8_UNORM, DXGI_FORMAT_R8_UNORM,
    DXGI_SAMPLE_DESC,
};
use windows::core::PCSTR;

const SHADER_SOURCE: &[u8] = b"
Texture2D<float4> inputTex : register(t0);
RWTexture2D<float> outputY : register(u0);
RWTexture2D<float2> outputUV : register(u1);

[numthreads(16, 16, 1)]
void CSMain(uint3 id : SV_DispatchThreadID)
{
    uint width, height;
    inputTex.GetDimensions(width, height);

    if (id.x >= width || id.y >= height)
        return;

    float4 pixel = inputTex[id.xy];
    float r = pixel.x;
    float g = pixel.y;
    float b = pixel.z;

    float y = 0.257 * r + 0.504 * g + 0.098 * b + 0.0625;
    outputY[id.xy] = y;

    if ((id.x & 1) == 0 && (id.y & 1) == 0)
    {
        float u = -0.148 * r - 0.291 * g + 0.439 * b + 0.5;
        float v =  0.439 * r - 0.368 * g - 0.071 * b + 0.5;
        outputUV[uint2(id.x >> 1, id.y >> 1)] = float2(u, v);
    }
}\0";

pub struct BgraToNv12Converter {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    shader: ID3D11ComputeShader,
    nv12_texture: ID3D11Texture2D,
    y_texture: ID3D11Texture2D,
    uv_texture: ID3D11Texture2D,
    y_uav: ID3D11UnorderedAccessView,
    uv_uav: ID3D11UnorderedAccessView,
    width: u32,
    height: u32,
}

impl BgraToNv12Converter {
    pub fn new(device: &ID3D11Device, width: u32, height: u32) -> anyhow::Result<Self> {
        unsafe {
            let mut ctx = None;
            device.GetImmediateContext(&mut ctx);
            let context = ctx.ok_or_else(|| anyhow::anyhow!("failed to get device context"))?;

            let mut blob = None;
            let mut error_blob = None;
            D3DCompile(
                SHADER_SOURCE.as_ptr() as *const _,
                SHADER_SOURCE.len() - 1,
                PCSTR::null(),
                None,
                None,
                PCSTR(b"CSMain\0".as_ptr()),
                PCSTR(b"cs_5_0\0".as_ptr()),
                0,
                0,
                &mut blob,
                Some(&mut error_blob),
            )?;

            let blob = blob.ok_or_else(|| anyhow::anyhow!("shader compilation failed"))?;
            let bytecode = std::slice::from_raw_parts(
                blob.GetBufferPointer() as *const u8,
                blob.GetBufferSize(),
            );

            let shader = device.CreateComputeShader(bytecode, None)?;

            let sample_desc = DXGI_SAMPLE_DESC { Count: 1, Quality: 0 };
            let zero_cpu = windows::Win32::Graphics::Direct3D11::D3D11_CPU_ACCESS_FLAG(0);
            let zero_misc = windows::Win32::Graphics::Direct3D11::D3D11_RESOURCE_MISC_FLAG(0);

            // Intermediate Y texture (R8_UNORM, full resolution) for compute shader output
            let y_texture = device.CreateTexture2D(&D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8_UNORM,
                SampleDesc: sample_desc,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_UNORDERED_ACCESS,
                CPUAccessFlags: zero_cpu,
                MiscFlags: zero_misc,
            }, None)?;

            // Intermediate UV texture (R8G8_UNORM, half resolution) for compute shader output
            let uv_texture = device.CreateTexture2D(&D3D11_TEXTURE2D_DESC {
                Width: width / 2,
                Height: height / 2,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8G8_UNORM,
                SampleDesc: sample_desc,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_UNORDERED_ACCESS,
                CPUAccessFlags: zero_cpu,
                MiscFlags: zero_misc,
            }, None)?;

            // Final NV12 texture for the encoder (planes assembled via CopySubresourceRegion)
            let nv12_texture = device.CreateTexture2D(&D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: sample_desc,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE,
                CPUAccessFlags: zero_cpu,
                MiscFlags: zero_misc,
            }, None)?;

            let y_uav_desc = D3D11_UNORDERED_ACCESS_VIEW_DESC {
                Format: DXGI_FORMAT_R8_UNORM,
                ViewDimension: D3D11_UAV_DIMENSION_TEXTURE2D,
                Anonymous: windows::Win32::Graphics::Direct3D11::D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_UAV { MipSlice: 0 },
                },
            };
            let y_uav = device.CreateUnorderedAccessView(&y_texture, Some(&y_uav_desc))?;

            let uv_uav_desc = D3D11_UNORDERED_ACCESS_VIEW_DESC {
                Format: DXGI_FORMAT_R8G8_UNORM,
                ViewDimension: D3D11_UAV_DIMENSION_TEXTURE2D,
                Anonymous: windows::Win32::Graphics::Direct3D11::D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_UAV { MipSlice: 0 },
                },
            };
            let uv_uav = device.CreateUnorderedAccessView(&uv_texture, Some(&uv_uav_desc))?;

            Ok(Self {
                device: device.clone(),
                context,
                shader,
                nv12_texture,
                y_texture,
                uv_texture,
                y_uav,
                uv_uav,
                width,
                height,
            })
        }
    }

    pub fn convert(&self, bgra_texture: &ID3D11Texture2D) -> anyhow::Result<&ID3D11Texture2D> {
        unsafe {
            let srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                Anonymous: windows::Win32::Graphics::Direct3D11::D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_SRV {
                        MostDetailedMip: 0,
                        MipLevels: 1,
                    },
                },
            };
            let srv = self.device.CreateShaderResourceView(bgra_texture, Some(&srv_desc))?;

            self.context.CSSetShader(&self.shader, None);
            self.context.CSSetShaderResources(0, Some(&[Some(srv)]));
            self.context.CSSetUnorderedAccessViews(
                0,
                Some(&[Some(self.y_uav.clone()), Some(self.uv_uav.clone())]),
                None,
            );

            let groups_x = (self.width + 15) / 16;
            let groups_y = (self.height + 15) / 16;
            self.context.Dispatch(groups_x, groups_y, 1);

            let empty_srvs: [Option<ID3D11ShaderResourceView>; 1] = [None];
            let empty_uavs: [Option<ID3D11UnorderedAccessView>; 2] = [None, None];
            self.context.CSSetShaderResources(0, Some(&empty_srvs));
            self.context.CSSetUnorderedAccessViews(0, Some(&empty_uavs), None);

            // Assemble NV12: copy Y into plane 0, UV into plane 1
            self.context.CopySubresourceRegion(&self.nv12_texture, 0, 0, 0, 0, &self.y_texture, 0, None);
            self.context.CopySubresourceRegion(&self.nv12_texture, 1, 0, 0, 0, &self.uv_texture, 0, None);

            Ok(&self.nv12_texture)
        }
    }
}
