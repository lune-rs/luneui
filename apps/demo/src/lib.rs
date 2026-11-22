//! The demo: a small phone app over the toolkit, three tabs built from its public components (a
//! navigation stack with a list, a bottom sheet, a pager, settings), in one window. It builds for
//! the phone targets and shows what the platform reports there (`--probe`).
//!
//! The screens, their layout and their state are in [`showcase`], shared by this window shell, the
//! headless `--dump` ([`dump`]) and the tests; the shell is the wiring a real app needs: the
//! window, the GPU, the life cycle, the clipboard and the accessibility bridge.
//!
//! App code: it reads the clock (time since start is the frame's time) and runs the event loop. It
//! uses the facade only.

#![deny(unsafe_code)]

pub mod dump;
pub mod showcase;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use lune_ui::clipboard::Clipboard;
use lune_ui::frame::FrameRenderer;
use lune_ui::gpu::{self, Gpu};
use lune_ui::input::{Event, OutputState, Translator, control_flow};
use lune_ui::layout::Insets;
use lune_ui::mobile::{self, KeyboardInset, SceneAttach};
use lune_ui::scene::FrameParams;
use lune_ui::surface::{Acquired, SurfaceOptions, WindowSurface};
use lune_ui::text::TextCache;

use showcase::{Demo, Extras, MarkResult, Play};

/// The accessibility bridge, behind the `access` feature. Without the feature every call is a
/// no-op, which is the null bridge: the demo runs, with nothing for a screen reader.
#[cfg(feature = "access")]
mod access {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use lune_ui::access::{AccessBridge, RootInfo};
    use lune_ui::input::{FrameOutput, Translator};
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
    use winit::window::Window;

    /// The bridge, or none when it could not be made.
    #[derive(Debug)]
    pub(crate) struct Access(Option<AccessBridge>);

    impl Access {
        /// Makes the bridge for `window`, which must still be hidden, and says how it went for
        /// `probe access`: `ok`, `err <why>` or `panic`. A failure leaves the null bridge.
        pub(crate) fn new(
            event_loop: &ActiveEventLoop,
            window: &Window,
            proxy: EventLoopProxy<()>,
        ) -> (Self, String) {
            // A wake only posts to the event loop, which then draws a frame.
            let made = catch_unwind(AssertUnwindSafe(|| {
                AccessBridge::new(event_loop, window, move || {
                    let _ = proxy.send_event(());
                })
            }));
            match made {
                Ok(Ok(bridge)) => (Self(Some(bridge)), "ok".to_owned()),
                Ok(Err(e)) => (Self(None), format!("err {e}")),
                Err(_) => (Self(None), "panic".to_owned()),
            }
        }

        pub(crate) fn window_event(&mut self, window: &Window, event: &WindowEvent) {
            if let Some(bridge) = &mut self.0 {
                bridge.window_event(window, event);
            }
        }

        pub(crate) fn drain(&self, translator: &mut Translator) {
            if let Some(bridge) = &self.0 {
                bridge.drain(translator);
            }
        }

        pub(crate) fn update(&mut self, out: &FrameOutput, title: &str, scale: f64) {
            if let Some(bridge) = &mut self.0 {
                let root = RootInfo {
                    title: title.to_owned(),
                    scale,
                };
                bridge.update(out, &root);
            }
        }
    }
}

#[cfg(not(feature = "access"))]
mod access {
    use lune_ui::input::{FrameOutput, Translator};
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
    use winit::window::Window;

    /// The null bridge.
    #[derive(Debug)]
    pub(crate) struct Access;

    impl Access {
        pub(crate) fn new(
            _event_loop: &ActiveEventLoop,
            _window: &Window,
            _proxy: EventLoopProxy<()>,
        ) -> (Self, String) {
            (Self, "off".to_owned())
        }

        pub(crate) fn window_event(&mut self, _window: &Window, _event: &WindowEvent) {}

        pub(crate) fn drain(&self, _translator: &mut Translator) {}

        pub(crate) fn update(&mut self, _out: &FrameOutput, _title: &str, _scale: f64) {}
    }
}

