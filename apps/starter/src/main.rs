//! starter: the smallest app on the toolkit. Copy `screen.rs` (the screen: its state and one draw
//! function, no windowing); this file is the wiring a real app also needs: one frame function,
//! a window, and a headless render.
//!
//! - `starter`: opens a window titled "starter" and runs until it is closed;
//! - `starter --dump <file.png> [--window <W>x<H>] [--scale <S>]`: renders one frame headless
//!   (default 800x600 at scale 2) and writes the PNG. Exit 0 written, 1 a render error, 2 usage.
//!
//! The window path follows the duties in `docs/consuming.md`: the app owns the event loop, passes
//! time and size in every frame, honours the frame's `Repaint`, calls `Gpu::end_frame` once per
//! frame and stops with a message when the device is lost. Left out to stay small: the clipboard
//! (`lune_ui::clipboard`), the accessibility bridge (the facade's `accesskit` feature), glass.

mod screen;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use lune_ui::dump;
use lune_ui::frame::FrameRenderer;
use lune_ui::gpu::{self, Gpu};
use lune_ui::input::{
    FrameOutput, Keymap, OutputState, RawInput, Translator, control_flow, keymap,
};
use lune_ui::lunegit_dark::THEME;
use lune_ui::motion::{Cadence, MotionTier};
use lune_ui::paint::Rect;
use lune_ui::scene::{FrameParams, Scene};
use lune_ui::shortcuts::Shortcuts;
use lune_ui::strings::{DisplayFormat, StringTable};
use lune_ui::surface::{Acquired, SurfaceOptions, WindowSurface};
use lune_ui::testing::ScriptedInput;
use lune_ui::text::{TextCache, TextSystem};
use lune_ui::ui::{FrameConfig, Ui, UiStore};
use screen::Screen;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// What the app keeps between frames, besides the screen's own state: the toolkit's retained
/// store and the two tables every frame reads.
struct Core {
    store: UiStore,
    strings: StringTable,
    shortcuts: Shortcuts,
    screen: Screen,
}

impl Core {
    fn new() -> Self {
        Self {
            store: UiStore::default(),
            strings: StringTable::default(),
            shortcuts: Shortcuts::default(),
            screen: Screen::new(),
        }
    }

    /// One frame: the screen drawn into a window of `logical` pixels from `input`; returns the
    /// display list to render and what the frame asks of the app (cursor, repaint, commands).
    fn frame(
        &mut self,
        input: &RawInput,
        text: &mut dyn TextSystem,
        logical: (f32, f32),
        keymap: Keymap,
    ) -> (Scene, FrameOutput) {
        let config = FrameConfig {
            theme: THEME,
            tier: MotionTier::Full,
            busy_cadence: Cadence::BUSY,
            strings: &self.strings,
            arg_format: &DisplayFormat,
            keymap,
            shortcuts: &self.shortcuts,
        };
        let mut ui = Ui::begin(&mut self.store, input, config, text);
        self.screen
            .draw(&mut ui, Rect::new(0.0, 0.0, logical.0, logical.1));
        ui.finish()
    }
}

/// The fonts the app supplies (the toolkit bundles none); these are the gallery's files.
fn text_cache() -> TextCache {
    TextCache::new(
        &[
            include_bytes!("../../gallery/assets/RobotoMonoNerdFontMono-Regular.ttf").as_slice(),
            include_bytes!("../../gallery/assets/RznSans-Regular.ttf"),
            include_bytes!("../../gallery/assets/RznSans-Medium.ttf"),
        ],
        "RznSans",
        "RobotoMono Nerd Font Mono",
    )
}

/// What the command line asks for.
#[derive(Debug, PartialEq)]
enum Mode {
    Window,
    Dump {
        path: PathBuf,
        window: (u32, u32),
        scale: f32,
    },
}

const USAGE: &str = "usage: starter [--dump <file.png> [--window <W>x<H>] [--scale <S>]]\n  W and H 1..=4096, S 0.5..=4.0, W*S and H*S at most 8192 (default 800x600 at 2)";

/// The longest physical side a dump may have (what the gallery's dump allows).
const MAX_PHYSICAL: u32 = 8192;

