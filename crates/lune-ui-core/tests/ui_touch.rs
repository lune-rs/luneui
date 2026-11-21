//! Touch reduced to the single-pointer path (mobile M2 step 3, C1(c), C1(g), D344, D345): a tap
//! clicks as a mouse click does, a cancel drops the press quietly, secondary touches never touch
//! the pointer, the mouse copy a platform makes of a touch is dropped for 500 ms, and the insets
//! latch.

use lune_ui_core::drag::{DropResult, Metrics, ReorderDrag, Strip};
use lune_ui_core::input::{PointerButton, PointerKind};
use lune_ui_core::layout::Insets;
use lune_ui_core::motion::{Kind, Motion};
use lune_ui_core::paint::{LayerId, Rect};
use lune_ui_core::testing::UiHarness;
use lune_ui_core::ui::{Dismissal, OverlayOptions, Response, Sense, Ui};

const P: PointerButton = PointerButton::Primary;
const A: Rect = Rect::new(10.0, 10.0, 100.0, 30.0);
const B: Rect = Rect::new(130.0, 10.0, 100.0, 30.0);

/// Clicks counted per button over the frames of a test.
#[derive(Default)]
struct Clicks([u32; 2]);

impl Clicks {
    fn frame(&mut self, h: &mut UiHarness, sense: Sense) -> [Response; 2] {
        let r = two(h, sense);
        for (n, r) in self.0.iter_mut().zip(&r) {
            *n += u32::from(r.clicked);
        }
        r
    }
}

fn harness() -> UiHarness {
    UiHarness::new((400.0, 300.0))
}

/// One frame of two buttons `a` and `b` of `sense`; returns their responses.
fn two(h: &mut UiHarness, sense: Sense) -> [Response; 2] {
    let mut out = None;
    h.frame(|ui| {
        let (a, b) = (ui.id("a"), ui.id("b"));
        out = Some([ui.interact(a, A, sense), ui.interact(b, B, sense)]);
    });
    out.expect("the frame ran")
}

/// A harness that has drawn the two buttons once, so the next frame's input has regions to hit.
fn ready(sense: Sense) -> UiHarness {
    let mut h = harness();
    two(&mut h, sense);
    h
}

/// Touch focus follows the tap, not the press (D361): the press frame equals the mouse's but for
/// `focused`, and the lift focuses.
#[test]
fn a_tap_answers_as_a_mouse_click_does_except_the_hover_after_the_lift() {
    let mut mouse = ready(Sense::CLICK_FOCUS);
    mouse.input.pointer_to(20.0, 20.0).press(P);
    let m_press = two(&mut mouse, Sense::CLICK_FOCUS);
    mouse.input.release(P);
    let m_release = two(&mut mouse, Sense::CLICK_FOCUS);

    let mut touch = ready(Sense::CLICK_FOCUS);
    touch.input.touch_start(1, 20.0, 20.0);
    let t_press = two(&mut touch, Sense::CLICK_FOCUS);
    touch.input.touch_end(1, 20.0, 20.0);
    let t_release = two(&mut touch, Sense::CLICK_FOCUS);

    assert!(m_press[0].focused, "a mouse press focuses on the press");
    assert!(
        !t_press[0].focused,
        "a touch press leaves focus to the tap (D361)"
    );
    let mut t = t_press.clone();
    t[0].focused = m_press[0].focused;
    assert_eq!(t, m_press);
    assert!(t_press[0].pressed && t_press[0].press_started);
    assert!(t_release[0].clicked && m_release[0].clicked);
    assert!(t_release[0].focused, "the tap focuses");
    assert!(m_release[0].hovered && !t_release[0].hovered);
    let mut t = t_release.clone();
    t[0].hovered = true;
    assert_eq!(t, m_release);
    // The click happened once.
    let after = two(&mut touch, Sense::CLICK_FOCUS);
    assert!(!after[0].clicked && !after[0].pressed);
}

