//! Touch gestures in the pointer machine (mobile M3 step 3, R-core-12, D348): the touch slop, the
//! release velocity, the long press and the pinch reading. Scripted time and touches through
//! `UiHarness`; nothing here reads a clock.

use std::time::Duration;

use lune_ui_core::input::{PointerButton, Repaint};
use lune_ui_core::motion::Time;
use lune_ui_core::paint::Rect;
use lune_ui_core::testing::UiHarness;
use lune_ui_core::ui::{Pinch, Response, Sense, WidgetId};
use lune_ui_core::widgets::tab_strip::{Tab, TabKey, TabStripOptions};
use lune_ui_core::widgets::tone::Tone;

const P: PointerButton = PointerButton::Primary;
/// A wide drag widget the pans run across.
const W: Rect = Rect::new(0.0, 0.0, 400.0, 300.0);
/// A button.
const B: Rect = Rect::new(10.0, 10.0, 100.0, 30.0);

fn us(h: &mut UiHarness, micros: u64) {
    h.input.set_time(Duration::from_micros(micros));
}

/// One frame of the widget `w` over `rect`: its response and release velocity.
fn frame(h: &mut UiHarness, rect: Rect, sense: Sense) -> (Response, (f32, f32)) {
    let mut out = None;
    h.frame(|ui| {
        let id = ui.id("w");
        let r = ui.interact(id, rect, sense);
        out = Some((r, ui.release_velocity(id)));
    });
    out.expect("the frame ran")
}

fn ready(rect: Rect, sense: Sense) -> UiHarness {
    let mut h = UiHarness::new((400.0, 300.0));
    frame(&mut h, rect, sense);
    h
}

/// A touch pan at `v` px/s along x at `hz` frames per second: a start, `moves` moves, and the lift
/// at the next frame. Returns the lift frame's reading and the frame after's.
fn pan(h: &mut UiHarness, hz: u64, v: f32, moves: u64) -> ((Response, (f32, f32)), (f32, f32)) {
    let at = |i: u64| 1_000_000 * i / hz;
    let x = |i: u64| 20.0 + v * at(i) as f32 / 1e6;
    us(h, at(0));
    h.input.touch_start(1, x(0), 100.0);
    frame(h, W, Sense::DRAG);
    for i in 1..=moves {
        us(h, at(i));
        h.input.touch_move(1, x(i), 100.0);
        frame(h, W, Sense::DRAG);
    }
    us(h, at(moves + 1));
    h.input.touch_end(1, x(moves + 1), 100.0);
    let lift = frame(h, W, Sense::DRAG);
    us(h, at(moves + 2));
    let after = frame(h, W, Sense::DRAG).1;
    (lift, after)
}

fn near(got: (f32, f32), want: (f32, f32), tol: f32) {
    assert!(
        (got.0 - want.0).abs() <= tol && (got.1 - want.1).abs() <= tol,
        "{got:?} is not within {tol} of {want:?}"
    );
}

#[test]
fn a_touch_tap_with_a_wobble_clicks_and_a_mouse_press_with_it_drags() {
    let mut touch = ready(W, Sense::DRAG);
    touch.input.touch_start(1, 50.0, 50.0);
    frame(&mut touch, W, Sense::DRAG);
    touch.input.touch_move(1, 58.0, 50.0);
    let moved = frame(&mut touch, W, Sense::DRAG).0;
    touch.input.touch_end(1, 58.0, 50.0);
    let lifted = frame(&mut touch, W, Sense::DRAG).0;
    assert!(!moved.drag_started && !moved.dragged);
    assert!(lifted.clicked && !lifted.drag_stopped);

    let mut mouse = ready(W, Sense::DRAG);
    mouse.input.pointer_to(50.0, 50.0).press(P);
    frame(&mut mouse, W, Sense::DRAG);
    mouse.input.pointer_to(58.0, 50.0);
    let moved = frame(&mut mouse, W, Sense::DRAG).0;
    assert!(moved.drag_started, "8 px is past the mouse's 4");
}

