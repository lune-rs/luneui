//! The navigation stack (`layout.nav-stack`, D372): push, a queued pop and its one `popped`, a cut
//! on a depth the app changes by itself, the edge swipe and where it starts, presses swallowed
//! while it moves, a mouse press that still clicks, kept-alive levels, the motion tiers.

use lune_ui_core::access::{Node, Role};
use lune_ui_core::input::{FrameOutput, PointerButton, Repaint};
use lune_ui_core::motion::MotionTier;
use lune_ui_core::paint::Rect;
use lune_ui_core::testing::{UiHarness, disabled_violations};
use lune_ui_core::ui::{Ui, WidgetId};
use lune_ui_core::widgets::button::ButtonKind;
use lune_ui_core::widgets::nav_stack::NavStackResponse;
use lune_ui_core::widgets::scroll_area::{Extent, ScrollOptions};

const WIN: (f32, f32) = (400.0, 800.0);
const STACK: Rect = Rect::new(0.0, 0.0, 400.0, 800.0);

/// What one frame showed.
#[derive(Debug, Clone, Default)]
struct Seen {
    resp: NavStackResponse,
    /// The levels drawn and their left edges, by node.
    lefts: Vec<(usize, f32)>,
    /// A level's body button clicked this frame, and its edge button (in the 24 px band).
    body: Vec<usize>,
    edge: Vec<usize>,
    /// The offset of the scroll area of level `scrolling`.
    offset: f64,
    repaint: Option<Repaint>,
}

impl Seen {
    fn left(&self, level: usize) -> Option<f32> {
        self.lefts
            .iter()
            .find(|(l, _)| *l == level)
            .map(|(_, x)| *x)
    }
}

struct Rig {
    h: UiHarness,
    ms: u64,
    depth: usize,
    /// Level `scrolling` is a scroll area over its whole rect.
    scrolling: Option<usize>,
    /// Asks for a pop in the next frame, after the stack's call.
    pop: bool,
    disabled: bool,
}

impl Rig {
    fn new(depth: usize) -> Rig {
        Rig {
            h: UiHarness::new(WIN),
            ms: 0,
            depth,
            scrolling: None,
            pop: false,
            disabled: false,
        }
    }

    fn frame(&mut self) -> (Seen, FrameOutput) {
        self.ms += 16;
        self.h.at_ms(self.ms);
        let mut seen = Seen::default();
        let (depth, scrolling, pop, disabled) =
            (self.depth, self.scrolling, self.pop, self.disabled);
        self.pop = false;
        let (_, out) = self.h.frame(|ui| {
            let mut run = |ui: &mut Ui<'_>| {
                seen.resp = ui.nav_stack("stack", STACK, depth, |ui, level, r| {
                    let t = *ui.theme();
                    ui.painter().fill(r, 0.0, t.surface_overlay);
                    ui.node(Node::new(
                        WidgetId::ROOT.with(("page", level)),
                        Role::Group,
                        &format!("level{level}"),
                        r,
                    ));
                    if scrolling == Some(level) {
                        let o = ScrollOptions::default();
                        let res = ui.scroll_area("list", r, &o, |_, _| {
                            ((), Extent::new(f64::from(r.w), 4000.0))
                        });
                        seen.offset = res.offset.1;
                    }
                    let body = Rect::new(r.x + 60.0, r.y + 100.0, 80.0, 40.0);
                    if ui.button("body", body, "Open", ButtonKind::Primary).clicked {
                        seen.body.push(level);
                    }
                    let edge = Rect::new(r.x + 4.0, r.y + 300.0, 16.0, 40.0);
                    if ui.button("edge", edge, "E", ButtonKind::Primary).clicked {
                        seen.edge.push(level);
                    }
                    seen.lefts.push((level, r.x));
                });
                if pop {
                    ui.nav_stack_pop("stack");
                }
            };
            if disabled {
                ui.with_enabled(false, |ui| {
                    ui.node_container(
                        Node::new(WidgetId::ROOT.with("root"), Role::Group, "root", STACK),
                        |ui| run(ui),
                    );
                });
            } else {
                run(ui);
            }
        });
        seen.lefts.sort_by_key(|(l, _)| *l);
        seen.repaint = Some(out.repaint);
        (seen, out)
    }

    /// Frames for `ms` more of app time; the last one.
    fn run(&mut self, ms: u64) -> Seen {
        let end = self.ms + ms;
        let mut last = self.frame().0;
        while self.ms < end {
            last = self.frame().0;
        }
        last
    }

    /// Frames for `ms` of app time; how many reported `popped`.
    fn count_popped(&mut self, ms: u64) -> usize {
        let end = self.ms + ms;
        let mut n = 0;
        while self.ms < end {
            n += usize::from(self.frame().0.resp.popped);
        }
        n
    }

    /// A finger down at `at`, moving to `to` in `steps` frames; held.
    fn drag(&mut self, at: (f32, f32), to: (f32, f32), steps: u32) {
        self.h.input.touch_start(1, at.0, at.1);
        self.frame();
        for i in 1..=steps {
            let k = i as f32 / steps as f32;
            self.h
                .input
                .touch_move(1, at.0 + (to.0 - at.0) * k, at.1 + (to.1 - at.1) * k);
            self.frame();
        }
    }

    fn lift(&mut self, at: (f32, f32)) -> Seen {
        self.h.input.touch_end(1, at.0, at.1);
        self.frame().0
    }
}

