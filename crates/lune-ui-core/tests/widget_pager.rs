//! The pager (`layout.pager`, D373): the pages drawn and clipped, the touch swipe (1:1 follow,
//! rubber band at both ends, one page at most per release, a catch), a page the app sets and its
//! tiers, the dots (one tab stop, keys, clicks), a vertical pan that reaches a list inside a
//! page, a mouse that does not page.

use lune_ui_core::access::{Node, Role, State};
use lune_ui_core::input::{FrameOutput, Key, NamedKey, PointerButton, Repaint};
use lune_ui_core::motion::{MotionTier, rubber_band};
use lune_ui_core::paint::Rect;
use lune_ui_core::testing::{UiHarness, disabled_violations};
use lune_ui_core::ui::{Ui, WidgetId};
use lune_ui_core::widgets::button::ButtonKind;
use lune_ui_core::widgets::pager::{PagerOptions, PagerResponse};
use lune_ui_core::widgets::scroll_area::{Extent, ScrollOptions};

const WIN: (f32, f32) = (400.0, 600.0);
const PAGER: Rect = Rect::new(0.0, 0.0, 400.0, 600.0);
/// The y of the dots' centres: 14 above the rect's bottom.
const DOTS_Y: f32 = 586.0;

/// What one frame showed.
#[derive(Debug, Clone, Default)]
struct Seen {
    resp: PagerResponse,
    /// The pages drawn and their left edges, by node.
    lefts: Vec<(usize, f32)>,
    /// A page's body button clicked this frame.
    body: Vec<usize>,
    /// The offset of the scroll area of page `scrolling`.
    offset: f64,
    repaint: Option<Repaint>,
    out: Option<FrameOutput>,
}

impl Seen {
    fn left(&self, page: usize) -> Option<f32> {
        self.lefts.iter().find(|(p, _)| *p == page).map(|(_, x)| *x)
    }

    fn pages(&self) -> Vec<usize> {
        self.lefts.iter().map(|(p, _)| *p).collect()
    }
}

fn pager_id() -> WidgetId {
    WidgetId::ROOT.with("pager")
}

fn dot_id(i: usize) -> WidgetId {
    pager_id().with(("dot", i))
}

/// The centre of dot `i` of `count`, in a 400 wide pager.
fn dot_at(i: usize, count: usize) -> (f32, f32) {
    let x0 = (400.0 - 16.0 * count as f32) / 2.0;
    (x0 + 16.0 * i as f32 + 8.0, DOTS_Y)
}

struct Rig {
    h: UiHarness,
    ms: u64,
    count: usize,
    page: usize,
    dots: bool,
    /// Page `scrolling` is a scroll area over its whole rect.
    scrolling: Option<usize>,
    /// Page `sideways` is a scroll area wider than the pager over its whole rect.
    sideways: Option<usize>,
    disabled: bool,
}

impl Rig {
    fn new(count: usize) -> Rig {
        let mut rig = Rig {
            h: UiHarness::new(WIN),
            ms: 0,
            count,
            page: 0,
            dots: true,
            scrolling: None,
            sideways: None,
            disabled: false,
        };
        rig.frame();
        rig
    }

