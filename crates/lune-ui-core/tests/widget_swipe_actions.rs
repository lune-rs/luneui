//! Swipe row actions (`widget.swipe-actions`, D374): the content moving by the finger with the
//! buttons uncovered flush behind it, release to open or closed, the rubber band at an edge with
//! no buttons, the full swipe, taps on buttons, a press outside, a mouse that does not swipe,
//! `context` from a secondary click and a long press, and a list that keeps its vertical pans.

use lune_ui_core::access::{Node, Role};
use lune_ui_core::input::{FrameOutput, PointerButton, Repaint};
use lune_ui_core::motion::{MotionTier, rubber_band};
use lune_ui_core::paint::Rect;
use lune_ui_core::scene::Scene;
use lune_ui_core::testing::{UiHarness, disabled_violations};
use lune_ui_core::theme::DARK;
use lune_ui_core::ui::{Ui, WidgetId};
use lune_ui_core::widgets::button::ButtonKind;
use lune_ui_core::widgets::swipe_actions::{SwipeAction, SwipeActions, SwipeEdge, SwipeResponse};
use lune_ui_core::widgets::tone::Tone;
use lune_ui_core::widgets::virtual_list::{Heights, ListOptions, RowKey, SelectionMode};

const WIN: (f32, f32) = (400.0, 600.0);
const ROW_H: f32 = 64.0;
const TOP: f32 = 100.0;

fn row_rect(i: usize) -> Rect {
    Rect::new(0.0, TOP + ROW_H * i as f32, 400.0, ROW_H)
}

fn row_id(i: usize) -> WidgetId {
    WidgetId::ROOT.with(("swipe", i))
}

fn action_id(i: usize, trailing: bool, k: usize) -> WidgetId {
    row_id(i).with(("action", trailing, k))
}

fn content_id(i: usize) -> WidgetId {
    WidgetId::ROOT.with(("content", i))
}

/// What one frame showed.
#[derive(Debug, Default)]
struct Seen {
    /// Per row: the response without its value.
    action: Vec<Option<(SwipeEdge, usize)>>,
    open: Vec<Option<SwipeEdge>>,
    context: Vec<bool>,
    /// Per row: the content rect the closure was given.
    content: Vec<Rect>,
    /// Rows whose body button was clicked.
    body: Vec<usize>,
    repaint: Option<Repaint>,
    out: Option<FrameOutput>,
    scene: Option<Scene>,
}

impl Seen {
    fn left(&self, row: usize) -> f32 {
        self.content[row].x
    }

    fn node(&self, id: WidgetId) -> Option<Rect> {
        self.out
            .as_ref()
            .and_then(|o| o.tree.get(id))
            .map(|n| n.bounds)
    }
}

struct Rig {
    h: UiHarness,
    ms: u64,
    rows: usize,
    full: bool,
    leading: bool,
    trailing: bool,
    disabled: bool,
}