/// The window's title, which is also the accessibility tree's root label.
const TITLE: &str = "luneui demo";

const UI_FONT: &str = "RznSans";
const MONO_FONT: &str = "RobotoMono Nerd Font Mono";

/// The window's size in logical points on a desktop: an iPhone 16 Pro's screen. Ignored on iOS.
const PHONE: (f64, f64) = (402.0, 874.0);

/// The text the clipboard probe writes: unique to the process, so a stale pasteboard cannot pass
/// for this run's write.
fn clipboard_probe_text() -> String {
    format!("luneui-probe-{}", std::process::id())
}

/// The backdrop blur's sigma (physical pixels) and saturation, the gallery's values.
const BLUR_SIGMA: f32 = 8.0;
const BLUR_SATURATION: f32 = 1.8;

/// What a run does beyond drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// How many presented frames print a `presented <n> <width>x<height>` line, then none. A
    /// `--probe` run prints one for every frame.
    pub log_frames: u32,
    /// Print `probe <key> <value>` lines for what the platform reports: at window creation and on
    /// every resize, scale change and life-cycle callback (never per frame).
    pub probe: bool,
    /// Exit the event loop after this many presented frames. Desktop only: on iOS and Android the
    /// loop cannot return, so it is ignored there.
    pub frames: Option<u32>,
    /// Hand the window to the app's window scene after creating it (`mobile::attach_to_scene`,
    /// which does nothing off iOS). Off only for the control run that shows what an unattached
    /// window looks like.
    pub scene_attach: bool,
    /// Draw nothing between `suspended` and `resumed`. Off only for the control run that shows the
    /// smoke check can see a frame drawn while suspended.
    pub pause: bool,
    /// Focus the text field on the first frame, which asks the OS for its soft keyboard (a phone
    /// run's way to see the keyboard inset without a finger).
    pub focus_field: bool,
    /// On frame 2 write `luneui-probe-<pid>` to the clipboard, on frame 4 read it back, and print
    /// `probe clipboard_write <text>` and `probe clipboard <text>` (or `unavailable`).
    pub probe_clipboard: bool,
    /// Play the showcase tour hands-free through the frame's input events (`playback`), printing
    /// `demo mark <name> ok` or `demo mark <name> FAIL <reason>` at each mark and
    /// `demo end <ok>/<total>` at the end; live input still works while it plays.
    pub demo: bool,
    /// With `demo`: play the tour again when it ends, from a fresh app state (`demo loop <n>`).
    pub repeat: bool,
    /// With `demo`: draw a disc under each scripted contact, for a recording.
    pub touches: bool,
    /// With `demo`: exit the event loop after `demo end`, with an error when a mark failed.
    /// Desktop only: on iOS and Android the loop cannot return, so it is ignored there.
    pub exit_after_demo: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            log_frames: 3,
            probe: false,
            frames: None,
            scene_attach: true,
            pause: true,
            focus_field: false,
            probe_clipboard: false,
            demo: false,
            repeat: false,
            touches: false,
            exit_after_demo: false,
        }
    }
}

/// Whether this platform's event loop can return, so `Options::frames` applies.
const LOOP_RETURNS: bool = cfg!(not(any(target_os = "ios", target_os = "android")));

/// The presented frame at which a `--probe` run prints the scene state a second time; the demo
/// keeps requesting frames until it.
const PROBE_LAST_FRAME: u32 = 30;

/// The frame, counting from 1, in which `--probe-clipboard` requests its read; the demo keeps
/// requesting frames until it, so the flag works without `--probe` on an idle phone.
const CLIPBOARD_PROBE_FRAMES: u32 = 4;

/// How long after its last probe frame a `--probe` run asks for one more (`Repaint::At`), to show
/// a scheduled frame arrives while the app is idle.
const PROBE_TIMED_FRAME: Duration = Duration::from_millis(1000);

