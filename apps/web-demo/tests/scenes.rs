//! The six scenes, headless: no window, no GPU, app time only. CI runs these
//! on all three systems.

use std::time::Duration;

use lune_ui::input::{Event, Key, Modifiers, NamedKey, PointerButton, Repaint};
use lune_ui::layout::Insets;
use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::playback::{Player, Script};
use web_demo::scenes::controls::{self, Card as CCard};
use web_demo::scenes::data::{self, GROW};
use web_demo::scenes::glass::{self, Card as GCard, PARALLAX_MAX};
use web_demo::scenes::motion;
use web_demo::scenes::text::{self, Act, Part};
use web_demo::scenes::theme::{self, Accent, Look, Preset, contrast, lerp_theme};
use web_demo::scenes::{RESUME, SceneId};
use web_demo::stage::{Headless, Layout};

const FRAME: Duration = Duration::from_micros(16_667);
const PHONE_SAFE: Insets = Insets::new(0.0, 59.0, 0.0, 34.0);
/// The four sizes of the stage tests: two wide and two narrow.
const SIZES: [((f32, f32), Insets); 4] = [
    ((1440.0, 900.0), Insets::ZERO),
    ((1024.0, 768.0), Insets::ZERO),
    ((390.0, 844.0), PHONE_SAFE),
    ((320.0, 568.0), Insets::ZERO),
];
const SCENES: [SceneId; 6] = SceneId::ALL;

fn inside(outer: Rect, inner: Rect) -> bool {
    const EPS: f32 = 0.01;
    inner.x >= outer.x - EPS
        && inner.y >= outer.y - EPS
        && inner.right() <= outer.right() + EPS
        && inner.bottom() <= outer.bottom() + EPS
}

fn apart(a: Rect, b: Rect) -> bool {
    a.right() <= b.x + 0.01
        || b.right() <= a.x + 0.01
        || a.bottom() <= b.y + 0.01
        || b.bottom() <= a.y + 0.01
}

fn body_of(size: (f32, f32), safe: Insets) -> (Rect, bool, f32) {
    let l = Layout::new(size, safe);
    let t = lune_ui::lunegit_dark::THEME;
    let pad = l.pad();
    (l.body(&t), l.wide, pad)
}

/// A still stage on `scene` at `size` that has drawn a few frames.
fn still(size: (f32, f32), safe: Insets, scene: SceneId) -> Headless {
    let mut run = Headless::new(size, 2.0, safe);
    run.stage.still = true;
    run.stage.want = scene;
    run.run(6, FRAME);
    run
}

