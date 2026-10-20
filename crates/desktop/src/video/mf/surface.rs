//! Decoder-owned D3D11 NV12 surfaces to owned I420. Retain the output sample
//! through GPU copy, Map and plane copying, then unmap before releasing it.
use super::*;
impl Session {
    pub(super) fn readback(&mut self, sample: &IMFSample) -> Result<Vec<u8>, String> {
        // SAFETY: all COM/D3D resources belong to this worker; the sample
        // keeps its surface alive until after the staging copy is complete.
        unsafe {
            w(self.device.GetDeviceRemovedReason(), "device removed")?;
            if w(sample.GetBufferCount(), "output buffer count")? != 1 {
                return Err("MF requires one DXGI buffer".into());
            }
            let buffer = w(sample.GetBufferByIndex(0), "output buffer")?;
            let dxgi: IMFDXGIBuffer = w(buffer.cast(), "output is not a DXGI surface")?;
            let mut texture: Option<ID3D11Texture2D> = None;
            w(
                dxgi.GetResource(
                    &ID3D11Texture2D::IID,
                    &mut texture as *mut Option<ID3D11Texture2D> as *mut *mut std::ffi::c_void,
                ),
                "output texture",
            )?;
            let texture = texture.ok_or("MF null output texture")?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            let sub = w(dxgi.GetSubresourceIndex(), "texture subresource")?;
            if desc.Format != DXGI_FORMAT_NV12
                || desc.MipLevels != 1
                || desc.ArraySize == 0
                || desc.ArraySize > 32
                || sub >= desc.ArraySize
                || desc.SampleDesc.Count != 1
                || desc.Width < self.width
                || desc.Height < self.height
                || desc.Width > 4096
                || desc.Height > 4096
                || !desc.Width.is_multiple_of(2)
                || !desc.Height.is_multiple_of(2)
            {
                return Err("MF unsupported texture layout".into());
            }
            if w(texture.GetDevice(), "texture device")? != self.device {
                return Err("MF output belongs to another D3D device".into());
            }
            if self
                .staging
                .as_ref()
                .is_none_or(|(width, height, _)| *width != desc.Width || *height != desc.Height)
            {
                let staging_desc = D3D11_TEXTURE2D_DESC {
                    Width: desc.Width,
                    Height: desc.Height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_NV12,
                    SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                };
                let mut staging = None;
                w(
                    self.device
                        .CreateTexture2D(&staging_desc, None, Some(&mut staging)),
                    "staging texture",
                )?;
                self.staging = Some((
                    desc.Width,
                    desc.Height,
                    staging.ok_or("MF null staging texture")?,
                ));
            }
            let staging = &self.staging.as_ref().unwrap().2;
            self.context
                .CopySubresourceRegion(staging, 0, 0, 0, 0, &texture, sub, None);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            w(
                self.context
                    .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)),
                "readback map",
            )?;
            struct Unmap<'a>(&'a ID3D11DeviceContext, &'a ID3D11Texture2D);
            impl Drop for Unmap<'_> {
                fn drop(&mut self) {
                    unsafe {
                        self.0.Unmap(self.1, 0);
                    }
                }
            }
            let _guard = Unmap(&self.context, staging);
            let len = mf_core::span(mapped.RowPitch as usize, desc.Height as usize)?;
            if mapped.pData.is_null() || mapped.RowPitch < desc.Width {
                return Err("MF invalid mapped NV12 pitch".into());
            }
            // D3D11 NV12 maps both planes at RowPitch: UV follows the full
            // allocated luma height. span bounds arithmetic and allocation.
            let bytes = std::slice::from_raw_parts(mapped.pData.cast::<u8>(), len);
            let planes = mf_core::nv12(
                bytes,
                mapped.RowPitch as usize,
                desc.Height as usize,
                self.width as usize,
                self.height as usize,
            )?;
            w(
                self.device.GetDeviceRemovedReason(),
                "device removed after readback",
            )?;
            Ok(planes)
        }
    }
}
