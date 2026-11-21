//! Pull to refresh (`layout.pull-to-refresh`, D375): `refresh` on the release of a touch pan that
//! showed at least 64 px past the top (not at 63), never from a wheel, a mouse, a key or a clamped
//! area, once per pan; the hold (the rect 56 px lower and shorter while the app refreshes, with
//! the top of the content continuous across the release); the band closing; the scroll offset
//! unchanged by the band; the spinner's node.

use lune_ui_core::access::{Node, Role, State};
use lune_ui_core::input::{FrameOutput, PointerButton, Repaint};
use lune_ui_core::motion::MotionTier;
use lune_ui_core::paint::Rect;
use lune_ui_core::testing::{UiHarness, disabled_violations};
use lune_ui_core::ui::{Ui, WidgetId};
use lune_ui_core::widgets::pull_to_refresh::{BAND, THRESHOLD};
use lune_ui_core::widgets::scroll_area::{Extent, Overscroll, ScrollOptions, ScrollPhysics};

const WIN: (f32, f32) = (400.0, 700.0);
const TOP: f32 = 100.0;
const AREA: Rect = Rect::new(0.0, TOP, 400.0, 500.0);
const ROW_H: f32 = 50.0;
const ROWS: usize = 30;

fn row_id(i: usize) -> WidgetId {
    WidgetId::ROOT.with(("row", i))
}

/// What one frame showed.
struct Seen {
    refresh: bool,
    offset: (f64, f64),
    viewport: Rect,
    repaint: Repaint,
    out: FrameOutput,
}

impl Seen {
    /// The first row's top, as the node says: where the content shows.
    fn top(&self) -> f32 {
        self.out
            .tree
            .get(row_id(0))
            .map(|n| n.bounds.y)
            .expect("the first row's node")
    }

    /// How far the content shows past the top of the whole rect.
    fn shown(&self) -> f32 {
        self.top() - TOP
    }
}

struct Rig {
    h: UiHarness,
    ms: u64,
    refreshing: bool,
    clamp: bool,
    disabled: bool,
}

impl Rig {
    fn new() -> Rig {
        let mut rig = Rig {
            h: UiHarness::new(WIN),
            ms: 0,
            refreshing: false,
            clamp: false,
            disabled: false,
        };
        rig.frame();
        rig
    }

    fn frame(&mut self) -> Seen {
        self.ms += 16;
        self.h.at_ms(self.ms);
        let (refreshing, clamp, disabled) = (self.refreshing, self.clamp, self.disabled);
        let mut got = None;
        let (_, out) = self.h.frame(|ui| {
            let mut run = |ui: &mut Ui<'_>| {
                let opts = ScrollOptions::default();
                let res = ui.pull_to_refresh("list", AREA, refreshing, &opts, |ui, view| {
                    for i in 0..ROWS {
                        let r = Rect::new(
                            view.viewport.x,
                            view.viewport.y + i as f32 * ROW_H,
                            view.viewport.w,
                            ROW_H,
                        );
                        ui.node(Node::new(row_id(i), Role::Group, "row", r));
                    }
                    ((), Extent::new(400.0, ROWS as f64 * f64::from(ROW_H)))
                });
                got = Some((res.refresh, res.scroll.offset, res.scroll.viewport));
            };
            let mut inner = |ui: &mut Ui<'_>| {
                if clamp {
                    let physics = ScrollPhysics::default().with_overscroll(Overscroll::Clamp);
                    ui.with_scroll_physics(physics, &mut run);
                } else {
                    run(ui);
                }
            };
            if disabled {
                ui.with_enabled(false, |ui| {
                    ui.node_container(
                        Node::new(
                            WidgetId::ROOT.with("root"),
                            Role::Group,
                            "root",
                            Rect::new(0.0, 0.0, WIN.0, WIN.1),
                        ),
                        |ui| inner(ui),
                    );
                });
            } else {
                inner(ui);
            }
        });
        let (refresh, offset, viewport) = got.expect("the frame ran");
        Seen {
            refresh,
            offset,
            viewport,
            repaint: out.repaint,
            out,
        }
    }

    fn run(&mut self, ms: u64) -> Seen {
        let end = self.ms + ms;
        let mut last = self.frame();
        while self.ms < end {
            last = self.frame();
        }
        last
    }

    /// A finger down at the area's middle, then moves of `step` px down, one frame each, until
    /// the content shows at least `shown` px past the top (or 1500 moves). Returns the finger's
    /// position and the last frame.
    fn pull_to(&mut self, shown: f32, step: f32) -> ((f32, f32), Seen) {
        let mut p = (200.0, 300.0);
        self.h.input.touch_start(1, p.0, p.1);
        let mut s = self.frame();
        for _ in 0..1500 {
            if s.shown() >= shown {
                break;
            }
            p.1 += step;
            self.h.input.touch_move(1, p.0, p.1);
            s = self.frame();
        }
        (p, s)
    }

    fn lift(&mut self, p: (f32, f32)) -> Seen {
        self.h.input.touch_end(1, p.0, p.1);
        self.frame()
    }

    /// A pull to `shown`, a still frame, then the lift.
    fn pull_and_lift(&mut self, shown: f32) -> (Seen, Seen) {
        let (p, held) = self.pull_to(shown, 0.25);
        let held = if held.shown() >= shown {
            self.frame()
        } else {
            held
        };
        let released = self.lift(p);
        (held, released)
    }
}