#[test]
fn a_touch_drag_past_the_slop_starts_with_the_whole_travel() {
    let mut h = ready(W, Sense::DRAG);
    h.input.touch_start(1, 50.0, 50.0);
    frame(&mut h, W, Sense::DRAG);
    h.input.touch_move(1, 62.0, 50.0);
    let r = frame(&mut h, W, Sense::DRAG).0;
    assert!(r.drag_started && r.dragged);
    assert_eq!(r.drag_delta, (12.0, 0.0));
}

#[test]
fn a_touch_pan_leaves_its_velocity_at_both_rates_on_the_stop_frame_only() {
    for hz in [60, 120] {
        let mut h = ready(W, Sense::DRAG);
        let ((r, v), after) = pan(&mut h, hz, 600.0, 8);
        assert!(r.drag_stopped, "{hz} Hz");
        near(v, (600.0, 0.0), 6.0);
        assert_eq!(after, (0.0, 0.0), "{hz} Hz: a reading lasts one frame");
    }
}

#[test]
fn a_finger_held_still_before_the_lift_leaves_no_velocity() {
    let mut h = ready(W, Sense::DRAG);
    let at = |i: u64| 1_000_000 * i / 60;
    us(&mut h, at(0));
    h.input.touch_start(1, 20.0, 100.0);
    frame(&mut h, W, Sense::DRAG);
    for i in 1..=6 {
        us(&mut h, at(i));
        h.input.touch_move(1, 20.0 + 10.0 * i as f32, 100.0);
        frame(&mut h, W, Sense::DRAG);
    }
    us(&mut h, at(6) + 50_000);
    h.input.touch_end(1, 80.0, 100.0);
    let (r, v) = frame(&mut h, W, Sense::DRAG);
    assert!(r.drag_stopped);
    assert_eq!(v, (0.0, 0.0));
}

#[test]
fn a_very_fast_pan_is_clamped_to_8000() {
    let mut h = ready(W, Sense::DRAG);
    let ((r, v), _) = pan(&mut h, 120, 9000.0, 3);
    assert!(r.drag_stopped);
    near(v, (8000.0, 0.0), 1.0);
}

#[test]
fn a_mouse_drag_leaves_a_velocity_too() {
    let mut h = ready(W, Sense::DRAG);
    let at = |i: u64| 1_000_000 * i / 60;
    us(&mut h, at(0));
    h.input.pointer_to(20.0, 100.0).press(P);
    frame(&mut h, W, Sense::DRAG);
    for i in 1..=6 {
        us(&mut h, at(i));
        h.input.pointer_to(20.0 + 10.0 * i as f32, 100.0);
        frame(&mut h, W, Sense::DRAG);
    }
    us(&mut h, at(7));
    h.input.pointer_to(90.0, 100.0).release(P);
    let (r, v) = frame(&mut h, W, Sense::DRAG);
    assert!(r.drag_stopped);
    near(v, (600.0, 0.0), 6.0);
}

fn button(h: &mut UiHarness) -> Response {
    frame(h, B, Sense::CLICK_FOCUS).0
}

#[test]
fn a_touch_held_500_ms_is_a_secondary_click_once_and_its_lift_clicks_nothing() {
    let mut h = ready(B, Sense::CLICK_FOCUS);
    h.at_ms(0).input.touch_start(1, 20.0, 20.0);
    button(&mut h);
    let mut fired = Vec::new();
    for ms in [100, 400, 499, 500, 501, 700] {
        h.at_ms(ms);
        fired.push((ms, button(&mut h).secondary_clicked));
    }
    assert_eq!(
        fired,
        [
            (100, false),
            (400, false),
            (499, false),
            (500, true),
            (501, false),
            (700, false)
        ]
    );
    h.at_ms(800).input.touch_end(1, 20.0, 20.0);
    let r = button(&mut h);
    assert!(!r.clicked && !r.double_clicked && !r.secondary_clicked);
    // The next tap is a plain click, not a double click.
    h.at_ms(850).input.touch_start(1, 20.0, 20.0);
    button(&mut h);
    h.at_ms(860).input.touch_end(1, 20.0, 20.0);
    let r = button(&mut h);
    assert!(r.clicked && !r.double_clicked);
}