/// The `probe scene` word for an attach state.
fn scene_word(state: SceneAttach) -> &'static str {
    match state {
        SceneAttach::Attached => "attached",
        SceneAttach::Waiting => "waiting",
        SceneAttach::NotNeeded => "not-needed",
        _ => "unknown",
    }
}

/// Prints `probe scene <state>`: with `attach`, the result of attaching `window` to its scene;
/// without it, the attach state queried the same way (the call is idempotent), or `skipped`.
fn probe_scene(window: &Window, attach: bool) {
    let word = if attach {
        scene_word(mobile::attach_to_scene(window))
    } else {
        "skipped"
    };
    probe_line("scene", word);
}

/// Prints one `probe <key> <value>` line.
fn probe_line(key: &str, value: impl std::fmt::Display) {
    println!("probe {key} {value}");
}

/// `Ok(position)` or the error, as one token run.
fn position_text<E: std::fmt::Display>(p: Result<winit::dpi::PhysicalPosition<i32>, E>) -> String {
    match p {
        Ok(p) => format!("{},{}", p.x, p.y),
        Err(e) => format!("error({e})"),
    }
}

/// What the window reports now, one `probe` line per fact, under a `probe event <why>` marker.
fn probe_window(why: &str, window: &Window, surface: &WindowSurface) {
    probe_line("event", why);
    let inner = window.inner_size();
    let outer = window.outer_size();
    let scale = window.scale_factor();
    probe_line("inner_size", format!("{}x{}", inner.width, inner.height));
    probe_line("outer_size", format!("{}x{}", outer.width, outer.height));
    probe_line("inner_position", position_text(window.inner_position()));
    probe_line("outer_position", position_text(window.outer_position()));
    probe_line("scale_factor", scale);
    let safe = mobile::safe_area(window);
    probe_line(
        "safe_area_logical",
        format!(
            "left={:.2} top={:.2} right={:.2} bottom={:.2}",
            safe.left, safe.top, safe.right, safe.bottom
        ),
    );
    let rate = window
        .current_monitor()
        .and_then(|m| m.refresh_rate_millihertz());
    probe_line(
        "refresh_mhz",
        rate.map_or_else(|| "none".to_owned(), |r| r.to_string()),
    );
    let (sw, sh) = surface.size();
    probe_line("surface_size", format!("{sw}x{sh}"));
    probe_line("surface_format", format!("{:?}", surface.format()));
}

/// The demo's fonts: the gallery's own files, loaded in a fixed order.
pub fn text_cache() -> TextCache {
    TextCache::new(
        &[
            include_bytes!("../../gallery/assets/RobotoMonoNerdFontMono-Regular.ttf").as_slice(),
            include_bytes!("../../gallery/assets/RznSans-Regular.ttf"),
            include_bytes!("../../gallery/assets/RznSans-Medium.ttf"),
        ],
        UI_FONT,
        MONO_FONT,
    )
}

/// The frame parameters for a target of `size` physical pixels at `scale`: the backdrop blur's
/// values are the gallery's.
pub(crate) fn frame_params(size: (u32, u32), scale: f32) -> FrameParams {
    FrameParams {
        size,
        scale,
        blur_sigma: BLUR_SIGMA,
        saturation: BLUR_SATURATION,
        ..FrameParams::default()
    }
}

/// The window and everything that draws into it.
struct Shown {
    window: Arc<Window>,
    surface: WindowSurface,
    gpu: Gpu,
    frame: FrameRenderer,
    text: TextCache,
    translator: Translator,
    output: OutputState,
    keyboard: KeyboardInset,
    access: access::Access,
}

/// What the tour has reported so far.
#[derive(Debug, Default)]
struct Tour {
    /// Marks that passed, and marks passed in all.
    ok: u32,
    total: u32,
    /// Restarts of a repeating tour.
    loops: u32,
    /// `demo end` has been printed.
    ended: bool,
}

