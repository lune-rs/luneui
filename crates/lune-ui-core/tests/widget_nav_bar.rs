//! The navigation bar (`widget.nav-bar`, D370): the row at the rect's bottom, the back button, the
//! title and the actions, by mouse, finger and key, in a disabled scope, with a long title.

use lune_ui_core::access::{Role, State};
use lune_ui_core::color::premul;
use lune_ui_core::icon;
use lune_ui_core::input::{FrameOutput, Key, NamedKey, PointerButton};
use lune_ui_core::paint::Rect;
use lune_ui_core::scene::Scene;
use lune_ui_core::testing::{FakeText, UiHarness, disabled_violations};
use lune_ui_core::text::{Spec, Style, TextKey, TextSystem, elide};
use lune_ui_core::theme::DARK;
use lune_ui_core::ui::WidgetId;
use lune_ui_core::widgets::nav_bar::{HEIGHT, NavAction, NavBar, NavBarResponse};

const WIN: (f32, f32) = (402.0, 300.0);
/// A bar under a 62 px notch band: the row is the bottom 44.
const BAR: Rect = Rect::new(0.0, 0.0, 402.0, 106.0);
const PLUS: NavAction<'static> = NavAction::new(icon::ADD, "Add");
const MORE: NavAction<'static> = NavAction::new(icon::ELLIPSIS, "More");

fn bar_id() -> WidgetId {
    WidgetId::ROOT.with("bar")
}

fn back_id() -> WidgetId {
    bar_id().with("back")
}

fn action_id(i: usize) -> WidgetId {
    bar_id().with(("action", i))
}

struct Run {
    r: NavBarResponse,
    out: FrameOutput,
    scene: Scene,
}

struct Rig {
    h: UiHarness,
    ms: u64,
}

impl Rig {
    fn new() -> Self {
        let mut rig = Rig {
            h: UiHarness::new(WIN),
            ms: 0,
        };
        rig.frame(
            BAR,
            &NavBar::new("Inbox")
                .with_back("Mail")
                .with_actions(&[PLUS, MORE]),
        );
        rig
    }

    fn frame(&mut self, rect: Rect, bar: &NavBar<'_>) -> Run {
        self.ms += 16;
        self.h.at_ms(self.ms);
        let mut r = NavBarResponse::default();
        let (scene, out) = self.h.frame(|ui| r = ui.nav_bar("bar", rect, bar));
        Run { r, out, scene }
    }

    /// Focuses `id` (the request lands at the next frame's begin) and draws the frame after.
    fn focus(&mut self, id: WidgetId) {
        self.ms += 16;
        self.h.at_ms(self.ms);
        self.h.frame(|ui| ui.request_focus(id));
        self.std();
    }

    fn std(&mut self) -> Run {
        self.frame(
            BAR,
            &NavBar::new("Inbox")
                .with_back("Mail")
                .with_actions(&[PLUS, MORE]),
        )
    }
}

fn key_of(spec: &Spec) -> TextKey {
    FakeText.prepare(spec, 1.0).0
}

fn title_spec(text: &str) -> Spec {
    Spec::plain(text, Style::ui(DARK.font_title, DARK.fg_primary).bold())
}

fn text_x(scene: &Scene, key: TextKey) -> Option<f32> {
    scene.layers[0]
        .texts
        .iter()
        .find(|t| t.key == key)
        .map(|t| t.x)
}

fn bounds(out: &FrameOutput, id: WidgetId) -> Rect {
    out.tree.get(id).expect("a node").bounds
}

