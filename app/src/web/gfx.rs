//! wgpu bootstrap on an HTML canvas: WebGPU first, WebGL2 fallback.
//!
//! The two backends live in separate `wgpu::Instance`s because an instance
//! created while `navigator.gpu` exists is WebGPU-only. WebGPU availability is
//! probed at the JS level first because wgpu 30 mistakes a `null`
//! `requestAdapter()` result for a real adapter (see heddobureika).

use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlCanvasElement;

/// Which backend to try.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum BackendPreference {
    /// WebGPU when available, else WebGL2.
    #[default]
    Auto,
    /// Force WebGL2 (debugging the fallback, or working around driver bugs).
    WebGl2,
}

pub(crate) struct Gfx {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
}

async fn js_webgpu_adapter_available(power: wgpu::PowerPreference) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(gpu) = js_sys::Reflect::get(&window.navigator(), &JsValue::from_str("gpu")) else {
        return false;
    };
    if gpu.is_undefined() || gpu.is_null() {
        return false;
    }
    let options = js_sys::Object::new();
    let pref = match power {
        wgpu::PowerPreference::None => None,
        wgpu::PowerPreference::LowPower => Some("low-power"),
        wgpu::PowerPreference::HighPerformance => Some("high-performance"),
    };
    if let Some(pref) = pref {
        let _ = js_sys::Reflect::set(
            &options,
            &JsValue::from_str("powerPreference"),
            &JsValue::from_str(pref),
        );
    }
    let Ok(request) = js_sys::Reflect::get(&gpu, &JsValue::from_str("requestAdapter")) else {
        return false;
    };
    let Ok(request) = request.dyn_into::<js_sys::Function>() else {
        return false;
    };
    let Ok(promise) = request.call1(&gpu, &options) else {
        return false;
    };
    let Ok(promise) = promise.dyn_into::<js_sys::Promise>() else {
        return false;
    };
    match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(adapter) => !adapter.is_null() && !adapter.is_undefined(),
        Err(_) => false,
    }
}

async fn request_adapter(
    instance: &wgpu::Instance,
    surface: &wgpu::Surface<'_>,
    probe_js: bool,
) -> Result<wgpu::Adapter, String> {
    let attempts = [
        (wgpu::PowerPreference::HighPerformance, false),
        (wgpu::PowerPreference::LowPower, false),
        (wgpu::PowerPreference::None, false),
        (wgpu::PowerPreference::None, true),
    ];
    let mut last = None;
    for (power_preference, force_fallback_adapter) in attempts {
        if probe_js && !js_webgpu_adapter_available(power_preference).await {
            last = Some(format!(
                "requestAdapter({power_preference:?}) returned null"
            ));
            continue;
        }
        match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference,
                compatible_surface: Some(surface),
                force_fallback_adapter,
                apply_limit_buckets: false,
            })
            .await
        {
            Ok(adapter) => return Ok(adapter),
            Err(err) => last = Some(format!("{err:?}")),
        }
    }
    Err(last.unwrap_or_else(|| "no adapter".into()))
}

/// The GLES web backend ignores the display handle, but the safe canvas path
/// rejects a missing one; supply a dummy Web handle.
fn gl_surface(
    instance: &wgpu::Instance,
    canvas: &HtmlCanvasElement,
) -> Result<wgpu::Surface<'static>, wgpu::CreateSurfaceError> {
    unsafe {
        let value: &JsValue = canvas.as_ref();
        let obj = core::ptr::NonNull::from(value).cast();
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(wgpu::rwh::RawDisplayHandle::Web(
                wgpu::rwh::WebDisplayHandle::new(),
            )),
            raw_window_handle: wgpu::rwh::WebCanvasWindowHandle::new(obj).into(),
        })
    }
}

/// Swap a tainted canvas for a fresh one with the same class and size.
fn replace_canvas(old: &HtmlCanvasElement) -> Result<HtmlCanvasElement, String> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let fresh = document
        .create_element("canvas")
        .map_err(|e| format!("createElement: {e:?}"))?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| "not a canvas".to_string())?;
    fresh.set_class_name(&old.class_name());
    fresh.set_width(old.width());
    fresh.set_height(old.height());
    old.replace_with_with_node_1(&fresh)
        .map_err(|e| format!("replaceWith: {e:?}"))?;
    Ok(fresh)
}

fn has_webgpu() -> bool {
    web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w.navigator(), &JsValue::from_str("gpu")).ok())
        .is_some_and(|gpu| !gpu.is_undefined() && !gpu.is_null())
}

impl Gfx {
    /// Creates the context on `canvas`. May replace the canvas element in the
    /// DOM (when a failed WebGPU attempt has tainted it) and returns the
    /// element actually in use.
    pub(crate) async fn new(
        canvas: HtmlCanvasElement,
        preference: BackendPreference,
    ) -> Result<(Gfx, HtmlCanvasElement), String> {
        let mut canvas = canvas;
        let mut acquired: Option<(wgpu::Surface<'static>, wgpu::Adapter)> = None;
        // Probe at the JS level before touching the canvas: `getContext("webgpu")`
        // marks the canvas as in use, after which `getContext("webgl2")` fails.
        if preference == BackendPreference::Auto
            && has_webgpu()
            && js_webgpu_adapter_available(wgpu::PowerPreference::None).await
        {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::BROWSER_WEBGPU,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            });
            match instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone())) {
                Ok(surface) => match request_adapter(&instance, &surface, true).await {
                    Ok(adapter) => acquired = Some((surface, adapter)),
                    Err(detail) => {
                        web_sys::console::warn_1(&JsValue::from_str(&format!(
                            "WebGPU unavailable, falling back to WebGL2: {detail}"
                        )));
                        canvas = replace_canvas(&canvas)?;
                    }
                },
                Err(err) => web_sys::console::warn_1(&JsValue::from_str(&format!(
                    "WebGPU surface failed, falling back to WebGL2: {err:?}"
                ))),
            }
        }
        let (surface, adapter) = match acquired {
            Some(pair) => pair,
            None => {
                let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                    backends: wgpu::Backends::GL,
                    ..wgpu::InstanceDescriptor::new_without_display_handle()
                });
                let surface = gl_surface(&instance, &canvas)
                    .map_err(|e| format!("create_surface (WebGL2) failed: {e:?}"))?;
                let adapter = request_adapter(&instance, &surface, false).await?;
                (surface, adapter)
            }
        };

        let is_gl = adapter.get_info().backend == wgpu::Backend::Gl;
        let limits = if is_gl {
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
        } else {
            wgpu::Limits::default().using_resolution(adapter.limits())
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("ddi-device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                ..Default::default()
            })
            .await
            .map_err(|e| format!("request_device failed: {e:?}"))?;

        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| "surface configuration unsupported".to_string())?;
        let caps = surface.get_capabilities(&adapter);
        if let Some(format) = caps.formats.iter().copied().find(|f| f.is_srgb()) {
            config.format = format;
        }
        if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
            config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);
        Ok((
            Gfx {
                surface,
                adapter,
                device,
                queue,
                config,
            },
            canvas,
        ))
    }

    pub(crate) fn backend_name(&self) -> &'static str {
        match self.adapter.get_info().backend {
            wgpu::Backend::BrowserWebGpu => "WebGPU",
            wgpu::Backend::Gl => "WebGL2",
            _ => "other",
        }
    }

    /// Reconfigures the surface when the canvas backing store changes size.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        let (w, h) = (width.max(1), height.max(1));
        if w == self.config.width && h == self.config.height {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }
}