#[test]
fn every_scene_lays_out_inside_the_body_at_the_four_sizes() {
    for (size, safe) in SIZES {
        let (body, wide, pad) = body_of(size, safe);
        let view = Rect::new(body.x - pad, body.y, body.w + 2.0 * pad, body.h + pad);

        // Glass: at rest each card is inside the body, and its bob and the pointer's pull (at most
        // the card's own bound) keep it inside the scroll view.
        let plan = glass::plan(body, wide);
        assert!(plan.height >= body.h - 0.01, "{size:?}: glass is short");
        let tall = Rect::new(view.x, view.y, view.w, plan.height + pad);
        for (card, r) in &plan.cards {
            let b = card.bob();
            let reach = (
                PARALLAX_MAX * card.depth() + b.amp.0,
                PARALLAX_MAX * card.depth() + b.amp.1,
            );
            let moved = Rect::new(
                r.x - reach.0,
                r.y - reach.1,
                r.w + 2.0 * reach.0,
                r.h + 2.0 * reach.1,
            );
            assert!(
                inside(tall, moved),
                "{size:?}: glass {card:?} leaves the view: {moved:?} in {tall:?}"
            );
            assert!(r.w > 200.0 && r.h > 120.0, "{size:?}: {card:?} is {r:?}");
        }
        for (i, (ka, a)) in plan.cards.iter().enumerate() {
            for (kb, b) in &plan.cards[i + 1..] {
                assert!(apart(*a, *b), "{size:?}: glass {ka:?} and {kb:?} overlap");
            }
        }

        // Controls: every card in the body, every control inside its card.
        let plan = controls::plan(body, wide);
        let g = controls::geometry(body, wide);
        let full = Rect::new(body.x, body.y, body.w, plan.height);
        for (card, r) in &plan.cards {
            assert!(
                inside(full, *r),
                "{size:?}: controls {card:?} outside: {r:?} in {full:?}"
            );
        }
        for (i, (ka, a)) in plan.cards.iter().enumerate() {
            for (kb, b) in &plan.cards[i + 1..] {
                assert!(
                    apart(*a, *b),
                    "{size:?}: controls {ka:?} and {kb:?} overlap"
                );
            }
        }
        let inner = |c: CCard| {
            g.inner
                .iter()
                .find(|(k, _)| *k == c)
                .map(|(_, r)| *r)
                .expect("a card")
        };
        for r in g.toggles {
            assert!(inside(inner(CCard::Toggles), r), "{size:?}: toggle {r:?}");
        }
        for r in g.sliders {
            assert!(inside(inner(CCard::Sliders), r), "{size:?}: slider {r:?}");
        }
        assert!(
            inside(inner(CCard::Stepper), g.stepper),
            "{size:?}: stepper"
        );
        let stars = Rect::new(g.stars.x, g.stars.y, 128.0, 24.0);
        assert!(
            inside(inner(CCard::Stepper), stars),
            "{size:?}: stars {stars:?}"
        );
        assert!(
            apart(g.stepper, stars),
            "{size:?}: stepper and stars overlap"
        );
        let radios = g.radios;
        assert!(inside(inner(CCard::View), radios), "{size:?}: radios");
        assert!(inside(inner(CCard::View), g.preview), "{size:?}: preview");
        assert!(
            apart(radios, g.preview),
            "{size:?}: radios over the preview"
        );
        assert!(
            g.preview.w > 100.0 && g.preview.h > 60.0,
            "{size:?}: preview {:?}",
            g.preview
        );
        for r in g.checks.into_iter().chain(g.buttons) {
            assert!(inside(inner(CCard::Options), r), "{size:?}: option {r:?}");
        }
        assert!(
            apart(g.checks[2], g.buttons[0]),
            "{size:?}: a check over the buttons"
        );

        // Motion: four frames of one size, each with its caption row above it, all in the body.
        let plan = motion::plan(body, wide);
        let full = Rect::new(body.x, body.y, body.w, plan.height);
        let first = plan.frames[0].1;
        for (demo, r) in &plan.frames {
            assert!(
                inside(full, *r),
                "{size:?}: motion {demo:?} outside: {r:?} in {full:?}"
            );
            assert_eq!(
                (r.w, r.h),
                (first.w, first.h),
                "{size:?}: {demo:?} is not the frames' size"
            );
            assert!(
                r.y - body.y >= 26.0 - 0.01,
                "{size:?}: no room for {demo:?}'s caption"
            );
            let screen = motion::screen_of(*r);
            assert!(
                screen.w > 150.0 && screen.h > 180.0,
                "{size:?}: {demo:?} screen {screen:?}"
            );
        }
        for (i, (ka, a)) in plan.frames.iter().enumerate() {
            for (kb, b) in &plan.frames[i + 1..] {
                assert!(apart(*a, *b), "{size:?}: motion {ka:?} and {kb:?} overlap");
            }
        }

        // Data, Text and Theme: every card inside the body's width and the content, none over
        // another.
        let rects = |name: &str, cards: Vec<Rect>, height: f32| {
            let full = Rect::new(body.x, body.y, body.w, height);
            for (i, a) in cards.iter().enumerate() {
                assert!(
                    inside(full, *a),
                    "{size:?}: {name} card {i} outside: {a:?} in {full:?}"
                );
                assert!(
                    a.w > 100.0 && a.h > 60.0,
                    "{size:?}: {name} card {i} is {a:?}"
                );
                for (j, b) in cards.iter().enumerate().skip(i + 1) {
                    assert!(apart(*a, *b), "{size:?}: {name} cards {i} and {j} overlap");
                }
            }
        };
        let p = data::plan(body, wide);
        rects("data", p.cards.iter().map(|c| c.1).collect(), p.height);
        let p = text::plan(body, wide);
        rects("text", p.cards.iter().map(|c| c.1).collect(), p.height);
        let p = theme::plan(body, wide);
        rects("theme", p.cards.iter().map(|c| c.1).collect(), p.height);

        // At the biggest window nothing scrolls.
        if size == (1440.0, 900.0) {
            for (name, h) in [
                ("glass", glass::plan(body, wide).height),
                ("controls", controls::plan(body, wide).height),
                ("motion", motion::plan(body, wide).height),
                ("data", data::plan(body, wide).height),
                ("text", text::plan(body, wide).height),
                ("theme", theme::plan(body, wide).height),
            ] {
                assert!(
                    h <= body.h + 0.5,
                    "{name} scrolls at {size:?}: {h} in {}",
                    body.h
                );
            }
        }
    }
}

#[test]
fn every_scene_draws_at_every_size_and_the_scenes_differ() {
    for (size, safe) in SIZES {
        let mut seen = Vec::new();
        for scene in SCENES {
            let mut run = still(size, safe, scene);
            let r = run.run(1, FRAME).expect("a frame");
            assert!(
                r.scene != lune_ui::scene::Scene::default(),
                "{size:?} {scene:?}: nothing drawn"
            );
            seen.push(r.scene);
        }
        for i in 0..seen.len() {
            for j in i + 1..seen.len() {
                assert!(
                    seen[i] != seen[j],
                    "{size:?}: {:?} = {:?}",
                    SCENES[i],
                    SCENES[j]
                );
            }
        }
    }
}

/// Plays `script` against `run`, one frame at a time, for `frames` frames.
fn play(run: &mut Headless, script: Script, frames: u32) {
    let mut player = Player::new(script);
    for _ in 0..frames {
        let mut events = Vec::new();
        player.advance(run.time + FRAME, &mut events);
        run.step(FRAME, events);
    }
}

fn tap_at(run: &mut Headless, at: (f32, f32)) {
    let mut s = Script::new();
    s.wait(Duration::from_millis(60)).tap(at);
    play(run, s, 40);
}

/// A tap (touch) on the control `name` of `scene` as its last frame drew it.
fn tap(run: &mut Headless, scene: SceneId, name: &str) {
    let at = run
        .stage
        .scenes
        .targets(scene)
        .and_then(|t| t.centre(name))
        .unwrap_or_else(|| panic!("{scene:?} drew no {name}"));
    tap_at(run, at);
}

fn key(run: &mut Headless, k: NamedKey) {
    for pressed in [true, false] {
        run.step(
            FRAME,
            vec![Event::Key {
                key: Key::Named(k),
                pressed,
                repeat: false,
                modifiers: Modifiers::default(),
            }],
        );
    }
    run.run(3, FRAME);
}

