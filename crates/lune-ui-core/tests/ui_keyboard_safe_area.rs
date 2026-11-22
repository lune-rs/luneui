//! Keyboard avoidance and overlay bounds (mobile M5 step 5, D365): a scroll area under the
//! on-screen keyboard extends its y range by the covered part and reveals the focused field's
//! caret; menus, tooltips, toasts and the dialog card place inside the safe area and above the
//! keyboard. Scripted time and touches through `UiHarness`; nothing here reads a clock.

use std::time::Duration;

use lune_ui_core::access::{Role, Value};
use lune_ui_core::icon;
use lune_ui_core::input::FrameOutput;
use lune_ui_core::layout::Insets;
use lune_ui_core::paint::Rect;
use lune_ui_core::scene::Scene;
use lune_ui_core::testing::UiHarness;
use lune_ui_core::ui::{Sense, Ui, WidgetId};
use lune_ui_core::widgets::dialog::{Cancel, Dialog, DialogGlass};
use lune_ui_core::widgets::field::FieldOptions;
use lune_ui_core::widgets::menu::MenuRow;
use lune_ui_core::widgets::scroll_area::{Extent, ScrollOptions, ScrollResponse};
use lune_ui_core::widgets::toast::{ToastKind, ToastSpec, Toasts};

const WIN: (f32, f32) = (400.0, 600.0);
const FRAME_MS: u64 = 16;

fn keys(h: f32) -> Insets {
    Insets::new(0.0, 0.0, 0.0, h)
}

fn area_id() -> WidgetId {
    WidgetId::ROOT.with("page")
}

/// The vertical offset and the node's `max` of the area the last frame drew.
fn node_range(out: &FrameOutput, id: WidgetId) -> (f64, f64) {
    match out.tree.get(id).and_then(|n| n.value.clone()) {
        Some(Value::Number { value, max, .. }) => (value, max),
        other => panic!("the area's node has no number: {other:?}"),
    }
}

/// A full-window page with a field low in it.
struct Page {
    text: String,
    rect: Rect,
    /// The area's rect.
    area: Rect,
    ext: f64,
}

impl Page {
    fn new(area: Rect, field: Rect, ext: f64) -> Self {
        Self {
            text: String::new(),
            rect: field,
            area,
            ext,
        }
    }

    /// One frame: the area, the field in its content (in content coordinates, the area's own
    /// translation moves it). Returns the field's window rect and the area's response.
    fn frame(&mut self, h: &mut UiHarness) -> (Rect, FrameOutput, ScrollResponse<Rect>) {
        let mut shown = None;
        let (field, area) = (self.rect, self.area);
        let ext = self.ext;
        let text = &mut self.text;
        let (_, out) = h.frame(|ui| {
            shown = Some(
                ui.scroll_area("page", area, &ScrollOptions::default(), |ui, _| {
                    let r = ui.text_field_with("f", field, text, &FieldOptions::default());
                    (r.response.rect, Extent::new(f64::from(area.w), ext))
                }),
            );
        });
        let s = shown.expect("the frame ran");
        (s.value, out, s)
    }

    /// Frames from `t0` for `n` frames at 16 ms; the last one.
    fn run(
        &mut self,
        h: &mut UiHarness,
        t0: u64,
        n: u64,
    ) -> (Rect, FrameOutput, ScrollResponse<Rect>) {
        let mut last = None;
        for k in 0..n {
            h.at_ms(t0 + k * FRAME_MS);
            last = Some(self.frame(h));
        }
        last.expect("at least one frame")
    }

    /// A tap at `(x, y)` from `t0` (press, lift); returns the time after it.
    fn tap(&mut self, h: &mut UiHarness, t0: u64, at: (f32, f32)) -> u64 {
        h.at_ms(t0).input.touch_start(1, at.0, at.1);
        self.frame(h);
        h.at_ms(t0 + 40).input.touch_end(1, at.0, at.1);
        self.frame(h);
        t0 + 80
    }
}

fn warm(page: &mut Page) -> UiHarness {
    let mut h = UiHarness::new(WIN);
    page.run(&mut h, 0, 3);
    h
}