#[test]
fn after_the_lift_nothing_is_hovered() {
    let mut h = ready(Sense::CLICK_FOCUS);
    h.input.touch_start(1, 20.0, 20.0);
    two(&mut h, Sense::CLICK_FOCUS);
    h.input.touch_end(1, 20.0, 20.0);
    let mut pos = Some((0.0, 0.0));
    h.frame(|ui| {
        let a = ui.id("a");
        ui.interact(a, A, Sense::CLICK_FOCUS);
        pos = ui.pointer_pos();
    });
    assert_eq!(pos, None);
    assert!(!two(&mut h, Sense::CLICK_FOCUS)[0].hovered);
}

#[test]
fn a_cancel_after_a_drag_drops_the_press_with_no_click_and_no_drag_stop() {
    let mut h = ready(Sense::DRAG);
    let mut seen = Vec::new();
    h.input.touch_start(1, 20.0, 20.0);
    seen.push(two(&mut h, Sense::DRAG));
    h.input.touch_move(1, 40.0, 20.0);
    seen.push(two(&mut h, Sense::DRAG));
    assert!(seen[1][0].drag_started, "the drag began past 4 px");
    h.input.touch_cancel(1, 40.0, 20.0);
    seen.push(two(&mut h, Sense::DRAG));
    seen.push(two(&mut h, Sense::DRAG));
    for r in &seen[2..] {
        assert!(!r[0].clicked && !r[0].drag_stopped && !r[0].pressed && !r[0].secondary_clicked);
        assert!(!r[0].hovered);
    }
}

#[test]
fn a_cancel_ends_a_reorder_gesture_as_cancelled_with_no_commit() {
    let widths = [100.0, 100.0, 100.0];
    let mut drag = ReorderDrag::new();
    let mut h = harness();
    let frame = |h: &mut UiHarness, drag: &mut ReorderDrag| {
        let mut out = None;
        h.frame(|ui| {
            let id = ui.id("item");
            let r = ui.interact(id, Rect::new(20.0, 40.0, 100.0, 30.0), Sense::DRAG);
            let strip = Strip {
                index: 0,
                origin: 20.0,
                metrics: Metrics {
                    widths: &widths,
                    spacing: 5.0,
                    viewport: 400.0,
                    offset: 0.0,
                    extent: 310.0,
                },
            };
            out = Some(ui.drag_gesture(id, &r, drag, &strip));
        });
        out.expect("the frame ran")
    };
    frame(&mut h, &mut drag);
    h.at_ms(10).input.touch_start(1, 70.0, 50.0);
    frame(&mut h, &mut drag);
    h.at_ms(20).input.touch_move(1, 130.0, 50.0);
    frame(&mut h, &mut drag);
    h.at_ms(30).input.touch_move(1, 190.0, 50.0);
    assert!(frame(&mut h, &mut drag).dragging);
    h.at_ms(40).input.touch_cancel(1, 190.0, 50.0);
    let f = frame(&mut h, &mut drag);
    assert!(f.cancelled);
    assert_eq!(f.result, DropResult::None);
    assert!(!f.dragging);
}

#[test]
fn a_second_touch_never_touches_the_pointer_and_the_first_lift_clicks_the_first() {
    let mut h = ready(Sense::CLICK);
    let mut c = Clicks::default();
    h.input.touch_start(1, 20.0, 20.0);
    c.frame(&mut h, Sense::CLICK);
    h.input.touch_start(2, 140.0, 20.0);
    let r = c.frame(&mut h, Sense::CLICK);
    assert!(r[0].pressed && !r[1].pressed && !r[1].hovered);
    h.input.touch_end(2, 140.0, 20.0);
    c.frame(&mut h, Sense::CLICK);
    assert_eq!(c.0, [0, 0]);
    h.input.touch_end(1, 20.0, 20.0);
    c.frame(&mut h, Sense::CLICK);
    assert_eq!(c.0, [1, 0]);
}