#[test]
fn a_tap_on_each_glass_control_changes_its_state() {
    let mut run = still((1440.0, 900.0), Insets::ZERO, SceneId::Glass);
    let g = |run: &Headless| {
        let g = &run.stage.scenes.glass;
        (g.playing, g.song, g.wifi, g.focus, g.sync, g.dialog)
    };
    let before = g(&run);
    tap(&mut run, SceneId::Glass, "play");
    assert_eq!(g(&run).0, !before.0, "play");
    tap(&mut run, SceneId::Glass, "play");
    assert_eq!(g(&run).0, before.0, "pause again");
    tap(&mut run, SceneId::Glass, "next");
    assert_eq!(g(&run).1, 1, "next song");
    run.stage.scenes.glass.track = 0.6;
    tap(&mut run, SceneId::Glass, "back");
    assert_eq!(run.stage.scenes.glass.track, 0.0, "back to the start");
    tap(&mut run, SceneId::Glass, "wifi");
    assert_eq!(g(&run).2, !before.2, "wifi");
    tap(&mut run, SceneId::Glass, "focus");
    assert_eq!(g(&run).3, !before.3, "focus");
    tap(&mut run, SceneId::Glass, "sync");
    assert_eq!(g(&run).4, before.4 + 1, "sync");
    // The dialog opens from the button and closes on Escape.
    let closed = run.run(1, FRAME).expect("a frame").scene;
    tap(&mut run, SceneId::Glass, "open");
    assert!(g(&run).5, "the dialog opened");
    let open = run.run(1, FRAME).expect("a frame").scene;
    assert!(open != closed, "an open dialog draws");
    key(&mut run, NamedKey::Escape);
    run.run(30, FRAME);
    assert!(!g(&run).5, "Escape closes it");
}

#[test]
fn a_pointer_click_works_as_well_as_a_touch() {
    let mut run = still((1440.0, 900.0), Insets::ZERO, SceneId::Glass);
    let at = run
        .stage
        .scenes
        .glass
        .targets
        .centre("wifi")
        .expect("a toggle");
    let was = run.stage.scenes.glass.wifi;
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    };
    run.step(FRAME, vec![Event::PointerMoved { pos: at }]);
    run.step(FRAME, vec![button(true)]);
    run.step(FRAME, vec![button(false)]);
    run.run(3, FRAME);
    assert_eq!(run.stage.scenes.glass.wifi, !was);
}

#[test]
fn a_tap_on_each_control_changes_its_state() {
    let mut run = still((1440.0, 900.0), Insets::ZERO, SceneId::Controls);
    let c = SceneId::Controls;
    for i in 0..3 {
        let was = run.stage.scenes.controls.toggles[i];
        tap(&mut run, c, &format!("toggle{i}"));
        assert_eq!(run.stage.scenes.controls.toggles[i], !was, "toggle {i}");
    }
    for i in 0..3 {
        let was = run.stage.scenes.controls.sliders[i];
        let r = run
            .stage
            .scenes
            .controls
            .targets
            .get(&format!("slider{i}"))
            .expect("a slider");
        let right = (r.x + r.w * 0.95, r.y + r.h / 2.0);
        let left = (r.x + r.w * 0.05, r.y + r.h / 2.0);
        // A touch in a scroll area drags a slider (a tap there is a scroll's start).
        let (from, to) = if was < 0.5 {
            (left, right)
        } else {
            (right, left)
        };
        let mut s = Script::new();
        s.wait(Duration::from_millis(60))
            .drag(from, to, Duration::from_millis(300));
        play(&mut run, s, 50);
        let now = run.stage.scenes.controls.sliders[i];
        assert!((now - was).abs() > 0.2, "slider {i}: {was} -> {now}");
    }
    let q = run.stage.scenes.controls.qty;
    tap(&mut run, c, "plus");
    assert_eq!(run.stage.scenes.controls.qty, q + 1, "stepper plus");
    tap(&mut run, c, "minus");
    tap(&mut run, c, "minus");
    assert_eq!(run.stage.scenes.controls.qty, q - 1, "stepper minus");
    tap(&mut run, c, "star2");
    assert_eq!(run.stage.scenes.controls.stars, 2, "second star");
    tap(&mut run, c, "radio2");
    assert_eq!(run.stage.scenes.controls.view, 2, "radio");
    for i in 0..3 {
        let was = run.stage.scenes.controls.checks[i];
        tap(&mut run, c, &format!("check{i}"));
        assert_eq!(run.stage.scenes.controls.checks[i], !was, "check {i}");
    }
    tap(&mut run, c, "apply");
    assert_eq!(run.stage.scenes.controls.applied, 1, "apply");
    tap(&mut run, c, "reset");
    assert_eq!(run.stage.scenes.controls.applied, 0, "reset");
    assert_eq!(
        run.stage.scenes.controls.checks,
        [false, true, true],
        "reset"
    );
}

#[test]
fn a_tap_or_a_swipe_on_each_motion_demo_changes_its_state() {
    let mut run = still((1440.0, 900.0), Insets::ZERO, SceneId::Motion);
    let m = SceneId::Motion;
    // The pager: a swipe across it moves a page.
    let r = run
        .stage
        .scenes
        .motion
        .targets
        .get("pager")
        .expect("a pager");
    let y = r.y + r.h * 0.4;
    let mut s = Script::new();
    s.wait(Duration::from_millis(60)).swipe(
        (r.x + r.w * 0.85, y),
        (r.x + r.w * 0.15, y),
        Duration::from_millis(180),
    );
    play(&mut run, s, 60);
    assert_eq!(run.stage.scenes.motion.page, 1, "swiped to the second page");
    // The sheet: a tap on the grabber steps to the next detent, and a drag follows the hand.
    let was = run.stage.scenes.motion.detent;
    tap(&mut run, m, "grabber");
    assert_eq!(
        run.stage.scenes.motion.detent,
        (was + 1) % 3,
        "grabber step"
    );
    let g = run
        .stage
        .scenes
        .motion
        .targets
        .centre("grabber")
        .expect("a grabber");
    let mut s = Script::new();
    s.wait(Duration::from_millis(60))
        .drag(g, (g.0, g.1 + 420.0), Duration::from_millis(300));
    play(&mut run, s, 80);
    assert_eq!(
        run.stage.scenes.motion.detent, 0,
        "dragged down to the peek"
    );
    // The stack: a tap on a row pushes the detail; its back button pops it.
    tap(&mut run, m, "row0");
    assert_eq!(run.stage.scenes.motion.depth, 2, "pushed");
    assert_eq!(run.stage.scenes.motion.opened, 1);
    // The real sheet opens from its button.
    tap(&mut run, m, "real-open");
    assert!(
        run.stage.scenes.motion.real_sheet,
        "the toolkit's sheet opened"
    );
}