/// `<W>x<H>`: digits only, each side 1 to 4096.
fn parse_window(v: &str) -> Option<(u32, u32)> {
    let (w, h) = v.split_once('x')?;
    let side = |t: &str| {
        let digits = !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
        t.parse::<u32>()
            .ok()
            .filter(|n| digits && (1..=4096).contains(n))
    };
    Some((side(w)?, side(h)?))
}

fn parse(args: &[String]) -> Result<Mode, String> {
    let [flag, path, rest @ ..] = args else {
        return if args.is_empty() {
            Ok(Mode::Window)
        } else {
            Err(USAGE.into())
        };
    };
    if flag != "--dump" {
        return Err(USAGE.into());
    }
    let (mut window, mut scale) = (None, None);
    let mut rest = rest.iter();
    while let Some(name) = rest.next() {
        let value = rest
            .next()
            .ok_or_else(|| format!("{name} needs a value\n{USAGE}"))?;
        let taken = match name.as_str() {
            "--window" => parse_window(value).map(|w| window.replace(w).is_some()),
            "--scale" => value
                .parse::<f32>()
                .ok()
                .filter(|s| (0.5..=4.0).contains(s))
                .map(|s| scale.replace(s).is_some()),
            _ => return Err(format!("unknown argument `{name}`\n{USAGE}")),
        };
        match taken {
            Some(false) => {}
            Some(true) => return Err(format!("{name} given more than once\n{USAGE}")),
            None => return Err(format!("{name} `{value}` is out of range\n{USAGE}")),
        }
    }
    let (window, scale) = (window.unwrap_or((800, 600)), scale.unwrap_or(2.0));
    // The target's physical sides are the GPU's to take: refuse what it cannot make.
    let physical = |side: u32| (side as f32 * scale).round() as u32;
    if physical(window.0).max(physical(window.1)) > MAX_PHYSICAL {
        return Err(format!(
            "--window {}x{} at --scale {scale} is {}x{} physical pixels; a side is at most {MAX_PHYSICAL}\n{USAGE}",
            window.0,
            window.1,
            physical(window.0),
            physical(window.1)
        ));
    }
    Ok(Mode::Dump {
        path: PathBuf::from(path),
        window,
        scale,
    })
}

/// Renders one frame of a fresh app headless: the size in physical pixels and its RGBA bytes.
fn render(window: (u32, u32), scale: f32) -> Result<((u32, u32), Vec<u8>), String> {
    let mut gpu = pollster::block_on(Gpu::new(gpu::instance(None), None))?;
    render_with(&mut gpu, window, scale)
}

/// [`render`] on a device the caller made.
fn render_with(
    gpu: &mut Gpu,
    window: (u32, u32),
    scale: f32,
) -> Result<((u32, u32), Vec<u8>), String> {
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let mut renderer = FrameRenderer::new(gpu, format);
    let mut text = text_cache();
    let mut input = ScriptedInput::new((window.0 as f32, window.1 as f32), scale);
    input.focus(true);
    let (scene, _) = Core::new().frame(
        &input.end_frame(),
        &mut text,
        (window.0 as f32, window.1 as f32),
        Keymap::MAC,
    );
    let size = (
        (window.0 as f32 * scale).round() as u32,
        (window.1 as f32 * scale).round() as u32,
    );
    let target = dump::target(&gpu.device, format, size);
    let view = target.create_view(&Default::default());
    let params = FrameParams {
        size,
        scale,
        ..FrameParams::default()
    };
    // Twice: the first draw fills the glyph atlas, the second is the one to keep.
    for _ in 0..2 {
        renderer.draw(gpu, &mut text, &scene, &view, params)?;
        gpu.end_frame(&mut text);
    }
    Ok((size, dump::read_rgba(&gpu.device, &gpu.queue, &target)?))
}

/// The window, its surface and everything that draws into it.
struct Shown {
    window: Arc<Window>,
    surface: WindowSurface,
    gpu: Gpu,
    text: TextCache,
    renderer: FrameRenderer,
    translator: Translator,
    output: OutputState,
}

struct Starter {
    core: Core,
    shown: Option<Shown>,
    /// The time origin: `RawInput.time` is the time since it.
    start: Instant,
    exit_code: u8,
}

