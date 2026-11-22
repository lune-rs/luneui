//! The bottom sheet (`overlay.bottom-sheet`, D371): the detent geometry, opening and closing by the
//! app, Escape and the scrim, a finger's drag and release to each detent and to closed, a scrolling
//! content under the finger, the grabber, the keyboard inset, a disabled scope, the motion tiers.

use lune_ui_core::access::{Role, State};
use lune_ui_core::input::{FrameOutput, Key, NamedKey, PointerButton, Repaint};
use lune_ui_core::layout::Insets;
use lune_ui_core::motion::MotionTier;
use lune_ui_core::paint::Rect;
use lune_ui_core::testing::{UiHarness, disabled_violations};
use lune_ui_core::ui::WidgetId;
use lune_ui_core::widgets::bottom_sheet::{BottomSheet, Detent};
use lune_ui_core::widgets::scroll_area::{Extent, ScrollOptions};

const WIN: (f32, f32) = (400.0, 800.0);
/// The top safe area: the largest height is 800 - 60 - 10.
const TOP: f32 = 60.0;
const H: f32 = 730.0;

fn sheet_id() -> WidgetId {
    WidgetId::ROOT.with("sheet")
}

fn grabber_id() -> WidgetId {
    sheet_id().with("grabber")
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Seen {
    /// The sheet node's top, `None` when no sheet node is drawn.
    top: Option<f32>,
    value: bool,
    detent: Option<usize>,
    dismissed: bool,
    content: Option<Rect>,
    offset: f64,
    repaint: Option<Repaint>,
}

struct Rig {
    h: UiHarness,
    ms: u64,
    open: bool,
    detents: Vec<Detent>,
    scrolling: bool,
    disabled: bool,
}

impl Rig {
    fn new(detents: &[Detent]) -> Self {
        let mut h = UiHarness::new(WIN);
        h.input.safe_area(Insets::new(0.0, TOP, 0.0, 34.0));
        Rig {
            h,
            ms: 0,
            open: false,
            detents: detents.to_vec(),
            scrolling: false,
            disabled: false,
        }
    }

    fn frame(&mut self) -> (Seen, FrameOutput) {
        self.ms += 16;
        self.h.at_ms(self.ms);
        let mut seen = Seen::default();
        let (open, detents, scrolling, disabled) =
            (&mut self.open, &self.detents, self.scrolling, self.disabled);
        let (_, out) = self.h.frame(|ui| {
            let sheet = BottomSheet::new("Filters").with_detents(detents);
            let mut run = |ui: &mut lune_ui_core::ui::Ui<'_>| {
                ui.bottom_sheet("sheet", open, &sheet, |ui, rect| {
                    seen.content = Some(rect);
                    if scrolling {
                        let o = ScrollOptions::default();
                        let r = ui.scroll_area("list", rect, &o, |_, _| {
                            ((), Extent::new(f64::from(rect.w), 3000.0))
                        });
                        seen.offset = r.offset.1;
                    }
                })
            };
            let r = if disabled {
                ui.with_enabled(false, |ui| run(ui))
            } else {
                run(ui)
            };
            seen.value = r.value.is_some();
            seen.detent = r.detent;
            seen.dismissed = r.dismissed;
        });
        seen.top = out
            .tree
            .nodes
            .iter()
            .find(|n| n.role == Role::Dialog && n.name == "Filters")
            .map(|n| n.bounds.y);
        seen.repaint = Some(out.repaint);
        (seen, out)
    }

    /// Frames until `ms` more milliseconds of app time have passed; the last one.
    fn run(&mut self, ms: u64) -> Seen {
        let end = self.ms + ms;
        let mut last = self.frame().0;
        while self.ms < end {
            last = self.frame().0;
        }
        last
    }

    /// Opens the sheet and lets it settle.
    fn opened(detents: &[Detent]) -> Rig {
        let mut rig = Rig::new(detents);
        rig.frame();
        rig.open = true;
        rig.run(1500);
        rig
    }

    fn top(&mut self) -> f32 {
        self.frame().0.top.expect("the sheet is drawn")
    }

    /// A finger down at `at`, then a move in `steps` frames of `step_ms` to `to`; held.
    fn drag(&mut self, at: (f32, f32), to: (f32, f32), steps: u32, step_ms: u64) {
        self.h.input.touch_start(1, at.0, at.1);
        self.frame();
        for i in 1..=steps {
            let k = i as f32 / steps as f32;
            let (x, y) = (at.0 + (to.0 - at.0) * k, at.1 + (to.1 - at.1) * k);
            self.ms += step_ms.saturating_sub(16);
            self.h.input.touch_move(1, x, y);
            self.frame();
        }
    }

    fn lift(&mut self, at: (f32, f32)) -> Seen {
        self.h.input.touch_end(1, at.0, at.1);
        self.frame().0
    }
}

