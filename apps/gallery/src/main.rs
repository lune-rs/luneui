//! Gallery: the pages of the toolkit.
//!
//! Every page is a [`page::Page`] in the registry (`pages/mod.rs`): legacy pages hold the fixed
//! Painter-API scenes the pixel gate compares, Ui pages draw through the `Ui` with scripted scenes.
//!
//! - no arguments: a glass window showing one page (Cmd/Ctrl+] and Cmd/Ctrl+[ switch pages, Up /
//!   Down cycle a legacy page's scenes, Cmd/Ctrl+Q quits; every other key reaches the page);
//! - `--dump <dir>`: renders every scene offscreen on a headless GPU device (no event loop, no
//!   window) and writes `<id>@<scale>x.png` files, the frames the pixel gate compares;
//! - `--dump <dir> --only <name> [--window WxH] [--scale S]`: renders only the scenes `<name>`
//!   names (a listed name or a bare id) and, for a Ui scene, in a window of another size and
//!   scale (`docs/screen-review.md`);
//! - `--list`: prints the scene names, legacy pages first;
//! - `--bench [--runs N] [--frames N] [--size WxH]`: times frames offscreen (see `bench`);
//! - `--smoke-platform`: puts an item in the tray, opens and closes its surface and tears down,
//!   in about two seconds (see `smoke`). It needs a logged-in GUI session: exit 0 when it ran
//!   and held, 1 when a check failed, 2 when there is no session to run in;
//! - `--smoke-panels`: opens the docked panels and the menu window of the `docked-panels` page and
//!   checks them on the real desktop with synthetic pointers (see `smoke_panels`). It needs a
//!   logged-in GUI session: exit 0 when every applicable check held, 1 when one failed, 2 when there
//!   is no session to run in;
//! - `--pace-probe`: times the frame pacing on the real renderer in four phases (see `pace`). It
//!   needs a logged-in GUI session; a window opens for about 15 s, leave it visible and untouched;
//!   exit 0 held, 1 did not hold, 2 could not measure.
//!
//! Dump mode reads no clock: time comes from the scene tables and scripts, pixels from the
//! offscreen target's readback (no screen or window capture). It reads one environment variable,
//! `LUNEUI_GALLERY_TEXT` (`on` or `off`): the text rendering every dumped scene is drawn with, in
//! place of the scene's own (`off` is the look before font smoothing, what the pixel gate proves).

mod bench;
#[cfg(test)]
mod catalogue;
mod custom;
mod own_chrome;
mod pace;
mod page;
mod pages;
mod panels;
mod popover;
mod scenes;
mod showcase;
mod smoke;
mod smoke_panels;
mod window;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lune_ui::dump;
use lune_ui::frame::{FrameParams, FrameRenderer};
use lune_ui::gpu::{self, Gpu};
use lune_ui::scene::Scene;
use lune_ui::text::{TextCache, TextRendering};
use page::{
    FrameTweaks, PageKind, UiScene, meets_expectation, register_images, release_images,
    run_scene_at,
};
use pages::PAGES;
use scenes::{H, LegacyScene, W};

const USAGE: &str = "usage: gallery [--dump <dir> [--only <name> [--window <W>x<H>] [--scale <S>]] | --list | --bench [--runs N] [--frames N] [--size WxH] | --smoke-platform | --smoke-panels | --pace-probe]\n  --only <name> dumps the scene(s) a listed name or a bare id names (`gallery --list` prints them); --window and --scale (Ui scenes only; W and H 1..=4096, S 0.5..=4.0) run the scene in another logical window and scale and write <id>@<S>x-<W>x<H>.png\n  --smoke-platform needs a logged-in GUI session (it puts an item in the tray for about 2 s); exit 2 without one\n  --smoke-panels needs a logged-in GUI session (three panel windows and a menu open for about 10 s; it never moves the cursor); exit 0 held, 1 failed, 2 no session\n  --pace-probe needs a logged-in GUI session; a window opens for about 15 s, leave it visible and untouched; exit 0 held, 1 did not hold, 2 could not measure";

enum Command {
    Window,
    Dump(DumpArgs),
    List,
    Bench(bench::Options),
    SmokePlatform,
    SmokePanels,
    PaceProbe,
}