#[test]
fn a_touch_lifted_at_499_ms_clicks_and_fires_no_long_press() {
    let mut h = ready(B, Sense::CLICK_FOCUS);
    h.at_ms(0).input.touch_start(1, 20.0, 20.0);
    button(&mut h);
    h.at_ms(499).input.touch_end(1, 20.0, 20.0);
    let r = button(&mut h);
    assert!(r.clicked && !r.secondary_clicked);
}

#[test]
fn a_mouse_press_held_long_is_no_long_press() {
    let mut h = ready(B, Sense::CLICK_FOCUS);
    h.at_ms(0).input.pointer_to(20.0, 20.0).press(P);
    button(&mut h);
    h.at_ms(900);
    assert!(!button(&mut h).secondary_clicked);
    h.input.release(P);
    assert!(button(&mut h).clicked);
}

#[test]
fn a_pending_long_press_asks_for_its_frame_and_a_lift_ends_the_ask() {
    let mut h = ready(B, Sense::CLICK_FOCUS);
    h.at_ms(0).input.touch_start(1, 20.0, 20.0);
    h.frame(|ui| {
        let id = ui.id("w");
        ui.interact(id, B, Sense::CLICK_FOCUS);
    });
    h.at_ms(400);
    let (_, out) = h.frame(|ui| {
        let id = ui.id("w");
        ui.interact(id, B, Sense::CLICK_FOCUS);
    });
    assert_eq!(out.repaint, Repaint::At(Time::from_millis(500)));
    h.at_ms(450).input.touch_end(1, 20.0, 20.0);
    h.frame(|ui| {
        let id = ui.id("w");
        ui.interact(id, B, Sense::CLICK_FOCUS);
    });
    h.at_ms(460);
    let (_, out) = h.frame(|ui| {
        let id = ui.id("w");
        ui.interact(id, B, Sense::CLICK_FOCUS);
    });
    assert_eq!(out.repaint, Repaint::Idle);
}

#[test]
fn a_long_press_on_a_tab_is_its_context() {
    let strip = Rect::new(40.0, 40.0, 400.0, 32.0);
    let frame = |h: &mut UiHarness| {
        let mut ctx = None;
        h.frame(|ui| {
            let tabs: Vec<Tab<'_>> = (0..3)
                .map(|k| Tab {
                    key: TabKey(k),
                    title: "A tab",
                    closable: false,
                    progress: None,
                })
                .collect();
            let o = TabStripOptions {
                active: Some(TabKey(0)),
                panel_focused: false,
                busy: None,
                busy_tone: Tone::Neutral,
                card: Rect::new(40.0, 40.0, 400.0, 200.0),
                name: "Sessions",
                header_tint: None,
            };
            ctx = ui.tab_strip("s", strip, &tabs, &o).context;
        });
        ctx
    };
    let mut h = UiHarness::new((500.0, 300.0));
    frame(&mut h);
    h.at_ms(0).input.touch_start(1, 70.0, 56.0);
    assert_eq!(frame(&mut h), None);
    h.at_ms(500);
    let ctx = frame(&mut h);
    assert!(ctx.is_some_and(|(k, _)| k == TabKey(0)), "{ctx:?}");
}

#[test]
fn a_long_press_then_a_drag_drags_and_never_clicks() {
    let mut h = ready(W, Sense::DRAG);
    h.at_ms(0).input.touch_start(1, 50.0, 50.0);
    frame(&mut h, W, Sense::DRAG);
    h.at_ms(500);
    assert!(frame(&mut h, W, Sense::DRAG).0.secondary_clicked);
    h.at_ms(600).input.touch_move(1, 70.0, 50.0);
    let moved = frame(&mut h, W, Sense::DRAG).0;
    assert!(moved.drag_started);
    h.at_ms(700).input.touch_end(1, 70.0, 50.0);
    let lifted = frame(&mut h, W, Sense::DRAG).0;
    assert!(lifted.drag_stopped && !lifted.clicked);
}

#[test]
fn a_press_that_left_the_slop_and_came_back_is_no_long_press() {
    let mut h = ready(B, Sense::CLICK_FOCUS);
    h.at_ms(0).input.touch_start(1, 20.0, 20.0);
    button(&mut h);
    h.at_ms(100).input.touch_move(1, 40.0, 20.0);
    button(&mut h);
    h.at_ms(200).input.touch_move(1, 20.0, 20.0);
    button(&mut h);
    h.at_ms(600);
    assert!(!button(&mut h).secondary_clicked);
}