const BOTH: [Detent; 2] = [Detent::Fraction(0.5), Detent::Fraction(1.0)];

#[test]
fn the_detents_are_fractions_and_heights_of_the_largest_height_clamped_and_sorted() {
    let cases: [(&[Detent], f32); 8] = [
        (&[Detent::Fraction(0.5)], 800.0 - 0.5 * H),
        (&[Detent::Fraction(1.0)], 800.0 - H),
        (&[Detent::Height(300.0)], 500.0),
        (&[Detent::Height(5000.0)], 800.0 - H),
        (&[Detent::Fraction(2.0)], 800.0 - H),
        (&[], 800.0 - H),
        // The smallest height is detent 0 whatever the order.
        (&[Detent::Fraction(1.0), Detent::Height(200.0)], 600.0),
        (&[Detent::Fraction(f32::NAN), Detent::Height(250.0)], 550.0),
    ];
    for (detents, top) in cases {
        let mut rig = Rig::opened(detents);
        let got = rig.top();
        assert!((got - top).abs() < 0.01, "{detents:?}: {got} vs {top}");
    }
    // Equal heights are one detent: a grabber click wraps back onto it.
    let mut rig = Rig::opened(&[Detent::Fraction(0.5), Detent::Height(0.5 * H)]);
    let rest = rig.top();
    click_grabber(&mut rig);
    assert_eq!(rig.run(1000).detent, Some(0));
    assert!((rig.top() - rest).abs() < 0.01);
}

#[test]
fn opening_slides_up_from_below_the_window_and_settles_at_the_first_detent() {
    let mut rig = Rig::new(&BOTH);
    assert_eq!(rig.frame().0.top, None, "closed draws nothing");
    rig.open = true;
    let first = rig.frame().0;
    assert!(
        first.value && first.top.expect("drawn") > 780.0,
        "{first:?}"
    );
    let mid = rig.run(150).top.expect("drawn");
    assert!(mid < 780.0 && mid > 800.0 - 0.5 * H, "{mid}");
    let end = rig.run(1500);
    assert!((end.top.expect("drawn") - (800.0 - 0.5 * H)).abs() < 0.01);
    assert_eq!(end.detent, Some(0));
    assert_eq!(end.repaint, Some(Repaint::Idle), "settled");
    let (_, out) = rig.frame();
    let node = out.tree.get(sheet_id()).expect("the sheet node");
    assert_eq!(node.role, Role::Dialog);
    assert!(node.states.contains(State::Modal));
}

#[test]
fn the_app_closing_slides_it_out_and_the_overlay_then_ends() {
    let mut rig = Rig::opened(&BOTH);
    rig.open = false;
    let first = rig.frame().0;
    assert!(first.value, "still drawn while it slides");
    assert!(!first.dismissed, "the app closed it, the sheet did not");
    let mid = rig.run(150).top.expect("still sliding");
    assert!(mid > 800.0 - 0.5 * H && mid < 800.0, "{mid}");
    let end = rig.run(1500);
    assert_eq!((end.top, end.value, end.detent), (None, false, None));
    assert_eq!(end.repaint, Some(Repaint::Idle));
    // And it never closes by a cross-fade: the surface keeps its size while it moves.
    let mut rig = Rig::opened(&BOTH);
    let (_, before) = rig.frame();
    rig.open = false;
    rig.run(100);
    let (_, during) = rig.frame();
    let w = |o: &FrameOutput| o.tree.get(sheet_id()).map(|n| n.bounds.w);
    assert_eq!(w(&before), w(&during));
}

