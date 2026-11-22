//! The input player (R-core-13, D415): a script's gestures expand into the same events at any frame
//! rate, the player runs on the app's time alone (a pause, a stall, a repeat), and what it plays
//! reaches the widgets as a finger does. Nothing here reads a clock.

use std::time::Duration;

use lune_ui_core::input::{Event, TouchEvent, TouchId, TouchPhase};
use lune_ui_core::motion::MotionTier;
use lune_ui_core::paint::Rect;
use lune_ui_core::playback::{
    Advance, DRAG_SETTLE, FADE, Player, SAMPLE, Script, TAP_HOLD, TOUCH_ID_BASE, draw_touches,
};
use lune_ui_core::scene::FrameParams;
use lune_ui_core::testing::{DrawCall, RecordingRenderer, UiHarness};
use lune_ui_core::ui::{Response, Sense};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn touch(e: &Event) -> &TouchEvent {
    match e {
        Event::Touch(t) => t,
        other => panic!("not a touch: {other:?}"),
    }
}

/// Every event the script yields with the player asked every millisecond up to `until`, each with
/// the millisecond it first arrived at, plus the marks the same way.
fn timeline(script: Script, repeat: bool, until: u64) -> Vec<(u64, Event)> {
    let mut p = Player::new(script).with_repeat(repeat);
    let mut out = Vec::new();
    for t in 0..=until {
        let mut ev = Vec::new();
        p.advance(ms(t), &mut ev);
        out.extend(ev.into_iter().map(|e| (t, e)));
    }
    out
}

/// Plays `script` at `hz` frames per second from app time 0 until it is over and returns every
/// event in the order it arrived.
fn play_at(script: &Script, hz: u64) -> Vec<Event> {
    let mut p = Player::new(script.clone());
    let mut out = Vec::new();
    let end = script.length() + ms(100);
    let mut i = 0_u64;
    loop {
        let now = Duration::from_nanos(i * 1_000_000_000 / hz);
        if now > end {
            break;
        }
        p.advance(now, &mut out);
        i += 1;
    }
    out
}

/// A script of everything: a tap, a swipe, a drag, a mark between.
fn busy() -> Script {
    let mut s = Script::new();
    s.tap((10.0, 10.0))
        .wait(ms(100))
        .swipe((10.0, 10.0), (200.0, 10.0), ms(150))
        .mark("swiped")
        .wait(ms(30))
        .drag((200.0, 10.0), (20.0, 120.0), ms(300))
        .mark("dragged");
    s
}

/// What a frame feeds the harness from a scripted event.
fn feed(h: &mut UiHarness, e: &Event) {
    let t = touch(e);
    let id = t.id.get();
    let (x, y) = t.pos;
    match t.phase {
        TouchPhase::Started => h.input.touch_start(id, x, y),
        TouchPhase::Moved => h.input.touch_move(id, x, y),
        TouchPhase::Ended => h.input.touch_end(id, x, y),
        TouchPhase::Cancelled => h.input.touch_cancel(id, x, y),
        _ => unreachable!("a phase the player never makes"),
    };
}

const AREA: Rect = Rect::new(0.0, 0.0, 400.0, 300.0);

/// What a widget reported over a whole run at a fixed rate.
#[derive(Debug, Default)]
struct Run {
    clicks: u32,
    secondary: u32,
    stopped: Vec<(f32, f32)>,
}

/// Plays `script` through a real `Ui` at `hz` over one widget (`sense`) covering the window.
fn run_ui(script: Script, hz: u64, sense: Sense, until: Duration) -> Run {
    let mut h = UiHarness::new((400.0, 300.0));
    let mut p = Player::new(script);
    let mut run = Run::default();
    let mut i = 0_u64;
    loop {
        let now = Duration::from_nanos(i * 1_000_000_000 / hz);
        if now > until {
            break;
        }
        h.input.set_time(now);
        let mut events = Vec::new();
        p.advance(now, &mut events);
        events.iter().for_each(|e| feed(&mut h, e));
        let mut seen: Option<(Response, (f32, f32))> = None;
        h.frame(|ui| {
            let id = ui.id("w");
            let r = ui.interact(id, AREA, sense);
            seen = Some((r, ui.release_velocity(id)));
        });
        let (r, v) = seen.expect("the frame ran");
        run.clicks += u32::from(r.clicked);
        run.secondary += u32::from(r.secondary_clicked);
        if r.drag_stopped {
            run.stopped.push(v);
        }
        i += 1;
    }
    run
}

