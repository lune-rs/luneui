//! The window shell: the wiring a real app needs around the stage. The window (or the page's
//! canvas), the GPU, the life cycle, the clipboard and the text bridge, and the event loop. App
//! code: it reads the clock (time since start is the frame's time) and runs the event loop.
//!
//! In a browser (the `web` module) the window is a `<canvas>`, the GPU opens asynchronously and
//! every line the demo prints goes to the console.

use std::cell::Cell;
#[cfg(target_family = "wasm")]
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use web_time::Instant;

use winit::application::ApplicationHandler;
#[cfg(not(target_family = "wasm"))]
use winit::dpi::LogicalSize;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use lune_ui::clipboard::Clipboard;
use lune_ui::frame::FrameRenderer;
#[cfg(not(target_family = "wasm"))]
use lune_ui::gpu;
use lune_ui::gpu::Gpu;
use lune_ui::input::{Event, ImePlan, OutputState, Translator, control_flow, ime_area_physical};
use lune_ui::mobile;
use lune_ui::motion::MotionTier;
use lune_ui::surface::{Acquired, SurfaceOptions, WindowSurface};
use lune_ui::text::TextCache;
use lune_ui::web::TextInput;

use crate::frame_params;
use crate::scenes::SceneId;
use crate::stage::{Layout, Stage};

/// Prints one line of the demo's report (`presented`, `probe`): to stdout, or to the browser's
/// console where there is no stdout.
pub(crate) fn say(line: &str) {
    #[cfg(target_family = "wasm")]
    web_sys::console::log_1(&line.into());
    #[cfg(not(target_family = "wasm"))]
    println!("{line}");
}

/// [`say`] with a format string.
macro_rules! say {
    ($($arg:tt)*) => {
        $crate::shell::say(&format!($($arg)*))
    };
}

/// The window's title.
const TITLE: &str = "luneui web demo";

/// The window's size in logical points on a desktop.
#[cfg(not(target_family = "wasm"))]
const WINDOW: (f64, f64) = (1280.0, 800.0);
/// The least size a desktop window can be given: the narrow layout's floor.
#[cfg(not(target_family = "wasm"))]
const MIN_WINDOW: (f64, f64) = (320.0, 480.0);

/// What a run does beyond drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// How many presented frames print a `presented <n> <width>x<height>` line, then none. A
    /// `--probe` run prints one for every frame.
    pub log_frames: u32,
    /// Print `probe <key> <value>` lines for what the platform reports.
    pub probe: bool,
    /// Exit the event loop after this many presented frames (desktop only).
    pub frames: Option<u32>,
    /// The background does not move and asks for no frames.
    pub still: bool,
    /// The motion tier; `None` asks the host (a page's `prefers-reduced-motion`; Full elsewhere).
    pub motion: Option<MotionTier>,
    /// In a browser: draw through WebGL2 even where WebGPU exists (`?backend=gl`).
    pub force_gl: bool,
    /// The scene the app starts on.
    pub scene: SceneId,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            log_frames: 3,
            probe: false,
            frames: None,
            still: false,
            motion: None,
            force_gl: false,
            scene: SceneId::Glass,
        }
    }
}

/// The window flags' usage line.
pub const WINDOW_USAGE: &str = "usage: web-demo [--probe] [--frames <n>] [--still] [--motion full|reduced|off] [--scene glass|controls|motion|data|text|theme] [--backend gl|auto]";

/// The motion tier a word names.
pub fn tier_from_word(word: &str) -> Option<MotionTier> {
    match word {
        "full" => Some(MotionTier::Full),
        "reduced" => Some(MotionTier::Reduced),
        "off" => Some(MotionTier::Off),
        _ => None,
    }
}

/// The options the window flags ask for, or `None` for an argument that is not understood. The
/// command line's `--dump` is not a window flag: the binary reads it before calling this.
pub fn parse_args(args: impl Iterator<Item = String>) -> Option<Options> {
    let mut options = Options::default();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--probe" => options.probe = true,
            "--frames" => options.frames = Some(args.next()?.parse().ok()?),
            "--still" => options.still = true,
            "--motion" => options.motion = Some(tier_from_word(&args.next()?)?),
            "--scene" => options.scene = SceneId::from_slug(&args.next()?)?,
            "--backend" => match args.next()?.as_str() {
                "gl" => options.force_gl = true,
                "auto" => options.force_gl = false,
                _ => return None,
            },
            _ => return None,
        }
    }
    Some(options)
}