    fn frame(&mut self) -> Seen {
        self.ms += 16;
        self.h.at_ms(self.ms);
        let mut seen = Seen::default();
        let (count, scrolling, sideways, dots, disabled) = (
            self.count,
            self.scrolling,
            self.sideways,
            self.dots,
            self.disabled,
        );
        let page = &mut self.page;
        let (_, out) = self.h.frame(|ui| {
            let mut run = |ui: &mut Ui<'_>| {
                let opts = PagerOptions::new("Photos").with_dots(dots);
                seen.resp = ui.pager("pager", PAGER, count, page, &opts, |ui, i, r| {
                    let t = *ui.theme();
                    ui.painter().fill(r, 0.0, t.surface_overlay);
                    ui.node(Node::new(
                        WidgetId::ROOT.with(("page", i)),
                        Role::Group,
                        &format!("page{i}"),
                        r,
                    ));
                    if scrolling == Some(i) {
                        let o = ScrollOptions::default();
                        let res = ui.scroll_area("list", r, &o, |_, _| {
                            ((), Extent::new(f64::from(r.w), 4000.0))
                        });
                        seen.offset = res.offset.1;
                    }
                    if sideways == Some(i) {
                        let o = ScrollOptions::default();
                        let res = ui.scroll_area("side", r, &o, |_, _| {
                            ((), Extent::new(4000.0, f64::from(r.h)))
                        });
                        seen.offset = res.offset.0;
                    }
                    let body = Rect::new(r.x + 60.0, r.y + 100.0, 80.0, 40.0);
                    if ui.button("body", body, "Open", ButtonKind::Primary).clicked {
                        seen.body.push(i);
                    }
                    seen.lefts.push((i, r.x));
                });
            };
            if disabled {
                ui.with_enabled(false, |ui| {
                    ui.node_container(
                        Node::new(WidgetId::ROOT.with("root"), Role::Group, "root", PAGER),
                        |ui| run(ui),
                    );
                });
            } else {
                run(ui);
            }
        });
        seen.lefts.sort_by_key(|(p, _)| *p);
        seen.repaint = Some(out.repaint);
        seen.out = Some(out);
        seen
    }

    fn run(&mut self, ms: u64) -> Seen {
        let end = self.ms + ms;
        let mut last = self.frame();
        while self.ms < end {
            last = self.frame();
        }
        last
    }

    /// A finger down at `at`, then `moves` frames each moving it by `step`; held. Returns the
    /// finger's position.
    fn drag(&mut self, at: (f32, f32), step: (f32, f32), moves: u32) -> (f32, f32) {
        self.h.input.touch_start(1, at.0, at.1);
        self.frame();
        let mut p = at;
        for _ in 0..moves {
            p = (p.0 + step.0, p.1 + step.1);
            self.h.input.touch_move(1, p.0, p.1);
            self.frame();
        }
        p
    }

    /// Frames with the finger still, then it lifts at `p`; the lift frame.
    fn lift_after_rest(&mut self, p: (f32, f32), rest_frames: u32) -> Seen {
        for _ in 0..rest_frames {
            self.frame();
        }
        self.h.input.touch_end(1, p.0, p.1);
        self.frame()
    }

    /// Lifts at `p` on the next frame, the finger still moving.
    fn lift(&mut self, p: (f32, f32)) -> Seen {
        self.h.input.touch_end(1, p.0, p.1);
        self.frame()
    }

    fn key(&mut self, key: NamedKey) -> (Seen, Seen) {
        self.h.input.key_down(Key::Named(key), None);
        let first = self.frame();
        (first, self.frame())
    }

    fn focus(&mut self, id: WidgetId) {
        self.ms += 16;
        self.h.at_ms(self.ms);
        self.h.frame(|ui| ui.request_focus(id));
        self.frame();
    }

    fn click_at(&mut self, at: (f32, f32)) -> Seen {
        self.h.input.pointer_to(at.0, at.1);
        self.h.input.click(PointerButton::Primary);
        let s = self.frame();
        self.h.input.pointer_leave();
        self.frame();
        s
    }
}

const FINGER: (f32, f32) = (200.0, 300.0);

#[test]
fn no_pages_draw_nothing_and_the_page_is_clamped() {
    let mut rig = Rig::new(0);
    let s = rig.run(100);
    assert!(s.lefts.is_empty() && !s.resp.changed);
    rig.count = 3;
    rig.page = 9;
    let s = rig.frame();
    assert_eq!(rig.page, 2, "clamped to the last page");
    assert_eq!(s.pages(), vec![2]);
    assert!(!s.resp.changed);
}

#[test]
fn at_rest_only_the_current_page_is_drawn_edge_to_edge_and_nothing_repaints() {
    let mut rig = Rig::new(5);
    rig.page = 3;
    rig.frame();
    let s = rig.run(200);
    assert_eq!(s.lefts, vec![(3, 0.0)]);
    assert_eq!(s.repaint, Some(Repaint::Idle));
    let out = s.out.expect("frame output");
    let n = out.tree.get(pager_id()).expect("the pager's node");
    assert_eq!(
        (n.role, n.name.as_str(), n.bounds),
        (Role::Group, "Photos", PAGER)
    );
}

