//! The arbitration of a touch pan against the widgets and scroll regions it starts on (mobile M3
//! step 4, D349). A unit module: the pan axes and the moving flag are `pub(crate)`, set by the
//! scroll area from step 5, and the tests register them by hand.

use crate::input::PointerButton;
use crate::motion::{Kind, Motion};
use crate::paint::{LayerId, Rect};
use crate::testing::{ScriptedInput, UiHarness};
use crate::ui::{OverlayOptions, Response, Sense};

use super::pointer::PanReading;

const REGION: Rect = Rect::new(0.0, 0.0, 400.0, 300.0);
const BUTTON: Rect = Rect::new(10.0, 10.0, 100.0, 30.0);
/// A point on the button.
const ON: (f32, f32) = (20.0, 20.0);

/// One region of a layout: its key, rect, pan axes and whether it is moving.
#[derive(Clone, Copy)]
struct Region(&'static str, Rect, (bool, bool), bool);

/// The frame's contents: regions in call order, then the button.
#[derive(Clone)]
struct Layout {
    regions: Vec<Region>,
    sense: Sense,
    claim: Option<(bool, bool)>,
}

impl Layout {
    fn new(pan: (bool, bool), sense: Sense) -> Self {
        Self {
            regions: vec![Region("region", REGION, pan, false)],
            sense,
            claim: None,
        }
    }
}

/// What a frame read.
struct Seen {
    button: Response,
    pan: Vec<(&'static str, PanReading, (f32, f32))>,
}

impl Seen {
    fn region(&self, key: &str) -> (PanReading, (f32, f32)) {
        let (_, p, v) = self.pan.iter().find(|r| r.0 == key).expect("a region");
        (*p, *v)
    }
}

fn run(h: &mut UiHarness, l: &Layout) -> Seen {
    let mut seen = None;
    h.frame(|ui| {
        let mut pan = Vec::new();
        let mut ids = Vec::new();
        for r in &l.regions {
            let id = ui.id(r.0);
            let mark = ui.registration_mark();
            ui.interact(id, r.1, Sense::SCROLL);
            ui.set_pan_axes(r.2);
            ui.set_scroll_moving(mark, r.3);
            ids.push((r.0, id));
        }
        let b = ui.id("button");
        let button = ui.interact(b, BUTTON, l.sense);
        if let Some((x, y)) = l.claim {
            ui.set_drag_axes(b, x, y);
        }
        for (key, id) in ids {
            pan.push((key, ui.pan(id), ui.release_velocity(id)));
        }
        seen = Some(Seen { button, pan });
    });
    seen.expect("the frame ran")
}

/// A harness that has drawn `l` once.
fn ready(l: &Layout) -> UiHarness {
    let mut h = UiHarness::new((400.0, 300.0));
    run(&mut h, l);
    h
}

fn at(h: &mut UiHarness, ms: u64) -> &mut ScriptedInput {
    h.at_ms(ms);
    &mut h.input
}

#[test]
fn a_vertical_pan_from_a_button_goes_to_the_region_and_the_button_never_clicks() {
    let l = Layout::new((false, true), Sense::CLICK_FOCUS);
    let mut h = ready(&l);
    at(&mut h, 0).touch_start(1, ON.0, ON.1);
    let s = run(&mut h, &l);
    assert!(s.button.pressed && s.button.press_started);
    at(&mut h, 16).touch_move(1, 20.0, 40.0);
    let s = run(&mut h, &l);
    let (pan, _) = s.region("region");
    assert!(pan.started && pan.active && !pan.ended);
    assert_eq!(
        pan.delta,
        (0.0, 10.0),
        "20 px of travel less the 10 px slop"
    );
    assert!(!s.button.pressed && !s.button.drag_started && !s.button.dragged);
    at(&mut h, 32).touch_move(1, 20.0, 60.0);
    let (pan, _) = run(&mut h, &l).region("region");
    assert!(!pan.started && pan.active);
    assert_eq!(pan.delta, (0.0, 20.0));
    at(&mut h, 48).touch_end(1, 20.0, 60.0);
    let s = run(&mut h, &l);
    let (pan, _) = s.region("region");
    assert!(pan.ended && !pan.active);
    assert!(!s.button.clicked && !s.button.drag_stopped && !s.button.secondary_clicked);
    // The whole pan moved the content by the travel less the slop: 10 + 20.
}

#[test]
fn a_drag_widget_that_never_claims_axes_keeps_the_touch() {
    let l = Layout::new((false, true), Sense::DRAG);
    let mut h = ready(&l);
    at(&mut h, 0).touch_start(1, ON.0, ON.1);
    run(&mut h, &l);
    at(&mut h, 16).touch_move(1, 20.0, 40.0);
    let s = run(&mut h, &l);
    assert!(s.button.drag_started && s.button.dragged);
    assert_eq!(s.button.drag_delta, (0.0, 20.0));
    let (pan, _) = s.region("region");
    assert_eq!(
        pan,
        PanReading {
            active: false,
            ..PanReading::default()
        }
    );
    at(&mut h, 32).touch_end(1, 20.0, 40.0);
    let s = run(&mut h, &l);
    assert!(s.button.drag_stopped && !s.button.clicked);
}

#[test]
fn a_drag_widget_claiming_horizontal_only_yields_a_vertical_pan_and_keeps_a_horizontal_one() {
    let mut l = Layout::new((true, true), Sense::DRAG);
    l.claim = Some((true, false));
    let mut h = ready(&l);
    run(&mut h, &l);
    // Vertical: the region takes it, and the widget's press is cancelled quietly.
    at(&mut h, 0).touch_start(1, ON.0, ON.1);
    run(&mut h, &l);
    at(&mut h, 16).touch_move(1, 20.0, 40.0);
    let s = run(&mut h, &l);
    assert!(s.region("region").0.started);
    assert!(!s.button.drag_started && !s.button.pressed);
    at(&mut h, 32).touch_end(1, 20.0, 40.0);
    let s = run(&mut h, &l);
    assert!(!s.button.drag_stopped && !s.button.clicked);
    // Horizontal: the widget drags.
    at(&mut h, 100).touch_start(1, ON.0, ON.1);
    run(&mut h, &l);
    at(&mut h, 116).touch_move(1, 50.0, 22.0);
    let s = run(&mut h, &l);
    assert!(s.button.drag_started);
    assert_eq!(s.region("region").0, PanReading::default());
}

#[test]
fn nested_regions_split_a_pan_by_axis() {
    let l = Layout {
        regions: vec![
            Region("outer", REGION, (false, true), false),
            Region(
                "inner",
                Rect::new(0.0, 0.0, 300.0, 200.0),
                (true, false),
                false,
            ),
        ],
        sense: Sense::CLICK_FOCUS,
        claim: None,
    };
    let mut h = ready(&l);
    at(&mut h, 0).touch_start(1, 100.0, 100.0);
    run(&mut h, &l);
    at(&mut h, 16).touch_move(1, 130.0, 102.0);
    let s = run(&mut h, &l);
    assert!(s.region("inner").0.started);
    assert!(!s.region("outer").0.started);
    at(&mut h, 32).touch_end(1, 130.0, 102.0);
    run(&mut h, &l);
    at(&mut h, 100).touch_start(1, 100.0, 100.0);
    run(&mut h, &l);
    at(&mut h, 116).touch_move(1, 102.0, 130.0);
    let s = run(&mut h, &l);
    assert!(s.region("outer").0.started);
    assert!(!s.region("inner").0.started);
}

#[test]
fn a_region_on_both_axes_locks_a_shallow_start_and_pans_a_diagonal_free() {
    let l = Layout::new((true, true), Sense::CLICK_FOCUS);
    let mut h = ready(&l);
    // 10 degrees: tan 0.176 < 0.414, locked to x.
    at(&mut h, 0).touch_start(1, 100.0, 100.0);
    run(&mut h, &l);
    at(&mut h, 16).touch_move(1, 130.0, 105.3);
    let pan = run(&mut h, &l).region("region").0;
    assert!(pan.started);
    assert_eq!(pan.delta.1, 0.0);
    assert!((pan.delta.0 - 20.0).abs() < 0.2, "{:?}", pan.delta);
    // Later moves keep the lock.
    at(&mut h, 32).touch_move(1, 140.0, 140.0);
    let pan = run(&mut h, &l).region("region").0;
    assert_eq!(pan.delta, (10.0, 0.0));
    at(&mut h, 48).touch_end(1, 140.0, 140.0);
    run(&mut h, &l);
    // 40 degrees: tan 0.839 > 0.414, both axes.
    at(&mut h, 100).touch_start(1, 100.0, 100.0);
    run(&mut h, &l);
    at(&mut h, 116).touch_move(1, 123.0, 119.3);
    let pan = run(&mut h, &l).region("region").0;
    assert!(pan.delta.0 > 0.0 && pan.delta.1 > 0.0, "{:?}", pan.delta);
    at(&mut h, 132).touch_move(1, 133.0, 129.3);
    let pan = run(&mut h, &l).region("region").0;
    assert_eq!(pan.delta, (10.0, 10.0));
}

#[test]
fn a_press_on_a_moving_region_catches_it_and_reaches_nothing_under_it() {
    let mut l = Layout::new((false, true), Sense::CLICK_FOCUS);
    l.regions[0].3 = true;
    let mut h = ready(&l);
    at(&mut h, 0).touch_start(1, ON.0, ON.1);
    let s = run(&mut h, &l);
    assert!(s.region("region").0.caught);
    assert!(!s.button.pressed && !s.button.press_started && !s.button.focused);
    at(&mut h, 40).touch_end(1, ON.0, ON.1);
    let s = run(&mut h, &l);
    assert!(!s.button.clicked && !s.button.focused);
    assert!(!s.region("region").0.ended);
    // A press that lands then travels pans the region, with no widget held.
    at(&mut h, 100).touch_start(1, ON.0, ON.1);
    assert!(run(&mut h, &l).region("region").0.caught);
    at(&mut h, 116).touch_move(1, 20.0, 50.0);
    let s = run(&mut h, &l);
    assert!(s.region("region").0.started);
    let d = s.region("region").0.delta;
    assert!(d.0 == 0.0 && (d.1 - 20.0).abs() < 1e-3, "{d:?}");
    // A moving region that no longer moves is no catch.
    at(&mut h, 130).touch_end(1, 20.0, 50.0);
    run(&mut h, &l);
    l.regions[0].3 = false;
    run(&mut h, &l);
    at(&mut h, 200).touch_start(1, ON.0, ON.1);
    let s = run(&mut h, &l);
    assert!(!s.region("region").0.caught && s.button.pressed);
}

#[test]
fn a_cancel_mid_pan_ends_it_with_no_velocity() {
    let l = Layout::new((false, true), Sense::CLICK_FOCUS);
    let mut h = ready(&l);
    at(&mut h, 0).touch_start(1, 20.0, 20.0);
    run(&mut h, &l);
    for i in 1..=5u64 {
        at(&mut h, i * 16).touch_move(1, 20.0, 20.0 + 20.0 * i as f32);
        run(&mut h, &l);
    }
    at(&mut h, 96).touch_cancel(1, 20.0, 120.0);
    let s = run(&mut h, &l);
    let (pan, v) = s.region("region");
    assert!(pan.ended && !pan.active);
    assert_eq!(v, (0.0, 0.0));
    assert!(!s.button.clicked && !s.button.drag_stopped);
}

#[test]
fn the_pans_end_carries_the_finger_velocity_on_its_locked_axes() {
    let l = Layout::new((true, true), Sense::CLICK_FOCUS);
    let mut h = ready(&l);
    let t = |i: u64| 1_000_000 * i / 60;
    let us = |h: &mut UiHarness, i: u64| {
        h.input.set_time(std::time::Duration::from_micros(t(i)));
    };
    us(&mut h, 0);
    h.input.touch_start(1, 20.0, 20.0);
    run(&mut h, &l);
    // 600 px/s down with 60 px/s of sideways drift: locked to y.
    let pos = |i: u64| {
        (
            20.0 + 60.0 * t(i) as f32 / 1e6,
            20.0 + 600.0 * t(i) as f32 / 1e6,
        )
    };
    for i in 1..=8 {
        us(&mut h, i);
        h.input.touch_move(1, pos(i).0, pos(i).1);
        run(&mut h, &l);
    }
    us(&mut h, 9);
    h.input.touch_end(1, pos(9).0, pos(9).1);
    let s = run(&mut h, &l);
    let (pan, v) = s.region("region");
    assert!(pan.ended);
    assert_eq!(v.0, 0.0);
    assert!((v.1 - 600.0).abs() <= 6.0, "{v:?}");
    // Read once: the frame after has none.
    us(&mut h, 10);
    assert_eq!(run(&mut h, &l).region("region").1, (0.0, 0.0));
}

#[test]
fn a_mouse_drag_in_the_same_layout_is_as_it_was_at_0_7_0() {
    let l = Layout::new((false, true), Sense::CLICK_FOCUS);
    let mut h = ready(&l);
    h.input.pointer_to(ON.0, ON.1).press(PointerButton::Primary);
    run(&mut h, &l);
    h.input.pointer_to(20.0, 80.0);
    let s = run(&mut h, &l);
    assert!(s.button.pressed, "the press is still the button's");
    assert_eq!(s.region("region").0, PanReading::default());
    h.input.release(PointerButton::Primary);
    let s = run(&mut h, &l);
    assert!(!s.button.clicked, "released away from the button");
    assert_eq!(s.region("region").0, PanReading::default());
}

#[test]
fn a_pan_over_a_modal_goes_to_no_region_below_it() {
    let l = Layout::new((false, true), Sense::CLICK_FOCUS);
    let mut h = ready(&l);
    let opts = OverlayOptions {
        layer: LayerId::Modal,
        modal: true,
        dismiss_on_escape: false,
        dismiss_on_outside_press: false,
        dismiss_on_focus_loss: false,
        anchor: None,
        initial_focus: None,
        open: Motion::of(Kind::Appear),
        close: Motion::of(Kind::Dismiss),
    };
    let frame = |h: &mut UiHarness| {
        let mut pan = PanReading::default();
        h.frame(|ui| {
            let rid = ui.id("region");
            ui.interact(rid, REGION, Sense::SCROLL);
            ui.set_pan_axes((false, true));
            ui.overlay(
                "modal",
                &opts,
                |_, _, _| {},
                |ui, _| {
                    let card = ui.id("card");
                    ui.interact(card, Rect::new(200.0, 150.0, 100.0, 100.0), Sense::HOVER);
                },
            );
            pan = ui.pan(rid);
        });
        pan
    };
    frame(&mut h);
    frame(&mut h);
    at(&mut h, 0).touch_start(1, 20.0, 20.0);
    frame(&mut h);
    at(&mut h, 16).touch_move(1, 20.0, 60.0);
    assert_eq!(frame(&mut h), PanReading::default());
}