#[test]
fn tab_reaches_the_controls_in_reading_order() {
    let mut run = still((1440.0, 900.0), Insets::ZERO, SceneId::Controls);
    // The rail is the first tab stop (one stop for its six rows); then the scene's controls follow
    // in reading order, so the next three stops are the three toggles. Tab then Space acts.
    key(&mut run, NamedKey::Tab);
    let before = run.stage.scenes.controls.toggles;
    for n in 0..3 {
        key(&mut run, NamedKey::Tab);
        key(&mut run, NamedKey::Space);
        let now = run.stage.scenes.controls.toggles;
        let changed: Vec<usize> = (0..3).filter(|i| now[*i] != before[*i]).collect();
        assert_eq!(
            changed.len(),
            n + 1,
            "after {} tabs the toggles are {now:?}",
            n + 1
        );
    }
}

/// Runs `scene` on a moving stage for `secs` seconds of app time in 100 ms steps; returns the
/// stage and the repaint request of every frame.
fn run_for(scene: SceneId, secs: u64) -> (Headless, Vec<Repaint>) {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.want = scene;
    let mut asked = Vec::new();
    for _ in 0..secs * 10 {
        asked.push(run.step(Duration::from_millis(100), Vec::new()).out.repaint);
    }
    (run, asked)
}

#[test]
fn idle_motion_asks_for_frames_and_still_asks_for_none() {
    for scene in SCENES {
        let (_, asked) = run_for(scene, 12);
        assert!(
            asked.iter().all(|r| *r == Repaint::Now),
            "{scene:?}: a moving scene asks for every frame"
        );
        let mut run = still((1440.0, 900.0), Insets::ZERO, scene);
        for _ in 0..60 {
            let r = run.step(FRAME, Vec::new());
            assert_eq!(
                r.out.repaint,
                Repaint::Idle,
                "{scene:?}: a still scene asks for a frame"
            );
        }
        for tier in [MotionTier::Reduced, MotionTier::Off] {
            let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
            run.stage.tier = tier;
            run.stage.want = scene;
            run.run(8, FRAME);
            let r = run.step(FRAME, Vec::new());
            assert_eq!(
                r.out.repaint,
                Repaint::Idle,
                "{scene:?} {tier:?}: nothing moves"
            );
            let a = run.step_to(Duration::from_secs(5), Vec::new()).scene;
            let b = run.step_to(Duration::from_secs(50), Vec::new()).scene;
            assert!(
                a == b,
                "{scene:?} {tier:?}: the picture depends on the clock"
            );
        }
    }
}

#[test]
fn the_autopilot_changes_the_scene_and_a_still_scene_does_not_change() {
    for scene in SCENES {
        let (run, _) = run_for(scene, 20);
        let acted = run.stage.scenes.pilot(scene).expect("a pilot").acted;
        assert!(
            acted >= 6,
            "{scene:?}: the autopilot ran {acted} times in 20 s"
        );
        let mut run = still((1440.0, 900.0), Insets::ZERO, scene);
        let a = run.step_to(Duration::from_secs(3), Vec::new()).scene;
        let b = run.step_to(Duration::from_secs(300), Vec::new()).scene;
        assert!(a == b, "{scene:?}: a still scene follows the clock");
        assert_eq!(run.stage.scenes.pilot(scene).expect("a pilot").acted, 0);
    }
}

#[test]
fn a_real_input_pauses_the_autopilot_and_it_resumes_after_the_timeout() {
    let (w, h) = (1440.0, 900.0);
    for scene in SCENES {
        let mut run = Headless::new((w, h), 2.0, Insets::ZERO);
        run.stage.want = scene;
        let dt = Duration::from_millis(100);
        let acted = |run: &Headless| run.stage.scenes.pilot(scene).expect("a pilot").acted;
        for _ in 0..80 {
            run.step(dt, Vec::new());
        }
        assert!(acted(&run) > 0, "{scene:?}: the autopilot runs untouched");
        // A pointer moves over the scene (on its card, away from any control that would act).
        let on = (700.0, 120.0);
        run.step(dt, vec![Event::PointerMoved { pos: on }]);
        let held = acted(&run);
        let t0 = run.time;
        while run.time < t0 + RESUME - Duration::from_millis(600) {
            run.step(dt, Vec::new());
        }
        assert_eq!(
            acted(&run),
            held,
            "{scene:?}: the autopilot ran while the pointer was recent"
        );
        while run.time < t0 + RESUME + Duration::from_millis(2600) {
            run.step(dt, Vec::new());
        }
        assert!(
            acted(&run) > held,
            "{scene:?}: it did not resume after {RESUME:?}"
        );
    }
}