#[test]
fn a_new_touch_is_no_primary_until_every_touch_is_up() {
    let mut h = ready(Sense::CLICK);
    let mut c = Clicks::default();
    h.input
        .touch_start(1, 20.0, 20.0)
        .touch_start(2, 140.0, 20.0);
    c.frame(&mut h, Sense::CLICK);
    h.input.touch_end(1, 20.0, 20.0);
    c.frame(&mut h, Sense::CLICK);
    assert_eq!(c.0, [1, 0], "the primary's lift clicks");
    h.input.touch_start(3, 20.0, 20.0);
    c.frame(&mut h, Sense::CLICK);
    h.input.touch_end(3, 20.0, 20.0);
    c.frame(&mut h, Sense::CLICK);
    assert_eq!(c.0, [1, 0], "C was never the primary");
    h.input.touch_end(2, 140.0, 20.0);
    c.frame(&mut h, Sense::CLICK);
    assert_eq!(c.0, [1, 0]);
    h.input.touch_start(4, 20.0, 20.0);
    c.frame(&mut h, Sense::CLICK);
    h.input.touch_end(4, 20.0, 20.0);
    c.frame(&mut h, Sense::CLICK);
    assert_eq!(c.0, [2, 0], "all up: a tap clicks again");
}

#[test]
fn a_mouse_copy_of_a_touch_is_dropped_for_500_ms_after_the_lift() {
    let mut h = ready(Sense::CLICK);
    let mut clicks = 0;
    let mut run = |h: &mut UiHarness| {
        clicks += u32::from(two(h, Sense::CLICK)[0].clicked);
        clicks
    };
    h.at_ms(0).input.touch_start(1, 20.0, 20.0);
    run(&mut h);
    // The lift and the mouse copy (a move, a press, a release) in one frame.
    h.at_ms(50).input.touch_end(1, 20.0, 20.0);
    h.input.pointer_to(20.0, 20.0).click(P);
    assert_eq!(run(&mut h), 1);
    // The same copy 100 ms later.
    h.at_ms(150).input.pointer_to(20.0, 20.0).click(P);
    assert_eq!(run(&mut h), 1);
    h.at_ms(549).input.pointer_to(20.0, 20.0).click(P);
    assert_eq!(run(&mut h), 1, "499 ms after the lift");
    h.at_ms(550).input.pointer_to(20.0, 20.0).click(P);
    assert_eq!(run(&mut h), 2, "500 ms after the lift");
}

#[test]
fn a_wheel_scroll_passes_the_mouse_guard() {
    let mut h = harness();
    let scroll = |h: &mut UiHarness| {
        let mut got = (0.0, 0.0);
        h.frame(|ui| {
            let id = ui.id("s");
            got = ui.interact(id, A, Sense::SCROLL).scroll_pixels;
        });
        got
    };
    scroll(&mut h);
    h.at_ms(0).input.touch_start(1, 500.0, 500.0);
    scroll(&mut h);
    h.at_ms(10).input.touch_end(1, 500.0, 500.0);
    scroll(&mut h);
    h.at_ms(110)
        .input
        .pointer_to(20.0, 20.0)
        .scroll_pixels(0.0, 7.0);
    assert_eq!(scroll(&mut h), (0.0, 7.0));
}

#[test]
fn a_duplicate_start_and_stray_ids_do_not_panic_and_do_not_click() {
    let mut h = ready(Sense::CLICK);
    h.input
        .touch_end(9, 20.0, 20.0)
        .touch_move(9, 20.0, 20.0)
        .touch_cancel(9, 20.0, 20.0);
    let r = two(&mut h, Sense::CLICK);
    assert!(!r[0].clicked && !r[0].pressed && !r[0].hovered);
    h.input
        .touch_start(1, 20.0, 20.0)
        .touch_start(1, 22.0, 20.0);
    assert!(two(&mut h, Sense::CLICK)[0].pressed);
    h.input.touch_end(9, 20.0, 20.0).touch_end(1, 22.0, 20.0);
    let r = two(&mut h, Sense::CLICK);
    assert!(r[0].clicked, "the duplicate start was a move");
    h.input.touch_end(1, 22.0, 20.0);
    assert!(
        !two(&mut h, Sense::CLICK)[0].clicked,
        "a second end is stray"
    );
}

