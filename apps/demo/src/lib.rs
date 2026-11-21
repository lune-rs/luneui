//! The demo: the smallest app over the toolkit, one window that draws one frame (a heading, a
//! coloured shape, a frost surface over part of it, one button and a marker along each edge of the
//! surface). It exists to build for the phone targets and to show what the platform reports there.
//!
//! App code: it reads the clock (time since start is the frame's time) and runs the event loop. It
//! uses the facade only.

#![deny(unsafe_code)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use lune_ui::frame::FrameRenderer;
use lune_ui::gpu::{self, Gpu};
use lune_ui::input::{Event, OutputState, Translator, control_flow, keymap};
use lune_ui::layout::Insets;
use lune_ui::lunegit_dark::THEME;
use lune_ui::mobile::{self, KeyboardInset, SceneAttach};
use lune_ui::motion::{Cadence, MotionTier};
use lune_ui::paint::Rect;
use lune_ui::scene::{FrameParams, LayerId};
use lune_ui::shortcuts::Shortcuts;
use lune_ui::strings::{DisplayFormat, StringTable};
use lune_ui::surface::{Acquired, SurfaceOptions, WindowSurface};
use lune_ui::text::TextCache;
use lune_ui::ui::{FrameConfig, Ui, UiStore};
use lune_ui::widgets::button::ButtonKind;
use lune_ui::widgets::field::FieldState;
use lune_ui::widgets::label::LabelRole;

const UI_FONT: &str = "RznSans";
const MONO_FONT: &str = "RobotoMono Nerd Font Mono";

/// The window's size in logical points on a desktop: an iPhone 16 Pro's screen. Ignored on iOS.
const PHONE: (f64, f64) = (402.0, 874.0);

/// The text field's key.
const FIELD_KEY: &str = "field";

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
        }
    }
}

/// Whether this platform's event loop can return, so `Options::frames` applies.
const LOOP_RETURNS: bool = cfg!(not(any(target_os = "ios", target_os = "android")));

/// The presented frame at which a `--probe` run prints the scene state a second time; the demo
/// keeps requesting frames until it.
const PROBE_LAST_FRAME: u32 = 30;

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
fn text_cache() -> TextCache {
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
}