#[test]
fn the_row_is_the_bottom_44_and_the_background_fills_the_whole_rect() {
    assert_eq!(HEIGHT, 44.0);
    let mut rig = Rig::new();
    let run = rig.std();
    let bar = run.out.tree.get(bar_id()).expect("the bar's node");
    assert_eq!(bar.role, Role::Toolbar);
    assert_eq!(bar.name, "Inbox");
    assert_eq!(bar.bounds, BAR);
    // Back at the left edge, the actions in 44 wide cells ending 8 from the right edge.
    let back = bounds(&run.out, back_id());
    assert_eq!((back.x, back.y, back.h), (0.0, 62.0, 44.0));
    assert_eq!(
        bounds(&run.out, action_id(0)),
        Rect::new(402.0 - 8.0 - 88.0, 62.0, 44.0, 44.0)
    );
    assert_eq!(
        bounds(&run.out, action_id(1)),
        Rect::new(402.0 - 8.0 - 44.0, 62.0, 44.0, 44.0)
    );
    // The fill covers the whole rect, the safe-area band included; a hairline closes the bottom.
    let under = &run.scene.layers[0].geometry.under;
    let fill = under
        .iter()
        .find(|s| s.color == premul(DARK.surface_overlay))
        .expect("the background");
    assert_eq!(fill.rect, [0.0, 0.0, 402.0, 106.0]);
    let hair = under
        .iter()
        .find(|s| s.color == premul(DARK.surface_separator))
        .expect("the hairline");
    assert_eq!(hair.rect, [0.0, 105.0, 402.0, 1.0]);
}

#[test]
fn a_rect_shorter_than_the_row_lays_it_from_the_top() {
    let mut rig = Rig::new();
    let short = Rect::new(0.0, 10.0, 402.0, 30.0);
    let run = rig.frame(short, &NavBar::new("Inbox").with_back("Mail"));
    assert_eq!(bounds(&run.out, back_id()).y, 10.0);
    assert_eq!(bounds(&run.out, back_id()).h, 44.0);
}

#[test]
fn a_bar_with_no_back_and_no_actions_has_only_its_node() {
    let mut rig = Rig::new();
    let run = rig.frame(BAR, &NavBar::new("Inbox"));
    assert!(run.out.tree.get(back_id()).is_none());
    assert!(run.out.tree.get(action_id(0)).is_none());
    assert!(
        text_x(&run.scene, key_of(&title_spec("Inbox"))).is_some(),
        "the title is drawn"
    );
}

#[test]
fn the_title_is_centred_on_the_bar_when_the_sides_leave_room() {
    let mut rig = Rig::new();
    let run = rig.std();
    // "Inbox" is 5 chars of 18 px at half width each: 45 wide, centred on 201.
    let x = text_x(&run.scene, key_of(&title_spec("Inbox"))).expect("the title");
    assert!((x - (201.0 - 22.5)).abs() < 0.01, "x {x}");
}

#[test]
fn a_long_title_is_elided_between_the_buttons_and_never_wrapped() {
    let mut rig = Rig::new();
    let long = "A title far too long to fit between the back button and the actions of a phone";
    let run = rig.frame(
        BAR,
        &NavBar::new(long)
            .with_back("Mail")
            .with_actions(&[PLUS, MORE]),
    );
    assert!(
        text_x(&run.scene, key_of(&title_spec(long))).is_none(),
        "the whole title is not drawn"
    );
    let back = bounds(&run.out, back_id());
    let first = bounds(&run.out, action_id(0));
    let (lo, hi) = (back.right() + 8.0, first.x - 8.0);
    let cut = elide(&mut FakeText, &title_spec(long), 1.0, hi - lo);
    let x = text_x(&run.scene, key_of(&cut)).expect("the elided title");
    let w = FakeText.measure(&cut, 1.0).0;
    assert!(
        x >= lo - 0.01 && x + w <= hi + 0.01,
        "x {x} w {w} in {lo}..{hi}"
    );
}

