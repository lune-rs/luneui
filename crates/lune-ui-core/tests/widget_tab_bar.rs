//! The bottom tab bar (`widget.tab-bar`, D370): the row at the rect's top, one tab stop with
//! roving focus, selection by mouse, finger and key, `reselected`, `context` by a secondary click
//! and a long press, the badge, a disabled scope and an out-of-range selection.

use lune_ui_core::access::{Role, State};
use lune_ui_core::color::premul;
use lune_ui_core::icon;
use lune_ui_core::input::{FrameOutput, Key, NamedKey, PointerButton};
use lune_ui_core::paint::Rect;
use lune_ui_core::scene::Scene;
use lune_ui_core::testing::{FakeText, UiHarness, disabled_violations};
use lune_ui_core::text::{Spec, Style, TextKey, TextSystem};
use lune_ui_core::theme::DARK;
use lune_ui_core::ui::WidgetId;
use lune_ui_core::widgets::tab_bar::{HEIGHT, TabBarItem, TabBarResponse};

const WIN: (f32, f32) = (400.0, 300.0);
/// A bar of five 80 wide items over a 34 px home-indicator band.
const BAR: Rect = Rect::new(0.0, 200.0, 400.0, 83.0);

const ITEMS: [TabBarItem<'static>; 5] = [
    TabBarItem::new(icon::NOTE, "Notes"),
    TabBarItem::new(icon::SEARCH, "Search").with_badge("3"),
    TabBarItem::new(icon::CALENDAR, "Plan"),
    TabBarItem::new(icon::SYNC, "Sync"),
    TabBarItem::new(icon::GEAR, "Settings"),
];

fn bar_id() -> WidgetId {
    WidgetId::ROOT.with("tabs")
}

fn item_id(i: usize) -> WidgetId {
    bar_id().with(i)
}