#[test]
fn reopening_while_it_slides_out_turns_it_back() {
    let mut rig = Rig::opened(&BOTH);
    rig.open = false;
    let started = rig.run(100).top.expect("sliding");
    rig.open = true;
    let back = rig.frame().0.top.expect("still there");
    assert!((back - started).abs() < 40.0, "no jump: {started} {back}");
    assert!((rig.run(1500).top.expect("up") - (800.0 - 0.5 * H)).abs() < 0.01);
}

#[test]
fn escape_and_a_press_on_the_scrim_dismiss_and_a_press_on_the_sheet_does_not() {
    let mut rig = Rig::opened(&BOTH);
    rig.h.input.key_down(Key::Named(NamedKey::Escape), None);
    let s = rig.frame().0;
    assert!(s.dismissed && !rig.open, "Escape: {s:?}");
    assert_eq!(rig.run(1500).top, None);

    let mut rig = Rig::opened(&BOTH);
    // Above the sheet (its top is 435): the scrim.
    rig.h.input.pointer_to(200.0, 200.0);
    rig.h.input.click(PointerButton::Primary);
    let s = rig.frame().0;
    assert!(s.dismissed && !rig.open, "scrim: {s:?}");
    assert_eq!(rig.run(1500).top, None);

    let mut rig = Rig::opened(&BOTH);
    rig.h.input.pointer_to(200.0, 600.0);
    rig.h.input.click(PointerButton::Primary);
    let s = rig.frame().0;
    assert!(!s.dismissed && rig.open, "a click on the sheet: {s:?}");

    // A finger's tap on the sheet's body does not reach the scrim either.
    rig.h.input.touch_start(1, 200.0, 600.0);
    rig.frame();
    rig.h.input.touch_end(1, 200.0, 600.0);
    let s = rig.frame().0;
    assert!(!s.dismissed && rig.open, "a tap on the sheet: {s:?}");
    // A tap above it is the scrim's.
    rig.h.input.touch_start(1, 200.0, 200.0);
    rig.frame();
    rig.h.input.touch_end(1, 200.0, 200.0);
    let s = rig.frame().0;
    assert!(s.dismissed && !rig.open, "a tap on the scrim: {s:?}");
}

const REST0: f32 = 800.0 - 0.5 * H;
const REST1: f32 = 800.0 - H;

#[test]
fn a_drag_follows_the_finger_and_a_slow_release_picks_the_nearest_detent() {
    // Down on the grabber, up 100 px, held still, lifted: back to the first detent.
    let mut rig = Rig::opened(&BOTH);
    let at = (200.0, REST0 + 10.0);
    rig.drag(at, (200.0, at.1 - 100.0), 6, 16);
    let held = rig.frame().0;
    // 100 px of travel less the 10 px touch slop.
    let top = held.top.expect("drawn");
    assert!((top - (REST0 - 90.0)).abs() < 0.5, "followed 1:1: {top}");
    rig.ms += 400;
    let s = rig.lift((200.0, at.1 - 100.0));
    assert_eq!(s.detent, Some(0));
    assert!((rig.run(2000).top.expect("drawn") - REST0).abs() < 0.01);

    // Up 300: nearer the large detent.
    let mut rig = Rig::opened(&BOTH);
    rig.drag(at, (200.0, at.1 - 300.0), 12, 16);
    rig.ms += 400;
    let s = rig.lift((200.0, at.1 - 300.0));
    assert_eq!(s.detent, Some(1));
    assert!(!s.dismissed);
    assert!((rig.run(2000).top.expect("drawn") - REST1).abs() < 0.01);

    // Down from the large one by 300: nearer the first one.
    let at1 = (200.0, REST1 + 10.0);
    rig.drag(at1, (200.0, at1.1 + 300.0), 12, 16);
    rig.ms += 400;
    let s = rig.lift((200.0, at1.1 + 300.0));
    assert_eq!(s.detent, Some(0));
    assert!((rig.run(2000).top.expect("drawn") - REST0).abs() < 0.01);
}