#[test]
fn a_field_tapped_low_in_a_full_window_area_is_revealed_above_the_keyboard() {
    let full = Rect::new(0.0, 0.0, WIN.0, WIN.1);
    // The content fits the window exactly: the old range is 0.
    let mut page = Page::new(full, Rect::new(20.0, 500.0, 200.0, 32.0), 600.0);
    let mut h = warm(&mut page);
    let t = page.tap(&mut h, 100, (40.0, 516.0));
    let (field, out, _) = page.run(&mut h, t, 2);
    assert_eq!(field.y, 500.0, "a tap with no keyboard moves nothing");
    assert_eq!(node_range(&out, area_id()), (0.0, 0.0));
    // The keyboard comes up a frame or more after the focus.
    h.input.keyboard_inset(keys(300.0));
    let (field, out, resp) = page.run(&mut h, t + 40, 60);
    let (offset, max) = node_range(&out, area_id());
    assert_eq!(max, 300.0, "the covered height extends the range");
    assert!(offset > 0.0, "past the old maximum: {offset}");
    assert!(
        field.bottom() <= WIN.1 - 300.0,
        "the field is above the keyboard: {field:?}"
    );
    assert_eq!(resp.viewport, full, "the reported viewport is the rect");
    assert_eq!(resp.offset.1, offset);
    // The keyboard goes: the clamp brings the offset back.
    h.input.keyboard_inset(Insets::ZERO);
    let (field, out, _) = page.run(&mut h, t + 40 + 60 * FRAME_MS, 40);
    assert_eq!(node_range(&out, area_id()), (0.0, 0.0));
    assert_eq!(field.y, 500.0);
}

#[test]
fn an_area_the_app_laid_out_above_the_keyboard_gets_no_extra_range() {
    let above = Rect::new(0.0, 0.0, WIN.0, 300.0);
    // A field half cut by the area's own bottom edge, the content taller than the view.
    let mut page = Page::new(above, Rect::new(20.0, 285.0, 200.0, 32.0), 900.0);
    let mut h = warm(&mut page);
    let t = page.tap(&mut h, 100, (40.0, 292.0));
    let (field, _, _) = page.run(&mut h, t, 30);
    assert_eq!(field.y, 285.0, "a touch focus alone reveals nothing");
    h.input.keyboard_inset(keys(300.0));
    let (field, out, _) = page.run(&mut h, t + 30 * FRAME_MS, 40);
    let (_, max) = node_range(&out, area_id());
    assert_eq!(
        max, 600.0,
        "the area ends where the keyboard begins: no cut"
    );
    assert!(field.bottom() <= 300.0 && field.y >= 0.0, "{field:?}");
}

#[test]
fn the_keyboard_changing_its_height_reveals_again() {
    let full = Rect::new(0.0, 0.0, WIN.0, WIN.1);
    let mut page = Page::new(full, Rect::new(20.0, 500.0, 200.0, 32.0), 900.0);
    let mut h = warm(&mut page);
    let t = page.tap(&mut h, 100, (40.0, 516.0));
    h.input.keyboard_inset(keys(200.0));
    let (field, _, _) = page.run(&mut h, t, 60);
    assert!(field.bottom() <= WIN.1 - 200.0, "{field:?}");
    // A taller keyboard (a suggestion bar): the caret is moved up again.
    h.input.keyboard_inset(keys(320.0));
    let (field, _, _) = page.run(&mut h, t + 60 * FRAME_MS, 60);
    assert!(field.bottom() <= WIN.1 - 320.0, "{field:?}");
}