fn parse(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Ok(Command::Window),
        [a] if a == "--list" => Ok(Command::List),
        [a] if a == "--smoke-platform" => Ok(Command::SmokePlatform),
        [a] if a == "--smoke-panels" => Ok(Command::SmokePanels),
        [a] if a == "--pace-probe" => Ok(Command::PaceProbe),
        [a, dir, rest @ ..] if a == "--dump" => parse_dump(dir, rest).map(Command::Dump),
        [a, rest @ ..] if a == "--bench" => bench::Options::parse(rest).map(Command::Bench),
        _ => Err(USAGE.to_owned()),
    }
}

/// `--dump <dir>` and its single-scene form.
#[derive(Debug, PartialEq)]
struct DumpArgs {
    dir: PathBuf,
    /// `--only`: the scenes to write; `None` writes every scene.
    only: Option<Only>,
}

/// `--only <name>` with its overrides.
#[derive(Debug, PartialEq)]
struct Only {
    /// A listed scene name (`ui-controls@1.5x`) or a bare id (`ui-controls`).
    name: String,
    /// `--window`: the logical window, in place of the scene's own.
    window: Option<(u32, u32)>,
    /// `--scale`: the device scale, in place of the scene's own.
    scale: Option<f32>,
}

/// The longest logical side `--window` takes, and the longest physical side a dump may have.
const MAX_WINDOW: u32 = 4096;
const MAX_PHYSICAL: u32 = 8192;

/// The flags after `--dump <dir>`, in any order; each at most once.
fn parse_dump(dir: &str, rest: &[String]) -> Result<DumpArgs, String> {
    let bad = |problem: String| format!("{problem}\n{USAGE}");
    let (mut name, mut window, mut scale) = (None, None, None);
    let mut i = 0;
    while i < rest.len() {
        let flag = rest[i].as_str();
        if !matches!(flag, "--only" | "--window" | "--scale") {
            return Err(bad(format!("unknown argument `{flag}` after --dump <dir>")));
        }
        let Some(value) = rest.get(i + 1).filter(|v| !v.starts_with("--")) else {
            return Err(bad(format!("{flag} needs a value")));
        };
        let repeated = match flag {
            "--only" => name.replace(value.clone()).is_some(),
            "--window" => window.replace(parse_window(value).map_err(&bad)?).is_some(),
            _ => scale.replace(parse_scale(value).map_err(&bad)?).is_some(),
        };
        if repeated {
            return Err(bad(format!("{flag} given more than once")));
        }
        i += 2;
    }
    let only = match name {
        Some(name) => Some(Only {
            name,
            window,
            scale,
        }),
        None if window.is_some() || scale.is_some() => {
            return Err(bad("--window and --scale need --only <name>".to_owned()));
        }
        None => None,
    };
    Ok(DumpArgs {
        dir: PathBuf::from(dir),
        only,
    })
}

/// `<W>x<H>`: digits only, each side 1 to 4096.
fn parse_window(v: &str) -> Result<(u32, u32), String> {
    let err = || format!("--window `{v}` is not <W>x<H> with W and H in 1..={MAX_WINDOW}");
    let (w, h) = v.split_once('x').ok_or_else(err)?;
    let side = |t: &str| -> Result<u32, String> {
        if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err());
        }
        t.parse::<u32>()
            .ok()
            .filter(|n| (1..=MAX_WINDOW).contains(n))
            .ok_or_else(err)
    };
    Ok((side(w)?, side(h)?))
}

/// A number from 0.5 to 4.0.
fn parse_scale(v: &str) -> Result<f32, String> {
    v.parse::<f32>()
        .ok()
        .filter(|s| (0.5..=4.0).contains(s))
        .ok_or_else(|| format!("--scale `{v}` is not a number in 0.5..=4.0"))
}

/// A scene a dump can write.
#[derive(Clone, Copy)]
enum Target {
    Legacy(&'static LegacyScene),
    /// A Ui scene with the id of its page.
    Ui(&'static str, &'static UiScene),
}

impl Target {
    fn id(&self) -> &'static str {
        match self {
            Target::Legacy(s) => s.id,
            Target::Ui(_, s) => s.id,
        }
    }

    fn name(&self) -> String {
        match self {
            Target::Legacy(s) => s.name(),
            Target::Ui(_, s) => s.name(),
        }
    }
}