fn spinner_node(s: &Seen) -> Option<&Node> {
    s.out
        .tree
        .nodes
        .iter()
        .find(|n| n.role == Role::Progress && n.name == "Refreshing")
}

#[test]
fn at_rest_the_content_starts_at_the_rect_and_nothing_repaints() {
    let mut rig = Rig::new();
    let s = rig.run(200);
    assert_eq!(s.top(), TOP);
    assert_eq!(s.viewport, AREA);
    assert!(!s.refresh);
    assert_eq!(s.repaint, Repaint::Idle);
    assert!(spinner_node(&s).is_none());
}

#[test]
fn a_release_at_64_px_refreshes_and_one_at_63_does_not() {
    let mut rig = Rig::new();
    let (held, released) = rig.pull_and_lift(THRESHOLD);
    assert!(held.shown() >= THRESHOLD && held.shown() < THRESHOLD + 1.0);
    assert!(!held.refresh, "nothing fires while the finger is down");
    assert!(released.refresh);

    let mut rig = Rig::new();
    let (held, released) = rig.pull_and_lift(THRESHOLD - 1.0);
    assert!(held.shown() >= THRESHOLD - 1.0 && held.shown() < THRESHOLD - 0.2);
    assert!(!released.refresh);
    // And nothing opens: the content springs back to the top.
    let end = rig.run(1500);
    assert_eq!(end.top(), TOP);
    assert_eq!(end.viewport, AREA);
}

#[test]
fn it_fires_once_for_one_pan() {
    let mut rig = Rig::new();
    let (_, released) = rig.pull_and_lift(80.0);
    assert!(released.refresh);
    rig.refreshing = true;
    let mut fired = 0;
    for _ in 0..120 {
        fired += usize::from(rig.frame().refresh);
    }
    assert_eq!(fired, 0);
    // The app drops the flag, and the next pan fires again.
    rig.refreshing = false;
    rig.run(800);
    let (_, released) = rig.pull_and_lift(80.0);
    assert!(released.refresh);
}

#[test]
fn a_pull_while_refreshing_does_not_fire_again() {
    let mut rig = Rig::new();
    rig.refreshing = true;
    rig.run(800);
    let (_, released) = rig.pull_and_lift(90.0);
    assert!(!released.refresh);
}

#[test]
fn the_wheel_the_mouse_and_a_clamped_area_never_fire() {
    // A wheel pull at the top does nothing past the top.
    let mut rig = Rig::new();
    rig.h.input.pointer_to(200.0, 300.0);
    rig.h.input.scroll_pixels(0.0, 300.0);
    let s = rig.frame();
    assert!(!s.refresh);
    assert_eq!(s.top(), TOP);
    // A mouse drag downwards pans nothing.
    rig.h.input.pointer_to(200.0, 300.0);
    rig.h.input.press(PointerButton::Primary);
    rig.frame();
    for k in 1..=20 {
        rig.h.input.pointer_to(200.0, 300.0 + 10.0 * k as f32);
        let s = rig.frame();
        assert_eq!(s.top(), TOP);
    }
    rig.h.input.release(PointerButton::Primary);
    assert!(!rig.frame().refresh);
    // A clamped area shows no overscroll, so a long pull never reaches the threshold.
    let mut rig = Rig::new();
    rig.clamp = true;
    let (p, held) = rig.pull_to(THRESHOLD, 8.0);
    assert_eq!(held.shown(), 0.0);
    let released = rig.lift(p);
    assert!(!released.refresh);
}