fn parallax(top: f32) -> f32 {
    -0.3 * (WIN.0 - top)
}

#[test]
fn depth_zero_draws_nothing() {
    let mut rig = Rig::new(0);
    let s = rig.run(100);
    assert!(s.lefts.is_empty() && !s.resp.moving && !s.resp.popped);
}

#[test]
fn the_first_frame_and_a_root_at_rest_draw_only_the_top() {
    let mut rig = Rig::new(1);
    let s = rig.frame().0;
    assert_eq!(s.lefts, vec![(0, 0.0)]);
    assert!(!s.resp.moving);
    assert_eq!(rig.run(100).repaint, Some(Repaint::Idle));
}

#[test]
fn a_push_slides_the_new_top_in_and_parallaxes_the_level_below() {
    let mut rig = Rig::new(1);
    rig.run(100);
    rig.depth = 2;
    let first = rig.frame().0;
    assert!(first.resp.moving);
    let x = first.left(1).expect("the new top is drawn");
    assert!(x > 300.0 && x <= 400.0, "starts at the right edge: {x}");
    // Both levels drawn, the lower one 30 % of the uncovered width left of its rest.
    let lower = first.left(0).expect("the level below is drawn");
    assert!(
        (lower - parallax(x)).abs() < 0.01,
        "{lower} vs {}",
        parallax(x)
    );
    // It only ever moves left, and ends exactly at rest.
    let mut last = x;
    let mut mid = false;
    for _ in 0..60 {
        let s = rig.frame().0;
        if let Some(x) = s.left(1) {
            assert!(x <= last + 0.001, "moves one way: {last} -> {x}");
            mid |= x > 20.0 && x < 380.0;
            last = x;
        }
    }
    assert!(mid, "it passes through the middle");
    let end = rig.run(600);
    assert_eq!(end.lefts, vec![(1, 0.0)], "only the top at rest");
    assert!(!end.resp.moving && !end.resp.popped);
    assert_eq!(end.repaint, Some(Repaint::Idle));
}

#[test]
fn a_queued_pop_reverses_the_push_and_reports_popped_once() {
    let mut rig = Rig::new(2);
    rig.run(100);
    rig.pop = true;
    let ask = rig.frame().0;
    assert!(!ask.resp.moving, "taken by the next call: {ask:?}");
    let first = rig.frame().0;
    assert!(first.resp.moving && !first.resp.popped);
    let x = first.left(1).expect("the top moves out");
    assert!((0.0..100.0).contains(&x), "starts from rest: {x}");
    let mut seen_popped = 0;
    let mut frames = 0;
    let mut last = x;
    while rig.ms < 1500 {
        let (s, _) = rig.frame();
        if s.resp.popped {
            seen_popped += 1;
            // The frame it ends: the level below alone, at rest.
            assert_eq!(s.lefts, vec![(0, 0.0)], "{s:?}");
            assert!(!s.resp.moving);
            break;
        } else if let Some(x) = s.left(1) {
            assert!(x >= last - 0.001, "moves out only: {last} -> {x}");
            last = x;
            frames += 1;
        }
    }
    assert_eq!(seen_popped, 1);
    assert!(frames > 5, "it took time: {frames}");
}