#[test]
fn a_tap_expands_to_a_press_and_a_lift_80_ms_apart_with_one_id() {
    let mut s = Script::new();
    s.tap((5.0, 6.0));
    let got = timeline(s, false, 200);
    assert_eq!(got.len(), 2, "{got:?}");
    let (a, b) = (touch(&got[0].1), touch(&got[1].1));
    assert_eq!((got[0].0, a.phase), (0, TouchPhase::Started));
    assert_eq!(
        (got[1].0, b.phase),
        (TAP_HOLD.as_millis() as u64, TouchPhase::Ended)
    );
    assert_eq!(a.id, b.id);
    assert_eq!(a.id, TouchId::new(TOUCH_ID_BASE));
    assert_eq!((a.pos, b.pos), ((5.0, 6.0), (5.0, 6.0)));
}

#[test]
fn a_swipe_of_200_ms_is_25_moves_the_last_at_the_end_and_the_lift_with_it() {
    let mut s = Script::new();
    s.swipe((0.0, 0.0), (100.0, 0.0), ms(200));
    let got = timeline(s, false, 300);
    let moves: Vec<_> = got
        .iter()
        .filter(|(_, e)| touch(e).phase == TouchPhase::Moved)
        .collect();
    assert_eq!(moves.len(), 25);
    assert_eq!(touch(&moves[24].1).pos, (100.0, 0.0));
    // The moves are 8 ms apart on a straight line.
    assert_eq!(moves[0].0, SAMPLE.as_millis() as u64);
    assert_eq!(touch(&moves[0].1).pos, (4.0, 0.0));
    let last = got.last().expect("events");
    assert_eq!((last.0, touch(&last.1).phase), (200, TouchPhase::Ended));
    assert_eq!(touch(&last.1).pos, (100.0, 0.0));
    assert_eq!(moves[24].0, 200);
}

#[test]
fn a_drag_ends_with_80_ms_of_moves_at_the_end_point_then_the_lift() {
    let mut s = Script::new();
    s.drag((0.0, 0.0), (100.0, 0.0), ms(200));
    let got = timeline(s, false, 400);
    let end = 200 + DRAG_SETTLE.as_millis() as u64;
    let last = got.last().expect("events");
    assert_eq!((last.0, touch(&last.1).phase), (end, TouchPhase::Ended));
    let tail: Vec<_> = got.iter().filter(|(t, _)| *t >= 200).collect();
    // Moves at 200 (the eased end), then every 8 ms to 280, then the lift.
    assert_eq!(tail.len(), 1 + 10 + 1);
    for (_, e) in &tail {
        assert_eq!(touch(e).pos, (100.0, 0.0));
    }
}

#[test]
fn a_drag_lifts_with_no_speed_and_the_same_swipe_flings() {
    let end = ms(1500);
    let mut drag = Script::new();
    drag.wait(ms(50))
        .drag((50.0, 100.0), (300.0, 100.0), ms(400));
    let mut swipe = Script::new();
    swipe
        .wait(ms(50))
        .swipe((50.0, 100.0), (300.0, 100.0), ms(400));
    let d = run_ui(drag, 60, Sense::DRAG, end);
    let s = run_ui(swipe, 60, Sense::DRAG, end);
    assert_eq!(d.stopped, [(0.0, 0.0)], "a drag flings nothing");
    assert_eq!(s.stopped.len(), 1);
    assert!(s.stopped[0].0 > 100.0, "a swipe flings: {:?}", s.stopped);
}

#[test]
fn the_events_are_the_same_at_60_and_at_120_hz() {
    let s = busy();
    let (a, b) = (play_at(&s, 60), play_at(&s, 120));
    assert!(a.len() > 60, "{}", a.len());
    assert_eq!(a, b);
}

#[test]
fn a_pause_emits_nothing_and_the_next_cue_comes_the_pause_later() {
    let mut s = Script::new();
    s.tap((1.0, 1.0));
    let mut p = Player::new(s);
    let mut ev = Vec::new();
    p.advance(ms(0), &mut ev);
    assert_eq!(ev.len(), 1);
    p.pause(ms(20));
    let during = p.advance(ms(5000), &mut ev);
    assert_eq!((ev.len(), during.next), (1, None));
    p.resume(ms(10_020));
    // The lift was due at 80 ms of script time: 10 s later in app time.
    p.advance(ms(10_079), &mut ev);
    assert_eq!(ev.len(), 1);
    p.advance(ms(10_080), &mut ev);
    assert_eq!(ev.len(), 2);
    assert_eq!(touch(&ev[1]).phase, TouchPhase::Ended);
}

#[test]
fn time_that_goes_back_emits_nothing() {
    let mut s = Script::new();
    s.tap((1.0, 1.0));
    let mut p = Player::new(s);
    let mut ev = Vec::new();
    p.advance(ms(1000), &mut ev);
    ev.clear();
    p.advance(ms(900), &mut ev);
    assert!(ev.is_empty());
    p.advance(ms(1079), &mut ev);
    assert!(ev.is_empty());
    p.advance(ms(1080), &mut ev);
    assert_eq!(ev.len(), 1);
}