/// Whether this platform's event loop can return, so `Options::frames` applies.
const LOOP_RETURNS: bool = cfg!(not(any(
    target_os = "ios",
    target_os = "android",
    target_family = "wasm"
)));

/// The motion tier the host asks for: a page's `prefers-reduced-motion`; Full elsewhere.
fn host_motion() -> MotionTier {
    #[cfg(target_family = "wasm")]
    if crate::web::prefers_reduced_motion() {
        return MotionTier::Reduced;
    }
    MotionTier::Full
}

/// The GPU backend's name as the rail's footer shows it.
fn backend_label(backend: wgpu::Backend) -> &'static str {
    match backend {
        wgpu::Backend::BrowserWebGpu => "WebGPU",
        wgpu::Backend::Gl if cfg!(target_family = "wasm") => "WebGL2",
        wgpu::Backend::Gl => "OpenGL",
        wgpu::Backend::Metal => "Metal",
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Dx12 => "Direct3D 12",
        _ => "GPU",
    }
}

/// The surface options the demo asks for: an opaque window.
fn surface_options() -> SurfaceOptions {
    SurfaceOptions {
        opaque: true,
        ..SurfaceOptions::default()
    }
}

/// The window and everything that draws into it.
pub(crate) struct Shown {
    window: Arc<Window>,
    surface: Option<WindowSurface>,
    gpu: Gpu,
    frame: FrameRenderer,
    text: TextCache,
    translator: Translator,
    output: OutputState,
    /// The text bridge (a browser's hidden textarea; inert elsewhere).
    text_input: TextInput,
    /// The backend's name, for the rail.
    backend: &'static str,
}

pub(crate) struct App {
    options: Options,
    shown: Option<Shown>,
    stage: Stage,
    start: Instant,
    presented: u32,
    clipboard: Clipboard,
    /// Set by the text bridge when it has something to give; `about_to_wait` turns it into a frame.
    wake: Rc<Cell<bool>>,
    paused: bool,
    #[cfg(target_family = "wasm")]
    setting_up: bool,
    #[cfg(target_family = "wasm")]
    pending: crate::web::Slot,
    want_frame: bool,
    /// The layout word the last `probe layout` line said.
    last_layout: Option<bool>,
    error: Option<String>,
}

/// Opens the window.
fn open_window(event_loop: &ActiveEventLoop) -> Result<Arc<Window>, String> {
    let attrs = Window::default_attributes().with_title(TITLE);
    #[cfg(not(target_family = "wasm"))]
    let attrs = attrs
        .with_inner_size(LogicalSize::new(WINDOW.0, WINDOW.1))
        .with_min_inner_size(LogicalSize::new(MIN_WINDOW.0, MIN_WINDOW.1));
    #[cfg(target_family = "wasm")]
    let attrs = crate::web::with_page_canvas(attrs)?;
    Ok(Arc::new(
        event_loop
            .create_window(attrs)
            .map_err(|e| format!("window: {e}"))?,
    ))
}

