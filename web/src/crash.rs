//! Post-panic containment. A wasm panic aborts without unwinding, so the
//! whole instance becomes unusable — the only real recovery is a reload,
//! driven by the boot shell (`__DDI_BOOT.crash`); this module reports the
//! panic and hands off to JS.

pub(crate) fn install_hook() {
    let hook: Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static> =
        Box::new(|info| {
            console_error_panic_hook::hook(info);
            crate::boot::crash(&info.to_string());
        });
    // Yew's Renderer::render() overwrites any panic hook that wasn't
    // registered through yew::set_custom_panic_hook.
    yew::set_custom_panic_hook(hook);
    #[cfg(debug_assertions)]
    install_test_hooks();
}

/// Debug-only console helper to exercise the crash path end-to-end:
/// `__DDI_TEST_PANIC()` panics immediately.
#[cfg(debug_assertions)]
fn install_test_hooks() {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};

    let Some(window) = web_sys::window() else {
        return;
    };
    let panic_now: Box<dyn Fn()> = Box::new(|| {
        panic!("injected test panic");
    });
    let panic_now = Closure::wrap(panic_now);
    let _ = js_sys::Reflect::set(
        &window,
        &JsValue::from_str("__DDI_TEST_PANIC"),
        panic_now.as_ref().unchecked_ref(),
    );
    panic_now.forget();
}