impl Tour {
    /// Prints the marks of a frame and the restart or end it reports; true on the frame the tour
    /// ends.
    fn report(&mut self, marks: &[MarkResult], restarted: bool, finished: bool) -> bool {
        if restarted {
            self.loops += 1;
            println!("demo loop {}", self.loops);
        }
        for mark in marks {
            self.total += 1;
            match &mark.result {
                Ok(()) => {
                    self.ok += 1;
                    println!("demo mark {} ok", mark.name);
                }
                Err(why) => println!("demo mark {} FAIL {why}", mark.name),
            }
        }
        if finished && !self.ended {
            self.ended = true;
            println!("demo end {}/{}", self.ok, self.total);
            return true;
        }
        false
    }
}

struct App {
    options: Options,
    tour: Tour,
    shown: Option<Shown>,
    /// The screens and what one frame needs besides the GPU.
    demo: Demo,
    start: Instant,
    presented: u32,
    /// Wakes the loop for the accessibility bridge.
    proxy: EventLoopProxy<()>,
    /// The clipboard: the phone's pasteboard on iOS, the OS clipboard elsewhere.
    clipboard: Clipboard,
    /// The keyboard inset the last frame saw, to print it when it changes.
    last_keyboard: Insets,
    /// Set by the keyboard observer when the inset moves; `about_to_wait` turns it into a frame.
    keyboard_changed: Rc<Cell<bool>>,
    /// Between `suspended` and `resumed`: nothing is acquired, drawn or presented.
    paused: bool,
    /// The last frame, or an event since, asks for another: `about_to_wait` requests it. A request
    /// made inside `RedrawRequested` is lost while iOS is idle (docs/mobile.md, M1 reading).
    want_frame: bool,
    error: Option<String>,
}

/// Opens the window, the GPU and the surface.
fn setup(
    event_loop: &ActiveEventLoop,
    options: Options,
    keyboard_changed: Rc<Cell<bool>>,
    proxy: EventLoopProxy<()>,
) -> Result<Shown, String> {
    let probe = options.probe;
    let attrs = Window::default_attributes()
        .with_title(TITLE)
        .with_inner_size(LogicalSize::new(PHONE.0, PHONE.1))
        // The accessibility bridge wants a window that was never shown: with the feature the
        // window starts hidden and is shown after the scene attach.
        .with_visible(!cfg!(feature = "access"));
    let window = Arc::new(
        event_loop
            .create_window(attrs)
            .map_err(|e| format!("window: {e}"))?,
    );
    // The bridge goes before the attach (it wants the window hidden); a failure is recorded and
    // the demo runs without one.
    let (access, access_state) = access::Access::new(event_loop, &window, proxy);
    probe_line("access", access_state);
    // Under the iOS scene life cycle the window shows only once it belongs to a window scene.
    probe_scene(&window, options.scene_attach);
    if cfg!(feature = "access") {
        // An attach shows only a window that is not hidden, so the app shows this one itself.
        window.set_visible(true);
    }
    let instance = gpu::instance(Some(Box::new(event_loop.owned_display_handle())));
    let surface = instance
        .create_surface(window.clone())
        .map_err(|e| format!("surface: {e}"))?;
    let mut gpu = pollster::block_on(Gpu::new(instance, Some(&surface)))?;
    let size = window.inner_size();
    let options = SurfaceOptions {
        opaque: true,
        ..SurfaceOptions::default()
    };
    let surface = WindowSurface::with_options(&gpu, surface, (size.width, size.height), options)?;
    let frame = FrameRenderer::for_surface(&mut gpu, &surface);
    let mut translator = Translator::new(size, window.scale_factor());
    // The demo is a phone app: a finger is a touch, not a mouse (D16).
    translator.set_touch_enabled(true);
    // The observer runs inside the OS's notification, which can fall inside a frame: it only wakes
    // the app. A request made inside `RedrawRequested` is lost while iOS is idle, so the flag
    // `about_to_wait` reads is the one that counts.
    let keyboard = {
        let waker = window.clone();
        KeyboardInset::new(&window, move || {
            keyboard_changed.set(true);
            waker.request_redraw();
        })
    };
    if probe {
        let info = gpu.adapter.get_info();
        probe_window("created", &window, &surface);
        probe_line("adapter_name", &info.name);
        probe_line("adapter_backend", format!("{:?}", info.backend));
        probe_line("adapter_type", format!("{:?}", info.device_type));
        probe_line("adapter_driver", &info.driver);
        probe_line("alpha_mode", format!("{:?}", surface.alpha_mode()));
    }
    Ok(Shown {
        window,
        surface,
        gpu,
        frame,
        text: text_cache(),
        translator,
        output: OutputState::new(),
        keyboard,
        access,
    })
}