#[test]
fn the_autopilot_changes_one_thing_at_most_every_second() {
    println!("glass: a change every {:?}", glass::STEP);
    println!("controls: a change every {:?}", controls::STEP);
    println!(
        "motion: a change every {:?}, each demo every {:?}",
        motion::STEP,
        motion::STEP * motion::EVERY as u32
    );
    assert!(glass::STEP >= Duration::from_secs(1));
    assert!(controls::STEP >= Duration::from_secs(1));
    assert!(motion::STEP >= Duration::from_secs(1));
    assert!(motion::STEP * motion::EVERY as u32 >= Duration::from_secs(3));
    println!("data: a change every {:?}", data::STEP);
    println!("theme: a change every {:?}", theme::STEP);
    assert!(data::STEP >= Duration::from_secs(1));
    assert!(theme::STEP >= Duration::from_secs(2));
}

#[test]
fn the_glass_cards_bob_slowly_and_the_pull_is_bounded() {
    let mut periods = Vec::new();
    for card in GCard::ALL {
        let b = card.bob();
        println!(
            "{card:?}: depth {}, amplitude {:?} px, periods {:?} s, phases {:?} turns",
            card.depth(),
            b.amp,
            b.period,
            b.phase
        );
        assert!(
            b.amp.0 <= 8.0 && b.amp.1 <= 8.0,
            "{card:?}: amplitude {:?}",
            b.amp
        );
        for p in [b.period.0, b.period.1] {
            assert!((6.0..=12.0).contains(&p), "{card:?}: period {p}");
            periods.push(p);
        }
        // The path never leaves its amplitude, sampled over a minute.
        let mut t = 0.0;
        while t < 60.0 {
            let (x, y) = card.bob_at(t);
            assert!(x.abs() <= b.amp.0 + 1e-3 && y.abs() <= b.amp.1 + 1e-3);
            t += 0.25;
        }
    }
    periods.sort_by(f32::total_cmp);
    println!("parallax maximum: {PARALLAX_MAX} px at depth 1 (0.6 and 0.3 of it at the rest)");
    assert_eq!(PARALLAX_MAX, 16.0);
    let depths: Vec<f32> = GCard::ALL.iter().map(|c| c.depth()).collect();
    for d in [0.3, 0.6, 1.0] {
        assert!(depths.contains(&d), "no card at depth {d}");
    }
}

#[test]
fn the_pointer_pulls_the_cards_by_their_depth_and_they_spring_back() {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.want = SceneId::Glass;
    let dt = Duration::from_millis(16);
    run.run(30, dt);
    let rest = run.step(dt, Vec::new()).scene;
    // The pointer at the stage's left edge, then back out of the window.
    let l = run.layout();
    let at = (l.card.x + 4.0, l.card.y + l.card.h / 2.0);
    for _ in 0..40 {
        run.step(dt, vec![Event::PointerMoved { pos: at }]);
    }
    let pulled = run.step(dt, Vec::new()).scene;
    assert!(pulled != rest, "the pointer moved the cards");
    for _ in 0..60 {
        run.step(dt, vec![Event::PointerLeft]);
    }
    run.step(dt, Vec::new());
}

// ---------------------------------------------------------------------------------------------
// Data.

#[test]
fn a_tap_on_a_header_sorts_the_table_and_a_second_one_turns_it() {
    let mut run = still((1440.0, 900.0), Insets::ZERO, SceneId::Data);
    let d = SceneId::Data;
    let before = run.stage.scenes.data.order.clone();
    assert_eq!(
        before,
        (0..before.len()).collect::<Vec<_>>(),
        "unsorted at the start"
    );
    let requests = |i: usize| web_demo_requests(i);
    tap(&mut run, d, "head1");
    let asc = run.stage.scenes.data.order.clone();
    assert_ne!(asc, before, "the rows were not reordered");
    for w in asc.windows(2) {
        assert!(requests(w[0]) <= requests(w[1]), "not ascending: {asc:?}");
    }
    tap(&mut run, d, "head1");
    let desc = run.stage.scenes.data.order.clone();
    for w in desc.windows(2) {
        assert!(requests(w[0]) >= requests(w[1]), "not descending: {desc:?}");
    }
    assert_eq!(desc, asc.iter().rev().copied().collect::<Vec<_>>());
    // The service column sorts by name.
    tap(&mut run, d, "head0");
    let by_name = run.stage.scenes.data.order.clone();
    assert_eq!(by_name.len(), before.len());
    assert_ne!(by_name, desc);
}

/// The requests of service `i`, in the order of the scene's list (a copy of its figures).
fn web_demo_requests(i: usize) -> u32 {
    [18_420, 9_210, 15_730, 2_860, 11_380, 6_140, 4_470][i]
}

#[test]
fn the_table_rows_slide_to_their_new_places() {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.want = SceneId::Data;
    run.run(40, FRAME);
    let at = run
        .stage
        .scenes
        .targets(SceneId::Data)
        .and_then(|t| t.centre("head1"))
        .expect("a header");
    let mut s = Script::new();
    s.wait(Duration::from_millis(60)).tap(at);
    let mut player = Player::new(s);
    let mut pictures = Vec::new();
    for _ in 0..50 {
        let mut events = Vec::new();
        player.advance(run.time + FRAME, &mut events);
        pictures.push(run.step(FRAME, events).scene);
    }
    // Between the click and the settling the picture changes frame after frame (a slide), and it
    // ends still.
    let moving = pictures.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(moving >= 8, "the rows jumped: {moving} frames of motion");
}