#[test]
fn the_script_starts_at_the_first_call_not_at_the_construction() {
    let mut s = Script::new();
    s.wait(ms(100)).mark("m");
    let mut p = Player::new(s);
    let mut ev = Vec::new();
    // A first frame an hour in: the script starts then, and nothing is due yet.
    let first = p.advance(Duration::from_secs(3600), &mut ev);
    assert!(first.marks.is_empty());
    assert_eq!(first.next, Some(Duration::from_secs(3600) + ms(100)));
    let second = p.advance(Duration::from_secs(3600) + ms(100), &mut ev);
    assert_eq!(second.marks, ["m"]);
}

#[test]
fn a_repeat_wraps_once_per_cycle_and_plays_the_first_cycle_again() {
    let mut s = Script::new();
    s.tap((3.0, 4.0)).end_hold(ms(20));
    let len = s.length().as_millis() as u64;
    assert_eq!(len, 100);
    let once = timeline(s.clone(), false, 99);
    let twice = timeline(s.clone(), true, 199);
    let mut p = Player::new(s).with_repeat(true);
    let mut restarts = 0;
    for t in 0..=199 {
        restarts += u32::from(p.advance(ms(t), &mut Vec::new()).restarted);
    }
    assert_eq!(restarts, 1);
    let first: Vec<_> = twice.iter().filter(|(t, _)| *t < len).cloned().collect();
    assert_eq!(first, once);
    let second: Vec<_> = twice.iter().filter(|(t, _)| *t >= len).collect();
    assert_eq!(second.len(), 2);
    for ((t, e), (t0, e0)) in second.iter().zip(&once) {
        assert_eq!(*t, t0 + len);
        let (a, b) = (touch(e), touch(e0));
        assert_eq!((a.phase, a.pos), (b.phase, b.pos));
        assert_eq!(a.id, b.id, "a cycle plays the same events again");
    }
}

#[test]
fn a_contact_down_at_the_wrap_is_cancelled_before_the_next_cycle() {
    let id = TouchId::new(7);
    let mut s = Script::new();
    s.event(Event::Touch(TouchEvent::new(
        id,
        TouchPhase::Started,
        (1.0, 2.0),
    )))
    .end_hold(ms(50));
    let mut p = Player::new(s).with_repeat(true);
    let mut ev = Vec::new();
    p.advance(ms(0), &mut ev);
    let adv = p.advance(ms(50), &mut ev);
    assert!(adv.restarted);
    let phases: Vec<_> = ev.iter().map(|e| touch(e).phase).collect();
    assert_eq!(
        phases,
        [
            TouchPhase::Started,
            TouchPhase::Cancelled,
            TouchPhase::Started
        ]
    );
    assert_eq!(touch(&ev[1]).pos, (1.0, 2.0));
}

#[test]
fn a_repeat_that_is_behind_by_whole_cycles_skips_them() {
    let mut s = Script::new();
    s.tap((3.0, 4.0)).end_hold(ms(20));
    let mut p = Player::new(s).with_repeat(true);
    let mut ev = Vec::new();
    p.advance(ms(0), &mut ev);
    ev.clear();
    // 5 cycles and a bit: the cycles in between are not replayed in one frame.
    let adv = p.advance(ms(530), &mut ev);
    assert!(adv.restarted);
    assert!(ev.len() <= 2, "{ev:?}");
    assert!(adv.next.is_some_and(|n| n > ms(530)));
}

#[test]
fn next_is_the_next_cue_then_the_fade_then_the_end() {
    let mut s = Script::new();
    s.tap((1.0, 1.0)).mark("done");
    let mut p = Player::new(s);
    let mut ev = Vec::new();
    let a = p.advance(ms(0), &mut ev);
    assert_eq!(a.next, Some(ms(80)));
    let b = p.advance(ms(80), &mut ev);
    assert_eq!(b.marks, ["done"]);
    // The lift fades for 250 ms: that frame is asked for before the end of the script.
    assert_eq!(b.next, Some(ms(80) + FADE));
    let c = p.advance(ms(80) + FADE, &mut ev);
    assert_eq!(c.next, Some(ms(80 + 1500)));
    assert!(!c.finished);
    let d = p.advance(ms(80 + 1500), &mut ev);
    assert!(d.finished && p.is_finished());
    assert_eq!(d.next, None);
}

#[test]
fn next_is_later_than_now_on_every_call() {
    let mut p = Player::new(busy());
    let mut ev = Vec::new();
    for t in (0..3000).step_by(7) {
        let adv = p.advance(ms(t), &mut ev);
        if let Some(n) = adv.next {
            assert!(n > ms(t), "at {t} ms next is {n:?}");
        }
    }
    assert!(p.is_finished());
}