#[test]
fn a_nested_field_in_an_inner_list_in_an_outer_page_is_revealed_in_both_areas() {
    let mut text = String::new();
    let outer = Rect::new(0.0, 0.0, WIN.0, WIN.1);
    let inner = Rect::new(0.0, 200.0, WIN.0, 300.0);
    let frame = |h: &mut UiHarness, text: &mut String| {
        let mut found = None;
        let mut offsets = (0.0, 0.0);
        let (_, _) = h.frame(|ui| {
            let o = ui.scroll_area("outer", outer, &ScrollOptions::default(), |ui, _| {
                let i = ui.scroll_area("inner", inner, &ScrollOptions::default(), |ui, _| {
                    let f = Rect::new(20.0, 450.0, 200.0, 32.0);
                    let r = ui.text_field_with("f", f, text, &FieldOptions::default());
                    (r.response.rect, Extent::new(400.0, 800.0))
                });
                ((i.value, i.offset.1), Extent::new(400.0, 1000.0))
            });
            let (rect, inner_off) = o.value;
            found = Some(rect);
            offsets = (o.offset.1, inner_off);
        });
        (found.expect("ran"), offsets)
    };
    let mut h = UiHarness::new(WIN);
    for k in 0..3 {
        h.at_ms(k * FRAME_MS);
        frame(&mut h, &mut text);
    }
    // The field sits at window y 450 to 482 (inner offset 0): a tap focuses it.
    h.at_ms(100).input.touch_start(1, 40.0, 466.0);
    frame(&mut h, &mut text);
    h.at_ms(140).input.touch_end(1, 40.0, 466.0);
    frame(&mut h, &mut text);
    h.input.keyboard_inset(keys(300.0));
    let mut last = (Rect::new(0.0, 0.0, 0.0, 0.0), (0.0, 0.0));
    for k in 0..90 {
        h.at_ms(200 + k * FRAME_MS);
        last = frame(&mut h, &mut text);
    }
    let (field, (outer_off, inner_off)) = last;
    assert!(
        field.y >= 0.0 && field.bottom() <= WIN.1 - 300.0,
        "the field is above the keyboard: {field:?}"
    );
    // The inner area rides on the outer one's offset.
    let inner_at = Rect::new(inner.x, inner.y - outer_off as f32, inner.w, inner.h);
    assert!(
        field.y >= inner_at.y.max(0.0) && field.bottom() <= inner_at.bottom().min(WIN.1 - 300.0),
        "and inside the inner area {inner_at:?} (offsets {outer_off} {inner_off}): {field:?}"
    );
    assert!(
        outer_off > 0.0 && inner_off > 0.0,
        "both areas scrolled: {outer_off} {inner_off}"
    );
}

#[test]
fn a_virtual_lists_viewport_is_the_whole_rect_under_the_keyboard() {
    let full = Rect::new(0.0, 0.0, WIN.0, WIN.1);
    let mut h = UiHarness::new(WIN);
    h.input.keyboard_inset(keys(300.0));
    let mut seen = Vec::new();
    for k in 0..3 {
        h.at_ms(k * FRAME_MS);
        h.frame(|ui| {
            ui.scroll_area_untranslated(
                "page",
                full,
                &ScrollOptions {
                    extent: Some((400.0, 100_000.0)),
                    ..ScrollOptions::default()
                },
                |ui, view| {
                    seen.push(view.viewport);
                    ui.interact(ui.id("body"), full, Sense::SCROLL);
                    ((), Extent::new(400.0, 100_000.0))
                },
            );
        });
    }
    assert!(seen.iter().all(|v| *v == full), "{seen:?}");
}

const ROWS: [MenuRow<'static>; 2] = [
    MenuRow::Item {
        glyph: Some(icon::ADD),
        text: "New Window",
        shortcut: "",
    },
    MenuRow::Item {
        glyph: Some(icon::COPY),
        text: "Copy",
        shortcut: "",
    },
];

fn glass() -> DialogGlass {
    DialogGlass {
        fill: [0.2, 0.2, 0.2, 0.62],
        noise: true,
    }
}

/// A frame with a menu at y 10, a dialog, a toast and a tooltip on a button.
fn overlays(h: &mut UiHarness, toasts: &mut Toasts) -> (Scene, FrameOutput) {
    h.frame(|ui: &mut Ui<'_>| {
        let b = ui.id("anchor");
        let r = ui.interact(b, Rect::new(150.0, 100.0, 60.0, 30.0), Sense::CLICK);
        ui.tooltip(&r, "Anchor tip");
        ui.menu("m", &ROWS, (100.0, 10.0));
        ui.dialog(
            "d",
            &Dialog {
                title: "Discard",
                message: "Closing discards the changes.",
                extra_h: 0.0,
                confirm: "Discard",
                cancel: Cancel::Default,
            },
            &glass(),
            |_, _, _, _| {},
        );
        ui.toasts("t", toasts);
    })
}