#[test]
fn above_the_top_detent_the_sheet_rubber_bands() {
    let mut rig = Rig::opened(&BOTH);
    let at1 = (200.0, REST0 + 10.0);
    rig.drag(at1, (200.0, at1.1 - 300.0), 12, 16);
    rig.ms += 400;
    rig.lift((200.0, at1.1 - 300.0));
    rig.run(2000);
    let at = (200.0, REST1 + 10.0);
    rig.drag(at, (200.0, at.1 - 200.0), 10, 16);
    let top = rig.frame().0.top.expect("drawn");
    assert!(
        top < REST1 && top > REST1 - 190.0,
        "past the end by less than the pull: {top}"
    );
    rig.ms += 400;
    rig.lift((200.0, at.1 - 200.0));
    assert!((rig.run(2000).top.expect("drawn") - REST1).abs() < 0.01);
}

#[test]
fn a_drag_down_released_to_closed_dismisses_and_slides_out() {
    let mut rig = Rig::opened(&BOTH);
    let at = (200.0, REST0 + 10.0);
    rig.drag(at, (200.0, at.1 + 330.0), 12, 16);
    rig.ms += 400;
    let s = rig.lift((200.0, at.1 + 330.0));
    assert!(s.dismissed && !rig.open, "{s:?}");
    assert_eq!(s.detent, None);
    assert!(s.value, "it keeps drawing while it goes");
    assert_eq!(rig.run(2000).top, None);
}

#[test]
fn a_fast_flick_picks_by_its_speed() {
    // 60 px up in 50 ms from the first detent reaches the large one.
    let mut rig = Rig::opened(&BOTH);
    let at = (200.0, REST0 + 10.0);
    rig.drag(at, (200.0, at.1 - 60.0), 4, 16);
    let s = rig.lift((200.0, at.1 - 60.0));
    assert_eq!(s.detent, Some(1), "{s:?}");
    // A fast flick down from the first detent closes it.
    let mut rig = Rig::opened(&BOTH);
    rig.drag(at, (200.0, at.1 + 80.0), 4, 16);
    let s = rig.lift((200.0, at.1 + 80.0));
    assert!(s.dismissed, "{s:?}");
}

#[test]
fn a_finger_on_a_moving_sheet_catches_it_and_a_tap_settles_it_on_a_detent() {
    let mut rig = Rig::new(&BOTH);
    rig.frame();
    rig.open = true;
    rig.run(100);
    let top = rig.frame().0.top.expect("sliding");
    assert!(top > REST0 + 5.0);
    rig.h.input.touch_start(1, 200.0, top + 10.0);
    let a = rig.frame().0.top.expect("held");
    let b = rig.run(200).top.expect("held");
    assert!((a - b).abs() < 0.5, "it stopped where it was: {a} {b}");
    assert!(rig.open);
    rig.h.input.touch_end(1, 200.0, top + 10.0);
    let s = rig.frame().0;
    assert!(
        !s.dismissed && rig.open,
        "a tap on the sheet closes nothing"
    );
    assert!((rig.run(2000).top.expect("drawn") - REST0).abs() < 0.01);
}

