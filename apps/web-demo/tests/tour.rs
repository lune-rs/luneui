//! The attract tour and the HUD, headless: the stage's own per-frame function driven in app time
//! with no window and no GPU, under the frame schedules a real run can have. CI runs these on all
//! three systems.

use std::time::Duration;

use lune_ui::input::{
    Event, ImeEvent, Key, Modifiers, NamedKey, PointerButton, ScrollDelta, TouchEvent, TouchId,
    TouchPhase,
};
use lune_ui::layout::Insets;
use lune_ui::scene::LayerId;
use web_demo::hud::{Hud, IDLE_GAP, WINDOW};
use web_demo::scenes::SceneId;
use web_demo::scenes::theme::Look;
use web_demo::stage::{Headless, Rendered};
use web_demo::tour::{END, IDLE, MarkResult, Play, marks_from, nav_point};

const PHONE_SAFE: Insets = Insets::new(0.0, 59.0, 0.0, 34.0);

/// A window the tour runs in: its logical size, its scale and its safe area.
#[derive(Debug, Clone, Copy)]
struct Screen {
    name: &'static str,
    size: (f32, f32),
    scale: f32,
    safe: Insets,
}

const DESKTOP: Screen = Screen {
    name: "desktop",
    size: (1440.0, 900.0),
    scale: 2.0,
    safe: Insets::ZERO,
};
const PHONE: Screen = Screen {
    name: "phone",
    size: (390.0, 844.0),
    scale: 3.0,
    safe: PHONE_SAFE,
};

const FRAME: Duration = Duration::from_micros(16_667);

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// What a schedule says about the next frame: its app time.
type Next = Box<dyn FnMut(Duration) -> Duration>;

fn fixed(period: Duration) -> Next {
    Box::new(move |t| t + period)
}

/// Frames 8 to 33 ms apart in a fixed pattern, and a 100 ms hitch about every second.
fn jittered() -> Next {
    const PATTERN: [u64; 12] = [8, 17, 33, 12, 25, 16, 9, 30, 16, 21, 14, 27];
    let mut i = 0usize;
    let mut since_hitch = Duration::ZERO;
    Box::new(move |t| {
        let mut dt = ms(PATTERN[i % PATTERN.len()]);
        i += 1;
        since_hitch += dt;
        if since_hitch >= Duration::from_secs(1) {
            since_hitch = Duration::ZERO;
            dt = ms(100);
        }
        t + dt
    })
}

/// A stage on `screen` that plays the tour.
fn playing(screen: Screen, play: Play) -> Headless {
    let mut h = Headless::new(screen.size, screen.scale, screen.safe);
    h.stage.play(play);
    h
}

const ONCE: Play = Play {
    repeat: false,
    touches: false,
};
const LOOPING: Play = Play {
    repeat: true,
    touches: true,
};

/// How long a run may last in app time before the test calls it stuck.
const LIMIT: Duration = Duration::from_secs(300);

/// The marks a run passed, in order.
type Passed = Vec<MarkResult>;

/// Plays `h` from its time on until `done` says so; returns every mark passed. `next` gives the
/// next frame's app time; `events` the real events of the frame at that time.
fn play_until(
    h: &mut Headless,
    next: &mut Next,
    mut events: impl FnMut(Duration) -> Vec<Event>,
    mut done: impl FnMut(&Headless, &Rendered, &Passed) -> bool,
) -> Passed {
    let mut passed = Passed::new();
    let mut t = h.time;
    loop {
        let r = h.step_to(t, events(t));
        passed.extend(r.marks.iter().cloned());
        if done(h, &r, &passed) {
            return passed;
        }
        t = next(t);
        assert!(t < LIMIT, "the run did not end by {LIMIT:?}");
    }
}

fn names(passed: &Passed) -> Vec<&str> {
    passed.iter().map(|m| m.name.as_str()).collect()
}

fn assert_all_ok(passed: &Passed) {
    for m in passed {
        assert_eq!(m.result, Ok(()), "mark {}", m.name);
    }
}