const MORE: SwipeAction<'static> = SwipeAction::new("More");
const DELETE: SwipeAction<'static> = SwipeAction::new("Delete").with_tone(Tone::Err);
const READ: SwipeAction<'static> = SwipeAction::new("Read").with_tone(Tone::Ok);
const TRAILING: [SwipeAction<'static>; 2] = [MORE, DELETE];
const LEADING: [SwipeAction<'static>; 1] = [READ];

fn actions(leading: bool, trailing: bool, full: bool) -> SwipeActions<'static> {
    let mut a = SwipeActions::new().with_full_swipe(full);
    if leading {
        a = a.with_leading(&LEADING);
    }
    if trailing {
        a = a.with_trailing(&TRAILING);
    }
    a
}

/// A row's content: a fill, a node over it and a button on its left.
fn content(ui: &mut Ui<'_>, i: usize, r: Rect, body: &mut Vec<usize>) {
    let t = *ui.theme();
    ui.painter().fill(r, 0.0, t.surface_overlay);
    ui.node(Node::new(content_id(i), Role::Group, "row", r));
    let b = Rect::new(r.x + 16.0, r.y + 12.0, 80.0, 40.0);
    if ui
        .button(("body", i), b, "Open", ButtonKind::Primary)
        .clicked
    {
        body.push(i);
    }
    // A second button near the content's right end: it stays on screen while the row is open.
    let b2 = Rect::new(r.x + 300.0, r.y + 12.0, 80.0, 40.0);
    if ui
        .button(("body2", i), b2, "Next", ButtonKind::Primary)
        .clicked
    {
        body.push(i + 100);
    }
}

impl Rig {
    fn new(rows: usize) -> Rig {
        let mut rig = Rig {
            h: UiHarness::new(WIN),
            ms: 0,
            rows,
            full: false,
            leading: true,
            trailing: true,
            disabled: false,
        };
        rig.frame();
        rig
    }

    fn frame(&mut self) -> Seen {
        self.ms += 16;
        self.h.at_ms(self.ms);
        let mut seen = Seen::default();
        let (rows, a, disabled) = (
            self.rows,
            actions(self.leading, self.trailing, self.full),
            self.disabled,
        );
        let (scene, out) = self.h.frame(|ui| {
            let mut run = |ui: &mut Ui<'_>| {
                for i in 0..rows {
                    let mut body = Vec::new();
                    let mut got = Rect::new(0.0, 0.0, 0.0, 0.0);
                    let res: SwipeResponse<()> =
                        ui.swipe_actions(("swipe", i), row_rect(i), &a, |ui, r| {
                            got = r;
                            content(ui, i, r, &mut body);
                        });
                    seen.action.push(res.action);
                    seen.open.push(res.open);
                    seen.context.push(res.context);
                    seen.content.push(got);
                    seen.body.extend(body);
                }
            };
            if disabled {
                ui.with_enabled(false, |ui| {
                    ui.node_container(
                        Node::new(
                            WidgetId::ROOT.with("root"),
                            Role::Group,
                            "root",
                            Rect::new(0.0, 0.0, 400.0, 600.0),
                        ),
                        |ui| run(ui),
                    );
                });
            } else {
                run(ui);
            }
        });
        seen.repaint = Some(out.repaint);
        seen.out = Some(out);
        seen.scene = Some(scene);
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

    /// A finger down at `at`, then `moves` frames each moving it by `step`; held.
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

    fn lift_after_rest(&mut self, p: (f32, f32), rest_frames: u32) -> Seen {
        for _ in 0..rest_frames {
            self.frame();
        }
        self.h.input.touch_end(1, p.0, p.1);
        self.frame()
    }

    fn lift(&mut self, p: (f32, f32)) -> Seen {
        self.h.input.touch_end(1, p.0, p.1);
        self.frame()
    }

    /// A tap of a finger at `at`: down, a frame, up.
    fn tap(&mut self, at: (f32, f32)) -> Seen {
        self.h.input.touch_start(1, at.0, at.1);
        self.frame();
        self.h.input.touch_end(1, at.0, at.1);
        self.frame()
    }

    fn click_at(&mut self, at: (f32, f32), button: PointerButton) -> Seen {
        self.h.input.pointer_to(at.0, at.1);
        self.h.input.click(button);
        let s = self.frame();
        self.h.input.pointer_leave();
        self.frame();
        s
    }

    /// Opens row `i`'s trailing edge by a slow drag of 120 px and a still lift, then settles.
    fn open_trailing(&mut self, i: usize) {
        let y = row_rect(i).y + 32.0;
        let p = self.drag((300.0, y), (-30.0, 0.0), 6);
        self.lift_after_rest(p, 10);
        self.run(900);
    }
}

const FINGER_Y: f32 = TOP + 32.0;

/// The widths of the trailing buttons of row `i` as shown, in order.
fn trailing_rects(s: &Seen, i: usize) -> Vec<Rect> {
    (0..2)
        .filter_map(|k| s.node(action_id(i, true, k)))
        .collect()
}

#[test]
fn a_row_with_no_actions_is_its_closure_and_nothing_else() {
    let mut rig = Rig::new(1);
    rig.leading = false;
    rig.trailing = false;
    let s = rig.run(100);
    assert_eq!(s.content[0], row_rect(0));
    assert_eq!(s.repaint, Some(Repaint::Idle));
    let p = rig.drag((300.0, FINGER_Y), (-30.0, 0.0), 5);
    let s = rig.lift(p);
    assert_eq!(s.content[0], row_rect(0), "nothing to uncover");
    assert!(s.open[0].is_none());
}

#[test]
fn at_rest_the_content_fills_the_row_with_no_buttons_and_nothing_repaints() {
    let mut rig = Rig::new(1);
    let s = rig.run(200);
    assert_eq!(s.content[0], row_rect(0));
    assert!(s.node(action_id(0, true, 0)).is_none());
    assert!(s.node(action_id(0, false, 0)).is_none());
    assert_eq!(s.repaint, Some(Repaint::Idle));
    assert_eq!((s.open[0], s.action[0], s.context[0]), (None, None, false));
}

#[test]
fn a_leftward_pan_follows_the_finger_and_uncovers_the_trailing_buttons_flush() {
    let mut rig = Rig::new(1);
    let p = rig.drag((300.0, FINGER_Y), (-10.0, 0.0), 7);
    let s = rig.frame();
    let x = s.left(0);
    // 70 px of travel less the pan slop.
    assert!(x < -50.0 && x > -71.0, "{x}");
    // One more step of 10 moves it by exactly 10.
    rig.h.input.touch_move(1, p.0 - 10.0, p.1);
    let t = rig.frame();
    assert!((t.left(0) - (x - 10.0)).abs() < 0.01);
    assert!(t.open[0].is_none(), "the lift decides");
    let b = trailing_rects(&t, 0);
    assert_eq!(b.len(), 2);
    let row = row_rect(0);
    // Flush with the row's height; the content edge touches the first button, the outermost
    // touches the window edge, neighbours touch.
    for r in &b {
        assert_eq!((r.y, r.h), (row.y, row.h));
    }
    assert!((b[0].x - (t.left(0) + 400.0)).abs() < 0.01, "no gap");
    assert!((b[0].right() - b[1].x).abs() < 0.01);
    assert!((b[1].right() - 400.0).abs() < 0.01);
}

#[test]
fn the_buttons_are_at_least_74_wide_when_open_and_in_screen_order() {
    let mut rig = Rig::new(1);
    rig.open_trailing(0);
    let s = rig.frame();
    assert_eq!(s.open[0], Some(SwipeEdge::Trailing));
    let b = trailing_rects(&s, 0);
    assert_eq!(b.len(), 2);
    for r in &b {
        assert!(r.w >= 74.0, "{r:?}");
    }
    assert!((b[1].right() - 400.0).abs() < 0.01);
    assert!((s.left(0) + (b[0].w + b[1].w)).abs() < 0.01);
    assert_eq!(s.repaint, Some(Repaint::Idle), "open and still");
    // The nodes name the actions.
    let out = s.out.as_ref().expect("frame output");
    let n = out.tree.get(action_id(0, true, 1)).expect("a node");
    assert_eq!((n.role, n.name.as_str()), (Role::Button, "Delete"));
}

#[test]
fn a_slow_release_past_the_middle_opens_and_a_short_one_springs_closed() {
    let mut rig = Rig::new(1);
    let p = rig.drag((300.0, FINGER_Y), (-30.0, 0.0), 6);
    let s = rig.lift_after_rest(p, 10);
    assert_eq!(s.open[0], Some(SwipeEdge::Trailing));
    let end = rig.run(900);
    let open_at = end.left(0);
    assert!(open_at <= -147.99 && open_at > -400.0, "{open_at}");
    assert_eq!(end.repaint, Some(Repaint::Idle));

    let mut rig = Rig::new(1);
    let p = rig.drag((300.0, FINGER_Y), (-6.0, 0.0), 4);
    let s = rig.lift_after_rest(p, 10);
    assert_eq!(s.open[0], None);
    let end = rig.run(900);
    assert_eq!(end.left(0), 0.0);
    assert_eq!(end.repaint, Some(Repaint::Idle));
}

#[test]
fn a_fast_flick_opens_and_a_flick_back_closes() {
    let mut rig = Rig::new(1);
    let p = rig.drag((300.0, FINGER_Y), (-25.0, 0.0), 3);
    let s = rig.lift((p.0 - 25.0, p.1));
    assert_eq!(s.open[0], Some(SwipeEdge::Trailing), "a flick opens");
    rig.run(900);
    // Now open: a flick to the right closes it.
    let p = rig.drag((100.0, FINGER_Y), (25.0, 0.0), 3);
    let s = rig.lift((p.0 + 25.0, p.1));
    assert_eq!(s.open[0], None);
    assert_eq!(rig.run(900).left(0), 0.0);
}

#[test]
fn a_rightward_pan_uncovers_the_leading_button_at_the_left_edge() {
    let mut rig = Rig::new(1);
    let p = rig.drag((100.0, FINGER_Y), (30.0, 0.0), 6);
    let s = rig.lift_after_rest(p, 10);
    assert_eq!(s.open[0], Some(SwipeEdge::Leading));
    let end = rig.run(900);
    let b = end
        .node(action_id(0, false, 0))
        .expect("the leading button");
    assert!((b.x - 0.0).abs() < 0.01 && b.w >= 74.0, "{b:?}");
    assert!((end.left(0) - b.w).abs() < 0.01, "content edge touches it");
}

#[test]
fn an_edge_with_no_buttons_rubber_bands_and_springs_back() {
    let mut rig = Rig::new(1);
    rig.leading = false;
    let p = rig.drag((100.0, FINGER_Y), (30.0, 0.0), 8);
    let s = rig.frame();
    let x = s.left(0);
    let travel = p.0 - 100.0;
    assert!(x > 20.0 && x < 0.6 * travel, "{x} of {travel}");
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
    assert!(s.node(action_id(0, false, 0)).is_none(), "no buttons there");
    let s = rig.lift_after_rest(p, 10);
    assert_eq!(s.open[0], None);
    assert_eq!(rig.run(900).left(0), 0.0);
}

#[test]
fn without_a_full_swipe_a_long_pull_rubber_bands_past_the_open_width() {
    let mut rig = Rig::new(1);
    rig.open_trailing(0);
    let open = rig.frame().left(0);
    let p = rig.drag((300.0, FINGER_Y), (-30.0, 0.0), 8);
    let s = rig.frame();
    let more = open - s.left(0);
    assert!(more > 5.0 && more < 150.0, "{more}");
    let s = rig.lift_after_rest(p, 10);
    assert_eq!(s.open[0], Some(SwipeEdge::Trailing));
    assert!((rig.run(900).left(0) - open).abs() < 0.5);
}

#[test]
fn a_full_swipe_stretches_the_outermost_and_reports_it_on_arrival_then_closes() {
    let mut rig = Rig::new(1);
    rig.full = true;
    // 280 px: past 60 % of the row's 400.
    let p = rig.drag((380.0, FINGER_Y), (-35.0, 0.0), 9);
    let mid = rig.frame();
    let x = mid.left(0);
    assert!(x < -240.0, "{x}");
    let b = trailing_rects(&mid, 0);
    let outer = b.last().expect("the outermost");
    assert!((outer.right() - 400.0).abs() < 0.01);
    assert!(
        outer.w >= 240.0,
        "the outermost fills the uncovered width: {outer:?}"
    );
    assert_eq!(b.len(), 1, "the other button is gone: {b:?}");
    assert!((outer.x - (x + 400.0)).abs() < 0.01, "touches the content");

    rig.lift_after_rest(p, 10);
    let mut got = None;
    let mut closed_after = false;
    for _ in 0..120 {
        let s = rig.frame();
        if got.is_none() {
            got = s.action[0];
            if got.is_some() {
                assert!(
                    s.left(0) < -390.0,
                    "reported on arrival at the full width: {}",
                    s.left(0)
                );
            }
        } else {
            assert!(s.action[0].is_none(), "reported once");
            closed_after |= s.left(0) > -390.0;
        }
    }
    assert_eq!(got, Some((SwipeEdge::Trailing, 1)));
    assert!(closed_after);
    let end = rig.run(600);
    assert_eq!(end.left(0), 0.0);
    assert!(end.open[0].is_none());
    assert_eq!(end.repaint, Some(Repaint::Idle));
}

#[test]
fn a_full_swipe_short_of_60_percent_opens_and_reports_nothing() {
    let mut rig = Rig::new(1);
    rig.full = true;
    let p = rig.drag((380.0, FINGER_Y), (-20.0, 0.0), 9);
    let s = rig.lift_after_rest(p, 10);
    assert_eq!(s.open[0], Some(SwipeEdge::Trailing));
    for _ in 0..60 {
        assert!(rig.frame().action[0].is_none());
    }
}

#[test]
fn a_full_swipe_on_the_leading_edge_reports_its_outermost_button() {
    let mut rig = Rig::new(1);
    rig.full = true;
    let p = rig.drag((20.0, FINGER_Y), (35.0, 0.0), 9);
    rig.lift_after_rest(p, 10);
    let mut got = None;
    for _ in 0..120 {
        got = got.or(rig.frame().action[0]);
    }
    assert_eq!(got, Some((SwipeEdge::Leading, 0)));
}

#[test]
fn a_tap_on_a_button_reports_it_and_closes_the_row() {
    let mut rig = Rig::new(1);
    rig.open_trailing(0);
    let b = rig
        .frame()
        .node(action_id(0, true, 1))
        .expect("the outermost");
    let s = rig.tap((b.x + b.w / 2.0, b.y + b.h / 2.0));
    assert_eq!(s.action[0], Some((SwipeEdge::Trailing, 1)));
    assert_eq!(s.open[0], None);
    assert_eq!(rig.run(900).left(0), 0.0);

    // The first button, by a mouse click.
    let mut rig = Rig::new(1);
    rig.open_trailing(0);
    let b = rig
        .frame()
        .node(action_id(0, true, 0))
        .expect("the first button");
    let s = rig.click_at((b.x + b.w / 2.0, b.y + b.h / 2.0), PointerButton::Primary);
    assert_eq!(s.action[0], Some((SwipeEdge::Trailing, 0)));
    assert_eq!(rig.run(900).left(0), 0.0);
}

#[test]
fn a_tap_on_the_closed_rows_content_goes_to_the_content() {
    let mut rig = Rig::new(1);
    rig.run(50);
    let s = rig.tap((50.0, TOP + 32.0));
    assert_eq!(s.body, vec![0]);
    assert!(s.action[0].is_none());
    let mut rig = Rig::new(1);
    rig.run(50);
    let s = rig.click_at((50.0, TOP + 32.0), PointerButton::Primary);
    assert_eq!(s.body, vec![0]);
}

#[test]
fn a_press_on_the_open_rows_content_closes_it_and_reaches_nothing() {
    let mut rig = Rig::new(1);
    rig.run(50);
    // Closed, the second button takes a tap.
    let s = rig.tap((340.0, TOP + 32.0));
    assert_eq!(s.body, vec![100]);
    rig.open_trailing(0);
    let at = rig.frame();
    let x = at.left(0) + 340.0;
    assert!(
        x > 0.0 && x < 400.0 + at.left(0),
        "the button is still on screen: {x}"
    );
    let s = rig.tap((x, TOP + 32.0));
    assert!(s.body.is_empty(), "the content takes no press while open");
    assert!(s.action[0].is_none());
    assert_eq!(s.open[0], None);
    assert_eq!(rig.run(900).left(0), 0.0);
}

#[test]
fn a_press_outside_closes_an_open_row_and_goes_on_where_it_landed() {
    let mut rig = Rig::new(2);
    rig.open_trailing(0);
    assert!(rig.frame().left(0) <= -147.99);
    // A tap on the second row's body button: it clicks, and the first row closes.
    let y = row_rect(1).y + 32.0;
    let s = rig.tap((50.0, y));
    assert_eq!(s.body, vec![1], "the press was not swallowed");
    assert_eq!(s.open[0], None);
    assert_eq!(rig.run(900).left(0), 0.0);
}

#[test]
fn opening_a_second_row_closes_the_first() {
    let mut rig = Rig::new(2);
    rig.open_trailing(0);
    assert_eq!(rig.frame().open[0], Some(SwipeEdge::Trailing));
    rig.open_trailing(1);
    let s = rig.run(900);
    assert_eq!(s.open[0], None, "the first closed on the press");
    assert_eq!(s.open[1], Some(SwipeEdge::Trailing));
    assert_eq!(s.left(0), 0.0);
    assert!(s.left(1) <= -147.99);
}

#[test]
fn a_press_on_a_row_in_motion_catches_it_where_it_shows() {
    let mut rig = Rig::new(1);
    let p = rig.drag((300.0, FINGER_Y), (-30.0, 0.0), 6);
    rig.lift_after_rest(p, 10);
    let before = rig.frame().left(0);
    assert!(before < 0.0 && before > -400.0);
    rig.h.input.touch_start(1, 200.0, FINGER_Y);
    let caught = rig.frame();
    let held = caught.left(0);
    assert!((held - before).abs() < 80.0);
    let later = rig.run(200).left(0);
    assert!((later - held).abs() < 0.5, "held: {held} -> {later}");
    rig.lift((200.0, FINGER_Y));
    let end = rig.run(1000);
    assert!(
        end.left(0) == 0.0 || end.left(0) <= -147.99,
        "{}",
        end.left(0)
    );
}

#[test]
fn a_mouse_never_swipes() {
    let mut rig = Rig::new(1);
    rig.h.input.pointer_to(300.0, FINGER_Y);
    rig.h.input.press(PointerButton::Primary);
    rig.frame();
    for k in 1..=8 {
        rig.h.input.pointer_to(300.0 - 30.0 * k as f32, FINGER_Y);
        let s = rig.frame();
        assert_eq!(s.left(0), 0.0);
    }
    rig.h.input.release(PointerButton::Primary);
    let s = rig.frame();
    assert_eq!(s.left(0), 0.0);
    assert!(s.open[0].is_none());
    // A wheel does not either.
    rig.h.input.pointer_to(200.0, FINGER_Y);
    rig.h.input.scroll_lines(-5.0, 0.0);
    assert_eq!(rig.frame().left(0), 0.0);
}

#[test]
fn a_secondary_click_or_a_long_press_on_the_row_sets_context_and_a_button_does_not() {
    let mut rig = Rig::new(1);
    rig.run(50);
    let s = rig.click_at((200.0, FINGER_Y), PointerButton::Secondary);
    assert!(s.context[0]);
    // A long press on bare content.
    let mut rig = Rig::new(1);
    rig.run(50);
    rig.h.input.touch_start(1, 200.0, FINGER_Y);
    let mut seen = false;
    for _ in 0..50 {
        seen |= rig.frame().context[0];
    }
    assert!(seen, "a long press reports context");
    rig.lift((200.0, FINGER_Y));
    assert_eq!(rig.run(900).left(0), 0.0, "and opens nothing");

    // Not from a button.
    let mut rig = Rig::new(1);
    rig.open_trailing(0);
    let b = rig.frame().node(action_id(0, true, 0)).expect("a button");
    let s = rig.click_at((b.x + b.w / 2.0, b.y + b.h / 2.0), PointerButton::Secondary);
    assert!(!s.context[0]);
}

#[test]
fn a_vertical_pan_in_a_list_of_swipe_rows_scrolls_the_list_and_opens_no_row() {
    let mut h = UiHarness::new(WIN);
    let mut ms = 0u64;
    let rect = Rect::new(0.0, 0.0, 400.0, 600.0);
    let frame = |h: &mut UiHarness, ms: &mut u64, open: &mut Vec<usize>| -> f64 {
        *ms += 16;
        h.at_ms(*ms);
        let mut offset = 0.0;
        h.frame(|ui| {
            let opts = ListOptions {
                count: 30,
                heights: Heights::Fixed(ROW_H),
                selection: SelectionMode::None,
                index_of: None,
                label: "Rows",
            };
            let a = actions(false, true, false);
            let res = ui.virtual_list(
                "list",
                rect,
                &opts,
                |i| RowKey(i as u64),
                |ui, ctx| {
                    let i = ctx.index;
                    let r = ui.swipe_actions(("swipe", i), ctx.rect, &a, |ui, c| {
                        let t = *ui.theme();
                        ui.painter().fill(c, 0.0, t.surface_overlay);
                    });
                    if r.open.is_some() {
                        open.push(i);
                    }
                    ROW_H
                },
            );
            offset = res.offset;
        });
        offset
    };
    let mut open = Vec::new();
    frame(&mut h, &mut ms, &mut open);
    h.input.touch_start(1, 200.0, 300.0);
    frame(&mut h, &mut ms, &mut open);
    let mut y = 300.0;
    let mut offset = 0.0;
    for _ in 0..8 {
        y -= 20.0;
        h.input.touch_move(1, 200.0, y);
        offset = frame(&mut h, &mut ms, &mut open);
    }
    assert!(offset > 100.0, "the list scrolled: {offset}");
    h.input.touch_end(1, 200.0, y);
    for _ in 0..30 {
        frame(&mut h, &mut ms, &mut open);
    }
    assert!(open.is_empty(), "no row opened: {open:?}");
}

#[test]
fn a_swipe_in_a_list_opens_the_row() {
    let mut h = UiHarness::new(WIN);
    let mut ms = 0u64;
    let rect = Rect::new(0.0, 0.0, 400.0, 600.0);
    let frame = |h: &mut UiHarness, ms: &mut u64, open: &mut Vec<usize>, off: &mut f64| {
        *ms += 16;
        h.at_ms(*ms);
        h.frame(|ui| {
            let opts = ListOptions {
                count: 30,
                heights: Heights::Fixed(ROW_H),
                selection: SelectionMode::None,
                index_of: None,
                label: "Rows",
            };
            let a = actions(false, true, false);
            let res = ui.virtual_list(
                "list",
                rect,
                &opts,
                |i| RowKey(i as u64),
                |ui, ctx| {
                    let i = ctx.index;
                    let r = ui.swipe_actions(("swipe", i), ctx.rect, &a, |ui, c| {
                        let t = *ui.theme();
                        ui.painter().fill(c, 0.0, t.surface_overlay);
                    });
                    if r.open.is_some() {
                        open.push(i);
                    }
                    ROW_H
                },
            );
            *off = res.offset;
        });
    };
    let (mut open, mut off) = (Vec::new(), 0.0);
    frame(&mut h, &mut ms, &mut open, &mut off);
    h.input.touch_start(1, 300.0, 100.0);
    frame(&mut h, &mut ms, &mut open, &mut off);
    let mut x = 300.0;
    for _ in 0..6 {
        x -= 30.0;
        h.input.touch_move(1, x, 100.0);
        frame(&mut h, &mut ms, &mut open, &mut off);
    }
    for _ in 0..10 {
        frame(&mut h, &mut ms, &mut open, &mut off);
    }
    h.input.touch_end(1, x, 100.0);
    frame(&mut h, &mut ms, &mut open, &mut off);
    assert!(open.contains(&1), "row 1 (y 64..128) opened: {open:?}");
    assert_eq!(off, 0.0, "the list did not scroll");
}

#[test]
fn a_disabled_scope_swipes_nothing_and_breaks_no_key_rule() {
    let mut rig = Rig::new(1);
    rig.disabled = true;
    rig.run(50);
    let p = rig.drag((300.0, FINGER_Y), (-30.0, 0.0), 6);
    let s = rig.lift_after_rest(p, 10);
    assert_eq!(s.left(0), 0.0);
    assert!(s.open[0].is_none());
    let out = s.out.expect("frame output");
    let bad = disabled_violations(&out.tree, WidgetId::ROOT.with("root"));
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn a_resize_cuts_to_closed() {
    let mut rig = Rig::new(1);
    rig.open_trailing(0);
    assert!(rig.frame().left(0) <= -147.99);
    // The row is the same key at another width: the state starts over.
    rig.h.at_ms(rig.ms + 16);
    rig.ms += 16;
    let (_, out) = rig.h.frame(|ui| {
        let a = actions(true, true, false);
        let r = ui.swipe_actions(
            ("swipe", 0),
            Rect::new(0.0, TOP, 300.0, ROW_H),
            &a,
            |_, r| r,
        );
        assert_eq!(r.value.x, 0.0);
        assert!(r.open.is_none());
    });
    let _ = out;
}

#[test]
fn reduced_and_off_tiers_close_by_a_snap_but_a_release_still_springs() {
    for tier in [MotionTier::Reduced, MotionTier::Off] {
        let mut rig = Rig::new(2);
        rig.h.tier = tier;
        rig.open_trailing(0);
        let open = rig.frame().left(0);
        assert!(open <= -147.99, "{tier:?}: a release springs open: {open}");
        // A press outside closes by a programmatic move: it cuts.
        let y = row_rect(1).y + 32.0;
        rig.tap((50.0, y));
        let s = rig.frame();
        assert_eq!(s.left(0), 0.0, "{tier:?}: the close snapped");
    }
}

/// The straight colours (rounded to bytes) the scene's fills draw.
fn fill_colours(scene: &Scene) -> Vec<[u8; 3]> {
    let mut all = Vec::new();
    for layer in scene.in_order() {
        let g = &layer.geometry;
        all.extend(g.under.iter().map(|s| s.color));
        all.extend(g.over.iter().map(|s| s.color));
        all.extend(g.fills.iter().map(|v| v.color));
    }
    all.into_iter()
        .filter(|c| c[3] >= 0.02)
        .map(|c| {
            let k = |x: f32| (x / c[3] * 255.0).round().clamp(0.0, 255.0) as u8;
            [k(c[0]), k(c[1]), k(c[2])]
        })
        .collect()
}

#[test]
fn the_buttons_take_their_fills_from_the_theme() {
    let mut rig = Rig::new(1);
    let mut theme = DARK;
    theme.err = [201, 12, 13, 255];
    theme.ok = [14, 202, 15, 255];
    theme.accent = [16, 17, 203, 255];
    rig.h.theme = theme;
    rig.open_trailing(0);
    let s = rig.frame();
    let fills = fill_colours(s.scene.as_ref().expect("a scene"));
    // The trailing buttons: the neutral one in the accent, "Delete" in the error colour.
    assert!(fills.contains(&[16, 17, 203]), "accent: {fills:?}");
    assert!(fills.contains(&[201, 12, 13]), "err: {fills:?}");
    // And the leading one in the ok colour.
    let mut rig = Rig::new(1);
    rig.h.theme = theme;
    let p = rig.drag((100.0, FINGER_Y), (30.0, 0.0), 6);
    rig.lift_after_rest(p, 10);
    rig.run(900);
    let fills = fill_colours(rig.frame().scene.as_ref().expect("a scene"));
    assert!(fills.contains(&[14, 202, 15]), "ok: {fills:?}");
}