#[test]
fn a_cancel_before_the_long_press_fires_nothing() {
    let mut h = ready(B, Sense::CLICK_FOCUS);
    h.at_ms(0).input.touch_start(1, 20.0, 20.0);
    button(&mut h);
    h.at_ms(300).input.touch_cancel(1, 20.0, 20.0);
    button(&mut h);
    for ms in [500, 600, 900] {
        h.at_ms(ms);
        let r = button(&mut h);
        assert!(!r.secondary_clicked && !r.clicked && !r.pressed, "{ms} ms");
    }
}

#[test]
fn a_press_on_nothing_fires_no_long_press() {
    let mut h = ready(B, Sense::CLICK_FOCUS);
    h.at_ms(0).input.touch_start(1, 300.0, 200.0);
    button(&mut h);
    h.at_ms(600);
    assert!(!button(&mut h).secondary_clicked);
}

fn pinch_frame(h: &mut UiHarness, other: WidgetId) -> (Option<Pinch>, Option<Pinch>) {
    let mut out = (None, None);
    h.frame(|ui| {
        let id = ui.id("w");
        ui.interact(id, W, Sense::DRAG);
        out = (ui.pinch(id), ui.pinch(other));
    });
    out
}

#[test]
fn two_touches_that_spread_read_a_pinch_that_ends_with_the_lift() {
    let mut h = ready(W, Sense::DRAG);
    let other = WidgetId::ROOT.with("other");
    h.at_ms(0).input.touch_start(1, 100.0, 100.0);
    assert_eq!(pinch_frame(&mut h, other), (None, None));
    h.at_ms(10).input.touch_start(2, 200.0, 100.0);
    let (p, o) = pinch_frame(&mut h, other);
    let p = p.expect("a pinch began");
    assert!(p.started && !p.ended);
    assert_eq!(
        (p.scale, p.center, p.start_center),
        (1.0, (150.0, 100.0), (150.0, 100.0))
    );
    assert_eq!(o, None);
    h.at_ms(20).input.touch_move(2, 300.0, 100.0);
    let p = pinch_frame(&mut h, other).0.expect("it goes on");
    assert!(!p.started && !p.ended);
    assert_eq!(p.scale, 2.0);
    assert_eq!(p.center, (200.0, 100.0));
    assert_eq!(p.start_center, (150.0, 100.0));
    h.at_ms(30).input.touch_end(2, 300.0, 100.0);
    let p = pinch_frame(&mut h, other)
        .0
        .expect("the last frame is read");
    assert!(p.ended && !p.started);
    assert_eq!(p.scale, 2.0);
    assert_eq!(pinch_frame(&mut h, other).0, None);
}

#[test]
fn a_third_touch_changes_no_pinch_and_a_new_one_needs_every_touch_up() {
    let mut h = ready(W, Sense::DRAG);
    let other = WidgetId::ROOT.with("other");
    h.at_ms(0).input.touch_start(1, 100.0, 100.0);
    pinch_frame(&mut h, other);
    h.at_ms(10).input.touch_start(2, 200.0, 100.0);
    pinch_frame(&mut h, other);
    h.at_ms(20).input.touch_start(3, 100.0, 200.0);
    let p = pinch_frame(&mut h, other).0.expect("still the first pinch");
    assert!(!p.started);
    assert_eq!(p.scale, 1.0);
    // The second lifts: the pinch ends, and the third does not take its place.
    h.at_ms(30).input.touch_end(2, 200.0, 100.0);
    assert!(pinch_frame(&mut h, other).0.is_some_and(|p| p.ended));
    assert_eq!(pinch_frame(&mut h, other).0, None);
    // Every touch up, then a fresh pair pinches again.
    h.at_ms(40)
        .input
        .touch_end(3, 100.0, 200.0)
        .touch_end(1, 100.0, 100.0);
    pinch_frame(&mut h, other);
    h.at_ms(1000).input.touch_start(4, 100.0, 100.0);
    pinch_frame(&mut h, other);
    h.at_ms(1010).input.touch_start(5, 140.0, 100.0);
    assert!(pinch_frame(&mut h, other).0.is_some_and(|p| p.started));
}