#[test]
fn after_popped_the_app_lowers_its_depth_and_the_level_below_stays_at_rest() {
    let mut rig = Rig::new(2);
    rig.run(100);
    rig.pop = true;
    rig.frame();
    loop {
        let (s, _) = rig.frame();
        if s.resp.popped {
            break;
        }
        assert!(rig.ms < 2000, "popped never came");
    }
    rig.depth = 1;
    let s = rig.frame().0;
    assert_eq!(s.lefts, vec![(0, 0.0)]);
    assert!(!s.resp.moving && !s.resp.popped);
    assert_eq!(rig.run(500).repaint, Some(Repaint::Idle));
    assert_eq!(rig.count_popped(500), 0);
}

#[test]
fn an_app_that_keeps_its_depth_sees_the_top_again_at_rest_with_no_second_popped() {
    let mut rig = Rig::new(2);
    rig.run(100);
    rig.pop = true;
    rig.frame();
    loop {
        if rig.frame().0.resp.popped {
            break;
        }
        assert!(rig.ms < 2000, "popped never came");
    }
    let s = rig.frame().0;
    assert_eq!(s.lefts, vec![(1, 0.0)]);
    assert!(!s.resp.moving && !s.resp.popped);
    assert_eq!(rig.count_popped(1000), 0);
}

#[test]
fn a_pop_is_ignored_at_depth_one_and_while_a_pop_moves() {
    let mut rig = Rig::new(1);
    rig.run(100);
    rig.pop = true;
    rig.frame();
    let s = rig.run(500);
    assert_eq!((s.lefts.clone(), s.resp.moving), (vec![(0, 0.0)], false));
    assert_eq!(rig.count_popped(500), 0);

    let mut rig = Rig::new(2);
    rig.run(100);
    rig.pop = true;
    rig.frame();
    rig.frame();
    rig.run(50);
    rig.pop = true;
    rig.frame();
    // One pop, not two: one `popped`.
    assert_eq!(rig.count_popped(1500), 1);
}

#[test]
fn a_depth_the_app_changes_by_itself_cuts_with_no_motion() {
    let mut rig = Rig::new(1);
    rig.run(100);
    rig.depth = 3;
    let s = rig.frame().0;
    assert_eq!(s.lefts, vec![(2, 0.0)], "up by two cuts");
    assert!(!s.resp.moving);
    rig.run(100);
    rig.depth = 2;
    let s = rig.frame().0;
    assert_eq!(s.lefts, vec![(1, 0.0)], "lowered by the app cuts");
    assert!(!s.resp.moving && !s.resp.popped);
    rig.depth = 0;
    let s = rig.frame().0;
    assert!(s.lefts.is_empty());
}

#[test]
fn the_reduced_and_off_tiers_cut_a_push_and_a_pop() {
    for tier in [MotionTier::Reduced, MotionTier::Off] {
        let mut rig = Rig::new(1);
        rig.h.tier = tier;
        rig.run(100);
        rig.depth = 2;
        let s = rig.frame().0;
        assert_eq!(s.lefts, vec![(1, 0.0)], "{tier:?}");
        assert!(!s.resp.moving);
        rig.run(100);
        rig.pop = true;
        rig.frame();
        let n = rig.count_popped(200);
        assert_eq!(n, 1, "{tier:?}: a pop still reports popped");
    }
}

#[test]
fn a_touch_pan_that_starts_in_the_edge_band_drags_the_top_across() {
    let mut rig = Rig::new(2);
    rig.run(100);
    rig.drag((10.0, 500.0), (110.0, 500.0), 6);
    let s = rig.frame().0;
    assert!(s.resp.moving);
    let x = s.left(1).expect("the top is drawn");
    // 100 px of travel less the 10 px slop.
    assert!((x - 90.0).abs() < 0.5, "followed 1:1: {x}");
    let lower = s.left(0).expect("the level below shows");
    assert!((lower - parallax(x)).abs() < 0.01);
    // Dragged back past the left edge it stops at 0; the level is never pulled left.
    rig.drag_on((10.0, 500.0), (-200.0, 500.0), 6);
    let s = rig.frame().0;
    assert!(s.left(1).expect("drawn") >= 0.0);
}