#[test]
fn a_tap_or_click_on_back_and_on_an_action_reports_it() {
    let mut rig = Rig::new();
    let back = bounds(&rig.std().out, back_id());
    let act = bounds(&rig.std().out, action_id(1));

    // The mouse.
    rig.h.input.pointer_to(back.x + 20.0, back.y + 20.0);
    rig.h.input.click(PointerButton::Primary);
    let run = rig.std();
    assert!(run.r.back, "a click on back");
    assert_eq!(run.r.action, None);
    rig.h.input.pointer_to(act.x + 20.0, act.y + 20.0);
    rig.h.input.click(PointerButton::Primary);
    let run = rig.std();
    assert_eq!(run.r.action, Some(1), "a click on the second action");
    assert!(!run.r.back);

    // A finger: at the tap, not at the press.
    rig.h.input.pointer_leave();
    rig.std();
    rig.h.input.touch_start(1, back.x + 20.0, back.y + 20.0);
    assert!(!rig.std().r.back, "not on the press");
    rig.h.input.touch_end(1, back.x + 20.0, back.y + 20.0);
    assert!(rig.std().r.back, "on the tap");
    rig.h.input.touch_start(1, act.x + 20.0, act.y + 20.0);
    rig.std();
    rig.h.input.touch_end(1, act.x + 20.0, act.y + 20.0);
    assert_eq!(rig.std().r.action, Some(1), "a tap on an action");
}

#[test]
fn enter_and_space_on_a_focused_button_do_what_a_tap_does() {
    let mut rig = Rig::new();
    for (key, expect_back) in [(NamedKey::Enter, true), (NamedKey::Space, false)] {
        let target = if expect_back { back_id() } else { action_id(0) };
        rig.focus(target);
        rig.h.input.key_down(Key::Named(key), None);
        let run = rig.std();
        assert_eq!(run.r.back, expect_back, "{key:?}");
        assert_eq!(
            run.r.action,
            if expect_back { None } else { Some(0) },
            "{key:?}"
        );
    }
}

#[test]
fn a_finger_hits_a_narrow_back_button_within_44_by_44() {
    let mut rig = Rig::new();
    let bar = NavBar::new("Inbox").with_back("");
    rig.frame(BAR, &bar);
    let back = bounds(&rig.frame(BAR, &bar).out, back_id());
    assert!(back.w < 44.0, "a glyph alone is narrower: {}", back.w);
    // 10 px right of the drawn button is inside a 44 wide target centred on it, and outside it.
    let x = back.right() + 4.0;
    let y = back.y + 22.0;
    rig.h.input.touch_start(1, x, y);
    rig.frame(BAR, &bar);
    rig.h.input.touch_end(1, x, y);
    assert!(rig.frame(BAR, &bar).r.back, "a finger beside the glyph");
    rig.h.input.pointer_to(x, y);
    rig.h.input.click(PointerButton::Primary);
    assert!(
        !rig.frame(BAR, &bar).r.back,
        "a mouse needs the button itself"
    );
}

#[test]
fn a_disabled_scope_is_inert_and_leaves_no_violation() {
    let mut rig = Rig::new();
    let back = bounds(&rig.std().out, back_id());
    let draw = |rig: &mut Rig| {
        rig.ms += 16;
        rig.h.at_ms(rig.ms);
        let mut r = NavBarResponse::default();
        let (_, out) = rig.h.frame(|ui| {
            ui.with_enabled(false, |ui| {
                r = ui.nav_bar(
                    "bar",
                    BAR,
                    &NavBar::new("Inbox")
                        .with_back("Mail")
                        .with_actions(&[PLUS, MORE]),
                );
            });
        });
        (r, out)
    };
    draw(&mut rig);
    rig.h.input.pointer_to(back.x + 20.0, back.y + 20.0);
    rig.h.input.click(PointerButton::Primary);
    let (r, out) = draw(&mut rig);
    assert_eq!(r, NavBarResponse::default());
    assert_eq!(disabled_violations(&out.tree, bar_id()), []);
    assert!(
        out.tree
            .get(back_id())
            .expect("back")
            .states
            .contains(State::Disabled)
    );
}

#[test]
fn only_the_first_three_actions_are_drawn() {
    let mut rig = Rig::new();
    let four = [PLUS, MORE, PLUS, MORE];
    let run = rig.frame(BAR, &NavBar::new("Inbox").with_actions(&four));
    assert!(run.out.tree.get(action_id(2)).is_some());
    assert!(run.out.tree.get(action_id(3)).is_none());
}