/// Every scene, in the order `--list` prints them.
fn targets() -> Vec<Target> {
    let mut all = Vec::new();
    for page in PAGES {
        match page.kind {
            PageKind::Legacy(scenes) => all.extend(scenes.iter().map(Target::Legacy)),
            PageKind::Ui(ui) => all.extend(ui.scenes.iter().map(|s| Target::Ui(page.id, s))),
        }
    }
    all
}

/// A scene to dump and, for an overridden dump, the logical window and scale to run it in.
struct Pick {
    target: Target,
    over: Option<((u32, u32), f32)>,
}

/// The scenes `only` names (every listed scene with the name or the id, in list order). With
/// `--window` or `--scale` it is the first one only (one dump, whatever the scene's own scale),
/// and a legacy scene is refused.
fn resolve(only: &Only) -> Result<Vec<Pick>, String> {
    let mut picks: Vec<Pick> = targets()
        .into_iter()
        .filter(|t| t.name() == only.name || t.id() == only.name)
        .map(|target| Pick { target, over: None })
        .collect();
    if picks.is_empty() {
        return Err(format!(
            "--only `{}` matches no scene; `gallery --list` prints the names",
            only.name
        ));
    }
    if only.window.is_none() && only.scale.is_none() {
        return Ok(picks);
    }
    picks.truncate(1);
    let Target::Ui(_, scene) = picks[0].target else {
        return Err(format!(
            "--window and --scale apply to Ui scenes only; `{}` is a legacy scene",
            only.name
        ));
    };
    let window = only.window.unwrap_or_else(|| scene.window());
    let scale = only.scale.unwrap_or(scene.scale);
    let (pw, ph) = physical(window, scale);
    if pw.max(ph) > MAX_PHYSICAL {
        return Err(format!(
            "{}x{} at scale {scale} is {pw}x{ph} physical pixels; a side may be at most {MAX_PHYSICAL}",
            window.0, window.1
        ));
    }
    picks[0].over = Some((window, scale));
    Ok(picks)
}

/// The physical size of `window` logical pixels at `scale`.
fn physical(window: (u32, u32), scale: f32) -> (u32, u32) {
    (
        (window.0 as f32 * scale).round() as u32,
        (window.1 as f32 * scale).round() as u32,
    )
}