fn bounds_of(out: &FrameOutput, role: Role) -> Rect {
    out.tree
        .nodes
        .iter()
        .find(|n| n.role == role)
        .unwrap_or_else(|| panic!("no {role:?} node"))
        .bounds
}

fn toasts() -> Toasts {
    let mut t = Toasts::default();
    t.push(
        ToastSpec::new(ToastKind::Info, "Saved"),
        Duration::from_millis(0),
    );
    t
}

fn settle(h: &mut UiHarness, toasts: &mut Toasts) -> (Scene, FrameOutput) {
    let mut last = None;
    for k in 0..30 {
        h.at_ms(k * FRAME_MS);
        last = Some(overlays(h, toasts));
    }
    last.expect("frames ran")
}

#[test]
fn zero_insets_draw_every_overlay_exactly_as_no_insets_event_does() {
    let mut plain = UiHarness::new((600.0, 700.0));
    let mut ts = toasts();
    let (a, _) = settle(&mut plain, &mut ts);

    let mut explicit = UiHarness::new((600.0, 700.0));
    explicit.input.safe_area(Insets::ZERO);
    explicit.input.keyboard_inset(Insets::ZERO);
    let mut ts = toasts();
    let (b, _) = settle(&mut explicit, &mut ts);
    assert_eq!(a, b);

    let mut cut = UiHarness::new((600.0, 700.0));
    cut.input.safe_area(Insets::new(0.0, 62.0, 0.0, 34.0));
    let mut ts = toasts();
    let (c, _) = settle(&mut cut, &mut ts);
    assert_ne!(a, c, "a safe area moves the overlays");
}

#[test]
fn a_menu_opened_high_places_below_the_safe_area_and_its_margin() {
    let mut h = UiHarness::new((600.0, 700.0));
    h.input.safe_area(Insets::new(0.0, 62.0, 0.0, 34.0));
    let mut ts = toasts();
    let (_, out) = settle(&mut h, &mut ts);
    let menu = bounds_of(&out, Role::Menu);
    assert!(menu.y >= 62.0 + 8.0, "{menu:?}");
}

#[test]
fn the_dialog_card_centres_in_the_bounds() {
    let size = (600.0, 700.0);
    let mut plain = UiHarness::new(size);
    let mut ts = toasts();
    let (_, out) = settle(&mut plain, &mut ts);
    let card = bounds_of(&out, Role::Dialog);

    let (safe, kb) = (Insets::new(0.0, 62.0, 0.0, 34.0), 300.0);
    let mut h = UiHarness::new(size);
    h.input.safe_area(safe);
    h.input.keyboard_inset(keys(kb));
    let mut ts = toasts();
    let (_, out) = settle(&mut h, &mut ts);
    let moved = bounds_of(&out, Role::Dialog);
    assert_eq!(moved.w, card.w);
    // The bounds run from 62 to 700 - 300: a card that fits is centred in them.
    let (top, bottom) = (62.0, size.1 - kb);
    let centre = moved.y + moved.h / 2.0;
    assert!(
        (centre - (top + bottom) / 2.0).abs() <= 0.5 || moved.y == top + 20.0,
        "{moved:?}"
    );
    assert!(
        moved.y >= top && moved.bottom() <= bottom + 0.5,
        "{moved:?}"
    );
}

#[test]
fn a_toast_sits_above_a_300_keyboard_and_a_tooltip_stays_in_the_bounds() {
    let size = (600.0, 700.0);
    let mut h = UiHarness::new(size);
    h.input.keyboard_inset(keys(300.0));
    let mut ts = toasts();
    let (_, out) = settle(&mut h, &mut ts);
    let toast = out
        .tree
        .nodes
        .iter()
        .find(|n| matches!(n.role, Role::Status | Role::Alert))
        .expect("a toast node")
        .bounds;
    assert!(toast.bottom() <= size.1 - 300.0, "{toast:?}");
    let tip = out.tree.nodes.iter().find(|n| n.role == Role::Tooltip);
    if let Some(tip) = tip {
        assert!(tip.bounds.bottom() <= size.1 - 300.0, "{:?}", tip.bounds);
    }
}