#[test]
fn the_grow_in_plays_on_each_entry_after_the_push_lands() {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    // From the scene next to it: a push passes through the scenes in between.
    run.stage.want = SceneId::Motion;
    run.run(60, FRAME);
    let push = run.stage.push_duration().expect("a push");
    let frames = |ms: u128| (ms * 60 / 1000) as u32 + 2;
    for round in 1..=2u32 {
        run.stage.want = SceneId::Data;
        // While the page is pushed in it is held at the start.
        run.run(frames(push.as_millis() / 2), FRAME);
        assert_eq!(
            run.stage.scenes.data.grew,
            Some(Duration::ZERO),
            "round {round}: the grow-in ran during the push"
        );
        // After it has landed the clock starts, and is over after GROW.
        run.run(frames(push.as_millis() / 2 + 200), FRAME);
        let early = run.stage.scenes.data.grew.expect("growing");
        assert!(
            early > Duration::ZERO && early < GROW,
            "round {round}: {early:?} after the landing"
        );
        run.run(frames(GROW.as_millis() + 400), FRAME);
        let late = run.stage.scenes.data.grew.expect("a clock");
        assert!(late > GROW, "round {round}: {late:?}");
        assert_eq!(run.stage.scenes.data.entry.entries, round);
        // Away and back.
        run.stage.want = SceneId::Motion;
        run.run(frames(push.as_millis() + 600), FRAME);
    }
}

#[test]
fn the_grow_in_changes_the_picture_and_a_scene_at_rest_has_none() {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.want = SceneId::Data;
    run.stage.still = true;
    let rest = run.run(10, FRAME).expect("a frame").scene;
    let mut moving = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    moving.stage.want = SceneId::Data;
    moving.stage.still = false;
    let first = moving.step(FRAME, Vec::new()).scene;
    assert!(first != rest, "the first frame shows the finished charts");
    // Reduced shows the finished charts at once.
    let mut reduced = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    reduced.stage.want = SceneId::Data;
    reduced.stage.tier = MotionTier::Reduced;
    assert_eq!(reduced.step(FRAME, Vec::new()).out.repaint, Repaint::Idle);
    assert_eq!(reduced.stage.scenes.data.grew, None);
}

#[test]
fn the_board_keeps_every_lane_filled_and_none_over_three() {
    let mut d = data::Data::default();
    let mut seen = std::collections::HashSet::new();
    for round in 0..40 {
        d.advance_board(round);
        let lanes: Vec<usize> = (0..3).map(|l| d.lane_len(l)).collect();
        assert!(
            lanes.iter().all(|n| (1..=3).contains(n)),
            "round {round}: {lanes:?}"
        );
        assert_eq!(lanes.iter().sum::<usize>(), 5);
        seen.insert(d.lanes.clone());
    }
    assert!(
        seen.len() >= 4,
        "the cards only visited {} arrangements",
        seen.len()
    );
}

// ---------------------------------------------------------------------------------------------
// Text.

#[test]
fn text_typed_through_the_events_reaches_the_field() {
    let mut run = still((1440.0, 900.0), Insets::ZERO, SceneId::Text);
    let at = run
        .stage
        .scenes
        .targets(SceneId::Text)
        .and_then(|t| t.get("field"))
        .expect("a field");
    // A tap at the right end puts the caret at the end of the text and focuses the field.
    tap_at(&mut run, (at.right() - 6.0, at.y + at.h / 2.0));
    let was = run.stage.scenes.text.field.clone();
    for ch in "web".chars() {
        run.step(FRAME, vec![Event::Text(ch.to_string())]);
    }
    run.run(3, FRAME);
    assert_eq!(run.stage.scenes.text.field, format!("{was}web"));
    // The same into the Markdown source.
    let src = run
        .stage
        .scenes
        .targets(SceneId::Text)
        .and_then(|t| t.get("source"))
        .expect("a source");
    tap_at(&mut run, (src.x + src.w / 2.0, src.bottom() - 10.0));
    let was = run.stage.scenes.text.source.clone();
    run.step(FRAME, vec![Event::Text("!".to_owned())]);
    run.run(3, FRAME);
    assert_eq!(
        run.stage.scenes.text.source.chars().count(),
        was.chars().count() + 1
    );
}

#[test]
fn the_script_types_at_a_human_pace_and_the_preview_follows() {
    // The pace, from the schedule: 40 to 80 ms a character, and a pause after a word.
    let doc = lune_ui::code_doc::CodeDoc::new(
        "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\nq\nr\ns\nt",
    );
    let mut gaps = Vec::new();
    for k in 0..4 {
        let (acts, length) = text::round(k, &doc);
        assert!(
            length > Duration::from_secs(10) && length < Duration::from_secs(40),
            "{length:?}"
        );
        let typed: Vec<(Duration, char)> = acts
            .iter()
            .filter_map(|(t, a)| match a {
                Act::Type(Part::Field | Part::Source, c) => Some((*t, *c)),
                _ => None,
            })
            .collect();
        // Only between two characters of one text: a round's parts have their own pauses.
        let part = |t: Duration| {
            acts.iter()
                .find_map(|(at, a)| match a {
                    Act::Type(p, _) if *at == t => Some(*p),
                    _ => None,
                })
                .expect("a typed character")
        };
        for w in typed.windows(2) {
            if part(w[0].0) == part(w[1].0) {
                gaps.push((w[1].0 - w[0].0, w[0].1));
            }
        }
        // Times only go forward.
        for w in acts.windows(2) {
            assert!(w[0].0 <= w[1].0);
        }
    }
    let (min, max) = gaps
        .iter()
        .filter(|(_, c)| !matches!(c, ' ' | ',' | '\n'))
        .fold((Duration::MAX, Duration::ZERO), |(lo, hi), (g, _)| {
            (lo.min(*g), hi.max(*g))
        });
    let word_max = gaps
        .iter()
        .filter(|(_, c)| matches!(c, ' ' | ',' | '\n'))
        .map(|(g, _)| *g)
        .max()
        .expect("spaces");
    println!("typing: {min:?} to {max:?} a character, up to {word_max:?} after a word");
    assert!(
        min >= text::TYPE_MIN && max <= text::TYPE_MAX,
        "{min:?} {max:?}"
    );
    assert!(word_max <= text::TYPE_MAX + text::WORD_PAUSE);
    assert!(word_max > text::TYPE_MAX);
}