#[test]
fn losing_focus_with_a_touch_down_cancels_it_and_the_mouse_clicks_after_500_ms() {
    let mut h = ready(Sense::CLICK);
    h.at_ms(0).input.touch_start(1, 20.0, 20.0);
    assert!(two(&mut h, Sense::CLICK)[0].pressed);
    h.at_ms(100).input.focus(false);
    let r = two(&mut h, Sense::CLICK);
    assert!(!r[0].clicked && !r[0].pressed);
    h.at_ms(200).input.focus(true).touch_end(1, 20.0, 20.0);
    assert!(
        !two(&mut h, Sense::CLICK)[0].clicked,
        "the end is stray now"
    );
    h.at_ms(599).input.pointer_to(20.0, 20.0).click(P);
    assert!(!two(&mut h, Sense::CLICK)[0].clicked);
    h.at_ms(600).input.pointer_to(20.0, 20.0).click(P);
    assert!(two(&mut h, Sense::CLICK)[0].clicked);
}

#[test]
fn pointer_kind_follows_touch_and_the_mouse_press_after_the_window() {
    let mut h = ready(Sense::CLICK);
    let kind = |h: &mut UiHarness| {
        let mut k = PointerKind::Mouse;
        h.frame(|ui| k = ui.pointer_kind());
        k
    };
    assert_eq!(kind(&mut h), PointerKind::Mouse);
    h.at_ms(0).input.pointer_to(20.0, 20.0).click(P);
    assert_eq!(kind(&mut h), PointerKind::Mouse);
    h.at_ms(10).input.touch_start(1, 20.0, 20.0);
    assert_eq!(kind(&mut h), PointerKind::Touch);
    h.at_ms(20).input.touch_end(1, 20.0, 20.0);
    assert_eq!(kind(&mut h), PointerKind::Touch);
    h.at_ms(100).input.pointer_to(20.0, 20.0).click(P);
    assert_eq!(kind(&mut h), PointerKind::Touch, "inside the window");
    h.at_ms(520).input.pointer_to(20.0, 20.0).click(P);
    assert_eq!(kind(&mut h), PointerKind::Mouse);
}

#[test]
fn the_insets_default_to_zero_latch_and_update_independently() {
    let mut h = harness();
    let read = |h: &mut UiHarness| {
        let mut got = None;
        h.frame(|ui| got = Some((ui.safe_area(), ui.keyboard_inset())));
        got.expect("the frame ran")
    };
    assert_eq!(read(&mut h), (Insets::ZERO, Insets::ZERO));
    let safe = Insets::new(0.0, 47.0, 0.0, 34.0);
    let keys = Insets::new(0.0, 0.0, 0.0, 300.0);
    h.input.safe_area(safe);
    assert_eq!(read(&mut h), (safe, Insets::ZERO));
    assert_eq!(read(&mut h), (safe, Insets::ZERO), "latched with no event");
    h.input.keyboard_inset(keys);
    assert_eq!(read(&mut h), (safe, keys));
    h.input.keyboard_inset(Insets::ZERO);
    assert_eq!(read(&mut h), (safe, Insets::ZERO));
}