/// What draws a scene into a target: the GPU, text, one renderer and the custom draw.
struct Renderer {
    gpu: Gpu,
    text: TextCache,
    frame: FrameRenderer,
    fill: custom::Fill,
    /// A text rendering that replaces every frame's own (dump mode, `LUNEUI_GALLERY_TEXT`).
    text_rendering: Option<TextRendering>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = match parse(&args) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    match cmd {
        Command::List => {
            for name in scene_names() {
                println!("{name}");
            }
            ExitCode::SUCCESS
        }
        Command::Dump(DumpArgs { dir, only }) => {
            let picks = match only.as_ref().map(resolve).transpose() {
                Ok(p) => p,
                Err(msg) => {
                    eprintln!("{msg}");
                    return ExitCode::from(2);
                }
            };
            let var = std::env::var_os(TEXT_VAR);
            let text = match text_override(var.as_deref()) {
                Ok(text) => text,
                Err(msg) => {
                    eprintln!("{msg}");
                    return ExitCode::from(2);
                }
            };
            match run_dump(&dir, text, picks) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("dump: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Command::Bench(opts) => bench::run(&opts),
        Command::SmokePlatform => smoke::run(),
        Command::SmokePanels => smoke_panels::run(),
        Command::PaceProbe => pace::run(),
        Command::Window => window::run(),
    }
}

/// The environment variable that sets the text rendering of a dump.
const TEXT_VAR: &str = "LUNEUI_GALLERY_TEXT";

/// The text rendering `LUNEUI_GALLERY_TEXT` asks for: unset is `None` (each scene's own), `on` is
/// the smooth look and `off` is the look without smoothing. Any other value, or one that is not
/// Unicode, is an error naming the variable.
fn text_override(value: Option<&std::ffi::OsStr>) -> Result<Option<TextRendering>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value.to_str() {
        Some("on") => Ok(Some(TextRendering::SMOOTH)),
        Some("off") => Ok(Some(TextRendering::OFF)),
        _ => Err(format!(
            "{TEXT_VAR} must be `on` or `off` (got {})",
            value.to_string_lossy()
        )),
    }
}

const UI_FONT: &str = "RznSans";
const MONO_FONT: &str = "RobotoMono Nerd Font Mono";

/// The gallery's fonts, loaded in a fixed order.
fn text_cache() -> TextCache {
    TextCache::new(
        &[
            include_bytes!("../assets/RobotoMonoNerdFontMono-Regular.ttf").as_slice(),
            include_bytes!("../assets/RznSans-Regular.ttf"),
            include_bytes!("../assets/RznSans-Medium.ttf"),
        ],
        UI_FONT,
        MONO_FONT,
    )
}

/// A renderer over `gpu` for `Bgra8Unorm` offscreen targets.
fn offscreen_renderer(mut gpu: Gpu) -> Renderer {
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let frame = FrameRenderer::new(&mut gpu, format);
    let fill = custom::Fill::new(&gpu.device, format);
    Renderer {
        gpu,
        text: text_cache(),
        frame,
        fill,
        text_rendering: None,
    }
}

/// Dump mode: a headless device, `Bgra8Unorm` targets, every scene (or the `picks`) written to
/// `dir`. `text` replaces every scene's own text rendering when given.
fn run_dump(
    dir: &Path,
    text: Option<TextRendering>,
    picks: Option<Vec<Pick>>,
) -> Result<(), String> {
    let gpu = pollster::block_on(Gpu::new(gpu::instance(None), None))?;
    let mut r = offscreen_renderer(gpu);
    r.text_rendering = text;
    match picks {
        None => dump_scenes(&mut r, dir),
        Some(picks) => dump_picks(&mut r, dir, &picks),
    }
}

/// Every scene name, legacy pages first (the registry keeps them first).
fn scene_names() -> Vec<String> {
    let mut names = Vec::new();
    for page in PAGES {
        match page.kind {
            PageKind::Legacy(scenes) => names.extend(scenes.iter().map(|s| s.name())),
            PageKind::Ui(ui) => names.extend(ui.scenes.iter().map(|s| s.name())),
        }
    }
    names
}

/// Draws a finished display list at scale `s` into `target` (`size` physical pixels) with the
/// gallery's own blur.
fn draw_scene(
    r: &mut Renderer,
    scene: &Scene,
    s: f32,
    target: &wgpu::TextureView,
    size: (u32, u32),
) -> Result<(), String> {
    draw_with(r, scene, target, FrameTweaks::DEFAULT.params(size, s))
}

/// Draws a finished display list into `target` with `params`.
fn draw_with(
    r: &mut Renderer,
    scene: &Scene,
    target: &wgpu::TextureView,
    mut params: FrameParams,
) -> Result<(), String> {
    if let Some(text) = r.text_rendering {
        params.text = text;
    }
    let mut customs: [&mut dyn lune_ui::custom::CustomDraw; 1] = [&mut r.fill];
    r.frame
        .draw_with(&mut r.gpu, &mut r.text, scene, target, params, &mut customs)
}

/// Renders every scene offscreen and writes `<id>@<scale>x.png` into `dir`. Each scene's frame
/// is drawn twice (a warm-up, then the captured one). A legacy scene is painted again for each
/// draw, as it always was; a Ui scene runs its script once and draws its capture frame's scene
/// twice.
fn dump_scenes(r: &mut Renderer, dir: &Path) -> Result<(), String> {
    let picks: Vec<Pick> = targets()
        .into_iter()
        .map(|target| Pick { target, over: None })
        .collect();
    dump_picks(r, dir, &picks)
}

/// Writes each pick into `dir`: the full dump's path for every scene, `--only` included.
fn dump_picks(r: &mut Renderer, dir: &Path, picks: &[Pick]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    for pick in picks {
        match pick.target {
            Target::Legacy(scene) => {
                let s = scene.scale as f32;
                let size = (W * scene.scale, H * scene.scale);
                dump_one(r, dir, &scene.name(), size, |r, view| {
                    let display = scene.paint(&mut r.text, s, size);
                    draw_scene(r, &display, s, view, size)
                })?;
            }
            Target::Ui(page, scene) => dump_ui(r, dir, page, scene, pick.over)?,
        }
    }
    Ok(())
}

/// Dumps one Ui scene. `over` is a logical window and a scale in place of the scene's own: the
/// file is then `<id>@<S>x-<W>x<H>.png`, and a capture frame that is not the one the scene
/// expects (its script aims at fixed points of its own window) is a warning, not an error.
fn dump_ui(
    r: &mut Renderer,
    dir: &Path,
    page: &'static str,
    scene: &UiScene,
    over: Option<((u32, u32), f32)>,
) -> Result<(), String> {
    let (window, s) = over.unwrap_or_else(|| (scene.window(), scene.scale));
    let size = if over.is_some() {
        physical(window, s)
    } else {
        scene.size()
    };
    let name = match over {
        Some(_) => format!("{}@{}x-{}x{}", scene.id, s, window.0, window.1),
        None => scene.name(),
    };
    let images = register_images(&mut r.gpu, (scene.state)().images())
        .map_err(|e| format!("scene {}: {e}", scene.name()))?;
    let (display, out) = run_scene_at(page, scene, window, s, &mut r.text)
        .map_err(|e| format!("scene {}: {e}", scene.name()))?;
    if !meets_expectation(scene.expect, out.repaint) {
        let problem = format!(
            "scene {}: capture frame reports {:?}, expected {:?}",
            scene.name(),
            out.repaint,
            scene.expect
        );
        if over.is_none() {
            release_images(&mut r.gpu, &images);
            return Err(problem);
        }
        eprintln!("warning: {problem} (the scene's script may not suit this window)");
    }
    let dumped = dump_one(r, dir, &name, size, |r, view| {
        draw_scene(r, &display, s, view, size)
    });
    release_images(&mut r.gpu, &images);
    dumped
}

/// Draws a scene twice with `draw` into a fresh target and returns its RGBA bytes.
fn render_one(
    r: &mut Renderer,
    name: &str,
    size: (u32, u32),
    mut draw: impl FnMut(&mut Renderer, &wgpu::TextureView) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    let texture = dump::target(&r.gpu.device, r.frame.format(), size);
    let view = texture.create_view(&Default::default());
    for _ in 0..2 {
        draw(r, &view).map_err(|e| format!("scene {name}: {e}"))?;
        r.gpu.end_frame(&mut r.text);
    }
    dump::read_rgba(&r.gpu.device, &r.gpu.queue, &texture).map_err(|e| format!("scene {name}: {e}"))
}

/// Draws a scene twice with `draw` into a fresh target and writes it as `<name>.png`.
fn dump_one(
    r: &mut Renderer,
    dir: &Path,
    name: &str,
    size: (u32, u32),
    draw: impl FnMut(&mut Renderer, &wgpu::TextureView) -> Result<(), String>,
) -> Result<(), String> {
    let rgba = render_one(r, name, size, draw)?;
    dump::write_png(&dir.join(format!("{name}.png")), size, &rgba)
        .map_err(|e| format!("scene {name}: {e}"))?;
    eprintln!("dumped {name}.png");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;
    use crate::page::run_scene;

    #[test]
    fn the_text_variable_picks_the_dump_text_rendering() {
        assert_eq!(text_override(None), Ok(None));
        assert_eq!(
            text_override(Some(OsStr::new("on"))),
            Ok(Some(TextRendering::SMOOTH))
        );
        assert_eq!(
            text_override(Some(OsStr::new("off"))),
            Ok(Some(TextRendering::OFF))
        );
        for bad in ["", "ON", "Off", "true", "1", "bogus"] {
            let err = text_override(Some(OsStr::new(bad))).expect_err(bad);
            assert!(err.contains("LUNEUI_GALLERY_TEXT"), "{err}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_value_that_is_not_unicode_is_refused() {
        use std::os::unix::ffi::OsStrExt;
        let err = text_override(Some(OsStr::from_bytes(&[0x6f, 0xff]))).expect_err("not Unicode");
        assert!(err.contains("LUNEUI_GALLERY_TEXT"), "{err}");
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    /// `--dump <dir>` and the flags after it, as the parser reads them.
    fn dump(rest: &[&str]) -> Result<DumpArgs, String> {
        parse_dump("out", &args(rest))
    }

    #[test]
    fn the_two_argument_dump_is_still_the_plain_dump() {
        let Ok(Command::Dump(d)) = parse(&args(&["--dump", "out"])) else {
            panic!("--dump <dir> is a dump");
        };
        assert_eq!(
            d,
            DumpArgs {
                dir: PathBuf::from("out"),
                only: None
            }
        );
        // The other forms parse as before.
        assert!(matches!(parse(&args(&["--list"])), Ok(Command::List)));
        assert!(matches!(parse(&args(&[])), Ok(Command::Window)));
        assert!(parse(&args(&["--dump"])).is_err());
    }

    #[test]
    fn only_takes_its_flags_in_any_order() {
        let only = |name: &str, window, scale| {
            Some(Only {
                name: name.to_owned(),
                window,
                scale,
            })
        };
        assert_eq!(
            dump(&["--only", "ui-controls"]).expect("parses").only,
            only("ui-controls", None, None)
        );
        let a = dump(&["--only", "x", "--window", "402x874", "--scale", "3"]).expect("parses");
        let b = dump(&["--scale", "3", "--window", "402x874", "--only", "x"]).expect("parses");
        assert_eq!(a, b);
        assert_eq!(a.only, only("x", Some((402, 874)), Some(3.0)));
        assert_eq!(
            dump(&["--only", "x", "--scale", "1.5"])
                .expect("parses")
                .only,
            only("x", None, Some(1.5))
        );
    }

    #[test]
    fn each_usage_error_names_its_problem() {
        let cases: [(&[&str], &str); 16] = [
            (&["--window", "10x10"], "need --only"),
            (&["--scale", "2"], "need --only"),
            (
                &["--only", "a", "--only", "b"],
                "--only given more than once",
            ),
            (
                &["--only", "a", "--window", "1x1", "--window", "2x2"],
                "--window given more than once",
            ),
            (
                &["--only", "a", "--scale", "1", "--scale", "2"],
                "--scale given more than once",
            ),
            (
                &["--only", "a", "--bogus", "1"],
                "unknown argument `--bogus`",
            ),
            (&["--only", "a", "stray"], "unknown argument `stray`"),
            (&["--only"], "--only needs a value"),
            (&["--only", "a", "--window"], "--window needs a value"),
            (&["--only", "a", "--window", "402"], "not <W>x<H>"),
            (&["--only", "a", "--window", "0x100"], "not <W>x<H>"),
            (&["--only", "a", "--window", "4097x100"], "not <W>x<H>"),
            (&["--only", "a", "--window", "10x-5"], "not <W>x<H>"),
            (&["--only", "a", "--window", "+10x10"], "not <W>x<H>"),
            (
                &["--only", "a", "--scale", "0.4"],
                "not a number in 0.5..=4.0",
            ),
            (
                &["--only", "a", "--scale", "fast"],
                "not a number in 0.5..=4.0",
            ),
        ];
        for (rest, want) in cases {
            let err = dump(rest).expect_err(&rest.join(" "));
            assert!(err.contains(want), "{rest:?}: {err}");
        }
        assert!(dump(&["--only", "a", "--scale", "4.5"]).is_err());
        assert!(dump(&["--only", "a", "--scale", "nan"]).is_err());
        assert!(dump(&["--only", "a", "--window", "4096x1", "--scale", "0.5"]).is_ok());
    }

    /// The listed names of the scenes with `id`.
    fn listed(id: &str) -> Vec<String> {
        scene_names()
            .into_iter()
            .filter(|n| n.rsplit_once('@').is_some_and(|(i, _)| i == id))
            .collect()
    }

    fn picked(only: &Only) -> Vec<String> {
        resolve(only)
            .expect("resolves")
            .iter()
            .map(|p| p.target.name())
            .collect()
    }

    fn only(name: &str, window: Option<(u32, u32)>, scale: Option<f32>) -> Only {
        Only {
            name: name.to_owned(),
            window,
            scale,
        }
    }

    #[test]
    fn a_bare_id_and_a_full_name_resolve_to_the_listed_scenes() {
        let names = listed("ui-controls");
        assert!(!names.is_empty());
        assert_eq!(picked(&only("ui-controls", None, None)), names);
        for n in &names {
            assert_eq!(picked(&only(n, None, None)), vec![n.clone()]);
        }
        // The names come out in list order whatever the id.
        let all = scene_names();
        let pos: Vec<usize> = names
            .iter()
            .map(|n| all.iter().position(|a| a == n).expect("listed"))
            .collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn an_override_picks_one_scene_and_a_bad_one_is_refused() {
        let names = listed("ui-controls");
        let one = resolve(&only("ui-controls", Some((402, 874)), Some(3.0))).expect("resolves");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].target.name(), names[0]);
        assert_eq!(one[0].over, Some(((402, 874), 3.0)));
        // One override keeps the scene's own other value.
        let keep = resolve(&only("ui-controls", None, Some(2.0))).expect("resolves");
        assert_eq!(keep[0].over, Some(((W, H), 2.0)));
        // No such scene.
        let err = resolve(&only("recipe-x", None, None))
            .err()
            .expect("no match");
        assert!(
            err.contains("matches no scene") && err.contains("gallery --list"),
            "{err}"
        );
        // A legacy scene takes no override but dumps plain.
        let legacy = targets()
            .into_iter()
            .find(|t| matches!(t, Target::Legacy(_)))
            .expect("a legacy scene");
        let err = resolve(&only(&legacy.name(), Some((100, 100)), None))
            .err()
            .expect("refused");
        assert!(err.contains("Ui scenes only"), "{err}");
        assert_eq!(
            picked(&only(&legacy.name(), None, None)),
            vec![legacy.name()]
        );
        // A physical side over 8192.
        let err = resolve(&only("ui-controls", Some((4096, 100)), Some(3.0)))
            .err()
            .expect("too big");
        assert!(err.contains("8192"), "{err}");
        assert!(resolve(&only("ui-controls", Some((4096, 100)), Some(2.0))).is_ok());
    }

    /// A device for a test, or `None` (and a note) when there is no adapter; fails instead under
    /// `LUNEUI_REQUIRE_GPU=1`.
    fn test_renderer() -> Option<Renderer> {
        match pollster::block_on(Gpu::new(gpu::instance(None), None)) {
            Ok(g) => Some(offscreen_renderer(g)),
            Err(e) => {
                assert!(
                    !std::env::var("LUNEUI_REQUIRE_GPU").is_ok_and(|v| v == "1"),
                    "no GPU adapter: {e}"
                );
                println!("skipped: no GPU adapter");
                None
            }
        }
    }

    /// A scratch directory of this test run, removed by the caller.
    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("gallery-dump-{}-{tag}", std::process::id()))
    }

    /// The width and height in a PNG's IHDR.
    fn png_size(path: &Path) -> (u32, u32) {
        let b = std::fs::read(path).expect("the png was written");
        let be = |o: usize| u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        (be(16), be(20))
    }

    #[test]
    fn only_without_overrides_writes_what_the_full_dump_path_writes() {
        let Some(mut r) = test_renderer() else {
            return;
        };
        let dir = scratch("plain");
        let picks = resolve(&only("gradient-linear-fade", None, None)).expect("resolves");
        dump_picks(&mut r, &dir, &picks).expect("dumps");
        let (size, rgba) = render_gradient_scene(&mut r, "gradient-linear-fade");
        let want = dir.join("want.png");
        dump::write_png(&want, size, &rgba).expect("writes");
        let got = dir.join(format!("{}.png", picks[0].target.name()));
        assert_eq!(
            std::fs::read(&got).expect("the dump file"),
            std::fs::read(&want).expect("the reference file")
        );
        std::fs::remove_dir_all(&dir).expect("removes its scratch dir");
    }

    #[test]
    fn a_phone_window_and_scale_set_the_size_and_the_name() {
        let Some(mut r) = test_renderer() else {
            return;
        };
        let dir = scratch("phone");
        let picks = resolve(&only("ui-controls", Some((402, 874)), Some(3.0))).expect("resolves");
        dump_picks(&mut r, &dir, &picks).expect("dumps");
        assert_eq!(
            png_size(&dir.join("ui-controls@3x-402x874.png")),
            (1206, 2622)
        );
        std::fs::remove_dir_all(&dir).expect("removes its scratch dir");
    }

    /// The channels of `#141a33` and `#0d0f1a`, the fade's two ends.
    const FADE_FROM: [f64; 3] = [20.0, 26.0, 51.0];
    const FADE_TO: [f64; 3] = [13.0, 15.0, 26.0];

    /// Renders the gradients page's scene `id` as the dump does and returns its size and RGBA.
    fn render_gradient_scene(r: &mut Renderer, id: &str) -> ((u32, u32), Vec<u8>) {
        let page = PAGES
            .iter()
            .find(|p| p.id == "gradients")
            .expect("the gradients page");
        let PageKind::Ui(ui) = page.kind else {
            panic!("the gradients page is a Ui page");
        };
        let scene = ui.scenes.iter().find(|s| s.id == id).expect("the scene");
        let size = scene.size();
        let s = scene.scale;
        let (display, out) = run_scene(page.id, scene, &mut r.text).expect("run the scene");
        assert!(meets_expectation(scene.expect, out.repaint));
        let rgba = render_one(r, &scene.name(), size, |r, view| {
            draw_scene(r, &display, s, view, size)
        })
        .expect("render");
        (size, rgba)
    }

    /// The mean of channel `ch` of each aligned 8x8 tile, row-major.
    fn tile_means(size: (u32, u32), f: impl Fn(usize, usize) -> f64) -> Vec<f64> {
        let (w, h) = (size.0 as usize, size.1 as usize);
        assert!(w % 8 == 0 && h % 8 == 0, "{w}x{h} is not whole tiles");
        let mut out = Vec::with_capacity(w / 8 * (h / 8));
        for ty in 0..h / 8 {
            for tx in 0..w / 8 {
                let mut sum = 0.0;
                for y in 0..8 {
                    for x in 0..8 {
                        sum += f(tx * 8 + x, ty * 8 + y);
                    }
                }
                out.push(sum / 64.0);
            }
        }
        out
    }

    /// The largest difference between neighbouring tile means (right and below).
    fn max_adjacent_step(size: (u32, u32), means: &[f64]) -> f64 {
        let (nx, ny) = (size.0 as usize / 8, size.1 as usize / 8);
        let mut worst: f64 = 0.0;
        for ty in 0..ny {
            for tx in 0..nx {
                let m = means[ty * nx + tx];
                if tx + 1 < nx {
                    worst = worst.max((means[ty * nx + tx + 1] - m).abs());
                }
                if ty + 1 < ny {
                    worst = worst.max((means[(ty + 1) * nx + tx] - m).abs());
                }
            }
        }
        worst
    }

    fn max_tile_error(a: &[f64], b: &[f64]) -> f64 {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max)
    }