#[test]
fn the_autopilot_types_into_the_field_and_the_source_and_the_preview_follows() {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.want = SceneId::Text;
    let mut fields = std::collections::HashSet::new();
    let mut sources = std::collections::HashSet::new();
    for _ in 0..(40 * 10) {
        run.step(Duration::from_millis(100), Vec::new());
        fields.insert(run.stage.scenes.text.field.clone());
        sources.insert(run.stage.scenes.text.source.clone());
    }
    assert!(
        fields.len() > 8,
        "the field only held {} texts",
        fields.len()
    );
    assert!(
        sources.len() > 8,
        "the source only held {} texts",
        sources.len()
    );
}

#[test]
fn the_code_view_shows_a_selection_the_script_makes() {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.want = SceneId::Text;
    let doc = lune_ui::code_doc::CodeDoc::new("x");
    let (acts, _) = text::round(0, &doc);
    // Find when the script selects a line, and draw just before and just after.
    let at = acts
        .iter()
        .find_map(|(t, a)| matches!(a, Act::SelectLine(_)).then_some(*t))
        .expect("a selection");
    let dt = Duration::from_millis(50);
    while run.time + dt < at + Duration::from_millis(400) {
        run.step(dt, Vec::new());
    }
    let selected = run.step(dt, Vec::new()).scene;
    // Later the script has moved on to the source; the picture differs.
    for _ in 0..80 {
        run.step(dt, Vec::new());
    }
    let later = run.step(dt, Vec::new()).scene;
    assert!(selected != later);
}

// ---------------------------------------------------------------------------------------------
// Theme.

/// Steps a stage `secs` seconds in 16 ms frames.
fn settle_for(run: &mut Headless, secs: f32) {
    let end = run.time + Duration::from_secs_f32(secs);
    while run.time < end {
        run.step(Duration::from_millis(16), Vec::new());
    }
}

#[test]
fn the_theme_run_settles_exactly_on_the_target_and_asks_no_frame_after() {
    for (preset, accent) in [
        (Preset::NearBlack, Accent::Own),
        (Preset::Dark, Accent::Link),
        (Preset::NearBlack, Accent::Err),
    ] {
        let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
        run.stage.still = true;
        run.stage.want = SceneId::Theme;
        run.run(20, FRAME);
        assert!(run.stage.theme_settled());
        run.stage.scenes.theme.look = Look { preset, accent };
        let mut asked = 0;
        let mut mid = None;
        let start = run.time;
        let token = run.stage.push_duration().expect("a token");
        while run.time < start + token + Duration::from_millis(200) {
            let r = run.step(Duration::from_millis(16), Vec::new());
            if r.out.repaint != Repaint::Idle {
                asked += 1;
            }
            if mid.is_none() && run.time >= start + token / 2 {
                mid = Some(run.stage.theme);
            }
        }
        let target = Look { preset, accent }.theme();
        assert_eq!(
            run.stage.theme, target,
            "{preset:?} {accent:?}: not exactly the target"
        );
        assert!(run.stage.theme_settled());
        let from = Look::DEFAULT.theme();
        let mid = mid.expect("a mid frame");
        assert!(
            mid != from && mid != target,
            "{preset:?} {accent:?}: no in-between theme"
        );
        // It asked for frames during the run and for none after it.
        assert!(asked > 10, "only {asked} frames asked during the run");
        for _ in 0..60 {
            let r = run.step(Duration::from_millis(16), Vec::new());
            assert_eq!(
                r.out.repaint,
                Repaint::Idle,
                "{preset:?} {accent:?}: a frame after the run"
            );
        }
    }
}

#[test]
fn the_theme_run_lasts_moderate_and_fast_under_reduced_and_cuts_under_off() {
    let moderate = lune_ui::lunegit_dark::THEME.motion.moderate;
    let fast = lune_ui::lunegit_dark::THEME.motion.fast;
    println!(
        "theme run: {moderate:?} at Full (sine-in-out), {fast:?} under Reduced, a cut under Off"
    );
    for (tier, want) in [
        (MotionTier::Full, Some(moderate)),
        (MotionTier::Reduced, Some(fast)),
        (MotionTier::Off, None),
    ] {
        let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
        run.stage.still = true;
        run.stage.tier = tier;
        run.stage.want = SceneId::Theme;
        run.run(10, FRAME);
        assert_eq!(run.stage.push_duration(), want, "{tier:?}");
        let look = Look {
            preset: Preset::NearBlack,
            accent: Accent::Gold,
        };
        run.stage.scenes.theme.look = look;
        run.step(FRAME, Vec::new());
        match want {
            Some(d) => {
                assert!(!run.stage.theme_settled(), "{tier:?}: no run");
                settle_for(&mut run, d.as_secs_f32() + 0.1);
            }
            None => assert!(run.stage.theme_settled(), "Off did not cut"),
        }
        assert_eq!(run.stage.theme, look.theme(), "{tier:?}");
    }
}