#[test]
fn a_pan_on_scrolling_content_scrolls_it_and_leaves_the_sheet_a_pan_on_the_grabber_moves_it() {
    let mut rig = Rig::new(&BOTH);
    rig.scrolling = true;
    rig.frame();
    rig.open = true;
    rig.run(1500);
    // On the content (below the 20 px band): the list scrolls.
    let from = (200.0, REST0 + 200.0);
    rig.drag(from, (200.0, from.1 - 100.0), 8, 16);
    let s = rig.frame().0;
    assert!(s.offset > 50.0, "the list scrolled: {}", s.offset);
    assert!(
        (s.top.expect("drawn") - REST0).abs() < 0.01,
        "the sheet stayed"
    );
    rig.ms += 600;
    rig.lift((200.0, from.1 - 100.0));
    rig.run(3000);

    // On the grabber: the sheet moves, the list does not.
    let before = rig.frame().0.offset;
    let at = (200.0, REST0 + 10.0);
    rig.drag(at, (200.0, at.1 - 100.0), 8, 16);
    let s = rig.frame().0;
    assert!((s.offset - before).abs() < 0.01);
    assert!((s.top.expect("drawn") - (REST0 - 90.0)).abs() < 0.5);
}

#[test]
fn where_the_content_has_nothing_to_scroll_a_pan_on_the_body_drags_the_sheet() {
    let mut rig = Rig::opened(&BOTH);
    let from = (200.0, REST0 + 200.0);
    rig.drag(from, (200.0, from.1 - 100.0), 8, 16);
    let top = rig.frame().0.top.expect("drawn");
    assert!((top - (REST0 - 90.0)).abs() < 0.5, "{top}");
}

fn click_grabber(rig: &mut Rig) {
    let top = rig.top();
    rig.h.input.pointer_to(200.0, top + 10.0);
    rig.h.input.click(PointerButton::Primary);
    rig.frame();
}

#[test]
fn a_click_on_the_grabber_steps_to_the_next_detent_and_wraps() {
    let mut rig = Rig::opened(&BOTH);
    click_grabber(&mut rig);
    let s = rig.run(1500);
    assert_eq!(s.detent, Some(1));
    assert!((s.top.expect("drawn") - REST1).abs() < 0.01);
    click_grabber(&mut rig);
    let s = rig.run(1500);
    assert_eq!(s.detent, Some(0));
    assert!((s.top.expect("drawn") - REST0).abs() < 0.01);
}

#[test]
fn the_grabber_is_a_button_named_by_the_sheet_and_enter_on_it_steps() {
    let mut rig = Rig::opened(&BOTH);
    let (_, out) = rig.frame();
    let g = out.tree.get(grabber_id()).expect("the grabber node");
    assert_eq!(g.role, Role::Button);
    assert_eq!(g.name, "Filters");
    rig.h.frame(|ui| ui.request_focus(grabber_id()));
    rig.frame();
    rig.h.input.key_down(Key::Named(NamedKey::Enter), None);
    rig.frame();
    assert_eq!(rig.run(1500).detent, Some(1));
}

#[test]
fn focus_is_held_inside_the_sheet_while_it_is_open() {
    let mut rig = Rig::opened(&BOTH);
    let (_, out) = rig.frame();
    assert_eq!(
        out.focused,
        Some(grabber_id()),
        "the grabber is its first tab stop"
    );
    for _ in 0..3 {
        rig.h.input.key_down(Key::Named(NamedKey::Tab), None);
        let (_, out) = rig.frame();
        assert_eq!(
            out.focused,
            Some(grabber_id()),
            "Tab never leaves the sheet"
        );
    }
}