impl Rig {
    /// More moves of a finger that is already down.
    fn drag_on(&mut self, from: (f32, f32), to: (f32, f32), steps: u32) {
        for i in 1..=steps {
            let k = i as f32 / steps as f32;
            self.h.input.touch_move(
                1,
                from.0 + (to.0 - from.0) * k,
                from.1 + (to.1 - from.1) * k,
            );
            self.frame();
        }
    }
}

#[test]
fn a_pan_that_starts_outside_the_band_never_pops() {
    let mut rig = Rig::new(2);
    rig.run(100);
    rig.drag((100.0, 500.0), (350.0, 500.0), 8);
    let s = rig.frame().0;
    assert_eq!(s.lefts, vec![(1, 0.0)]);
    assert!(!s.resp.moving);
    rig.lift((350.0, 500.0));
    assert_eq!(rig.count_popped(1000), 0);
    // Nor does a band pan at depth 1, where there is nothing to pop.
    let mut rig = Rig::new(1);
    rig.run(100);
    rig.drag((10.0, 500.0), (250.0, 500.0), 8);
    assert_eq!(rig.frame().0.lefts, vec![(0, 0.0)]);
}

#[test]
fn a_swipe_released_past_the_middle_completes_and_one_short_of_it_springs_back() {
    let mut rig = Rig::new(2);
    rig.run(100);
    rig.drag((10.0, 500.0), (260.0, 500.0), 10);
    rig.ms += 400;
    rig.lift((260.0, 500.0));
    assert_eq!(rig.count_popped(1500), 1, "past the middle pops");

    let mut rig = Rig::new(2);
    rig.run(100);
    rig.drag((10.0, 500.0), (110.0, 500.0), 10);
    rig.ms += 400;
    rig.lift((110.0, 500.0));
    assert_eq!(rig.count_popped(1500), 0);
    let end = rig.run(100);
    assert_eq!(end.lefts, vec![(1, 0.0)], "back at rest");
    assert_eq!(end.repaint, Some(Repaint::Idle));
}

#[test]
fn a_fast_flick_completes_a_short_swipe() {
    let mut rig = Rig::new(2);
    rig.run(100);
    // 80 px in four frames of 16 ms: 1250 px/s at the lift.
    rig.drag((10.0, 500.0), (100.0, 500.0), 4);
    rig.lift((100.0, 500.0));
    assert_eq!(rig.count_popped(1500), 1);
}

#[test]
fn a_press_on_the_swipe_in_flight_catches_it() {
    let mut rig = Rig::new(2);
    rig.run(100);
    rig.drag((10.0, 500.0), (110.0, 500.0), 6);
    rig.ms += 400;
    rig.lift((110.0, 500.0));
    // Springing back now; a finger down in the band holds it where it shows.
    rig.run(32);
    let before = rig.frame().0.left(1).expect("drawn");
    assert!(before > 0.5, "still on its way: {before}");
    rig.h.input.touch_start(2, 12.0, 600.0);
    let held = rig.frame().0.left(1).expect("drawn");
    let later = rig.run(500).left(1).expect("drawn");
    assert!((held - later).abs() < 0.01, "held: {held} then {later}");
    assert!((held - before).abs() < 40.0, "no jump: {before} -> {held}");
    // Lifted without moving it settles somewhere at rest and pops nothing.
    rig.h.input.touch_end(2, 12.0, 600.0);
    rig.frame();
    assert_eq!(rig.count_popped(1500), 0);
    assert_eq!(rig.run(100).lefts, vec![(1, 0.0)]);
}

#[test]
fn a_vertical_pan_in_the_band_goes_to_the_levels_scroll_area() {
    let mut rig = Rig::new(2);
    rig.scrolling = Some(1);
    rig.run(100);
    rig.drag((10.0, 600.0), (10.0, 300.0), 8);
    let s = rig.frame().0;
    assert!(s.offset > 100.0, "the list scrolled: {}", s.offset);
    assert_eq!(s.lefts, vec![(1, 0.0)], "the stack did not move");
    assert!(!s.resp.moving);
}