struct App {
    options: Options,
    shown: Option<Shown>,
    store: UiStore,
    strings: StringTable,
    shortcuts: Shortcuts,
    start: Instant,
    presented: u32,
    taps: u32,
    /// What the text field holds.
    field: String,
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
) -> Result<Shown, String> {
    let probe = options.probe;
    let attrs = Window::default_attributes()
        .with_title("luneui demo")
        .with_inner_size(LogicalSize::new(PHONE.0, PHONE.1));
    let window = Arc::new(
        event_loop
            .create_window(attrs)
            .map_err(|e| format!("window: {e}"))?,
    );
    // Under the iOS scene life cycle the window shows only once it belongs to a window scene.
    probe_scene(&window, options.scene_attach);
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
    let translator = Translator::new(size, window.scale_factor());
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
        let mut input = translator.take(self.start.elapsed());
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
        let config = FrameConfig {
            theme: THEME,
            tier: MotionTier::Full,
            busy_cadence: Cadence::BUSY,
            strings: &self.strings,
            arg_format: &DisplayFormat,
            keymap: keymap(),
            shortcuts: &self.shortcuts,
        };
        let mut ui = Ui::begin(&mut self.store, &input, config, text);
        // The surface in logical points: the markers sit on its four edges.
        let (w, h) = (size.0 as f32 / s, size.1 as f32 / s);
        let safe = ui.safe_area();
        let keyboard_bottom = ui.keyboard_inset().bottom;
        if self.options.focus_field && self.presented == 0 {
            let id = ui.id(FIELD_KEY);
            ui.request_focus(id);
        }
        if self.options.probe && self.presented + 1 == PROBE_LAST_FRAME {
            ui.request_repaint_at(input.time + PROBE_TIMED_FRAME);
        }
        let mut taps = self.taps;
        draw(
            &mut ui,
            w,
            h,
            safe,
            keyboard_bottom,
            &mut self.field,
            &mut taps,
        );
        self.taps = taps;
        let (display, out) = ui.finish();
        if output.apply(window, &out, s) {
            event_loop.exit();
        }
        let params = FrameParams {
            size,
            scale: s,
            blur_sigma: BLUR_SIGMA,
            saturation: BLUR_SATURATION,
            ..FrameParams::default()
        };
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
        // The frames this run asks for by itself: the first few, and a probe run's first 30.
        if self.presented < self.options.log_frames || (probe && self.presented < PROBE_LAST_FRAME)
        {
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
}

/// One frame of the demo in a surface of `w` by `h` logical points, with `safe` the edges the
/// system covers and `keyboard_bottom` the height the soft keyboard covers at the bottom.
fn draw(
    ui: &mut Ui<'_>,
    w: f32,
    h: f32,
    safe: Insets,
    keyboard_bottom: f32,
    field: &mut String,
    taps: &mut u32,
) {
    // Keyboard avoidance: the shapes and the field move up by what the field would otherwise lack
    // to clear the keyboard (the button stays where it is, under the keyboard).
    let field_h = 44.0;
    let lift = (h * 0.68 + field_h + 12.0 - (h - keyboard_bottom)).max(0.0);
    let t = *ui.theme();
    let p = ui.painter();
    p.fill(Rect::new(0.0, 0.0, w, h), 0.0, [20, 22, 28, 255]);
    // Two overlapping shapes for the frost to blur.
    p.fill(
        Rect::new(w * 0.08, h * 0.30 - lift, w * 0.50, h * 0.22),
        28.0,
        [232, 96, 62, 255],
    );
    p.fill(
        Rect::new(w * 0.42, h * 0.38 - lift, w * 0.50, h * 0.22),
        28.0,
        [62, 150, 232, 255],
    );
    ui.label("heading", (24.0, h * 0.12), "luneui demo", LabelRole::Title);
    let button = Rect::new(24.0, h * 0.78, w - 48.0, 44.0);
    let text = if *taps == 0 {
        "Tap".to_owned()
    } else {
        format!("Tapped {taps}")
    };
    if ui.button("tap", button, &text, ButtonKind::Primary).clicked {
        *taps += 1;
    }
    // The frost surface over the lower part of the shapes: a blurred copy of what is under it.
    let panel = Rect::new(w * 0.20, h * 0.46 - lift, w * 0.60, h * 0.18);
    let p = ui.painter();
    p.on(LayerId::Popup);
    p.backdrop(panel, 20.0);
    p.fill(panel, 20.0, t.menu_glass);
    p.ring(panel, 20.0, 1.0, t.menu_popup_border, false);
    ui.label(
        "frost",
        (panel.x + 16.0, panel.y + 16.0),
        "frost",
        LabelRole::Strong,
    );
    // The text field, above the soft keyboard when one is up.
    let rest = h * 0.68 - lift;
    ui.painter().on(LayerId::Toast);
    ui.text_field(
        FIELD_KEY,
        Rect::new(24.0, rest, w - 48.0, field_h),
        field,
        "Type here",
        FieldState::default(),
    );
    // A marker along each edge of the surface, in four colours: top red, right green, bottom
    // blue, left yellow.
    let p = ui.painter();
    p.on(LayerId::Toast);
    let m = 2.0;
    p.fill(Rect::new(0.0, 0.0, w, m), 0.0, [255, 0, 0, 255]);
    p.fill(Rect::new(w - m, 0.0, m, h), 0.0, [0, 255, 0, 255]);
    p.fill(Rect::new(0.0, h - m, w, m), 0.0, [0, 128, 255, 255]);
    p.fill(Rect::new(0.0, 0.0, m, h), 0.0, [255, 255, 0, 255]);
    // An outline of the safe area: the part of the surface the content may use.
    let inside = Rect::new(
        safe.left,
        safe.top,
        w - safe.left - safe.right,
        h - safe.top - safe.bottom,
    );
    p.ring(inside, 0.0, 1.0, [255, 255, 255, 160], false);
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.options.probe {
            probe_line("lifecycle", "resumed");
        }
        self.paused = false;
        if let Some(shown) = &self.shown {
            if self.options.probe {
                probe_window("resumed", &shown.window, &shown.surface);
            }
            shown.window.request_redraw();
            return;
        }
        match setup(event_loop, self.options, self.keyboard_changed.clone()) {
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
    let mut app = App {
        options,
        shown: None,
        store: UiStore::default(),
        strings: StringTable::default(),
        shortcuts: Shortcuts::default(),
        start: Instant::now(),
        presented: 0,
        taps: 0,
        field: String::new(),
        last_keyboard: Insets::ZERO,
        keyboard_changed: Rc::new(Cell::new(false)),
        paused: false,
        want_frame: false,
        error: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop: {e}"))?;
    app.error.map_or(Ok(()), Err)
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