/// Opens the surface and the GPU over `window` on `instance`, and everything that draws into it.
/// In a browser the future is driven by the page's task queue ([`crate::web::spawn_setup`]).
pub(crate) async fn open_gpu(
    instance: wgpu::Instance,
    window: Arc<Window>,
    options: Options,
    wake: Rc<Cell<bool>>,
) -> Result<Shown, String> {
    let surface = instance
        .create_surface(window.clone())
        .map_err(|e| format!("surface: {e}"))?;
    let mut gpu = Gpu::new(instance, Some(&surface)).await?;
    let size = window.inner_size();
    let surface =
        WindowSurface::with_options(&gpu, surface, (size.width, size.height), surface_options())?;
    let frame = FrameRenderer::for_surface(&mut gpu, &surface);
    let mut translator = Translator::new(size, window.scale_factor());
    // A phone's finger is a touch, not a mouse (D16); a desktop never produces one.
    translator.set_touch_enabled(true);
    let text_input = {
        let waker = window.clone();
        TextInput::attach(&window, move || {
            wake.set(true);
            waker.request_redraw();
        })
    };
    let info = gpu.adapter.get_info();
    let backend = backend_label(info.backend);
    if options.probe || cfg!(target_family = "wasm") {
        // The browser checks read which backend the page got: WebGPU or the WebGL2 fallback.
        say!("probe adapter_backend {:?}", info.backend);
        let mac = lune_ui::web::keymap() == lune_ui::input::Keymap::MAC;
        say!("probe keymap {}", if mac { "Mac" } else { "PC" });
    }
    Ok(Shown {
        window,
        surface: Some(surface),
        gpu,
        frame,
        text: crate::text_cache(),
        translator,
        output: OutputState::new(),
        text_input,
        backend,
    })
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, why: String) {
        #[cfg(target_family = "wasm")]
        web_sys::console::error_1(&format!("web-demo: {why}").into());
        self.error = Some(why);
        event_loop.exit();
    }

    /// In a browser: takes the GPU setup's result once it is there and asks for the first frame.
    #[cfg(target_family = "wasm")]
    fn take_setup(&mut self, event_loop: &ActiveEventLoop) {
        let done = self.pending.borrow_mut().take();
        match done {
            Some(Ok(shown)) => {
                self.stage.backend = shown.backend.to_owned();
                shown.window.request_redraw();
                self.shown = Some(shown);
            }
            Some(Err(why)) => self.fail(event_loop, why),
            None => {}
        }
    }

    /// Draws one frame and presents it.
    fn render(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.paused {
            return Ok(());
        }
        let Some(shown) = &mut self.shown else {
            return Ok(());
        };
        let Shown {
            window,
            surface,
            gpu,
            frame,
            text,
            translator,
            output,
            text_input,
            ..
        } = shown;
        let Some(surface) = surface.as_mut() else {
            return Ok(());
        };
        if cfg!(target_family = "wasm") {
            // A canvas has no size until the page's resize observer reports one: a frame waits for
            // a real size and draws at the window's own.
            let inner = window.inner_size();
            if inner.width == 0 || inner.height == 0 {
                return Ok(());
            }
            if surface.size() != (inner.width, inner.height) {
                surface.resize(gpu, inner.width, inner.height);
            }
        }
        let target = match surface.acquire(gpu)? {
            Acquired::Frame(t) => t,
            Acquired::Skip => return Ok(()),
            Acquired::Redraw => {
                self.want_frame = true;
                return Ok(());
            }
        };
        let view = target.texture.create_view(&Default::default());
        let size = surface.size();
        let scale = window.scale_factor();
        translator.scale_factor(scale);
        let s = scale as f32;
        // The text bridge's focus and composition are the window's, as winit's own would be.
        if let Some(focus) = text_input.take_focus() {
            translator.focused(focus);
        }
        for ime in text_input.take() {
            translator.ime(&ime);
        }
        let mut input = translator.take(self.start.elapsed());
        input
            .events
            .push(Event::SafeArea(mobile::safe_area(window)));
        let rendered = self.stage.frame(input, text);
        let (display, out, layout) = (rendered.scene, rendered.out, rendered.layout);
        // What the output will set on the window, for the text bridge first (a browser's winit
        // window has no input method of its own).
        match output.plan_output(&out).ime {
            ImePlan::Area { enable, area } => {
                if enable {
                    text_input.set_allowed(true);
                }
                if let Some(physical) = ime_area_physical(area, s) {
                    text_input.set_area(physical);
                }
            }
            ImePlan::Disable => text_input.set_allowed(false),
            ImePlan::Keep => {}
        }
        if output.apply(window, &out, s) {
            event_loop.exit();
        }
        self.clipboard.service(&out, translator);
        if !out.clipboard.is_empty() {
            // A read's reply arrives in the next frame's input.
            self.want_frame = true;
        }
        let params = frame_params(size, s);
        frame.draw_with(gpu, text, &display, &view, params, &mut [])?;
        window.pre_present_notify();
        surface.present(gpu, target);
        gpu.end_frame(text);
        self.presented += 1;
        if self.options.probe || self.presented <= self.options.log_frames {
            say!("presented {} {}x{}", self.presented, size.0, size.1);
        }
        self.report_layout(&layout);
        if self.presented < self.options.log_frames {
            self.want_frame = true;
        }
        if LOOP_RETURNS && let Some(n) = self.options.frames {
            if self.presented >= n {
                event_loop.exit();
                return Ok(());
            }
            self.want_frame = true;
        }
        let (flow, redraw) = control_flow(out.repaint, self.start);
        event_loop.set_control_flow(flow);
        if redraw {
            self.want_frame = true;
        }
        Ok(())
    }

    /// Prints `probe layout wide|narrow <w>x<h>` when the frame's layout kind changes (the browser
    /// checks read it).
    fn report_layout(&mut self, layout: &Layout) {
        if self.last_layout != Some(layout.wide) {
            self.last_layout = Some(layout.wide);
            say!(
                "probe layout {} {}x{}",
                if layout.wide { "wide" } else { "narrow" },
                layout.size.0.round(),
                layout.size.1.round()
            );
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.paused = false;
        if let Some(shown) = &self.shown {
            shown.window.request_redraw();
            return;
        }
        // In a browser the GPU opens on the page's task queue and `take_setup` picks it up.
        #[cfg(target_family = "wasm")]
        if !self.setting_up {
            self.setting_up = true;
            match open_window(event_loop) {
                Ok(window) => crate::web::spawn_setup(
                    window,
                    self.options,
                    self.wake.clone(),
                    self.pending.clone(),
                ),
                Err(e) => self.fail(event_loop, e),
            }
        }
        #[cfg(not(target_family = "wasm"))]
        match open_window(event_loop).and_then(|window| {
            let instance = gpu::instance(Some(Box::new(event_loop.owned_display_handle())));
            pollster::block_on(open_gpu(instance, window, self.options, self.wake.clone()))
        }) {
            Ok(shown) => {
                self.stage.backend = shown.backend.to_owned();
                shown.window.request_redraw();
                self.shown = Some(shown);
            }
            Err(e) => self.fail(event_loop, e),
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.paused = true;
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        // A frame scheduled with `Repaint::At` is due.
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.want_frame = true;
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_family = "wasm")]
        self.take_setup(event_loop);
        #[cfg(not(target_family = "wasm"))]
        let _ = event_loop;
        if self.wake.take() {
            self.want_frame = true;
        }
        if self.want_frame
            && !self.paused
            && let Some(shown) = &self.shown
        {
            self.want_frame = false;
            shown.window.request_redraw();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        // A clipboard reply that settled after the frame asked for one.
        self.want_frame = true;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if event_loop.exiting() {
            return;
        }
        #[cfg(target_family = "wasm")]
        self.take_setup(event_loop);
        // An input event needs a frame to be read; a redraw request is not one.
        if !matches!(event, WindowEvent::RedrawRequested) {
            self.want_frame = true;
        }
        if let Some(shown) = &mut self.shown {
            // A focus event reaches the translator through the text bridge, which tells the focus
            // moving between the canvas and its textarea from a loss; elsewhere it passes as is.
            if shown.text_input.passes(&event) {
                shown.translator.window_event(&event);
            }
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(shown) = &mut self.shown
                    && size.width > 0
                    && size.height > 0
                {
                    if let Some(surface) = &mut shown.surface {
                        surface.resize(&shown.gpu, size.width, size.height);
                    }
                    shown.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.render(event_loop) {
                    self.fail(event_loop, e);
                }
            }
            _ => {}
        }
    }
}