#[test]
fn the_content_top_is_continuous_across_the_release_and_ends_56_lower() {
    let mut rig = Rig::new();
    let (held, released) = rig.pull_and_lift(THRESHOLD + 10.0);
    assert!(released.refresh);
    assert!(
        (released.top() - held.top()).abs() < 0.5,
        "{} then {}",
        held.top(),
        released.top()
    );
    // The app answers with the flag; the top never jumps by more than a frame's travel and
    // settles at the band's height under the rect's top.
    rig.refreshing = true;
    let mut prev = released.top();
    let mut last = rig.frame();
    for _ in 0..200 {
        let t = last.top();
        assert!((t - prev).abs() < 12.0, "{prev} to {t}");
        prev = t;
        last = rig.frame();
    }
    assert_eq!(last.top(), TOP + BAND);
    assert_eq!(
        last.viewport,
        Rect::new(0.0, TOP + BAND, 400.0, AREA.h - BAND)
    );
    assert!(rig.frame().repaint != Repaint::Idle, "the spinner turns");
}

#[test]
fn the_spinner_is_a_busy_progress_node_while_the_band_is_open() {
    let mut rig = Rig::new();
    rig.refreshing = true;
    let s = rig.run(800);
    let n = spinner_node(&s).expect("the spinner's node");
    assert!(n.states.contains(State::Busy));
    // Centred in the band, under the rect's top.
    let c = (n.bounds.x + n.bounds.w / 2.0, n.bounds.y + n.bounds.h / 2.0);
    assert!(
        (c.0 - 200.0).abs() < 0.5 && (c.1 - (TOP + BAND / 2.0)).abs() < 0.5,
        "{c:?}"
    );
}

#[test]
fn the_band_closes_when_the_flag_drops() {
    let mut rig = Rig::new();
    rig.refreshing = true;
    let open = rig.run(800);
    assert_eq!(open.top(), TOP + BAND);
    rig.refreshing = false;
    let first = rig.frame();
    assert!(
        first.top() <= TOP + BAND && first.top() >= TOP,
        "{}",
        first.top()
    );
    assert_ne!(first.repaint, Repaint::Idle, "the band is moving");
    let end = rig.run(800);
    assert_eq!(end.top(), TOP);
    assert_eq!(end.viewport, AREA);
    assert_eq!(end.repaint, Repaint::Idle);
    assert!(spinner_node(&end).is_none());
}

#[test]
fn a_flag_that_starts_true_opens_at_once_and_reduced_tiers_cut() {
    for tier in [MotionTier::Reduced, MotionTier::Off] {
        let mut r = Rig::new();
        r.h.tier = tier;
        r.refreshing = true;
        assert_eq!(r.frame().top(), TOP + BAND, "{tier:?}");
        r.refreshing = false;
        assert_eq!(r.frame().top(), TOP, "{tier:?}");
    }
    // A first frame with the flag already on shows the band, with no motion.
    let mut first = Rig {
        h: UiHarness::new(WIN),
        ms: 0,
        refreshing: true,
        clamp: false,
        disabled: false,
    };
    assert_eq!(first.frame().top(), TOP + BAND);
}

#[test]
fn the_band_does_not_change_the_scroll_offset() {
    let mut rig = Rig::new();
    rig.h.input.pointer_to(200.0, 300.0);
    rig.h.input.scroll_pixels(0.0, -120.0);
    let plain = rig.run(600);
    assert_eq!(plain.offset, (0.0, 120.0));
    rig.refreshing = true;
    let held = rig.run(800);
    assert_eq!(held.offset, (0.0, 120.0), "the offset is the content's own");
    assert_eq!(held.top(), TOP + BAND - 120.0);
}

#[test]
fn a_disabled_scope_has_no_enabled_node() {
    let mut rig = Rig::new();
    rig.disabled = true;
    rig.refreshing = true;
    let s = rig.run(800);
    let root = WidgetId::ROOT.with("root");
    assert_eq!(disabled_violations(&s.out.tree, root), vec![]);
}
