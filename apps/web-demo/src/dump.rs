//! `web-demo --dump <dir>`: the scenes as PNGs, rendered headless (no window), for a person or a
//! script that wants to see them without a browser. The app's time is a fixed clock, so two dumps of
//! the same state are the same bytes.
//!
//! With no other flag, every scene at 1440x900 points at scale 2 (the wide frame) and at 390x844
//! points at scale 3 with a phone's safe area (the narrow one). `--only <scene>` writes one image
//! with the flags that shape it: the window, the scale, the app time, a push in progress and the
//! motion tier.

use std::path::Path;
use std::time::Duration;

use lune_ui::dump;
use lune_ui::frame::FrameRenderer;
use lune_ui::gpu::{self, Gpu};
use lune_ui::layout::Insets;
use lune_ui::motion::MotionTier;
use lune_ui::scene::Scene;
use lune_ui::text::TextCache;

use crate::frame_params;
use crate::scenes::SceneId;
use crate::shell::tier_from_word;
use crate::stage::{Headless, NARROW_BELOW, STILL_AT};

/// The wide frame's window and scale.
pub const WIDE: ((f32, f32), f32) = ((1440.0, 900.0), 2.0);
/// The narrow frame's window and scale.
pub const NARROW: ((f32, f32), f32) = ((390.0, 844.0), 3.0);
/// A phone's safe area: the notch above, the home indicator below.
pub const SAFE: Insets = Insets::new(0.0, 59.0, 0.0, 34.0);
/// The app time a dump is taken at when the flags name none: the instant the still background
/// rests at.
pub const AT: Duration = STILL_AT;

/// One image to render.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The scene at rest, or the one a push starts from.
    pub scene: SceneId,
    /// The window in logical points.
    pub size: (f32, f32),
    /// Device pixels per point.
    pub scale: f32,
    /// The app time.
    pub at: Duration,
    /// A push towards this scene, at this share of its duration (0 to 1).
    pub to: Option<(SceneId, f32)>,
    /// The motion tier.
    pub tier: MotionTier,
    /// The background rests (`--still`): the strips of a push then differ only by the push.
    pub still: bool,
    /// The file name without `.png`.
    pub name: String,
}

/// The safe area a window of `size` gets: a phone's on a tall narrow one.
pub fn safe_for(size: (f32, f32)) -> Insets {
    if size.0 < NARROW_BELOW && size.1 >= 800.0 {
        SAFE
    } else {
        Insets::ZERO
    }
}

/// The request the flags describe: `Some(None)` for none of them, `None` for a bad one.
pub fn parse_request(args: impl Iterator<Item = String>) -> Option<Option<Request>> {
    let mut req = Request {
        scene: SceneId::Glass,
        size: WIDE.0,
        scale: WIDE.1,
        at: AT,
        to: None,
        tier: MotionTier::Full,
        still: false,
        name: String::new(),
    };
    let (mut any, mut only, mut to, mut progress, mut scale_given) =
        (false, false, None, None, false);
    let mut args = args;
    while let Some(arg) = args.next() {
        any = true;
        if arg == "--still" {
            req.still = true;
            continue;
        }
        let value = args.next()?;
        match arg.as_str() {
            "--only" => {
                req.scene = SceneId::from_slug(&value)?;
                only = true;
            }
            "--window" => {
                let (w, h) = value.split_once('x')?;
                let (w, h): (u32, u32) = (w.parse().ok()?, h.parse().ok()?);
                if !(1..=4096).contains(&w) || !(1..=4096).contains(&h) {
                    return None;
                }
                req.size = (w as f32, h as f32);
            }
            "--scale" => {
                req.scale = value.parse().ok().filter(|s| (0.5..=4.0).contains(s))?;
                scale_given = true;
            }
            "--at" => req.at = Duration::try_from_secs_f64(value.parse().ok()?).ok()?,
            "--to" => to = Some(SceneId::from_slug(&value)?),
            "--progress" => progress = Some(value.parse::<f32>().ok()?.clamp(0.0, 100.0) / 100.0),
            "--motion" => req.tier = tier_from_word(&value)?,
            "--name" => req.name = value,
            _ => return None,
        }
    }
    if !any {
        return Some(None);
    }
    if !only || to.is_some() != progress.is_some() {
        return None;
    }
    if !scale_given && req.size.0 < NARROW_BELOW {
        req.scale = NARROW.1;
    }
    req.to = to.zip(progress);
    if req.name.is_empty() {
        req.name = req.scene.slug().to_owned();
    }
    Some(Some(req))
}

/// Renders `req` into RGBA bytes and their size in pixels.
fn render(
    gpu: &mut Gpu,
    run: &mut Headless,
    req: &Request,
) -> Result<(Vec<u8>, (u32, u32)), String> {
    run.stage.tier = req.tier;
    run.stage.still = req.still;
    run.stage.want = req.scene;
    run.stage.backend = "headless".to_owned();
    let rendered = match req.to {
        None => run.step_to(req.at, Vec::new()),
        Some((to, share)) => {
            // At rest on `scene`, then the frame that starts the push, then the frame `share` of
            // the way through it.
            run.step_to(req.at, Vec::new());
            run.stage.go(to);
            run.step_to(req.at, Vec::new());
            let duration = run.stage.push_duration().unwrap_or(Duration::ZERO);
            run.step_to(req.at + duration.mul_f32(share), Vec::new())
        }
    };
    draw(gpu, &mut run.text, &rendered.scene, req.size, req.scale)
}

/// Writes `<name>.png` of `req` into `dir` (made if it is missing).
pub fn write_one(dir: &Path, req: &Request) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut gpu = pollster::block_on(Gpu::new(gpu::instance(None), None))?;
    let mut run = Headless::new(req.size, req.scale, safe_for(req.size));
    let (rgba, size) = render(&mut gpu, &mut run, req)?;
    dump::write_png(&dir.join(format!("{}.png", req.name)), size, &rgba)
}

/// Writes `wide-<scene>.png` and `narrow-<scene>.png` for every scene into `dir`.
pub fn write_all(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut gpu = pollster::block_on(Gpu::new(gpu::instance(None), None))?;
    for (prefix, (size, scale)) in [("wide", WIDE), ("narrow", NARROW)] {
        for scene in SceneId::ALL {
            let req = Request {
                scene,
                size,
                scale,
                at: AT,
                to: None,
                tier: MotionTier::Full,
                still: false,
                name: format!("{prefix}-{}", scene.slug()),
            };
            let mut run = Headless::new(size, scale, safe_for(size));
            let (rgba, px) = render(&mut gpu, &mut run, &req)?;
            dump::write_png(&dir.join(format!("{}.png", req.name)), px, &rgba)?;
        }
    }
    Ok(())
}

/// Draws `scene` into an offscreen target of `size` points at `scale` and returns its RGBA bytes.
fn draw(
    gpu: &mut Gpu,
    text: &mut TextCache,
    scene: &Scene,
    size: (f32, f32),
    scale: f32,
) -> Result<(Vec<u8>, (u32, u32)), String> {
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let mut renderer = FrameRenderer::new(gpu, format);
    let px = (
        (size.0 * scale).round() as u32,
        (size.1 * scale).round() as u32,
    );
    let target = dump::target(&gpu.device, format, px);
    let view = target.create_view(&Default::default());
    let params = frame_params(px, scale);
    // Twice: the first draw fills the glyph atlas, the second is the one to keep.
    for _ in 0..2 {
        renderer.draw(gpu, text, scene, &view, params)?;
        gpu.end_frame(text);
    }
    Ok((dump::read_rgba(&gpu.device, &gpu.queue, &target)?, px))
}