    /// The dump's `gradient-linear-fade` (the whole frame is the opaque fade): every 8x8 tile mean
    /// is the exact ramp's, the steps between tiles are at most 1 LSB, and the same ramp rounded
    /// on the CPU fails the check, so the check can see bands. It also shows that the dither
    /// reaches the `Bgra8Unorm` pipelines the dump uses.
    #[test]
    fn the_dumped_linear_fade_has_no_bands() {
        let gpu = match pollster::block_on(Gpu::new(gpu::instance(None), None)) {
            Ok(g) => g,
            Err(e) => {
                assert!(
                    !std::env::var("LUNEUI_REQUIRE_GPU").is_ok_and(|v| v == "1"),
                    "no GPU adapter: {e}"
                );
                println!("skipped: no GPU adapter");
                return;
            }
        };
        let mut r = offscreen_renderer(gpu);
        let (size, img) = render_gradient_scene(&mut r, "gradient-linear-fade");
        let (w, h) = (size.0 as usize, size.1 as usize);
        let exact = |y: usize, ch: usize| {
            FADE_FROM[ch] + (FADE_TO[ch] - FADE_FROM[ch]) * ((y as f64 + 0.5) / h as f64)
        };

        let mut control: f64 = 0.0;
        for ch in 0..3 {
            let want = tile_means(size, |_, y| exact(y, ch));
            let got = tile_means(size, |x, y| f64::from(img[(y * w + x) * 4 + ch]));
            let err = max_tile_error(&got, &want);
            assert!(err <= 0.2, "channel {ch}: tile mean error {err:.4} LSB");
            let step = max_adjacent_step(size, &got);
            assert!(
                step <= 1.0,
                "channel {ch}: adjacent tile step {step:.4} LSB"
            );
            let rounded = tile_means(size, |_, y| exact(y, ch).round());
            control = control.max(max_tile_error(&rounded, &want));
        }
        assert!(
            control > 0.4,
            "the CPU-rounded control's tile error is only {control:.4} LSB"
        );
        for p in img.chunks_exact(4) {
            assert_eq!(p[3], 255);
        }
    }
}
