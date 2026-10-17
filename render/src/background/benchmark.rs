//! Names retained for the explicit decoder-to-GPU benchmark.
use super::*;
impl BackgroundPipeline {
    pub fn new_nv12_benchmark(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        linear: bool,
    ) -> Self {
        Self::new(device, format, linear)
    }
    pub fn set_nv12_benchmark(
        &mut self,
        device: &wgpu::Device,
        id: usize,
        y: &wgpu::Texture,
        uv: &wgpu::Texture,
        bt709: bool,
        full_range: bool,
    ) {
        self.set_video_nv12(device, id, y, uv, bt709, full_range);
    }
}