impl Starter {
    fn open(event_loop: &ActiveEventLoop) -> Result<Shown, String> {
        let attrs = Window::default_attributes()
            .with_title("starter")
            .with_inner_size(LogicalSize::new(800.0, 600.0));
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
        let surface =
            WindowSurface::with_options(&gpu, surface, (size.width, size.height), options)?;
        Ok(Shown {
            renderer: FrameRenderer::for_surface(&mut gpu, &surface),
            translator: Translator::new(size, window.scale_factor()),
            output: OutputState::new(),
            text: text_cache(),
            window,
            surface,
            gpu,
        })
    }

    /// Draws one frame into the window and sets when the next one is due.
    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let Some(s) = &mut self.shown else {
            return Ok(());
        };
        if s.gpu.is_lost() {
            return Err("the GPU device was lost".into());
        }
        let frame = match s.surface.acquire(&s.gpu)? {
            Acquired::Frame(frame) => frame,
            Acquired::Skip => return Ok(()),
            Acquired::Redraw => {
                s.window.request_redraw();
                return Ok(());
            }
        };
        let (size, scale) = (s.surface.size(), s.window.scale_factor() as f32);
        let logical = (size.0 as f32 / scale, size.1 as f32 / scale);
        let input = s.translator.take(self.start.elapsed());
        let (scene, out) = self.core.frame(&input, &mut s.text, logical, keymap());
        if s.output.apply(&s.window, &out, scale) {
            event_loop.exit();
        }
        let view = frame.texture.create_view(&Default::default());
        let params = FrameParams {
            size,
            scale,
            ..FrameParams::default()
        };
        s.renderer
            .draw(&mut s.gpu, &mut s.text, &scene, &view, params)?;
        s.window.pre_present_notify();
        s.surface.present(&s.gpu, frame);
        s.gpu.end_frame(&mut s.text);
        let (flow, now) = control_flow(out.repaint, self.start);
        event_loop.set_control_flow(flow);
        if now {
            s.window.request_redraw();
        }
        Ok(())
    }
}