#[test]
fn a_tap_through_the_player_clicks_once_and_a_long_press_is_a_secondary_click() {
    for hz in [60, 120] {
        let mut tap = Script::new();
        tap.wait(ms(50)).tap((100.0, 100.0));
        let run = run_ui(tap, hz, Sense::CLICK, ms(1000));
        assert_eq!((run.clicks, run.secondary), (1, 0), "tap at {hz} Hz");

        let mut press = Script::new();
        press.wait(ms(50)).press((100.0, 100.0), ms(700));
        let run = run_ui(press, hz, Sense::CLICK, ms(1500));
        assert_eq!((run.clicks, run.secondary), (0, 1), "press at {hz} Hz");
    }
}

/// The fills drawn on the toast layer by one frame of `draw_touches`.
fn toast_fills(h: &mut UiHarness, p: &Player) -> Vec<[f32; 4]> {
    let (scene, _) = h.frame(|ui| draw_touches(ui, p));
    let mut r = RecordingRenderer::new();
    r.render(
        &scene,
        &FrameParams {
            size: (400, 300),
            scale: 1.0,
            ..FrameParams::default()
        },
    );
    r.last()
        .expect("a frame")
        .calls
        .iter()
        .filter_map(|c| match c {
            DrawCall::Under { layer: 6, shape } if shape.params[0] == 0.0 => Some(shape.rect),
            _ => None,
        })
        .collect()
}

fn corners() -> Script {
    let mut s = Script::new();
    for (i, pos) in [(0.0, 0.0), (400.0, 0.0), (0.0, 300.0), (400.0, 300.0)]
        .into_iter()
        .enumerate()
    {
        s.event(Event::Touch(TouchEvent::new(
            TouchId::new(i as u64),
            TouchPhase::Started,
            pos,
        )));
    }
    s
}

#[test]
fn a_contact_at_each_window_corner_draws_a_disc_on_the_toast_layer_there() {
    let mut h = UiHarness::new((400.0, 300.0));
    let mut p = Player::new(corners());
    p.advance(ms(0), &mut Vec::new());
    let rects = toast_fills(&mut h, &p);
    assert_eq!(rects.len(), 4, "{rects:?}");
    for (rect, (cx, cy)) in
        rects
            .iter()
            .zip([(0.0, 0.0), (400.0, 0.0), (0.0, 300.0), (400.0, 300.0)])
    {
        assert_eq!(*rect, [cx - 22.0, cy - 22.0, 44.0, 44.0]);
    }
}

#[test]
fn a_lifted_contact_fades_at_full_and_is_gone_at_once_under_reduced() {
    let mut s = Script::new();
    s.tap((200.0, 150.0));
    let mut p = Player::new(s);
    let mut ev = Vec::new();
    p.advance(ms(0), &mut ev);
    p.advance(ms(80), &mut ev);
    let lifted = ms(80) + FADE / 2;
    let mut full = UiHarness::new((400.0, 300.0));
    full.input.set_time(lifted);
    let half = toast_fills(&mut full, &p);
    assert_eq!(half.len(), 1, "the instrument sees a fading disc at Full");
    let mut reduced = UiHarness::new((400.0, 300.0));
    reduced.tier = MotionTier::Reduced;
    reduced.input.set_time(lifted);
    assert!(toast_fills(&mut reduced, &p).is_empty());
    // A contact still down draws under any tier.
    let mut down = Player::new(corners());
    down.advance(ms(0), &mut Vec::new());
    assert_eq!(toast_fills(&mut reduced, &down).len(), 4);
}

#[test]
fn the_disc_takes_its_colour_from_the_theme() {
    let mut h = UiHarness::new((400.0, 300.0));
    h.theme.fg_primary = [11, 222, 33, 0xF0];
    let mut p = Player::new(corners());
    p.advance(ms(0), &mut Vec::new());
    let (scene, _) = h.frame(|ui| draw_touches(ui, &p));
    let toast = &scene.layers[6];
    let fills = &toast.geometry.under;
    assert!(fills.len() >= 4);
    for shape in fills {
        let c = shape.color;
        assert!(c[3] > 0.0, "{c:?}");
        // Premultiplied: the hue is the theme's, not the preset's white.
        let (g, r) = (c[1] / c[3], c[0] / c[3]);
        assert!((g - 222.0 / 255.0).abs() < 0.02, "{c:?}");
        assert!((r - 11.0 / 255.0).abs() < 0.02, "{c:?}");
    }
}

#[test]
fn advance_is_plain_data_a_test_can_read() {
    let a = Advance::default();
    assert!(a.marks.is_empty() && a.next.is_none() && !a.restarted && !a.finished);
}