#[test]
fn presses_are_swallowed_while_it_moves_and_work_again_at_rest() {
    let mut rig = Rig::new(1);
    rig.run(100);
    rig.depth = 2;
    rig.frame();
    rig.run(120);
    let s = rig.frame().0;
    assert!(s.resp.moving);
    let x = s.left(1).expect("drawn");
    // Over the top level's button and over the lower one's.
    for at in [
        (x + 100.0, 120.0),
        (s.left(0).expect("lower drawn") + 100.0, 120.0),
    ] {
        rig.h.input.pointer_to(at.0, at.1);
        rig.h.input.click(PointerButton::Primary);
        let s = rig.frame().0;
        assert!(s.body.is_empty() && s.edge.is_empty(), "swallowed: {s:?}");
    }
    // A finger's tap too.
    rig.h.input.touch_start(1, x + 100.0, 120.0);
    rig.frame();
    rig.h.input.touch_end(1, x + 100.0, 120.0);
    let s = rig.frame().0;
    assert!(s.body.is_empty(), "{s:?}");
    // At rest the same click lands.
    rig.run(1000);
    rig.h.input.pointer_to(100.0, 120.0);
    rig.h.input.click(PointerButton::Primary);
    assert_eq!(rig.frame().0.body, vec![1]);
}

#[test]
fn a_mouse_press_in_the_band_clicks_the_widget_under_it_and_moves_nothing() {
    let mut rig = Rig::new(2);
    rig.run(100);
    rig.h.input.pointer_to(12.0, 320.0);
    rig.h.input.click(PointerButton::Primary);
    let s = rig.frame().0;
    assert_eq!(s.edge, vec![1]);
    // A mouse drag from the band moves nothing either.
    rig.h.input.pointer_to(10.0, 600.0);
    rig.h.input.press(PointerButton::Primary);
    rig.frame();
    for x in [60.0, 150.0, 250.0] {
        rig.h.input.pointer_to(x, 600.0);
        rig.frame();
    }
    let s = rig.frame().0;
    assert_eq!(s.lefts, vec![(1, 0.0)]);
    assert!(!s.resp.moving);
    rig.h.input.release(PointerButton::Primary);
    assert_eq!(rig.count_popped(1000), 0);
}

#[test]
fn a_covered_level_keeps_its_scroll_offset_through_a_push_and_a_pop() {
    let mut rig = Rig::new(1);
    rig.scrolling = Some(0);
    rig.run(100);
    rig.h.input.pointer_to(200.0, 400.0);
    rig.h.input.scroll_pixels(0.0, -300.0);
    rig.frame();
    let before = rig.run(1000).offset;
    assert!(before > 100.0, "scrolled: {before}");
    rig.depth = 2;
    rig.run(600);
    // Covered for a while: level 0 is not drawn.
    assert_eq!(rig.run(300).lefts, vec![(1, 0.0)]);
    rig.pop = true;
    rig.frame();
    loop {
        if rig.frame().0.resp.popped {
            break;
        }
        assert!(rig.ms < 4000, "popped never came");
    }
    rig.depth = 1;
    let s = rig.frame().0;
    assert_eq!(s.lefts, vec![(0, 0.0)]);
    assert!(
        (s.offset - before).abs() < 0.001,
        "{} vs {before}",
        s.offset
    );
}

#[test]
fn a_level_the_app_removes_lets_go_of_what_it_kept() {
    let mut rig = Rig::new(1);
    rig.scrolling = Some(1);
    rig.depth = 2;
    rig.run(600);
    rig.h.input.pointer_to(200.0, 400.0);
    rig.h.input.scroll_pixels(0.0, -300.0);
    rig.frame();
    assert!(rig.run(1000).offset > 100.0);
    rig.depth = 1;
    rig.run(100);
    rig.depth = 2;
    let s = rig.run(1000);
    assert_eq!(s.offset, 0.0, "a new level 1 starts at the top");
}

#[test]
fn a_disabled_scope_leaves_the_levels_nodes_disabled() {
    let mut rig = Rig::new(2);
    rig.disabled = true;
    let (_, out) = rig.frame();
    let bad = disabled_violations(&out.tree, WidgetId::ROOT.with("root"));
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn the_levels_are_drawn_in_their_own_id_scopes() {
    // Two levels draw the same keys ("body", "edge"): they do not collide while both show.
    let mut rig = Rig::new(1);
    rig.run(100);
    rig.depth = 2;
    rig.frame();
    let (s, out) = rig.frame();
    assert_eq!(s.lefts.len(), 2);
    let mut ids: Vec<_> = out
        .tree
        .nodes
        .iter()
        .filter(|n| n.role == Role::Button)
        .map(|n| n.id)
        .collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "every button has its own id");
    assert!(n >= 4);
}