#[test]
fn a_touch_pan_follows_the_finger_one_to_one_and_shows_two_pages_joined() {
    let mut rig = Rig::new(3);
    let p = rig.drag(FINGER, (-20.0, 0.0), 8);
    let s = rig.frame();
    let x = s.left(0).expect("page 0 is drawn");
    // 160 px of travel less the pan slop.
    assert!(x < -120.0 && x > -161.0, "{x}");
    let next = s.left(1).expect("page 1 is drawn beside it");
    assert!((next - (x + 400.0)).abs() < 0.01, "joined: {x} {next}");
    assert_eq!(s.pages(), vec![0, 1]);
    // One more step of 20 moves it by exactly 20.
    rig.h.input.touch_move(1, p.0 - 20.0, p.1);
    let t = rig.frame();
    assert!((t.left(0).expect("a value") - (x - 20.0)).abs() < 0.01);
    assert!(!t.resp.changed);
    assert_eq!(rig.page, 0, "the page waits for the lift");
}

#[test]
fn a_fast_fling_moves_exactly_one_page_and_reports_changed_on_the_lift() {
    let mut rig = Rig::new(5);
    // 60 px a frame for 8 frames: 3750 px/s, far past one page's worth of coast.
    let p = rig.drag(FINGER, (-60.0, 0.0), 8);
    let s = rig.lift((p.0 - 60.0, p.1));
    assert!(s.resp.changed);
    assert_eq!(rig.page, 1, "at most one page");
    let mut changed = 0;
    for _ in 0..80 {
        changed += usize::from(rig.frame().resp.changed);
    }
    assert_eq!(changed, 0);
    assert_eq!(rig.page, 1);
    let end = rig.run(600);
    assert_eq!(end.lefts, vec![(1, 0.0)]);
    assert_eq!(end.repaint, Some(Repaint::Idle));
}

#[test]
fn a_slow_release_past_the_middle_moves_and_a_short_one_springs_back() {
    // 250 px quickly, then still: past the middle, no speed.
    let mut rig = Rig::new(3);
    let p = rig.drag(FINGER, (-50.0, 0.0), 6);
    let s = rig.lift_after_rest(p, 10);
    assert!(s.resp.changed && rig.page == 1, "page {}", rig.page);
    assert_eq!(rig.run(800).lefts, vec![(1, 0.0)]);

    // 90 px, then still: back to where it was.
    let mut rig = Rig::new(3);
    let p = rig.drag(FINGER, (-30.0, 0.0), 4);
    let s = rig.lift_after_rest(p, 10);
    assert!(!s.resp.changed && rig.page == 0);
    assert_eq!(rig.run(800).lefts, vec![(0, 0.0)]);
}

#[test]
fn a_flick_to_the_left_from_a_later_page_and_one_to_the_right_page_back() {
    let mut rig = Rig::new(4);
    rig.page = 2;
    rig.run(100);
    let p = rig.drag(FINGER, (60.0, 0.0), 6);
    rig.lift((p.0 + 60.0, p.1));
    assert_eq!(rig.page, 1, "a flick right goes back one page");
}

#[test]
fn the_pull_past_either_end_shows_the_rubber_band_and_springs_back() {
    // Before page 0.
    let mut rig = Rig::new(3);
    let p = rig.drag(FINGER, (30.0, 0.0), 10);
    let s = rig.frame();
    let x = s.left(0).expect("page 0 moves right");
    let travel = p.0 - FINGER.0;
    assert!(x > 20.0 && x < 0.6 * travel, "{x} of {travel}");
    // The shown offset is the band's of the finger's raw travel past the slop.
    let raw = (0..400)
        .map(|k| k as f32)
        .min_by(|a, b| {
            (rubber_band(*a, 400.0) - x)
                .abs()
                .total_cmp(&(rubber_band(*b, 400.0) - x).abs())
        })
        .expect("a value");
    assert!(
        raw <= travel && raw > travel - 30.0,
        "raw {raw} of {travel}"
    );
    assert_eq!(s.pages(), vec![0], "no page behind the first");
    let s = rig.lift_after_rest(p, 10);
    assert!(!s.resp.changed);
    assert_eq!(rig.run(800).lefts, vec![(0, 0.0)]);

    // After the last page.
    let mut rig = Rig::new(3);
    rig.page = 2;
    rig.run(100);
    let p = rig.drag(FINGER, (-30.0, 0.0), 10);
    let s = rig.frame();
    let x = s.left(2).expect("the last page moves left");
    let travel = FINGER.0 - p.0;
    assert!(x < -20.0 && -x < 0.6 * travel, "{x} of {travel}");
    assert_eq!(s.pages(), vec![2]);
    let s = rig.lift_after_rest(p, 10);
    assert!(!s.resp.changed && rig.page == 2);
    assert_eq!(rig.run(800).lefts, vec![(2, 0.0)]);
}

