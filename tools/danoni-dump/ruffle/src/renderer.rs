//! Offscreen rendering and RGBA readback; no platform API escapes this module.
use crate::renderer_options::{Backend, RendererOptions};
use ruffle_render_wgpu::{
    backend::{WgpuRenderBackend, create_wgpu_instance, ddi_adapters, ddi_device},
    descriptors::Descriptors,
    target::TextureTarget,
    wgpu,
};
use serde_json::{Value, json};
use std::{any::Any, error::Error, fs, panic::AssertUnwindSafe, sync::Arc};

type Renderer = WgpuRenderBackend<TextureTarget>;
const MAX_ADAPTERS: usize = 32;
fn bounded(s: &str) -> String {
    s.chars().take(1024).collect()
}
fn backend_name(b: wgpu::Backend) -> &'static str {
    match b {
        wgpu::Backend::Metal => "metal",
        wgpu::Backend::Vulkan => "vulkan",
        wgpu::Backend::Dx12 => "dx12",
        wgpu::Backend::Gl => "gl",
        _ => "unsupported",
    }
}
fn allowed(options: &RendererOptions, info: &wgpu::AdapterInfo) -> bool {
    options.accepts_cpu(info.device_type == wgpu::DeviceType::Cpu)
        && options
            .adapter_name
            .as_ref()
            .is_none_or(|name| name == &info.name)
}
fn priority(info: &wgpu::AdapterInfo) -> u8 {
    match info.device_type {
        wgpu::DeviceType::IntegratedGpu => 0,
        wgpu::DeviceType::DiscreteGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::Cpu => 4,
    }
}
fn info_json(info: &wgpu::AdapterInfo) -> Value {
    json!({"backend":backend_name(info.backend),"name":bounded(&info.name),
        "deviceType":format!("{:?}",info.device_type),"software":info.device_type == wgpu::DeviceType::Cpu,
        "vendor":info.vendor,"device":info.device,"driver":bounded(&info.driver),"driverInfo":bounded(&info.driver_info)})
}
fn attempt<T>(f: impl FnOnce() -> Result<T, Box<dyn Error>>) -> Result<T, String> {
    match std::panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result.map_err(|e| bounded(&e.to_string())),
        Err(panic) => Err(format!(
            "renderer initialization panic: {}",
            bounded(
                panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string panic")
            )
        )),
    }
}
fn persist(report: &Value) -> Result<(), Box<dyn Error>> {
    fs::write("renderer.json", serde_json::to_vec_pretty(report)?)?;
    Ok(())
}
/// Try compiled native backends and matching adapters before any movie execution.
/// A software request only accepts adapters explicitly reported as CPU devices.
pub fn create(
    size: (u32, u32),
    options: &RendererOptions,
) -> Result<(Renderer, Value), Box<dyn Error>> {
    options.validate()?;
    let all = [
        (Backend::Metal, wgpu::Backends::METAL, "metal"),
        (Backend::Dx12, wgpu::Backends::DX12, "dx12"),
        (Backend::Vulkan, wgpu::Backends::VULKAN, "vulkan"),
        (Backend::Gl, wgpu::Backends::GL, "gl"),
    ];
    let compiled = wgpu::Instance::enabled_backend_features();
    let mut report =
        json!({"schema":"ddi-ruffle-renderer/1","requested":options,"selected":null,"attempts":[]});
    let mut count = 0;
    let mut candidates = Vec::new();
    for (kind, mask, name) in all {
        if options.backend != Backend::Auto && options.backend != kind {
            continue;
        }
        if !compiled.contains(mask) {
            if options.backend != Backend::Auto {
                report["attempts"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"backend":name,"error":"backend not compiled for this target"}));
            }
            continue;
        }
        let enumeration = attempt(|| {
            let instance = create_wgpu_instance(mask, Default::default(), None);
            let adapters = ddi_adapters(&instance, mask);
            Ok((instance, adapters))
        });
        let (instance, mut adapters) = match enumeration {
            Ok(found) => found,
            Err(error) => {
                report["attempts"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"backend":name,"error":error}));
                continue;
            }
        };
        count += adapters.len();
        if count > MAX_ADAPTERS {
            persist(&report)?;
            return Err("Renderer adapter inventory exceeds 32".into());
        }
        adapters.retain(|adapter| allowed(options, &adapter.get_info()));
        adapters.sort_by_key(|adapter| priority(&adapter.get_info()));
        if adapters.is_empty() {
            report["attempts"]
                .as_array_mut()
                .unwrap()
                .push(json!({"backend":name,"error":"no matching adapter"}));
        }
        for adapter in adapters {
            let info = adapter.get_info();
            candidates.push((instance.clone(), adapter, info));
        }
    }
    // Exhaust matching non-CPU devices across all APIs before software fallback.
    candidates
        .sort_by_key(|(_, _, info)| (info.device_type == wgpu::DeviceType::Cpu, priority(info)));
    for (instance, adapter, info) in candidates {
        let initialized = attempt(move || {
            let (device, queue) = ddi_device(&adapter)?;
            let target = TextureTarget::new(&device, size)?;
            let descriptors = Descriptors::new(instance, adapter, device, queue);
            Renderer::new(Arc::new(descriptors), target)
        });
        match initialized {
            Ok(renderer) => {
                report["selected"] = info_json(&info);
                options.validate_report(&report)?;
                persist(&report)?;
                return Ok((renderer, report));
            }
            Err(error) => report["attempts"]
                .as_array_mut()
                .unwrap()
                .push(json!({"adapter":info_json(&info),"error":error})),
        }
    }
    persist(&report)?;
    Err("No matching offscreen renderer initialized; see renderer.json".into())
}
/// Tightly packed top-to-bottom RGBA8 pixels, independent of the selected API.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
pub fn capture(renderer: &dyn Any) -> Result<Frame, Box<dyn Error>> {
    let renderer = renderer.downcast_ref::<Renderer>().ok_or("renderer type")?;
    let image = renderer.capture_frame().ok_or("no rendered frame")?;
    Ok(Frame {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer_options::Adapter;
    #[test]
    fn exact_name_and_cpu_filters_do_not_silently_relax_requests() {
        let mut info = wgpu::AdapterInfo::new(wgpu::DeviceType::Cpu, wgpu::Backend::Vulkan);
        info.name = "Example".into();
        let mut options = RendererOptions {
            adapter: Adapter::Software,
            ..Default::default()
        };
        assert!(allowed(&options, &info));
        info.device_type = wgpu::DeviceType::IntegratedGpu;
        assert!(!allowed(&options, &info));
        options.adapter = Adapter::Auto;
        options.adapter_name = Some("example".into());
        assert!(!allowed(&options, &info));
        options.adapter_name = Some("Example".into());
        assert!(allowed(&options, &info));
        options.adapter = Adapter::Hardware;
        info.device_type = wgpu::DeviceType::Cpu;
        assert!(!allowed(&options, &info));
    }
    #[test]
    fn initialization_failures_remain_diagnostics() {
        assert!(
            attempt::<()>(|| Err("device unavailable".into()))
                .unwrap_err()
                .contains("device unavailable")
        );
        assert!(
            attempt::<()>(|| panic!("shader unavailable"))
                .unwrap_err()
                .contains("shader unavailable")
        );
    }
}