fn popover(ui: &mut Ui<'_>, open: bool) -> Option<Dismissal> {
    let a = ui.id("a");
    ui.interact(a, A, Sense::CLICK_FOCUS);
    if !open {
        return None;
    }
    let opts = OverlayOptions {
        layer: LayerId::Popup,
        modal: false,
        dismiss_on_escape: true,
        dismiss_on_outside_press: true,
        dismiss_on_focus_loss: false,
        anchor: None,
        initial_focus: None,
        open: Motion::of(Kind::Appear),
        close: Motion::of(Kind::Appear),
    };
    let (f, ()) = ui.overlay(
        "pop",
        &opts,
        |_, _, _| {},
        |ui, _| {
            let id = ui.id("inside");
            ui.interact(id, Rect::new(200.0, 100.0, 80.0, 40.0), Sense::CLICK);
        },
    );
    f.dismissed
}

/// A touch focuses a focusable widget at its tap, not at its press (D361).
#[test]
fn a_touch_outside_a_popover_dismisses_it_and_a_touch_focuses_a_focusable_widget() {
    let mut h = harness();
    for _ in 0..2 {
        h.frame(|ui| {
            popover(ui, true);
        });
    }
    h.input.touch_start(1, 300.0, 250.0);
    let mut d = None;
    h.frame(|ui| d = popover(ui, true));
    assert_eq!(d, Some(Dismissal::OutsidePress));
    h.input.touch_end(1, 300.0, 250.0);
    h.frame(|ui| {
        popover(ui, false);
    });

    let mut h = ready(Sense::CLICK_FOCUS);
    h.input.touch_start(1, 20.0, 20.0);
    two(&mut h, Sense::CLICK_FOCUS);
    let r = two(&mut h, Sense::CLICK_FOCUS);
    assert!(
        !r[0].focused && !r[1].focused,
        "a held touch has not focused yet (D361)"
    );
    h.input.touch_end(1, 20.0, 20.0);
    let r = two(&mut h, Sense::CLICK_FOCUS);
    assert!(r[0].focused && !r[1].focused);
}

#[test]
fn a_touch_drag_past_the_threshold_reads_as_the_mouse_drag_does() {
    let mut mouse = ready(Sense::DRAG);
    mouse.input.pointer_to(20.0, 20.0).press(P);
    let m0 = two(&mut mouse, Sense::DRAG);
    mouse.input.pointer_to(50.0, 20.0);
    let m1 = two(&mut mouse, Sense::DRAG);
    let mut touch = ready(Sense::DRAG);
    touch.input.touch_start(1, 20.0, 20.0);
    let t0 = two(&mut touch, Sense::DRAG);
    touch.input.touch_move(1, 50.0, 20.0);
    let t1 = two(&mut touch, Sense::DRAG);
    assert_eq!(t0, m0);
    assert_eq!(t1, m1);
    assert!(t1[0].drag_started && t1[0].drag_delta == (30.0, 0.0));
}

#[test]
fn two_quick_taps_double_click() {
    let mut h = ready(Sense::CLICK);
    let mut taps = Vec::new();
    for t in [0, 100] {
        h.at_ms(t).input.touch_start(1, 20.0, 20.0);
        two(&mut h, Sense::CLICK);
        h.at_ms(t + 30).input.touch_end(1, 20.0, 20.0);
        taps.push(two(&mut h, Sense::CLICK)[0].clone());
    }
    assert!(taps[0].clicked && !taps[0].double_clicked);
    assert!(taps[1].clicked && taps[1].double_clicked);
}

#[test]
fn a_touch_takes_over_from_a_held_mouse_press_without_a_click() {
    let mut h = ready(Sense::CLICK);
    h.at_ms(0).input.pointer_to(20.0, 20.0).press(P);
    assert!(two(&mut h, Sense::CLICK)[0].pressed);
    h.at_ms(10).input.touch_start(1, 140.0, 20.0);
    let r = two(&mut h, Sense::CLICK);
    assert!(!r[0].pressed && !r[0].clicked && r[1].pressed);
    h.at_ms(20).input.touch_end(1, 140.0, 20.0);
    let r = two(&mut h, Sense::CLICK);
    assert!(r[1].clicked && !r[0].clicked);
}