#[test]
fn the_content_rect_sits_under_the_grabber_and_above_the_safe_area_or_the_keyboard() {
    let mut rig = Rig::opened(&BOTH);
    let c = rig.frame().0.content.expect("content");
    assert_eq!((c.x, c.w), (0.0, 400.0));
    assert!((c.y - (REST0 + 20.0)).abs() < 0.01, "{c:?}");
    assert!((c.bottom() - (800.0 - 34.0)).abs() < 0.01, "{c:?}");
    // The keyboard is taller than the home indicator: the content ends above it.
    rig.h
        .input
        .keyboard_inset(Insets::new(0.0, 0.0, 0.0, 300.0));
    rig.frame();
    let c = rig.run(100).content.expect("content");
    assert!((c.bottom() - 500.0).abs() < 0.01, "{c:?}");
    assert!(
        (c.y - (REST0 + 20.0)).abs() < 0.01,
        "the sheet did not move: {c:?}"
    );
    // No grabber: no band.
    let mut h = UiHarness::new(WIN);
    let mut open = true;
    let mut rect = None;
    for ms in (0..1600).step_by(16) {
        h.at_ms(ms);
        h.frame(|ui| {
            ui.bottom_sheet(
                "sheet",
                &mut open,
                &BottomSheet::new("Filters").with_grabber(false),
                |_, r| rect = Some(r),
            );
        });
    }
    let r = rect.expect("content");
    assert!((r.y - (800.0 - 0.5 * 790.0)).abs() < 0.01, "{r:?}");
}

