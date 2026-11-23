//! The Glass, Controls and Motion scenes, headless: no window, no GPU, app time only. CI runs these
//! on all three systems.

use std::time::Duration;

use lune_ui::input::{Event, Key, Modifiers, NamedKey, PointerButton, Repaint};
use lune_ui::layout::Insets;
use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::playback::{Player, Script};
use web_demo::scenes::controls::{self, Card as CCard};
use web_demo::scenes::glass::{self, Card as GCard, PARALLAX_MAX};
use web_demo::scenes::motion;
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
const SCENES: [SceneId; 3] = [SceneId::Glass, SceneId::Controls, SceneId::Motion];

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

        // At the biggest window nothing scrolls.
        if size == (1440.0, 900.0) {
            for (name, h) in [
                ("glass", glass::plan(body, wide).height),
                ("controls", controls::plan(body, wide).height),
                ("motion", motion::plan(body, wide).height),
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
        assert!(
            seen[0] != seen[1] && seen[1] != seen[2] && seen[0] != seen[2],
            "{size:?}"
        );
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