#[test]
fn a_finger_on_pages_in_motion_catches_them_where_they_show() {
    let mut rig = Rig::new(3);
    let p = rig.drag(FINGER, (-20.0, 0.0), 8);
    rig.lift((p.0 - 20.0, p.1));
    // Mid-spring a finger lands: the pages hold where they show.
    let before = rig.frame().left(1).expect("page 1 on its way");
    assert!(before > 0.0 && before < 400.0, "{before}");
    rig.h.input.touch_start(1, 200.0, 300.0);
    let caught = rig.frame();
    let held = caught.left(1).expect("still drawn");
    assert!(held <= before + 0.01 && held > 0.0, "{before} -> {held}");
    let later = rig.run(200).left(1).expect("held");
    assert!((later - held).abs() < 0.5, "held: {held} -> {later}");
    // It lifts without moving: the pages settle on the nearest page, and report no change.
    let s = rig.lift((200.0, 300.0));
    assert!(!s.resp.changed);
    let end = rig.run(1000);
    assert_eq!(end.pages().len(), 1);
    assert_eq!(end.left(rig.page), Some(0.0));
}

#[test]
fn a_page_the_app_sets_slides_by_the_page_swap_and_reports_no_change() {
    let mut rig = Rig::new(4);
    rig.run(100);
    rig.page = 1;
    let first = rig.frame();
    assert!(!first.resp.changed, "the app's own change is not `changed`");
    assert_eq!(
        first.lefts,
        vec![(0, 0.0)],
        "the first frame of a tween starts where it was"
    );
    let mut s = rig.frame();
    while s.left(1).is_none() {
        s = rig.frame();
    }
    let x = s.left(1).expect("page 1 is drawn");
    assert!(x > 300.0 && x <= 400.0, "starts at the right edge: {x}");
    assert_eq!(s.pages(), vec![0, 1]);
    let mut last = x;
    let mut mid = false;
    for _ in 0..60 {
        let s = rig.frame();
        if let Some(x) = s.left(1) {
            assert!(x <= last + 0.001, "{last} -> {x}");
            mid |= x > 20.0 && x < 380.0;
            last = x;
        }
        assert!(!s.resp.changed);
    }
    assert!(mid);
    let end = rig.run(600);
    assert_eq!(end.lefts, vec![(1, 0.0)]);
    assert_eq!(end.repaint, Some(Repaint::Idle));
    // Back by one.
    rig.page = 0;
    let mut s = rig.frame();
    while s.left(0).is_none() {
        s = rig.frame();
    }
    assert!(
        s.left(0).is_some_and(|x| x < 0.0 && x > -400.0),
        "{:?}",
        s.lefts
    );
}

#[test]
fn a_page_set_across_more_than_one_page_cuts_with_no_page_between() {
    let mut rig = Rig::new(5);
    rig.run(100);
    rig.page = 3;
    let s = rig.frame();
    assert_eq!(s.lefts, vec![(3, 0.0)], "{s:?}");
    assert_eq!(rig.run(200).repaint, Some(Repaint::Idle));
}

#[test]
fn the_reduced_and_off_tiers_cut_a_page_the_app_sets_but_not_a_release() {
    for tier in [MotionTier::Reduced, MotionTier::Off] {
        let mut rig = Rig::new(3);
        rig.h.tier = tier;
        rig.run(100);
        rig.page = 1;
        let s = rig.frame();
        assert_eq!(s.lefts, vec![(1, 0.0)], "{tier:?}");

        // A finger's release still continues by the spring.
        let p = rig.drag(FINGER, (-60.0, 0.0), 5);
        let s = rig.lift((p.0 - 60.0, p.1));
        assert!(s.resp.changed, "{tier:?}");
        let next = rig.frame();
        assert!(next.pages().len() == 2, "{tier:?}: {:?}", next.lefts);
    }
}

#[test]
fn a_vertical_pan_reaches_the_list_inside_a_page_and_pages_nothing() {
    let mut rig = Rig::new(3);
    rig.scrolling = Some(0);
    rig.run(100);
    let p = rig.drag((200.0, 500.0), (0.0, -40.0), 6);
    rig.lift_after_rest(p, 10);
    let s = rig.run(300);
    assert!(s.offset > 100.0, "the list scrolled: {}", s.offset);
    assert_eq!(s.lefts, vec![(0, 0.0)], "the pages did not move");
    assert_eq!(rig.page, 0);
}