/// Plays the tour once on `screen` under `next`; returns the marks and when it ended.
fn once(screen: Screen, next: Next) -> (Passed, Duration, Option<(u32, u32)>) {
    let mut h = playing(screen, ONCE);
    let mut next = next;
    let mut ended = None;
    let passed = play_until(
        &mut h,
        &mut next,
        |_| Vec::new(),
        |_, r, _| {
            ended = ended.or(r.ended);
            r.finished
        },
    );
    (passed, h.time, ended)
}

fn assert_tour(screen: Screen, next: Next, what: &str) {
    let (passed, took, ended) = once(screen, next);
    assert_eq!(
        names(&passed),
        marks_from(SceneId::Glass),
        "{} {what}: marks out of order",
        screen.name
    );
    assert_all_ok(&passed);
    assert_eq!(ended, Some((13, 13)), "{} {what}", screen.name);
    assert!(
        took > Duration::from_secs(40) && took < Duration::from_secs(80),
        "{} {what}: the tour took {took:?}",
        screen.name
    );
}

#[test]
fn the_tour_passes_every_mark_at_60_hz() {
    for screen in [DESKTOP, PHONE] {
        assert_tour(screen, fixed(Duration::from_micros(16_667)), "60 Hz");
    }
}

#[test]
fn the_tour_passes_every_mark_at_120_hz() {
    for screen in [DESKTOP, PHONE] {
        assert_tour(screen, fixed(Duration::from_micros(8_333)), "120 Hz");
    }
}

#[test]
fn the_tour_passes_every_mark_with_uneven_frames_and_hitches() {
    for screen in [DESKTOP, PHONE] {
        assert_tour(screen, jittered(), "jittered");
    }
}

#[test]
fn a_looping_tour_passes_every_mark_twice_wraps_once_and_starts_each_cycle_in_the_same_look() {
    let mut h = playing(DESKTOP, LOOPING);
    let mut next = fixed(Duration::from_micros(16_667));
    let (mut restarts, mut ends, mut changed_look) = (0, Vec::new(), false);
    let mut look_at_wrap = None;
    let passed = play_until(
        &mut h,
        &mut next,
        |_| Vec::new(),
        |h, r, passed| {
            changed_look |= h.stage.scenes.theme.look != Look::DEFAULT;
            if r.restarted {
                restarts += 1;
                look_at_wrap = Some(h.stage.scenes.theme.look);
            }
            ends.extend(r.ended);
            passed.len() >= 26
        },
    );
    let want: Vec<&str> = marks_from(SceneId::Glass)
        .into_iter()
        .chain(marks_from(SceneId::Glass))
        .collect();
    assert_eq!(names(&passed), want);
    assert_all_ok(&passed);
    assert_eq!(restarts, 1, "the loop wraps once between two cycles");
    assert_eq!(ends, vec![(13, 13), (13, 13)]);
    assert!(changed_look, "the Theme scene never changed the look");
    assert_eq!(look_at_wrap, Some(Look::DEFAULT));
}

#[test]
fn a_stage_that_was_not_told_to_play_has_no_tour() {
    let mut h = Headless::new(DESKTOP.size, DESKTOP.scale, DESKTOP.safe);
    let mut next = fixed(Duration::from_micros(16_667));
    let passed = play_until(
        &mut h,
        &mut next,
        |_| Vec::new(),
        |h, _, _| h.time > Duration::from_secs(70),
    );
    assert!(passed.is_empty());
    assert!(!h.stage.touring());
}

// ---- a person on the stage ----

fn touch(phase: TouchPhase, at: (f32, f32)) -> Event {
    Event::Touch(TouchEvent::new(TouchId::new(7), phase, at))
}

fn key(k: Key, pressed: bool) -> Event {
    Event::Key {
        key: k,
        pressed,
        repeat: false,
        modifiers: Modifiers::default(),
    }
}

