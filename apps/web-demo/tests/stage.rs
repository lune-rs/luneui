//! The stage, headless: no window, no GPU, app time only. CI runs these on all three systems.

use std::time::Duration;

use lune_ui::input::Repaint;
use lune_ui::layout::Insets;
use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::playback::{Player, Script};
use lune_ui::scene::Scene;
use web_demo::scenes::SceneId;
use web_demo::stage::{
    BLOBS, Headless, Layout, NARROW_BELOW, RAIL_W, STILL_AT, blob_centre, blob_speed_bound,
    old_opacity, scrim, shortest_period, title_opacities,
};

const FRAME: Duration = Duration::from_micros(16_667);
const PHONE_SAFE: Insets = Insets::new(0.0, 59.0, 0.0, 34.0);
/// The windows the layout must fit: two wide and two narrow (a phone, and the smallest one).
const SIZES: [((f32, f32), Insets); 4] = [
    ((1440.0, 900.0), Insets::ZERO),
    ((1024.0, 768.0), Insets::ZERO),
    ((390.0, 844.0), PHONE_SAFE),
    ((320.0, 568.0), Insets::ZERO),
];

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

#[test]
fn the_layout_fits_at_every_size() {
    for (size, safe) in SIZES {
        let l = Layout::new(size, safe);
        let win = Rect::new(0.0, 0.0, size.0, size.1);
        assert_eq!(l.wide, size.0 >= NARROW_BELOW, "{size:?}");
        assert!(l.card.w > 0.0 && l.card.h > 0.0, "{size:?}: no card");
        assert!(inside(win, l.card), "{size:?}: card outside the window");
        let t = lune_ui::lunegit_dark::THEME;
        let body = l.body(&t);
        assert!(
            body.w > 0.0 && body.h > 0.0,
            "{size:?}: no room for a scene"
        );
        assert!(inside(l.card, body), "{size:?}: body outside the card");
        assert!(inside(l.card, l.page()), "{size:?}: page outside the card");
        if l.wide {
            assert!(inside(win, l.rail), "{size:?}: rail outside the window");
            assert_eq!(l.rail.w, RAIL_W);
            assert!(apart(l.rail, l.card), "{size:?}: rail and stage overlap");
            for (i, row) in l.rail_rows().iter().enumerate() {
                assert!(
                    inside(l.rail, *row),
                    "{size:?}: rail row {i} outside the rail"
                );
            }
            assert!(
                inside(l.rail, l.brand()) && inside(l.rail, l.footer()),
                "{size:?}"
            );
            assert!(
                apart(l.brand(), l.rail_rows()[0]),
                "{size:?}: brand over the first row"
            );
            assert!(
                apart(l.rail_rows()[5], l.footer()),
                "{size:?}: last row over the footer"
            );
        } else {
            assert!(
                inside(win, l.title_bar) && inside(win, l.tab_bar),
                "{size:?}"
            );
            assert!(
                apart(l.title_bar, l.card),
                "{size:?}: title bar over the stage"
            );
            assert!(apart(l.tab_bar, l.card), "{size:?}: tab bar over the stage");
            assert!(apart(l.title_bar, l.tab_bar), "{size:?}");
            assert!(
                l.title_row().y >= safe.top - 0.01,
                "{size:?}: title under the notch"
            );
            assert!(l.tab_bar.bottom() <= size.1 + 0.01);
            let cells = l.tab_cells();
            for (i, c) in cells.iter().enumerate() {
                assert!(inside(l.tab_bar, *c), "{size:?}: tab {i} outside the bar");
                assert!(
                    c.bottom() <= size.1 - safe.bottom + 0.01,
                    "{size:?}: tab {i} under the home bar"
                );
            }
            for pair in cells.windows(2) {
                assert!(apart(pair[0], pair[1]), "{size:?}: tabs overlap");
            }
            // A label has to fit 11 px type in a cell: at 320 wide a cell is 53 px.
            assert!(cells[0].w >= 50.0, "{size:?}: a tab is {} px", cells[0].w);
        }
    }
}

#[test]
fn every_scene_draws_at_every_size() {
    for (size, safe) in SIZES {
        let mut seen: Vec<(SceneId, Scene)> = Vec::new();
        for scene in SceneId::ALL {
            let mut run = Headless::new(size, 2.0, safe);
            run.stage.want = scene;
            let last = run.run(3, FRAME).expect("a frame");
            assert!(
                last.scene != Scene::default(),
                "{size:?} {scene:?}: nothing drawn"
            );
            assert_eq!(last.position, scene.index() as f32, "{size:?} {scene:?}");
            for (other, was) in &seen {
                assert!(
                    *was != last.scene,
                    "{size:?}: {scene:?} and {other:?} draw the same"
                );
            }
            seen.push((scene, last.scene));
        }
    }
}