fn item_rect(i: usize) -> Rect {
    Rect::new(BAR.x + 80.0 * i as f32, BAR.y, 80.0, HEIGHT)
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

struct Run {
    r: TabBarResponse,
    out: FrameOutput,
    scene: Scene,
}

struct Rig {
    h: UiHarness,
    ms: u64,
    selected: usize,
}

impl Rig {
    fn new(selected: usize) -> Self {
        let mut rig = Rig {
            h: UiHarness::new(WIN),
            ms: 0,
            selected,
        };
        rig.frame();
        rig
    }

    fn frame(&mut self) -> Run {
        self.ms += 16;
        self.h.at_ms(self.ms);
        let mut r = TabBarResponse::default();
        let selected = &mut self.selected;
        let (scene, out) = self
            .h
            .frame(|ui| r = ui.tab_bar("tabs", BAR, &ITEMS, selected));
        Run { r, out, scene }
    }

    fn focus(&mut self, id: WidgetId) {
        self.ms += 16;
        self.h.at_ms(self.ms);
        self.h.frame(|ui| ui.request_focus(id));
        self.frame();
    }

    fn click(&mut self, i: usize) -> Run {
        let (x, y) = centre(item_rect(i));
        self.h.input.pointer_to(x, y);
        self.h.input.click(PointerButton::Primary);
        let run = self.frame();
        self.h.input.pointer_leave();
        self.frame();
        run
    }

    fn tap(&mut self, i: usize) -> Run {
        let (x, y) = centre(item_rect(i));
        self.h.input.touch_start(1, x, y);
        self.frame();
        self.h.input.touch_end(1, x, y);
        self.frame()
    }

    /// `key` pressed: the frame that handles it, then the one after it (a focus move lands there).
    fn key(&mut self, key: NamedKey) -> (Run, Run) {
        self.h.input.key_down(Key::Named(key), None);
        let first = self.frame();
        (first, self.frame())
    }
}

fn key_of(spec: &Spec) -> TextKey {
    FakeText.prepare(spec, 1.0).0
}

fn drawn(scene: &Scene, spec: &Spec) -> bool {
    let key = key_of(spec);
    scene.layers[0].texts.iter().any(|t| t.key == key)
}

fn label(text: &str, color: [u8; 4]) -> Spec {
    Spec::plain(text, Style::ui(DARK.font_s, color))
}

#[test]
fn the_row_is_the_top_49_and_the_background_fills_the_whole_rect() {
    assert_eq!(HEIGHT, 49.0);
    let mut rig = Rig::new(0);
    let run = rig.frame();
    let list = run.out.tree.get(bar_id()).expect("the list node");
    assert_eq!(list.role, Role::TabList);
    assert_eq!(list.name, "Tabs");
    assert_eq!(list.bounds, BAR);
    for (i, item) in ITEMS.iter().enumerate() {
        let node = run.out.tree.get(item_id(i)).expect("an item");
        assert_eq!(node.role, Role::Tab);
        assert_eq!(node.bounds, item_rect(i), "item {i}");
        assert_eq!(node.name, item.label);
    }
    let under = &run.scene.layers[0].geometry.under;
    let fill = under
        .iter()
        .find(|s| s.color == premul(DARK.surface_overlay))
        .expect("the background");
    assert_eq!(fill.rect, [0.0, 200.0, 400.0, 83.0]);
    let hair = under
        .iter()
        .find(|s| s.color == premul(DARK.surface_separator))
        .expect("the hairline");
    assert_eq!(hair.rect, [0.0, 200.0, 400.0, 1.0]);
}

#[test]
fn the_selected_item_is_marked_and_tinted_apart_from_the_others() {
    let mut rig = Rig::new(2);
    rig.frame();
    let run = rig.frame();
    for i in 0..5 {
        let node = run.out.tree.get(item_id(i)).expect("an item");
        assert_eq!(node.states.contains(State::Selected), i == 2, "item {i}");
    }
    assert!(drawn(&run.scene, &label("Plan", DARK.fg_primary)));
    assert!(drawn(&run.scene, &label("Notes", DARK.fg_tertiary)));
    assert!(!drawn(&run.scene, &label("Notes", DARK.fg_primary)));
    // The selected cell has its fill.
    let under = &run.scene.layers[0].geometry.under;
    assert!(
        under
            .iter()
            .any(|s| s.color == premul(DARK.surface_selected))
    );
}

#[test]
fn a_click_or_a_tap_selects_and_a_second_one_reselects() {
    let mut rig = Rig::new(0);
    let run = rig.click(3);
    assert!(
        run.r.changed && !run.r.reselected,
        "a click on another item"
    );
    assert_eq!(rig.selected, 3);
    let run = rig.click(3);
    assert!(
        !run.r.changed && run.r.reselected,
        "a click on the selected item"
    );
    assert_eq!(rig.selected, 3);

    let run = rig.tap(1);
    assert!(run.r.changed, "a tap on another item");
    assert_eq!(rig.selected, 1);
    let run = rig.tap(1);
    assert!(
        run.r.reselected && !run.r.changed,
        "a tap on the selected item"
    );
}

#[test]
fn a_finger_down_on_an_item_shows_the_press_and_moves_no_selection() {
    let mut rig = Rig::new(0);
    let (x, y) = centre(item_rect(3));
    rig.h.input.touch_start(1, x, y);
    let run = rig.frame();
    assert!(!run.r.changed);
    assert_eq!(rig.selected, 0);
    rig.ms += 1000;
    let run = rig.frame();
    let under = &run.scene.layers[0].geometry.under;
    let cell = under
        .iter()
        .find(|s| s.color == premul(DARK.surface_active) && s.rect[0] > 240.0 && s.rect[0] < 320.0);
    assert!(
        cell.is_some(),
        "the pressed cell is filled over item 3 only"
    );
    assert!(
        !under
            .iter()
            .any(|s| s.color == premul(DARK.surface_active) && s.rect[0] < 240.0),
        "no other item takes the press"
    );
}

#[test]
fn one_tab_stop_roves_with_arrows_home_and_end_and_enter_selects() {
    let mut rig = Rig::new(1);
    rig.frame();
    // Tab enters the bar once, at the selected item.
    let (_, after) = rig.key(NamedKey::Tab);
    assert_eq!(
        after.out.focused,
        Some(item_id(1)),
        "Tab enters at the selected item"
    );
    let (_, after) = rig.key(NamedKey::ArrowRight);
    assert_eq!(after.out.focused, Some(item_id(2)));
    let (_, after) = rig.key(NamedKey::End);
    assert_eq!(after.out.focused, Some(item_id(4)));
    let (_, after) = rig.key(NamedKey::ArrowRight);
    assert_eq!(after.out.focused, Some(item_id(0)), "wraps");
    let (_, after) = rig.key(NamedKey::ArrowLeft);
    assert_eq!(after.out.focused, Some(item_id(4)));
    let (_, after) = rig.key(NamedKey::Home);
    assert_eq!(after.out.focused, Some(item_id(0)));
    assert_eq!(rig.selected, 1, "focus moves, nothing is selected");
    let (first, _) = rig.key(NamedKey::Enter);
    assert!(first.r.changed);
    assert_eq!(rig.selected, 0);
    rig.focus(item_id(4));
    let (first, _) = rig.key(NamedKey::Space);
    assert!(first.r.changed);
    assert_eq!(rig.selected, 4);
}

#[test]
fn a_secondary_click_and_a_long_press_report_context_and_select_nothing() {
    let mut rig = Rig::new(0);
    let (x, y) = centre(item_rect(2));
    rig.h.input.pointer_to(x, y);
    rig.h.input.click(PointerButton::Secondary);
    let run = rig.frame();
    assert_eq!(run.r.context, Some(2));
    assert!(!run.r.changed);
    rig.h.input.pointer_leave();
    rig.frame();

    rig.h.input.touch_start(1, x, y);
    rig.frame();
    rig.ms += 700;
    let run = rig.frame();
    assert_eq!(run.r.context, Some(2), "a long press");
    rig.h.input.touch_end(1, x, y);
    let run = rig.frame();
    assert!(
        !run.r.changed && !run.r.reselected,
        "the release after a long press selects nothing"
    );
    assert_eq!(rig.selected, 0);
}

#[test]
fn a_selection_out_of_range_is_clamped_for_drawing_and_left_alone() {
    let mut rig = Rig::new(9);
    rig.frame();
    let run = rig.frame();
    assert_eq!(rig.selected, 9);
    assert!(
        run.out
            .tree
            .get(item_id(4))
            .expect("item")
            .states
            .contains(State::Selected)
    );
    assert!(!run.r.changed && !run.r.reselected);
}

#[test]
fn the_badge_is_a_pill_on_the_glyphs_top_right_inside_its_cell() {
    let mut rig = Rig::new(0);
    rig.frame();
    let run = rig.frame();
    let pill = run.scene.layers[0]
        .geometry
        .under
        .iter()
        .find(|s| s.color == premul(DARK.count_badge))
        .expect("the badge fill");
    let cell = item_rect(1);
    let [x, y, w, h] = pill.rect;
    assert_eq!((w, h), (16.0, 16.0), "one char: a circle");
    assert_eq!(pill.radii, [8.0; 4]);
    assert!(x >= cell.x && x + w <= cell.right(), "inside the cell");
    assert!(x > cell.x + 40.0, "right of the glyph's centre");
    assert!(y >= cell.y && y + h <= cell.bottom());
    let node = run.out.tree.get(item_id(1)).expect("item");
    assert_eq!(node.description.as_deref(), Some("3"));
}

#[test]
fn a_long_label_is_elided_to_the_item() {
    let mut rig = Rig::new(0);
    let items = [
        TabBarItem::new(icon::NOTE, "A label far too long for one item"),
        TabBarItem::new(icon::GEAR, "Two"),
        TabBarItem::new(icon::GEAR, "Three"),
        TabBarItem::new(icon::GEAR, "Four"),
        TabBarItem::new(icon::GEAR, "Five"),
    ];
    let mut sel = 0;
    rig.ms += 16;
    rig.h.at_ms(rig.ms);
    let (scene, _) = rig.h.frame(|ui| {
        ui.tab_bar("tabs", BAR, &items, &mut sel);
    });
    assert!(!drawn(
        &scene,
        &label("A label far too long for one item", DARK.fg_primary)
    ));
    assert!(!drawn(
        &scene,
        &label("A label far too long for one item", DARK.fg_tertiary)
    ));
}

#[test]
fn a_disabled_scope_is_inert_and_leaves_no_violation() {
    let mut rig = Rig::new(0);
    let draw = |rig: &mut Rig| {
        rig.ms += 16;
        rig.h.at_ms(rig.ms);
        let mut r = TabBarResponse::default();
        let selected = &mut rig.selected;
        let (_, out) = rig.h.frame(|ui| {
            ui.with_enabled(false, |ui| r = ui.tab_bar("tabs", BAR, &ITEMS, selected));
        });
        (r, out)
    };
    draw(&mut rig);
    let (x, y) = centre(item_rect(3));
    rig.h.input.pointer_to(x, y);
    rig.h.input.click(PointerButton::Primary);
    let (r, out) = draw(&mut rig);
    assert_eq!(r, TabBarResponse::default());
    assert_eq!(rig.selected, 0);
    assert_eq!(disabled_violations(&out.tree, bar_id()), []);
}

#[test]
fn no_items_draws_the_background_and_nothing_else() {
    let mut h = UiHarness::new(WIN);
    let mut sel = 0;
    let mut r = TabBarResponse::default();
    let (_, out) = h.frame(|ui| r = ui.tab_bar("tabs", BAR, &[], &mut sel));
    assert_eq!(r, TabBarResponse::default());
    assert!(out.tree.get(item_id(0)).is_none());
}

/// The bar is one tab stop: Tab from its selected item goes to the next widget, not the next item.
#[test]
fn tab_passes_through_the_bar_in_one_stop() {
    use lune_ui_core::widgets::button::{ButtonKind, ButtonOptions};
    let mut h = UiHarness::new(WIN);
    let mut selected = 1;
    let mut ms = 0;
    let mut frame = |h: &mut UiHarness| {
        ms += 16;
        h.at_ms(ms);
        h.frame(|ui| {
            ui.tab_bar("tabs", BAR, &ITEMS, &mut selected);
            let opts = ButtonOptions {
                kind: ButtonKind::Secondary,
                small: false,
                icon: None,
            };
            ui.button_with("after", Rect::new(10.0, 10.0, 80.0, 30.0), "After", &opts);
        })
        .1
    };
    frame(&mut h);
    h.input.key_down(Key::Named(NamedKey::Tab), None);
    frame(&mut h);
    assert_eq!(
        frame(&mut h).focused,
        Some(item_id(1)),
        "the first Tab enters at the selected item"
    );
    h.input.key_down(Key::Named(NamedKey::Tab), None);
    frame(&mut h);
    let out = frame(&mut h);
    assert_eq!(
        out.focused,
        Some(WidgetId::ROOT.with("after")),
        "the second Tab leaves the bar"
    );
}