impl App {
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
            keyboard,
            access,
        } = shown;
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
        access.drain(translator);
        let mut input = translator.take(self.start.elapsed());
        if self.options.probe_clipboard {
            for event in &input.events {
                match event {
                    Event::ClipboardText(text) => probe_line("clipboard", text),
                    Event::ClipboardUnavailable => probe_line("clipboard", "unavailable"),
                    _ => {}
                }
            }
        }
        // The insets are the app's to push every frame (the core keeps the last value).
        input
            .events
            .push(Event::SafeArea(mobile::safe_area(window)));
        let kb = keyboard.inset();
        input.events.push(Event::KeyboardInset(kb));
        if kb != self.last_keyboard {
            self.last_keyboard = kb;
            probe_line(
                "keyboard_inset",
                format!("{},{},{},{}", kb.left, kb.top, kb.right, kb.bottom),
            );
        }
        let mut extras = Extras::default();
        if self.options.probe_clipboard {
            // The clipboard probe goes through the frame's own request path, the one a text
            // edit's copy and paste use.
            if self.presented == 1 {
                let text = clipboard_probe_text();
                probe_line("clipboard_write", &text);
                extras.clipboard_write = Some(text);
            }
            extras.clipboard_read = self.presented == CLIPBOARD_PROBE_FRAMES - 1;
        }
        if self.options.probe && self.presented + 1 == PROBE_LAST_FRAME {
            extras.repaint_at = Some(input.time + PROBE_TIMED_FRAME);
        }
        if self.options.focus_field && self.presented == 0 {
            self.demo.showcase.want_focus = true;
        }
        let rendered = self.demo.frame(input, text, &extras);
        let (display, out, safe) = (rendered.scene, rendered.out, rendered.safe);
        let tour_ended = self
            .tour
            .report(&rendered.marks, rendered.restarted, rendered.finished);
        if output.apply(window, &out, s) {
            event_loop.exit();
        }
        self.clipboard.service(&out, translator);
        access.update(&out, TITLE, scale);
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
        let probe = self.options.probe;
        if probe || self.presented <= self.options.log_frames {
            println!("presented {} {}x{}", self.presented, size.0, size.1);
        }
        if self.presented == 1 || (probe && self.presented == PROBE_LAST_FRAME) {
            probe_scene(window, self.options.scene_attach);
            if probe {
                // What `ui.safe_area()` returned this frame.
                probe_line(
                    "safe_area_event",
                    format!("{},{},{},{}", safe.left, safe.top, safe.right, safe.bottom),
                );
            }
        }
        // The frames this run asks for by itself: the first few, a probe run's first 30, and the
        // clipboard probe's first four (its read goes out in frame 4; the reply asks for frame 5).
        if self.presented < self.options.log_frames
            || (probe && self.presented < PROBE_LAST_FRAME)
            || (self.options.probe_clipboard && self.presented < CLIPBOARD_PROBE_FRAMES)
        {
            self.want_frame = true;
        }
        if LOOP_RETURNS && tour_ended && self.options.exit_after_demo {
            event_loop.exit();
            return Ok(());
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
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.options.probe {
            probe_line("lifecycle", "resumed");
        }
        self.paused = false;
        self.demo.resume(self.start.elapsed());
        if let Some(shown) = &self.shown {
            if self.options.probe {
                probe_window("resumed", &shown.window, &shown.surface);
            }
            shown.window.request_redraw();
            return;
        }
        match setup(
            event_loop,
            self.options,
            self.keyboard_changed.clone(),
            self.proxy.clone(),
        ) {
            Ok(shown) => {
                shown.window.request_redraw();
                self.shown = Some(shown);
            }
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // The tour's clock stops with the app: a background of any length replays no burst.
        self.demo.pause(self.start.elapsed());
        // Stop drawing. The surface stays: on iOS its layer survives the background, so the frame
        // that follows `resumed` draws into it as before (docs/mobile.md, life cycle).
        if self.options.pause {
            self.paused = true;
        } else {
            self.want_frame = true;
        }
        if self.options.probe {
            probe_line("lifecycle", "suspended");
            if let Some(shown) = &self.shown {
                probe_window("suspended", &shown.window, &shown.surface);
            }
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        // A frame scheduled with `Repaint::At` (`ControlFlow::WaitUntil`) is due: the loop woke for
        // it and nothing else asks.
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.want_frame = true;
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Every frame this run asks for is requested here, never inside `RedrawRequested`, where a
        // request is lost while iOS is idle (docs/mobile.md, M1 reading).
        if self.keyboard_changed.take() {
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
        // The accessibility bridge asked for a frame.
        self.want_frame = true;
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        if self.options.probe {
            probe_line("lifecycle", "memory_warning");
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if event_loop.exiting() {
            return;
        }
        // An input event needs a frame to be read; a redraw request is not one.
        if !matches!(event, WindowEvent::RedrawRequested) {
            self.want_frame = true;
        }
        if let Some(shown) = &mut self.shown {
            shown.translator.window_event(&event);
            shown.access.window_event(&shown.window, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(shown) = &mut self.shown
                    && size.width > 0
                    && size.height > 0
                {
                    shown.surface.resize(&shown.gpu, size.width, size.height);
                    shown.window.request_redraw();
                }
                if self.options.probe {
                    probe_line("resized_payload", format!("{}x{}", size.width, size.height));
                    if let Some(shown) = &self.shown {
                        probe_window("resized", &shown.window, &shown.surface);
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if self.options.probe {
                    probe_line("scale_factor_changed", scale_factor);
                    if let Some(shown) = &self.shown {
                        probe_window("scale_factor_changed", &shown.window, &shown.surface);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.render(event_loop) {
                    self.error = Some(e);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}

/// Runs the demo on `event_loop` until its window closes. On iOS `run_app` never returns.
pub fn run(event_loop: EventLoop<()>, options: Options) -> Result<(), String> {
    let mut demo = Demo::new();
    if options.demo {
        demo.play(Play {
            repeat: options.repeat,
            touches: options.touches,
        });
    }
    let mut app = App {
        proxy: event_loop.create_proxy(),
        options,
        tour: Tour::default(),
        shown: None,
        demo,
        start: Instant::now(),
        presented: 0,
        clipboard: if cfg!(target_os = "ios") {
            Clipboard::with_backend(Box::new(mobile::Pasteboard::new()))
        } else {
            Clipboard::new()
        },
        last_keyboard: Insets::ZERO,
        keyboard_changed: Rc::new(Cell::new(false)),
        paused: false,
        want_frame: false,
        error: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop: {e}"))?;
    if let Some(e) = app.error {
        return Err(e);
    }
    if options.demo && options.exit_after_demo && LOOP_RETURNS {
        let t = &app.tour;
        if !t.ended {
            return Err("demo: the window closed before the tour ended".to_owned());
        }
        if t.ok != t.total {
            return Err(format!(
                "demo: {} of {} marks failed",
                t.total - t.ok,
                t.total
            ));
        }
    }
    Ok(())
}

/// The Android entry point: the one symbol android-activity's native glue looks up. It builds the
/// event loop over the activity and runs the same [`run`] as the other targets.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
// SAFETY: `android_main` is the name android-activity's glue resolves at start-up, and no other item
// in this crate or its dependencies uses it, so exporting it unmangled clashes with nothing.
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    let event_loop = match EventLoop::builder().with_android_app(app).build() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("event loop: {e}");
            return;
        }
    };
    if let Err(e) = run(event_loop, Options::default()) {
        eprintln!("{e}");
    }
}