#[test]
fn a_page_with_its_own_horizontal_scroller_keeps_the_pan_inside_it() {
    let mut rig = Rig::new(3);
    rig.sideways = Some(0);
    rig.run(100);
    let p = rig.drag((200.0, 500.0), (-40.0, 0.0), 6);
    rig.lift_after_rest(p, 10);
    let s = rig.run(300);
    assert!(s.offset > 100.0, "the page's scroller moved: {}", s.offset);
    assert_eq!(s.lefts, vec![(0, 0.0)], "the pages did not move");
    assert_eq!(rig.page, 0);
}

#[test]
fn a_horizontal_pan_on_a_page_still_pages_when_the_page_scrolls_only_vertically() {
    let mut rig = Rig::new(3);
    rig.scrolling = Some(0);
    rig.run(100);
    let p = rig.drag((200.0, 500.0), (-60.0, 0.0), 8);
    rig.lift((p.0 - 60.0, p.1));
    assert_eq!(rig.page, 1);
}

#[test]
fn a_mouse_drag_a_wheel_and_a_click_on_a_page_never_page() {
    let mut rig = Rig::new(3);
    rig.run(100);
    rig.h.input.pointer_to(300.0, 300.0);
    rig.h.input.press(PointerButton::Primary);
    rig.frame();
    for i in 1..=20 {
        rig.h.input.pointer_to(300.0 - 15.0 * i as f32, 300.0);
        let s = rig.frame();
        assert_eq!(s.lefts, vec![(0, 0.0)], "step {i}");
    }
    rig.h.input.release(PointerButton::Primary);
    rig.frame();
    rig.h.input.scroll_pixels(-200.0, 0.0);
    let s = rig.frame();
    assert_eq!(s.lefts, vec![(0, 0.0)]);
    assert_eq!(rig.page, 0);
    // A click on a page's button lands.
    let s = rig.click_at((100.0, 120.0));
    assert_eq!(s.body, vec![0]);
}

#[test]
fn a_touch_tap_on_a_button_in_a_page_clicks_it_and_the_pages_stay() {
    let mut rig = Rig::new(3);
    rig.run(100);
    rig.h.input.touch_start(1, 100.0, 120.0);
    rig.frame();
    rig.h.input.touch_end(1, 100.0, 120.0);
    let s = rig.frame();
    assert_eq!(s.body, vec![0]);
    assert_eq!(s.lefts, vec![(0, 0.0)]);
}

#[test]
fn a_swipe_that_starts_on_a_button_pages_along_x() {
    let mut rig = Rig::new(3);
    rig.run(100);
    let p = rig.drag((100.0, 120.0), (-60.0, 0.0), 8);
    let s = rig.lift((p.0 - 60.0, p.1));
    assert_eq!(rig.page, 1);
    assert!(s.body.is_empty(), "no click: {:?}", s.body);
}

#[test]
fn the_dots_show_the_page_nearest_the_pages_and_a_click_selects_a_dot() {
    let mut rig = Rig::new(5);
    rig.run(100);
    let out = rig.frame().out.expect("out");
    for i in 0..5 {
        let n = out.tree.get(dot_id(i)).expect("a dot node");
        assert_eq!(n.role, Role::Tab);
        assert_eq!(n.name, format!("Page {} of 5", i + 1));
        assert_eq!(n.states.contains(State::Selected), i == 0, "dot {i}");
    }
    let s = rig.click_at(dot_at(3, 5));
    assert!(s.resp.changed);
    assert_eq!(rig.page, 3);
    // It moved to the page with no page between (a step of two or more cuts).
    assert_eq!(rig.run(100).lefts, vec![(3, 0.0)]);
    // A click on the current dot changes nothing; one on the next dot slides.
    let s = rig.click_at(dot_at(3, 5));
    assert!(!s.resp.changed);
    let s = rig.click_at(dot_at(4, 5));
    assert!(s.resp.changed && rig.page == 4);
}

#[test]
fn a_tap_on_a_dot_selects_it() {
    let mut rig = Rig::new(4);
    rig.run(100);
    let at = dot_at(1, 4);
    rig.h.input.touch_start(1, at.0, at.1);
    rig.frame();
    rig.h.input.touch_end(1, at.0, at.1);
    let s = rig.frame();
    assert!(s.resp.changed);
    assert_eq!(rig.page, 1);
}