impl ApplicationHandler for Starter {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.shown.is_some() {
            return;
        }
        match Self::open(event_loop) {
            Ok(shown) => {
                shown.window.request_redraw();
                self.shown = Some(shown);
            }
            Err(e) => {
                eprintln!("{e}");
                self.exit_code = 1;
                event_loop.exit();
            }
        }
    }

    /// A `Repaint::At` deadline has come.
    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if let (StartCause::ResumeTimeReached { .. }, Some(s)) = (cause, &self.shown) {
            s.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if event_loop.exiting() {
            return;
        }
        match &event {
            WindowEvent::CloseRequested => return event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.redraw(event_loop) {
                    eprintln!("render: {e}");
                    self.exit_code = 1;
                    event_loop.exit();
                }
                return;
            }
            _ => {}
        }
        let Some(s) = &mut self.shown else { return };
        if let WindowEvent::Resized(size) = &event {
            s.surface.resize(&s.gpu, size.width, size.height);
        }
        s.translator.window_event(&event);
        s.window.request_redraw();
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(2)
        }
        Ok(Mode::Dump {
            path,
            window,
            scale,
        }) => {
            let written =
                render(window, scale).and_then(|(size, rgba)| dump::write_png(&path, size, &rgba));
            match written {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("dump: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Ok(Mode::Window) => {
            let event_loop = match EventLoop::new() {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("event loop: {e}");
                    return ExitCode::from(1);
                }
            };
            let mut app = Starter {
                core: Core::new(),
                shown: None,
                start: Instant::now(),
                exit_code: 0,
            };
            if let Err(e) = event_loop.run_app(&mut app) {
                eprintln!("event loop: {e}");
                return ExitCode::from(1);
            }
            ExitCode::from(app.exit_code)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use lune_ui::access::{Node, Role};
    use lune_ui::input::{Key, PointerButton};
    use lune_ui::testing::FakeText;

    use super::*;

    const WINDOW: (f32, f32) = (800.0, 600.0);

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    fn frame(core: &mut Core, input: &mut ScriptedInput) -> FrameOutput {
        core.frame(&input.end_frame(), &mut FakeText, WINDOW, Keymap::MAC)
            .1
    }

    fn find(out: &FrameOutput, role: Role) -> Vec<&Node> {
        out.tree.nodes.iter().filter(|n| n.role == role).collect()
    }

    fn centre(n: &Node) -> (f32, f32) {
        (n.bounds.x + n.bounds.w / 2.0, n.bounds.y + n.bounds.h / 2.0)
    }

    #[test]
    fn the_first_frame_has_five_rows_a_field_and_a_button() {
        let mut core = Core::new();
        let mut input = ScriptedInput::new(WINDOW, 2.0);
        input.focus(true);
        let out = frame(&mut core, &mut input);
        assert_eq!(find(&out, Role::ListItem).len(), 5);
        assert_eq!(find(&out, Role::TextInput).len(), 1);
        let buttons = find(&out, Role::Button);
        assert_eq!(buttons.len(), 1);
        assert_eq!(buttons[0].name, "Add");
    }

    #[test]
    fn typing_and_pressing_add_appends_a_row() {
        let mut core = Core::new();
        let mut input = ScriptedInput::new(WINDOW, 2.0);
        input.focus(true);
        let out = frame(&mut core, &mut input);
        let (x, y) = centre(find(&out, Role::TextInput)[0]);
        let add = centre(find(&out, Role::Button)[0]);
        input.pointer_to(x, y);
        frame(&mut core, &mut input);
        input.click(PointerButton::Primary);
        frame(&mut core, &mut input);
        input.key_down(Key::Character("x".into()), Some("x"));
        frame(&mut core, &mut input);
        assert_eq!(core.screen.text, "x");
        input.pointer_to(add.0, add.1);
        frame(&mut core, &mut input);
        input.click(PointerButton::Primary);
        let out = frame(&mut core, &mut input);
        assert_eq!(core.screen.items.len(), 6);
        assert_eq!(core.screen.items.last().map(String::as_str), Some("x"));
        assert!(core.screen.text.is_empty());
        let out = if find(&out, Role::ListItem).len() == 6 {
            out
        } else {
            frame(&mut core, &mut input)
        };
        assert_eq!(find(&out, Role::ListItem).len(), 6);
    }

    #[test]
    fn the_command_line_reads_the_dump_flags() {
        assert_eq!(parse(&args(&[])), Ok(Mode::Window));
        let dump = |w, s| Mode::Dump {
            path: "a.png".into(),
            window: w,
            scale: s,
        };
        assert_eq!(
            parse(&args(&["--dump", "a.png"])),
            Ok(dump((800, 600), 2.0))
        );
        assert_eq!(
            parse(&args(&[
                "--dump", "a.png", "--scale", "3", "--window", "402x874"
            ])),
            Ok(dump((402, 874), 3.0))
        );
        // The limit is on the physical side: 4096 at scale 2 is 8192 and is allowed.
        assert_eq!(
            parse(&args(&[
                "--dump", "a.png", "--window", "4096x10", "--scale", "2"
            ])),
            Ok(dump((4096, 10), 2.0))
        );
        for bad in [
            &["--dump"][..],
            &["--nope"],
            &["--dump", "a.png", "--window", "0x5"],
            &["--dump", "a.png", "--window", "4097x5"],
            &["--dump", "a.png", "--window", "5"],
            &["--dump", "a.png", "--scale", "0.4"],
            &["--dump", "a.png", "--scale", "4.1"],
            &["--dump", "a.png", "--scale", "2", "--scale", "2"],
            &["--dump", "a.png", "--bogus", "1"],
            &["--dump", "a.png", "--scale"],
            // A physical side above 8192: 4096 x 3 is 12288.
            &["--dump", "a.png", "--window", "4096x100", "--scale", "3"],
            &["--dump", "a.png", "--window", "100x3000", "--scale", "3"],
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_phone_sized_render_has_the_size_and_more_than_one_colour() {
        // Only a missing adapter skips; a broken draw or readback fails.
        let mut gpu = match pollster::block_on(Gpu::new(gpu::instance(None), None)) {
            Ok(gpu) => gpu,
            Err(e) => {
                assert!(
                    !std::env::var("LUNEUI_REQUIRE_GPU").is_ok_and(|v| v == "1"),
                    "no GPU adapter: {e}"
                );
                println!("skipped: no GPU adapter");
                return;
            }
        };
        let (size, rgba) = render_with(&mut gpu, (402, 874), 3.0).expect("the render works");
        assert_eq!(size, (1206, 2622));
        assert_eq!(rgba.len(), 1206 * 2622 * 4);
        let colours: HashSet<[u8; 4]> = rgba
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect();
        assert!(colours.len() > 1, "{} colour", colours.len());
    }
}
