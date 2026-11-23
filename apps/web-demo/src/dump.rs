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
use lune_ui::input::{
    Event, Key, Modifiers, NamedKey, ScrollDelta, TouchEvent, TouchId, TouchPhase,
};
use lune_ui::layout::Insets;
use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::scene::Scene;
use lune_ui::text::TextCache;

use crate::frame_params;
use crate::scenes::theme::{Accent, Look, Preset};
use crate::scenes::{SceneId, controls, data, glass, motion, text, theme};
use crate::shell::tier_from_word;
use crate::stage::{Headless, NARROW_BELOW, Rendered, STILL_AT};

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
    /// What a person does after the first frame, in order (`--do`).
    pub acts: Vec<Act>,
    /// Print the rects of the scene's cards (`--rects`).
    pub rects: bool,
    /// The look the theme rests on from the first frame (`--preset`, `--accent`).
    pub look: Look,
    /// The file name without `.png`.
    pub name: String,
}

/// Where an action lands: a point, or a control the scene drew (`name`, or `name@fx,fy` for a
/// point at that share of its rect).
#[derive(Debug, Clone, PartialEq)]
pub enum Spot {
    /// A point in logical px.
    Xy(f32, f32),
    /// A control by name, and the share of its rect.
    Target(String, f32, f32),
}

/// One thing a person does in a dump: a pointer move, a touch, a wheel turn, keys, or time passing.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// The mouse pointer moves to a spot.
    Move(Spot),
    /// A touch taps a spot.
    Click(Spot),
    /// A touch goes down at a spot and stays.
    Down(Spot),
    /// The held touch moves to a spot.
    MoveTo(Spot),
    /// The held touch lifts at a spot.
    Up(Spot),
    /// The wheel scrolls the page down by this many px, over the stage card.
    Scroll(f32),
    /// Tab pressed this many times.
    Tab(u32),
    /// The Escape key.
    Escape,
    /// This many milliseconds pass in 60 Hz frames.
    Wait(f32),
    /// This many seconds pass in 100 ms frames (the autopilot runs).
    Run(f32),
}

fn parse_spot(s: &str) -> Option<Spot> {
    if let Some((x, y)) = s.split_once(',') {
        if let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) {
            return Some(Spot::Xy(x, y));
        }
    }
    let (name, share) = match s.split_once('@') {
        Some((n, f)) => {
            let (fx, fy) = f.split_once(',')?;
            (n, (fx.parse().ok()?, fy.parse().ok()?))
        }
        None => (s, (0.5, 0.5)),
    };
    (!name.is_empty()).then(|| Spot::Target(name.to_owned(), share.0, share.1))
}