/// Runs `h` for `secs` of 60 Hz frames, with no real events; returns the marks passed.
fn idle_for(h: &mut Headless, secs: f32) -> Passed {
    let end = h.time + Duration::from_secs_f32(secs);
    let mut next = fixed(Duration::from_micros(16_667));
    play_until(h, &mut next, |_| Vec::new(), |h, _, _| h.time >= end)
}

/// A stage playing on `screen`, run for `secs`.
fn running(screen: Screen, secs: f32) -> Headless {
    let mut h = playing(screen, LOOPING);
    idle_for(&mut h, secs);
    h
}

#[test]
fn every_kind_of_real_event_stops_the_tour_and_nothing_else_does() {
    let events: [(&str, Event, bool); 15] = [
        (
            "pointer move",
            Event::PointerMoved {
                pos: (700.0, 400.0),
            },
            true,
        ),
        (
            "pointer button",
            Event::PointerButton {
                pos: (700.0, 400.0),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
            true,
        ),
        (
            "wheel",
            Event::Scroll {
                pos: (700.0, 400.0),
                delta: ScrollDelta::Lines { x: 0.0, y: -1.0 },
                modifiers: Modifiers::default(),
            },
            true,
        ),
        ("touch", touch(TouchPhase::Started, (700.0, 400.0)), true),
        ("key", key(Key::Named(NamedKey::ArrowDown), true), true),
        ("typed text", Event::Text("x".to_owned()), true),
        (
            "composition",
            Event::Ime(ImeEvent::Preedit {
                text: "に".to_owned(),
                cursor: None,
            }),
            true,
        ),
        (
            "committed text",
            Event::Ime(ImeEvent::Commit("日".to_owned())),
            true,
        ),
        // The app's own focus on a text field switches the input method on and off.
        ("input method on", Event::Ime(ImeEvent::Enabled), false),
        ("input method off", Event::Ime(ImeEvent::Disabled), false),
        ("pointer left", Event::PointerLeft, false),
        ("focus", Event::Focus(true), false),
        (
            "modifiers",
            Event::ModifiersChanged(Modifiers::default()),
            false,
        ),
        ("safe area", Event::SafeArea(Insets::ZERO), false),
        ("keyboard inset", Event::KeyboardInset(Insets::ZERO), false),
    ];
    for (what, event, stops) in events {
        let mut h = running(DESKTOP, 8.0);
        assert!(h.stage.touring(), "{what}: not touring before");
        h.step(ms(17), vec![event]);
        h.run(3, ms(17));
        assert_eq!(h.stage.touring(), !stops, "{what}");
    }
}

#[test]
fn a_stopped_tour_passes_no_mark_and_adds_no_event_until_ten_seconds_after_the_last_real_one() {
    let mut h = running(DESKTOP, 14.0);
    // Mid-tour: the hand may have a contact down. A real pointer press stops it.
    let press = Event::PointerButton {
        pos: (700.0, 700.0),
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::default(),
    };
    let r = h.step(ms(17), vec![press]);
    assert!(!h.stage.touring());
    let stopped_at = h.time;
    // No scripted contact is left down: the player's contacts are all fading or gone.
    let _ = r;
    let marks = idle_for(&mut h, 9.0);
    assert!(marks.is_empty(), "marks while stopped: {:?}", names(&marks));
    // A second real event restarts the ten seconds.
    h.step(
        ms(17),
        vec![Event::PointerMoved {
            pos: (720.0, 700.0),
        }],
    );
    let moved_at = h.time;
    assert!(moved_at - stopped_at > Duration::from_secs(9));
    let marks = idle_for(&mut h, 9.5);
    assert!(
        marks.is_empty(),
        "the tour came back after 9.5 s: {:?}",
        names(&marks)
    );
    assert!(!h.stage.touring());
    // Ten seconds on it plays again, from the start of the scene the stage is on.
    let scene = h.stage.want;
    let want = marks_from(scene);
    let mut next = fixed(Duration::from_micros(16_667));
    let passed = play_until(
        &mut h,
        &mut next,
        |_| Vec::new(),
        |_, _, p| p.len() >= want.len(),
    );
    assert!(h.time - moved_at >= IDLE);
    assert_eq!(names(&passed), want);
    assert_all_ok(&passed);
}

#[test]
fn a_person_who_chose_a_scene_gets_the_tour_from_that_scene() {
    for screen in [DESKTOP, PHONE] {
        let mut h = running(screen, 6.0);
        // A real tap on the Data row of the rail (or its tab).
        let l = h.layout();
        let at = nav_point(&l, SceneId::Data);
        h.step(ms(17), vec![touch(TouchPhase::Started, at)]);
        h.run(4, ms(17));
        h.step(ms(17), vec![touch(TouchPhase::Ended, at)]);
        h.run(40, ms(17));
        assert_eq!(h.stage.want, SceneId::Data, "{}", screen.name);
        assert!(!h.stage.touring());
        let want = marks_from(SceneId::Data);
        let mut next = fixed(Duration::from_micros(16_667));
        let passed = play_until(
            &mut h,
            &mut next,
            |_| Vec::new(),
            |_, _, p| p.len() >= want.len(),
        );
        assert_eq!(names(&passed), want, "{}", screen.name);
        assert_all_ok(&passed);
        assert_eq!(passed.last().map(|m| m.name.as_str()), Some(END));
    }
}

#[test]
fn the_resumed_tour_closes_what_the_person_left_open_with_escape() {
    // The Glass scene's dialog, open when the person walks away.
    let mut h = running(DESKTOP, 2.0);
    h.stage.want = SceneId::Glass;
    h.run(60, ms(17));
    let open = h
        .stage
        .scenes
        .glass
        .targets
        .centre("open")
        .expect("an open button");
    h.step(ms(17), vec![touch(TouchPhase::Started, open)]);
    h.run(4, ms(17));
    h.step(ms(17), vec![touch(TouchPhase::Ended, open)]);
    h.run(20, ms(17));
    assert!(h.stage.scenes.glass.dialog, "the person opened the dialog");
    let before = h.time;
    let want = marks_from(SceneId::Glass);
    let mut next = fixed(Duration::from_micros(16_667));
    let passed = play_until(
        &mut h,
        &mut next,
        |_| Vec::new(),
        |_, _, p| p.len() >= want.len(),
    );
    assert!(h.time - before > IDLE);
    assert_eq!(names(&passed), want);
    assert_all_ok(&passed);
}

#[test]
fn a_window_that_changes_size_restarts_the_tour_from_the_scene_it_is_on() {
    let mut h = playing(DESKTOP, LOOPING);
    idle_for(&mut h, 16.0);
    let scene = h.stage.want;
    assert_ne!(
        scene,
        SceneId::Glass,
        "16 s in, the tour has left the first scene"
    );
    h.size = PHONE.size;
    h.safe = PHONE.safe;
    let want = marks_from(scene);
    let mut next = fixed(Duration::from_micros(16_667));
    let passed = play_until(
        &mut h,
        &mut next,
        |_| Vec::new(),
        |_, _, p| p.len() >= want.len(),
    );
    assert_eq!(names(&passed), want);
    assert_all_ok(&passed);
    assert!(!h.layout().wide);
}

#[test]
fn a_stall_of_thirty_seconds_is_not_played_back() {
    let mut h = playing(DESKTOP, ONCE);
    let frame = Duration::from_micros(16_667);
    let (mut t, mut passed, mut stalled) = (Duration::ZERO, Passed::new(), false);
    let mut after_stall = Vec::new();
    loop {
        let r = h.step_to(t, Vec::new());
        passed.extend(r.marks.iter().cloned());
        if stalled && after_stall.len() < 40 {
            after_stall.push(r.marks.len());
        }
        if r.finished {
            break;
        }
        t += frame;
        if !stalled && t > Duration::from_secs(20) {
            // The next frame comes thirty seconds late (a hidden tab).
            stalled = true;
            t += Duration::from_secs(30);
        }
        assert!(t < LIMIT, "the tour did not end");
    }
    // Nothing was skipped: every mark, in order, each ok, and none of them in one frame.
    assert_eq!(names(&passed), marks_from(SceneId::Glass));
    assert_all_ok(&passed);
    assert!(after_stall.iter().all(|n| *n <= 1), "{after_stall:?}");
}

#[test]
fn touch_discs_are_drawn_on_the_toast_layer_only_with_touches() {
    for touches in [true, false] {
        let mut h = playing(
            DESKTOP,
            Play {
                repeat: false,
                touches,
            },
        );
        // The first tap is at 1 s, the Glass scene's 5 s dwell and a beat: 6.4 s; the disc is up
        // while the finger is down (80 ms) and fades over 250 ms. Nothing else draws on the toast
        // layer.
        let mut next = fixed(Duration::from_micros(16_667));
        let mut seen_at = Vec::new();
        play_until(
            &mut h,
            &mut next,
            |_| Vec::new(),
            |h, r, _| {
                let toast = r.scene.merged(LayerId::Toast);
                if !toast.geometry.under.is_empty() || !toast.geometry.fills.is_empty() {
                    seen_at.push(h.time);
                }
                h.time > Duration::from_secs(10)
            },
        );
        if touches {
            let (first, last) = (seen_at.first().copied(), seen_at.last().copied());
            let first = first.expect("a disc");
            let last = last.expect("a disc");
            assert!(
                first >= ms(6300) && last <= ms(6900),
                "the disc was up from {first:?} to {last:?}"
            );
        } else {
            assert!(seen_at.is_empty(), "a disc at {seen_at:?} without touches");
        }
    }
}

// ---- the HUD ----

#[test]
fn the_hud_reads_known_frame_times_by_the_nearest_rank() {
    let mut hud = Hud::default();
    assert_eq!(hud.stats(), None);
    let mut t = Duration::ZERO;
    hud.record(t);
    assert_eq!(hud.stats(), None, "one frame is no frame time");
    // 100 frames of 10 ms then 20 of 40 ms: the window holds exactly these 120.
    for dt in std::iter::repeat_n(10, 100).chain(std::iter::repeat_n(40, 20)) {
        t += ms(dt);
        hud.record(t);
    }
    let s = hud.stats().expect("120 times");
    assert_eq!(s.frames, 120);
    assert!((s.p50 - 10.0).abs() < 0.01, "p50 {}", s.p50);
    assert!((s.p99 - 40.0).abs() < 0.01, "p99 {}", s.p99);
    // The mean is 15 ms.
    assert!((s.fps - 66.667).abs() < 0.1, "fps {}", s.fps);
    // The window slides: 120 more of 10 ms push the slow ones out.
    for _ in 0..WINDOW {
        t += ms(10);
        hud.record(t);
    }
    let s = hud.stats().expect("times");
    assert!(
        (s.p99 - 10.0).abs() < 0.01 && (s.fps - 100.0).abs() < 0.1,
        "{s:?}"
    );
    assert_eq!(s.frames, 240);
}

#[test]
fn an_idle_gap_and_a_clock_that_went_back_are_not_frame_times() {
    let mut hud = Hud::default();
    hud.record(ms(0));
    hud.record(ms(16));
    let before = hud.stats().expect("a time");
    hud.record(ms(16) + IDLE_GAP + ms(1));
    hud.record(ms(10)); // back
    assert_eq!(hud.stats(), Some(before), "neither gap counted");
    // The gap is measured from the frame after it.
    hud.record(ms(26));
    assert_eq!(hud.stats().map(|s| s.frames), Some(before.frames + 1));
}

#[test]
fn h_toggles_the_hud_and_is_not_a_real_event_for_the_tour_or_the_scenes() {
    let mut h = playing(DESKTOP, ONCE);
    h.run(10, ms(17));
    assert!(!h.stage.hud.shown);
    let h_key = |pressed| key(Key::Character("h".to_owned()), pressed);
    h.step(
        ms(17),
        vec![h_key(true), Event::Text("h".to_owned()), h_key(false)],
    );
    assert!(h.stage.hud.shown);
    assert!(h.stage.touring(), "H stopped the tour");
    assert!(
        !h.stage.scenes.glass.pilot.paused(&web_demo::scenes::Env {
            wide: true,
            now: h.time,
            animating: true,
            entering: false,
            arrived: false,
        }),
        "H paused the autopilot"
    );
    h.step(ms(17), vec![h_key(true), h_key(false)]);
    assert!(!h.stage.hud.shown, "a second H hides it");
    // A chord is not the panel's key, and it is a real event.
    let ctrl = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    h.step(
        ms(17),
        vec![Event::Key {
            key: Key::Character("h".to_owned()),
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        }],
    );
    assert!(!h.stage.hud.shown);
    assert!(!h.stage.touring());
}

#[test]
fn h_is_a_letter_while_a_text_field_has_the_focus() {
    let mut h = Headless::new(DESKTOP.size, DESKTOP.scale, DESKTOP.safe);
    h.stage.want = SceneId::Text;
    // The Text scene's autopilot focuses its field about 0.3 s after it is entered.
    let r = h.run(240, ms(17)).expect("frames");
    assert!(r.out.ime_area.is_some(), "the field has the focus");
    let h_key = |pressed| key(Key::Character("h".to_owned()), pressed);
    h.step(
        ms(17),
        vec![h_key(true), Event::Text("h".to_owned()), h_key(false)],
    );
    assert!(
        !h.stage.hud.shown,
        "H toggled the panel while a field was focused"
    );
}

#[test]
fn the_hud_draws_its_panel_text_only_when_shown_and_changes_nothing_else() {
    let frames = |shown: bool| {
        let mut h = Headless::new(DESKTOP.size, DESKTOP.scale, DESKTOP.safe);
        h.stage.hud.shown = shown;
        h.run(140, FRAME);
        let r = h.step(FRAME, Vec::new());
        let stats = h.stage.hud.stats().expect("times");
        (r.scene.merged(LayerId::Toast).texts.len(), stats)
    };
    let (hidden, hidden_stats) = frames(false);
    let (shown, shown_stats) = frames(true);
    assert_eq!(hidden, 0);
    assert!(shown >= 3, "{shown} texts");
    assert_eq!(
        hidden_stats, shown_stats,
        "the readings are kept whether or not it is up"
    );
    assert!((shown_stats.fps - 60.0).abs() < 0.5, "{shown_stats:?}");
}

#[test]
fn the_driver_expects_as_many_marks_as_the_tour_has() {
    let js = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/web/run.mjs"),
    )
    .expect("scripts/web/run.mjs");
    let n = marks_from(SceneId::Glass).len();
    assert!(
        js.contains(&format!("\"web-demo\": {n}")),
        "scripts/web/run.mjs does not say `\"web-demo\": {n}` marks"
    );
}

#[test]
fn the_input_method_switching_on_does_not_pause_a_scenes_autopilot_but_a_composition_does() {
    // The browser's text bridge answers the Text scene's own focus with `Ime::Enabled`; that is not a
    // hand on the scene, so the field keeps being typed to.
    let env = |h: &Headless| web_demo::scenes::Env {
        wide: true,
        now: h.time,
        animating: true,
        entering: false,
        arrived: false,
    };
    let mut h = Headless::new(DESKTOP.size, DESKTOP.scale, DESKTOP.safe);
    h.stage.want = SceneId::Text;
    h.run(30, FRAME);
    h.step(FRAME, vec![Event::Ime(ImeEvent::Enabled)]);
    assert!(
        !h.stage.scenes.text.pilot.paused(&env(&h)),
        "Ime::Enabled paused it"
    );
    h.step(
        FRAME,
        vec![Event::Ime(ImeEvent::Preedit {
            text: "に".to_owned(),
            cursor: None,
        })],
    );
    assert!(
        h.stage.scenes.text.pilot.paused(&env(&h)),
        "a preedit did not"
    );
}