/// Frames from the start of a push until the position is at its target and the frame asks for
/// nothing but the background: its count and the app time it took.
fn settle(run: &mut Headless, target: f32) -> (u32, Duration) {
    let begin = run.time;
    for n in 1..=120 {
        let r = run.step(FRAME, Vec::new());
        if r.position == target && !matches!(r.out.repaint, Repaint::At(_)) {
            return (n, run.time - begin);
        }
    }
    panic!("the push did not settle in 120 frames");
}

#[test]
fn a_push_settles_within_the_motion_token_and_asks_for_frames_only_until_then() {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.still = true;
    run.run(10, FRAME);
    let idle = run.step(FRAME, Vec::new());
    assert_eq!(
        idle.out.repaint,
        Repaint::Idle,
        "a still stage at rest asks for nothing"
    );
    let moderate = run.stage.theme.motion.moderate;
    assert_eq!(run.stage.push_duration(), Some(moderate));

    run.stage.go(SceneId::Controls);
    let first = run.step(FRAME, Vec::new());
    assert_eq!(first.out.repaint, Repaint::Now, "the push is in flight");
    let (_, took) = settle(&mut run, 1.0);
    assert!(
        took <= moderate + 2 * FRAME,
        "settled after {took:?}, the token is {moderate:?}"
    );
    // Settled: no frame is asked for, now or later.
    for _ in 0..60 {
        let r = run.step(FRAME, Vec::new());
        assert_eq!(r.position, 1.0);
        assert_eq!(
            r.out.repaint,
            Repaint::Idle,
            "a settled still stage asks for a frame"
        );
    }
}

#[test]
fn a_moving_background_asks_for_every_frame_and_a_still_one_for_none() {
    let mut run = Headless::new((1024.0, 768.0), 2.0, Insets::ZERO);
    for _ in 0..30 {
        let r = run.step(FRAME, Vec::new());
        assert_eq!(r.out.repaint, Repaint::Now);
    }
    for tier in [MotionTier::Reduced, MotionTier::Off] {
        let mut run = Headless::new((1024.0, 768.0), 2.0, Insets::ZERO);
        run.stage.tier = tier;
        run.run(5, FRAME);
        let r = run.step(FRAME, Vec::new());
        assert_eq!(
            r.out.repaint,
            Repaint::Idle,
            "{tier:?}: the background rests"
        );
    }
}

#[test]
fn reduced_motion_pushes_for_the_reduced_token_and_off_cuts() {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.tier = MotionTier::Reduced;
    let fast = run.stage.push_duration().expect("a Reduced push");
    assert!(
        fast < run.stage.theme.motion.moderate,
        "Reduced is shorter: {fast:?}"
    );
    run.run(3, FRAME);
    run.stage.go(SceneId::Motion);
    run.step(FRAME, Vec::new());
    let (_, took) = settle(&mut run, 2.0);
    assert!(took <= fast + 2 * FRAME, "{took:?} against {fast:?}");

    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.tier = MotionTier::Off;
    assert_eq!(run.stage.push_duration(), None);
    run.run(3, FRAME);
    run.stage.go(SceneId::Data);
    let r = run.step(FRAME, Vec::new());
    assert_eq!(r.position, 3.0, "Off cuts");
}

#[test]
fn a_push_changes_the_picture_in_between_and_a_click_retargets_it() {
    let mut run = Headless::new((1440.0, 900.0), 2.0, Insets::ZERO);
    run.stage.still = true;
    let at_rest = run.run(3, FRAME).expect("a frame").scene;
    run.stage.go(SceneId::Controls);
    run.step(FRAME, Vec::new());
    let mid = run.step(Duration::from_millis(120), Vec::new());
    assert!(mid.position > 0.0 && mid.position < 1.0, "{}", mid.position);
    assert!(mid.scene != at_rest);
    // A second choice mid-flight retargets from where it is: the position never jumps.
    let before = mid.position;
    run.stage.go(SceneId::Glass);
    let back = run.step(FRAME, Vec::new());
    assert!(
        (back.position - before).abs() < 0.1,
        "{before} -> {}",
        back.position
    );
    let (_, _) = settle(&mut run, 0.0);
}