#[test]
fn only_colours_interpolate_and_the_sizes_switch_at_once() {
    let a = lune_ui::lunegit_dark::THEME;
    let mut b = lune_ui::near_black::THEME;
    b.font_base = 17.0;
    b.font_s = 12.0;
    b.item_radius = 9.0;
    b.line_title = 30.0;
    let m = lerp_theme(&a, &b, 0.5);
    assert_eq!(
        (m.font_base, m.font_s, m.item_radius, m.line_title),
        (17.0, 12.0, 9.0, 30.0)
    );
    assert_eq!(m.motion, b.motion);
    // The colours are in between, channel by channel.
    for (x, (y, z)) in [
        (a.fg_primary, (b.fg_primary, m.fg_primary)),
        (a.menu_glass, (b.menu_glass, m.menu_glass)),
    ] {
        for i in 0..4 {
            let (lo, hi) = (x[i].min(y[i]), x[i].max(y[i]));
            assert!(z[i] >= lo && z[i] <= hi, "{x:?} {y:?} {z:?}");
        }
    }
    assert!(m.menu_glass != a.menu_glass && m.menu_glass != b.menu_glass);
    assert!(m.glass.panel.material_opacity > b.glass.panel.material_opacity);
    assert!(m.glass.panel.material_opacity < a.glass.panel.material_opacity);
    // Exact at both ends.
    assert_eq!(lerp_theme(&a, &b, 1.0), b);
    let zero = lerp_theme(&a, &b, 0.0);
    assert_eq!(zero.menu_glass, a.menu_glass);
    assert_eq!(zero.glass, a.glass);
    // The two presets differ in the glass and the menu fill, and in nothing else.
    let n = lune_ui::near_black::THEME;
    assert_eq!(lerp_theme(&a, &n, 1.0), n);
}

#[test]
fn a_swatch_and_the_preset_radio_choose_the_look() {
    let mut run = still((1440.0, 900.0), Insets::ZERO, SceneId::Theme);
    let t = SceneId::Theme;
    assert_eq!(run.stage.scenes.theme.look, Look::DEFAULT);
    tap(&mut run, t, "preset-near");
    assert_eq!(run.stage.scenes.theme.look.preset, Preset::NearBlack);
    tap(&mut run, t, "accent3");
    assert_eq!(run.stage.scenes.theme.look.accent, Accent::Gold);
    tap(&mut run, t, "preset-dark");
    assert_eq!(run.stage.scenes.theme.look.preset, Preset::Dark);
    // The stage follows.
    settle_for(&mut run, 1.0);
    assert_eq!(
        run.stage.theme,
        Look {
            preset: Preset::Dark,
            accent: Accent::Gold
        }
        .theme()
    );
    // The sample controls answer too.
    let on = run.stage.scenes.theme.toggle;
    tap(&mut run, t, "toggle");
    assert_eq!(run.stage.scenes.theme.toggle, !on);
    tap(&mut run, t, "mail2");
    assert_eq!(run.stage.scenes.theme.selected, 2);
}

#[test]
fn the_look_holds_across_every_scene() {
    let look = Look {
        preset: Preset::NearBlack,
        accent: Accent::Ok,
    };
    let mut run = still((1440.0, 900.0), Insets::ZERO, SceneId::Theme);
    run.stage.scenes.theme.look = look;
    settle_for(&mut run, 1.0);
    assert_eq!(run.stage.theme, look.theme());
    let l = run.layout();
    let _ = l;
    let mut pictures = Vec::new();
    for scene in SceneId::ALL {
        run.stage.want = scene;
        settle_for(&mut run, 1.0);
        assert_eq!(run.stage.theme, look.theme(), "{scene:?}: the theme moved");
        pictures.push(run.step(FRAME, Vec::new()).scene);
    }
    // And the same scene under the other look draws differently, on every scene.
    let mut other = still((1440.0, 900.0), Insets::ZERO, SceneId::Glass);
    for (i, scene) in SceneId::ALL.into_iter().enumerate() {
        other.stage.want = scene;
        settle_for(&mut other, 1.0);
        let p = other.step(FRAME, Vec::new()).scene;
        assert!(p != pictures[i], "{scene:?}: the look did not reach it");
    }
}

#[test]
fn the_label_on_every_accent_reads() {
    let mut worst = (f32::MAX, Preset::Dark, Accent::Own);
    for preset in Preset::ALL {
        for accent in Accent::ALL {
            let t = Look { preset, accent }.theme();
            let c = contrast(t.on_accent, t.accent);
            println!("{preset:?} {accent:?}: on_accent over accent {c:.2}:1");
            if accent != Accent::Own {
                assert!(c >= 4.5, "{preset:?} {accent:?}: {c:.2}");
            }
            if c < worst.0 {
                worst = (c, preset, accent);
            }
        }
    }
    println!("worst: {:.2}:1 ({:?} {:?})", worst.0, worst.1, worst.2);
}

#[test]
fn the_theme_autopilot_alternates_the_preset_and_the_accent() {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.want = SceneId::Theme;
    let mut looks = Vec::new();
    for _ in 0..(30 * 10) {
        run.step(Duration::from_millis(100), Vec::new());
        let l = run.stage.scenes.theme.look;
        if looks.last() != Some(&l) {
            looks.push(l);
        }
    }
    assert!(looks.len() >= 8, "{looks:?}");
    for w in looks.windows(2) {
        let changed = (w[0].preset != w[1].preset) as u8 + (w[0].accent != w[1].accent) as u8;
        assert_eq!(changed, 1, "one change at a time: {w:?}");
    }
    assert!(looks.iter().any(|l| l.preset == Preset::NearBlack));
    assert!(
        looks
            .iter()
            .map(|l| l.accent)
            .collect::<std::collections::HashSet<_>>()
            .len()
            >= 5
    );
}
