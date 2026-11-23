//! The browser entry (`wasm32-unknown-unknown`): the page's `<canvas id="luneui">` is the window,
//! the flags come from the page's query string, the panic message goes to the console and the GPU
//! opens on the page's task queue. Everything else is the shell in [`crate::shell`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use wasm_bindgen::JsCast;
use winit::event_loop::EventLoop;
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
use winit::window::{Window, WindowAttributes};

use crate::shell::{App, Options, Shown, open_gpu, parse_args, run};

/// The `id` of the page's canvas.
const CANVAS_ID: &str = "luneui";

/// What the page's query string asks for, as command-line arguments: `?still&scene=data` is
/// `--still --scene data`.
fn query_args(search: &str) -> Vec<String> {
    let mut args = Vec::new();
    for pair in search.trim_start_matches('?').split('&') {
        if pair.is_empty() {
            continue;
        }
        match pair.split_once('=') {
            Some((key, value)) => {
                args.push(format!("--{key}"));
                args.push(value.to_owned());
            }
            None => args.push(format!("--{pair}")),
        }
    }
    args
}

/// The entry point the page's module script reaches through the generated glue.
pub fn main() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("panicked: {info}").into());
    }));
    let search = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    let Some(options) = parse_args(query_args(&search).into_iter()) else {
        web_sys::console::error_1(&format!("web-demo: bad query `{search}`").into());
        return;
    };
    let event_loop = match EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            web_sys::console::error_1(&format!("web-demo: event loop: {e}").into());
            return;
        }
    };
    if let Err(e) = run(event_loop, options) {
        web_sys::console::error_1(&format!("web-demo: {e}").into());
    }
}

/// Whether the page asks for reduced motion (`prefers-reduced-motion: reduce`).
pub fn prefers_reduced_motion() -> bool {
    web_sys::window()
        .and_then(|w| {
            w.match_media("(prefers-reduced-motion: reduce)")
                .ok()
                .flatten()
        })
        .is_some_and(|q| q.matches())
}

/// `attrs` over the page's canvas: the window draws into it and the page's CSS sizes it.
pub(crate) fn with_page_canvas(attrs: WindowAttributes) -> Result<WindowAttributes, String> {
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(CANVAS_ID))
        .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .ok_or_else(|| format!("the page has no <canvas id=\"{CANVAS_ID}\">"))?;
    Ok(attrs.with_canvas(Some(canvas)).with_append(false))
}

/// Where the GPU setup leaves its result for the event loop to pick up.
pub(crate) type Slot = Rc<RefCell<Option<Result<Shown, String>>>>;

/// Opens the instance (WebGPU where the browser has an adapter, WebGL2 otherwise, or WebGL2 only
/// when `options.force_gl`), the surface and the device on the page's task queue, leaves the result
/// in `slot` and asks `window` for its first frame.
pub(crate) fn spawn_setup(window: Arc<Window>, options: Options, wake: Rc<Cell<bool>>, slot: Slot) {
    wasm_bindgen_futures::spawn_local(async move {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = if options.force_gl {
            wgpu::Backends::GL
        } else {
            wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
        };
        let instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
        let shown = open_gpu(instance, window.clone(), options, wake).await;
        *slot.borrow_mut() = Some(shown);
        window.request_redraw();
    });
}

/// Hands `app` to the page: the browser owns the loop, so this returns at once.
pub(crate) fn spawn(event_loop: EventLoop<()>, app: App) {
    event_loop.spawn_app(app);
}