#[test]
fn a_tap_on_a_rail_row_or_a_tab_chooses_its_scene() {
    for (size, safe) in [
        ((1440.0, 900.0), Insets::ZERO),
        ((390.0, 844.0), PHONE_SAFE),
    ] {
        let mut run = Headless::new(size, 2.0, safe);
        run.stage.still = true;
        run.run(5, FRAME);
        let l = run.layout();
        for target in [3, 5, 1] {
            let cell = if l.wide {
                l.rail_rows()[target]
            } else {
                l.tab_cells()[target]
            };
            let at = (cell.x + cell.w / 2.0, cell.y + cell.h / 2.0);
            let mut script = Script::new();
            script.wait(Duration::from_millis(100)).tap(at);
            let mut player = Player::new(script);
            for _ in 0..90 {
                let mut events = Vec::new();
                player.advance(run.time + FRAME, &mut events);
                run.step(FRAME, events);
            }
            assert_eq!(run.stage.want, SceneId::from_index(target), "{size:?}");
        }
    }
}

#[test]
fn the_background_is_slow_and_its_periods_are_long() {
    for size in [(1440.0, 900.0), (1024.0, 768.0), (390.0, 844.0)] {
        for (i, b) in BLOBS.iter().enumerate() {
            // The bound is analytic; the sampled peak of the path function never exceeds it.
            let bound = blob_speed_bound(i, size);
            let mut peak = 0.0_f32;
            let mut prev = blob_centre(i, 0.0, size);
            let dt = 0.05;
            let mut t = dt;
            while t < 400.0 {
                let c = blob_centre(i, t, size);
                peak = peak.max((c.0 - prev.0).hypot(c.1 - prev.1) / dt as f32);
                prev = c;
                t += dt;
            }
            if size == (1440.0, 900.0) {
                println!(
                    "blob {i}: sampled peak {peak:.2} px/s, bound {bound:.2} px/s, periods {:?} s",
                    b.period
                );
            }
            assert!(
                peak <= bound * 1.001,
                "blob {i} at {size:?}: sampled {peak}, bound {bound}"
            );
            assert!(bound <= 30.0, "blob {i} at {size:?}: bound {bound} px/s");
            assert!(
                b.period.0 >= 20.0
                    && b.period.0 <= 60.0
                    && b.period.1 >= 20.0
                    && b.period.1 <= 60.0
            );
            assert!(b.breath.0 <= 0.15, "breathing is at most 15 %");
        }
    }
    println!("shortest period of any motion: {} s", shortest_period());
    assert!(shortest_period() >= 20.0);
}

#[test]
fn frames_are_deterministic_and_a_still_stage_does_not_depend_on_the_clock() {
    let frame = |still: bool, at: u64| {
        let mut run = Headless::new((1024.0, 768.0), 2.0, Insets::ZERO);
        run.stage.still = still;
        run.step_to(Duration::from_secs(at), Vec::new()).scene
    };
    assert!(
        frame(false, 5) == frame(false, 5),
        "the same clock draws the same frame"
    );
    assert!(
        frame(false, 5) != frame(false, 6),
        "the background moves with the clock"
    );
    assert!(
        frame(true, 1) == frame(true, 400),
        "a still background is the clock's no matter when"
    );
    assert!(
        frame(true, 0) == frame(false, STILL_AT.as_secs()),
        "still rests where the dump is taken"
    );
}

#[test]
fn the_old_page_is_gone_by_the_middle_and_the_scrim_leaves_no_trace_at_the_end() {
    println!("old page opacity at 50 %: {}", old_opacity(0.5));
    assert!(old_opacity(0.5) <= 0.1);
    assert_eq!(old_opacity(0.0), 1.0);
    assert_eq!(scrim(0.0), 0.0);
    assert_eq!(scrim(1.0), 0.0);
    assert!(scrim(1.0 / 3.0) > 0.99, "the scrim reaches its peak");
    // At 90 % of the time the eased progress is 0.976: under 1 % of the backdrop's alpha.
    assert!(scrim(0.976) < 0.01, "{}", scrim(0.976));
}

#[test]
fn the_title_fades_through_and_never_shows_two_names_together() {
    for f in [0.45_f32, 0.5, 0.6] {
        let (old, new) = title_opacities(f);
        println!("title opacities at {f}: old {old:.3}, new {new:.3}");
    }
    for i in 0..=1000 {
        let (old, new) = title_opacities(i as f32 / 1000.0);
        assert!(
            old.min(new) == 0.0 || old.max(new) <= 0.2 || old.min(new) < 0.05,
            "{i}: {old} {new}"
        );
        assert!(old.min(new) <= 0.0, "both visible at {i}: {old} {new}");
    }
    assert_eq!(title_opacities(0.0), (1.0, 0.0));
    assert_eq!(title_opacities(1.0), (0.0, 1.0));
}
