//! The demo: the smallest app over the toolkit, one window that draws one frame (a heading, a
//! coloured shape, a frost surface over part of it, one button and a marker along each edge of the
//! surface). It exists to build for the phone targets and to show what the platform reports there.
//!
//! App code: it reads the clock (time since start is the frame's time) and runs the event loop. It
//! uses the facade only.

#![deny(unsafe_code)]

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use lune_ui::frame::FrameRenderer;
use lune_ui::gpu::{self, Gpu};
use lune_ui::input::{OutputState, Translator, control_flow, keymap};
use lune_ui::lunegit_dark::THEME;
use lune_ui::motion::{Cadence, MotionTier};
use lune_ui::paint::Rect;
use lune_ui::scene::{FrameParams, LayerId};
use lune_ui::shortcuts::Shortcuts;
use lune_ui::strings::{DisplayFormat, StringTable};
use lune_ui::surface::{Acquired, SurfaceOptions, WindowSurface};
use lune_ui::text::TextCache;
use lune_ui::ui::{FrameConfig, Ui, UiStore};
use lune_ui::widgets::button::ButtonKind;
use lune_ui::widgets::label::LabelRole;

const UI_FONT: &str = "RznSans";
const MONO_FONT: &str = "RobotoMono Nerd Font Mono";

/// The window's size in logical points on a desktop: an iPhone 16 Pro's screen. Ignored on iOS.
const PHONE: (f64, f64) = (402.0, 874.0);

/// The backdrop blur's sigma (physical pixels) and saturation, the gallery's values.
const BLUR_SIGMA: f32 = 8.0;
const BLUR_SATURATION: f32 = 1.8;

/// What a run does beyond drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// How many presented frames print a `presented <n> <width>x<height>` line, then none.
    pub log_frames: u32,
    /// Print `probe <key> <value>` lines for what the platform reports: at window creation and on
    /// every resize, scale change and life-cycle callback (never per frame).
    pub probe: bool,
    /// Exit the event loop after this many presented frames. Desktop only: on iOS and Android the
    /// loop cannot return, so it is ignored there.
    pub frames: Option<u32>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            log_frames: 3,
            probe: false,
            frames: None,
        }
    }
}

/// Whether this platform's event loop can return, so `Options::frames` applies.
const LOOP_RETURNS: bool = cfg!(not(any(target_os = "ios", target_os = "android")));

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
    let inner_pos = window.inner_position();
    let outer_pos = window.outer_position();
    probe_line("inner_position", position_text(inner_pos.clone()));
    probe_line("outer_position", position_text(outer_pos.clone()));
    probe_line("scale_factor", scale);
    match (inner_pos, outer_pos) {
        (Ok(i), Ok(o)) => {
            let left = i.x - o.x;
            let top = i.y - o.y;
            let right = outer.width as i32 - inner.width as i32 - left;
            let bottom = outer.height as i32 - inner.height as i32 - top;
            let l = |v: i32| f64::from(v) / scale;
            probe_line(
                "safe_area_logical",
                format!(
                    "left={:.2} top={:.2} right={:.2} bottom={:.2}",
                    l(left),
                    l(top),
                    l(right),
                    l(bottom)
                ),
            );
        }
        _ => probe_line("safe_area_logical", "n/a"),
    }
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
    error: Option<String>,
}

/// Opens the window, the GPU and the surface.
fn setup(event_loop: &ActiveEventLoop, probe: bool) -> Result<Shown, String> {
    let attrs = Window::default_attributes()
        .with_title("luneui demo")
        .with_inner_size(LogicalSize::new(PHONE.0, PHONE.1));
    let window = Arc::new(
        event_loop
            .create_window(attrs)
            .map_err(|e| format!("window: {e}"))?,
    );
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
    })
}

impl App {
    /// Draws one frame and presents it.
    fn render(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
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
        } = shown;
        let target = match surface.acquire(gpu)? {
            Acquired::Frame(t) => t,
            Acquired::Skip => return Ok(()),
            Acquired::Redraw => {
                window.request_redraw();
                return Ok(());
            }
        };
        let view = target.texture.create_view(&Default::default());
        let size = surface.size();
        let scale = window.scale_factor();
        translator.scale_factor(scale);
        let s = scale as f32;
        let input = translator.take(self.start.elapsed());
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
        let mut taps = self.taps;
        draw(&mut ui, w, h, &mut taps);
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
        if self.presented <= self.options.log_frames {
            println!("presented {} {}x{}", self.presented, size.0, size.1);
            window.request_redraw();
        }
        if LOOP_RETURNS && let Some(n) = self.options.frames {
            if self.presented >= n {
                event_loop.exit();
                return Ok(());
            }
            window.request_redraw();
        }
        let (flow, redraw) = control_flow(out.repaint, self.start);
        event_loop.set_control_flow(flow);
        if redraw {
            window.request_redraw();
        }
        Ok(())
    }
}

/// One frame of the demo in a surface of `w` by `h` logical points.
fn draw(ui: &mut Ui<'_>, w: f32, h: f32, taps: &mut u32) {
    let t = *ui.theme();
    let p = ui.painter();
    p.fill(Rect::new(0.0, 0.0, w, h), 0.0, [20, 22, 28, 255]);
    // Two overlapping shapes for the frost to blur.
    p.fill(
        Rect::new(w * 0.08, h * 0.30, w * 0.50, h * 0.22),
        28.0,
        [232, 96, 62, 255],
    );
    p.fill(
        Rect::new(w * 0.42, h * 0.38, w * 0.50, h * 0.22),
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
    let panel = Rect::new(w * 0.20, h * 0.46, w * 0.60, h * 0.18);
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
    // A marker along each edge of the surface, in four colours: top red, right green, bottom
    // blue, left yellow.
    let p = ui.painter();
    p.on(LayerId::Toast);
    let m = 2.0;
    p.fill(Rect::new(0.0, 0.0, w, m), 0.0, [255, 0, 0, 255]);
    p.fill(Rect::new(w - m, 0.0, m, h), 0.0, [0, 255, 0, 255]);
    p.fill(Rect::new(0.0, h - m, w, m), 0.0, [0, 128, 255, 255]);
    p.fill(Rect::new(0.0, 0.0, m, h), 0.0, [255, 255, 0, 255]);
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.options.probe {
            probe_line("lifecycle", "resumed");
        }
        if let Some(shown) = &self.shown {
            if self.options.probe {
                probe_window("resumed", &shown.window, &shown.surface);
            }
            shown.window.request_redraw();
            return;
        }
        match setup(event_loop, self.options.probe) {
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
        if self.options.probe {
            probe_line("lifecycle", "suspended");
            if let Some(shown) = &self.shown {
                probe_window("suspended", &shown.window, &shown.surface);
            }
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
        error: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop: {e}"))?;
    app.error.map_or(Ok(()), Err)
}