#[test]
fn the_dots_are_one_tab_stop_and_the_arrows_home_and_end_page() {
    let mut rig = Rig::new(4);
    rig.page = 1;
    rig.run(800);
    // Tab goes through the page's own button, then enters the dots at the current page's dot.
    let (_, after) = rig.key(NamedKey::Tab);
    assert_ne!(
        after.out.as_ref().expect("a value").focused,
        Some(dot_id(1))
    );
    let (_, after) = rig.key(NamedKey::Tab);
    assert_eq!(
        after.out.as_ref().expect("a value").focused,
        Some(dot_id(1))
    );
    let (first, after) = rig.key(NamedKey::ArrowRight);
    assert_eq!(
        after.out.as_ref().expect("a value").focused,
        Some(dot_id(2))
    );
    assert!(first.resp.changed || after.resp.changed);
    assert_eq!(rig.page, 2, "the page follows focus");
    let (_, after) = rig.key(NamedKey::End);
    assert_eq!(
        after.out.as_ref().expect("a value").focused,
        Some(dot_id(3))
    );
    assert_eq!(rig.page, 3);
    let (_, after) = rig.key(NamedKey::ArrowRight);
    assert_eq!(
        after.out.as_ref().expect("a value").focused,
        Some(dot_id(3)),
        "no wrap"
    );
    let (_, after) = rig.key(NamedKey::Home);
    assert_eq!(
        after.out.as_ref().expect("a value").focused,
        Some(dot_id(0))
    );
    assert_eq!(rig.page, 0);
    let (_, after) = rig.key(NamedKey::ArrowLeft);
    assert_eq!(
        after.out.as_ref().expect("a value").focused,
        Some(dot_id(0))
    );
    // The pages follow the keys with the page swap and end at rest.
    assert_eq!(rig.run(1000).lefts, vec![(0, 0.0)]);
    // A second Tab leaves the dots: it is one stop.
    let (_, after) = rig.key(NamedKey::Tab);
    assert_ne!(
        after.out.as_ref().expect("a value").focused,
        Some(dot_id(1))
    );
}

#[test]
fn enter_and_space_on_a_focused_dot_select_it() {
    let mut rig = Rig::new(4);
    rig.run(100);
    rig.focus(dot_id(2));
    let (first, second) = rig.key(NamedKey::Enter);
    assert!(first.resp.changed || second.resp.changed);
    assert_eq!(rig.page, 2);
}

#[test]
fn without_dots_there_are_no_dot_nodes_and_one_page_has_none_either() {
    let mut rig = Rig::new(3);
    rig.dots = false;
    let out = rig.frame().out.expect("out");
    assert!(out.tree.get(dot_id(0)).is_none());
    assert!(out.tree.nodes.iter().all(|n| n.role != Role::TabList));
    let mut rig = Rig::new(1);
    let out = rig.frame().out.expect("out");
    assert!(out.tree.get(dot_id(0)).is_none(), "one page, no dots");
    // And one page has nothing to swipe.
    let p = rig.drag(FINGER, (-60.0, 0.0), 8);
    rig.lift((p.0 - 60.0, p.1));
    assert_eq!(rig.run(200).lefts, vec![(0, 0.0)]);
}

#[test]
fn a_disabled_scope_leaves_the_nodes_disabled() {
    let mut rig = Rig::new(3);
    rig.disabled = true;
    let out = rig.frame().out.expect("out");
    assert!(out.tree.get(dot_id(1)).is_some());
    let bad = disabled_violations(&out.tree, WidgetId::ROOT.with("root"));
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn pages_are_drawn_in_their_own_id_scopes() {
    // Two pages draw the same widget keys ("body"): they do not collide while both show.
    let mut rig = Rig::new(3);
    rig.run(100);
    rig.drag(FINGER, (-20.0, 0.0), 8);
    let s = rig.frame();
    assert_eq!(s.lefts.len(), 2);
    let out = s.out.expect("out");
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
    assert_eq!(ids.len(), n);
    assert_eq!(n, 2);
}

#[test]
fn a_rect_that_resizes_or_a_count_that_changes_cuts_to_the_page() {
    let mut rig = Rig::new(4);
    rig.page = 2;
    rig.run(100);
    rig.count = 3;
    let s = rig.frame();
    assert_eq!(s.lefts, vec![(2, 0.0)]);
    rig.count = 2;
    let s = rig.frame();
    assert_eq!(rig.page, 1);
    assert_eq!(s.lefts, vec![(1, 0.0)]);
}