/// Reads `--do` text: `move:<spot>`, `click:<spot>`, `down:<spot>`, `moveto:<spot>`, `up:<spot>`,
/// `scroll:<px>`, `tab:<n>`, `escape`, `wait:<ms>`, `run:<secs>`.
pub fn parse_act(s: &str) -> Option<Act> {
    let (verb, arg) = s.split_once(':').unwrap_or((s, ""));
    Some(match verb {
        "move" => Act::Move(parse_spot(arg)?),
        "click" => Act::Click(parse_spot(arg)?),
        "down" => Act::Down(parse_spot(arg)?),
        "moveto" => Act::MoveTo(parse_spot(arg)?),
        "up" => Act::Up(parse_spot(arg)?),
        "scroll" => Act::Scroll(arg.parse().ok()?),
        "tab" => Act::Tab(arg.parse().ok()?),
        "escape" => Act::Escape,
        "wait" => Act::Wait(arg.parse().ok()?),
        "run" => Act::Run(arg.parse().ok()?),
        _ => return None,
    })
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
        acts: Vec::new(),
        rects: false,
        look: Look::DEFAULT,
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
        if arg == "--rects" {
            req.rects = true;
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
            "--do" => req.acts.push(parse_act(&value)?),
            "--preset" => {
                req.look.preset = Preset::from_word(&value)?;
            }
            "--accent" => {
                req.look.accent = Accent::from_word(&value)?;
            }
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
    // A scene is shown at rest after its first frame, as a person sees it a few seconds in; what a
    // request plays after that (`--do`) sees the entry motions as they run.
    run.stage.arrived = req.acts.is_empty();
    run.stage.set_look(req.look);
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
    let rendered = perform(run, req, rendered);
    if req.rects {
        for (name, r) in rects(run, req) {
            println!("card {name} {:.1} {:.1} {:.1} {:.1}", r.x, r.y, r.w, r.h);
        }
    }
    draw(gpu, &mut run.text, &rendered.scene, req.size, req.scale)
}

/// The rect of `spot` in the scene the request shows.
fn resolve(run: &Headless, req: &Request, spot: &Spot) -> (f32, f32) {
    match spot {
        Spot::Xy(x, y) => (*x, *y),
        Spot::Target(name, fx, fy) => {
            let r = run
                .stage
                .scenes
                .targets(req.scene)
                .and_then(|t| t.get(name))
                .unwrap_or_default();
            (r.x + r.w * fx, r.y + r.h * fy)
        }
    }
}

fn touch(phase: TouchPhase, at: (f32, f32)) -> Event {
    Event::Touch(TouchEvent::new(TouchId::new(1), phase, at))
}

const SIXTY: Duration = Duration::from_micros(16_667);

/// Plays the request's actions after its first frame; returns the last frame.
fn perform(run: &mut Headless, req: &Request, first: Rendered) -> Rendered {
    let mut last = first;
    let frame = |run: &mut Headless, dt: Duration, events: Vec<Event>| run.step(dt, events);
    for act in &req.acts {
        match act {
            Act::Move(s) => {
                let at = resolve(run, req, s);
                last = frame(run, SIXTY, vec![Event::PointerMoved { pos: at }]);
            }
            Act::Click(s) => {
                let at = resolve(run, req, s);
                frame(run, SIXTY, vec![touch(TouchPhase::Started, at)]);
                for _ in 0..3 {
                    frame(run, SIXTY, Vec::new());
                }
                frame(run, SIXTY, vec![touch(TouchPhase::Ended, at)]);
                last = frame(run, SIXTY, Vec::new());
            }
            Act::Down(s) => {
                let at = resolve(run, req, s);
                last = frame(run, SIXTY, vec![touch(TouchPhase::Started, at)]);
            }
            Act::MoveTo(s) => {
                let at = resolve(run, req, s);
                last = frame(run, SIXTY, vec![touch(TouchPhase::Moved, at)]);
            }
            Act::Up(s) => {
                let at = resolve(run, req, s);
                last = frame(run, SIXTY, vec![touch(TouchPhase::Ended, at)]);
            }
            Act::Scroll(px) => {
                let c = run.layout().card;
                let pos = (c.x + c.w / 2.0, c.y + c.h / 2.0);
                last = frame(
                    run,
                    SIXTY,
                    vec![Event::Scroll {
                        pos,
                        delta: ScrollDelta::Pixels { x: 0.0, y: -px },
                        modifiers: Modifiers::default(),
                    }],
                );
            }
            Act::Tab(n) => {
                for _ in 0..*n {
                    for pressed in [true, false] {
                        last = frame(
                            run,
                            SIXTY,
                            vec![Event::Key {
                                key: Key::Named(NamedKey::Tab),
                                pressed,
                                repeat: false,
                                modifiers: Modifiers::default(),
                            }],
                        );
                    }
                }
            }
            Act::Escape => {
                for pressed in [true, false] {
                    last = frame(
                        run,
                        SIXTY,
                        vec![Event::Key {
                            key: Key::Named(NamedKey::Escape),
                            pressed,
                            repeat: false,
                            modifiers: Modifiers::default(),
                        }],
                    );
                }
            }
            Act::Wait(ms) => {
                let end = run.time + Duration::from_secs_f32(ms / 1000.0);
                while run.time < end {
                    last = frame(run, SIXTY, Vec::new());
                }
            }
            Act::Run(secs) => {
                let end = run.time + Duration::from_secs_f32(*secs);
                while run.time < end {
                    last = frame(run, Duration::from_millis(100), Vec::new());
                }
            }
        }
    }
    last
}

/// The cards of the scene at the frame's clock: what `--rects` prints.
fn rects(run: &Headless, req: &Request) -> Vec<(String, Rect)> {
    let l = run.layout();
    let body = l.body(&run.stage.theme);
    let t = if req.still { STILL_AT } else { run.time }.as_secs_f64();
    match req.scene {
        SceneId::Glass => glass::plan(body, l.wide)
            .cards
            .into_iter()
            .map(|(c, r)| {
                let (bx, by) = c.bob_at(t);
                (format!("{c:?}"), Rect::new(r.x + bx, r.y + by, r.w, r.h))
            })
            .collect(),
        SceneId::Controls => controls::plan(body, l.wide)
            .cards
            .into_iter()
            .map(|(c, r)| (format!("{c:?}"), r))
            .collect(),
        SceneId::Motion => motion::plan(body, l.wide)
            .frames
            .into_iter()
            .map(|(d, r)| (format!("{d:?}"), r))
            .collect(),
        SceneId::Data => data::plan(body, l.wide)
            .cards
            .into_iter()
            .map(|(c, r)| (format!("{c:?}"), r))
            .collect(),
        SceneId::Text => text::plan(body, l.wide)
            .cards
            .into_iter()
            .map(|(c, r)| (format!("{c:?}"), r))
            .collect(),
        SceneId::Theme => theme::plan(body, l.wide)
            .cards
            .into_iter()
            .map(|(c, r)| (format!("{c:?}"), r))
            .collect(),
    }
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
                acts: Vec::new(),
                rects: false,
                look: Look::DEFAULT,
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