#[test]
fn a_disabled_scope_leaves_no_violation_in_the_sheet() {
    let mut rig = Rig::new(&BOTH);
    rig.disabled = true;
    rig.frame();
    rig.open = true;
    rig.run(1500);
    let (_, out) = rig.frame();
    assert!(out.tree.get(sheet_id()).is_some());
    let bad = disabled_violations(&out.tree, sheet_id());
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn a_secondary_click_opens_no_menu() {
    let mut rig = Rig::opened(&BOTH);
    let top = rig.top();
    rig.h.input.pointer_to(200.0, top + 10.0);
    rig.h.input.click(PointerButton::Secondary);
    rig.frame();
    let (_, out) = rig.frame();
    assert!(out.tree.nodes.iter().all(|n| n.role != Role::Menu));
}

#[test]
fn the_reduced_tier_does_not_slide_and_the_off_tier_snaps() {
    let mut rig = Rig::new(&BOTH);
    rig.h.tier = MotionTier::Reduced;
    rig.frame();
    rig.open = true;
    let first = rig.frame().0;
    assert!(
        (first.top.expect("drawn") - REST0).abs() < 0.01,
        "no slide: {first:?}"
    );
    assert_eq!(first.repaint, Some(Repaint::Now), "it fades in");
    let settled = rig.run(1000);
    assert_eq!(settled.repaint, Some(Repaint::Idle));
    rig.open = false;
    let s = rig.frame().0;
    assert!(
        s.value && (s.top.expect("drawn") - REST0).abs() < 0.01,
        "fades out in place"
    );
    assert_eq!(rig.run(1000).top, None);

    let mut rig = Rig::new(&BOTH);
    rig.h.tier = MotionTier::Off;
    rig.frame();
    rig.open = true;
    let first = rig.frame().0;
    assert!((first.top.expect("drawn") - REST0).abs() < 0.01);
    assert_eq!(rig.run(100).repaint, Some(Repaint::Idle), "nothing moves");
    rig.open = false;
    let s = rig.frame().0;
    assert_eq!((s.top, s.value), (None, false), "gone at once");
}

/// A sheet closed while a finger still holds it ends under the reduced tier: the fade is all
/// that shows, and it ends the overlay with the finger down and after it lifts.
#[test]
fn under_the_reduced_tier_a_sheet_closed_while_held_ends() {
    for by_escape in [false, true] {
        let mut rig = Rig::new(&BOTH);
        rig.h.tier = MotionTier::Reduced;
        rig.frame();
        rig.open = true;
        rig.run(1000);
        rig.drag((200.0, 600.0), (200.0, 660.0), 6, 16);
        if by_escape {
            rig.h.input.key_down(Key::Named(NamedKey::Escape), None);
        } else {
            rig.open = false;
        }
        rig.frame();
        assert!(!rig.open, "closed (escape: {by_escape})");
        // The finger is still down: the fade runs out and the overlay ends.
        let s = rig.run(1500);
        assert_eq!(
            s.top, None,
            "ended with the finger down (escape: {by_escape}): {s:?}"
        );
        let s = rig.lift((200.0, 660.0));
        assert_eq!(
            s.top, None,
            "stays ended after the lift (escape: {by_escape})"
        );
        assert!(!rig.open && rig.run(500).top.is_none());
    }
}

#[test]
fn a_window_that_changes_size_moves_the_settled_sheet_to_its_detent() {
    let mut rig = Rig::opened(&BOTH);
    rig.h.input.resize((400.0, 600.0));
    let s = rig.run(1500);
    let want = 600.0 - 0.5 * (600.0 - TOP - 10.0);
    assert!((s.top.expect("drawn") - want).abs() < 0.01, "{s:?}");
}

#[test]
fn a_tap_on_a_field_in_the_sheet_focuses_it() {
    use lune_ui_core::widgets::field::FieldState;
    let mut h = UiHarness::new(WIN);
    h.input.safe_area(Insets::new(0.0, TOP, 0.0, 34.0));
    let mut open = true;
    let mut text = String::new();
    // The content starts at the first detent's top (435) under the 20 px band.
    let at = Rect::new(16.0, 475.0, 368.0, 36.0);
    let mut frame = |h: &mut UiHarness, ms: u64| {
        h.at_ms(ms);
        let (_, out) = h.frame(|ui| {
            ui.bottom_sheet(
                "sheet",
                &mut open,
                &BottomSheet::new("Sheet"),
                |ui, rect| {
                    if ms >= 1000 {
                        assert_eq!(rect.y + 20.0, at.y);
                    }
                    ui.text_field("name", at, &mut text, "Name", FieldState::default());
                },
            );
        });
        out
    };
    for ms in (0..1600).step_by(16) {
        frame(&mut h, ms);
    }
    let (x, y) = (at.x + at.w / 2.0, at.y + at.h / 2.0);
    h.input.touch_start(1, x, y);
    frame(&mut h, 1616);
    h.input.touch_end(1, x, y);
    frame(&mut h, 1632);
    let out = frame(&mut h, 1648);
    let focused = out.focused.expect("something is focused");
    let node = out.tree.get(focused).expect("a node");
    assert_eq!(node.role, Role::TextInput, "{:?}", node.name);
    // The keyboard comes up: the field keeps the focus.
    h.input.keyboard_inset(Insets::new(0.0, 0.0, 0.0, 300.0));
    frame(&mut h, 1664);
    let out = frame(&mut h, 1700);
    assert_eq!(out.focused, Some(focused), "the keyboard moved the focus");
}

#[test]
fn a_drag_in_big_steps_follows_the_finger_from_the_slop_on() {
    let mut rig = Rig::opened(&BOTH);
    let at = (200.0, REST0 + 10.0);
    // Three moves of 50, 60 and 50 px: 160 px of travel, 10 of them the slop.
    rig.h.input.touch_start(1, at.0, at.1);
    rig.frame();
    for dy in [50.0, 110.0, 160.0] {
        rig.h.input.touch_move(1, at.0, at.1 - dy);
        rig.frame();
    }
    let top = rig.frame().0.top.expect("drawn");
    assert!((top - (REST0 - 150.0)).abs() < 0.5, "{top}");
}

#[test]
fn a_safe_area_that_arrives_after_the_sheet_opened_moves_it_to_the_detent_at_once() {
    // The first frame has no safe area (the largest height is 790), the next one has 60 px.
    let mut h = UiHarness::new(WIN);
    let mut open = true;
    let mut top = 0.0;
    for ms in (0..1600).step_by(16) {
        if ms == 16 {
            h.input.safe_area(Insets::new(0.0, TOP, 0.0, 34.0));
        }
        h.at_ms(ms);
        let (_, out) = h.frame(|ui| {
            ui.bottom_sheet("sheet", &mut open, &BottomSheet::new("S"), |_, _| ());
        });
        top = out.tree.get(sheet_id()).map_or(top, |n| n.bounds.y);
    }
    assert!((top - REST0).abs() < 0.01, "{top}");
}