/// Runs the demo on `event_loop` until its window closes. In a browser the app goes to the page and
/// this returns at once.
pub fn run(event_loop: EventLoop<()>, options: Options) -> Result<(), String> {
    let mut stage = Stage::new();
    stage.still = options.still;
    stage.tier = options.motion.unwrap_or_else(host_motion);
    stage.want = options.scene;
    // A browser reports the machine it runs on (a Mac's chords are Cmd-based); elsewhere this is
    // the adapter's own keymap.
    stage.set_keymap(lune_ui::web::keymap());
    let wake = event_loop.create_proxy();
    let app = App {
        options,
        shown: None,
        stage,
        start: Instant::now(),
        presented: 0,
        // A reply that settles after the frame (the browser's clipboard) wakes the loop, which runs
        // the frame that takes it; a synchronous backend never calls it.
        clipboard: Clipboard::new().with_wake(move || {
            let _ = wake.send_event(());
        }),
        wake: Rc::new(Cell::new(false)),
        paused: false,
        #[cfg(target_family = "wasm")]
        setting_up: false,
        #[cfg(target_family = "wasm")]
        pending: Rc::new(RefCell::new(None)),
        want_frame: false,
        last_layout: None,
        error: None,
    };
    // A browser owns its loop: the app goes to the page and `run` returns at once, so a failure
    // reaches the console through `App::fail`.
    #[cfg(target_family = "wasm")]
    {
        crate::web::spawn(event_loop, app);
        Ok(())
    }
    #[cfg(not(target_family = "wasm"))]
    run_native(event_loop, app)
}

/// Runs `app` on `event_loop` until it exits, and reports its error.
#[cfg(not(target_family = "wasm"))]
fn run_native(event_loop: EventLoop<()>, mut app: App) -> Result<(), String> {
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop: {e}"))?;
    match app.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
