//! Touch text editing (mobile M5 step 4, D364): a tap places the caret, a double tap and a long
//! press select a word, a drag after the long press extends, a plain body drag does nothing;
//! selection handles after a touch selection; `Ui::edit_command` and `Ui::edit_actions` for an
//! app's own edit menu. The field, the text area and the code editor share one path and each is
//! run through the same rules; selectable text has its two cases. Scripted time and touches
//! through `UiHarness`; nothing here reads a clock.

use lune_ui_core::code_doc::CodeDoc;
use lune_ui_core::input::{
    ClipboardRequest, FrameOutput, ImeEvent, Key, Modifiers, NamedKey, PointerButton,
};
use lune_ui_core::paint::Rect;
use lune_ui_core::testing::{UiHarness, hit_regions};
use lune_ui_core::text::{Spec, Style};
use lune_ui_core::text_edit::{EditActions, EditCommand};
use lune_ui_core::theme::DARK;
use lune_ui_core::ui::{Response, Ui, WidgetId};
use lune_ui_core::widgets::code_editor::CodeOptions;
use lune_ui_core::widgets::field::{FieldOptions, Variant};
use lune_ui_core::widgets::scroll_area::{Extent, ScrollOptions};
use lune_ui_core::widgets::text_area::AreaOptions;

const WIN: (f32, f32) = (500.0, 400.0);
const P: PointerButton = PointerButton::Primary;
/// One ASCII glyph of the base style under the fake text system.
const GLYPH: f32 = 6.5;
const TEXT: &str = "hello world again";

/// The three widgets on the shared edit path.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Field,
    Area,
    Code,
}

const KINDS: [Kind; 3] = [Kind::Field, Kind::Area, Kind::Code];

/// One of the three widgets with its text.
struct Edit {
    kind: Kind,
    text: String,
    doc: CodeDoc,
}

impl Edit {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            text: TEXT.to_owned(),
            doc: CodeDoc::new(TEXT.to_owned()),
        }
    }

    fn id(&self) -> WidgetId {
        WidgetId::ROOT.with(match self.kind {
            Kind::Field => "f",
            Kind::Area => "a",
            Kind::Code => "c",
        })
    }

    /// The window point of the char boundary `i` on the first line, nudged into the glyph after it.
    fn at(&self, i: usize) -> (f32, f32) {
        let (x, y) = match self.kind {
            // TEXT_INSET 10 in a box at x 20; the box's middle.
            Kind::Field => (30.0, 36.0),
            // The frame and the padding.
            Kind::Area => (30.0, 34.0),
            // No gutter: the text's left edge; the first line is 20 to 40.
            Kind::Code => (22.0, 30.0),
        };
        (x + GLYPH * i as f32 + 1.0, y)
    }

    /// Draws the widget in `ui`, as it is (no box around it).
    fn draw(&mut self, ui: &mut Ui<'_>) -> Response {
        match self.kind {
            Kind::Field => {
                let rect = Rect::new(20.0, 20.0, 200.0, 32.0);
                ui.text_field_with("f", rect, &mut self.text, &FieldOptions::default())
                    .response
            }
            Kind::Area => {
                let rect = Rect::new(20.0, 20.0, 200.0, 100.0);
                ui.text_area("a", rect, &mut self.text, &AreaOptions::default())
                    .response
            }
            Kind::Code => {
                let opts = CodeOptions {
                    gutter: false,
                    label: "Code",
                    ..CodeOptions::default()
                };
                let rect = Rect::new(20.0, 20.0, 400.0, 200.0);
                ui.code_editor("c", rect, &mut self.doc, &opts).response
            }
        }
    }

    fn frame(&mut self, h: &mut UiHarness) -> (Response, FrameOutput) {
        let mut r = None;
        let (_, out) = h.frame(|ui| r = Some(self.draw(ui)));
        (r.expect("the frame ran"), out)
    }
}

/// The selection the node reports, as (anchor, caret).
fn selection(out: &FrameOutput, id: WidgetId) -> (usize, usize) {
    out.tree
        .get(id)
        .and_then(|n| n.text.as_ref())
        .expect("a text node")
        .selection
}

/// The selection as (start, end).
fn range(out: &FrameOutput, id: WidgetId) -> (usize, usize) {
    let (a, b) = selection(out, id);
    (a.min(b), a.max(b))
}

/// The selection handles the last frame registered for `id`, start first.
fn handles(h: &UiHarness, id: WidgetId) -> Vec<Rect> {
    let ids = [
        id.with(("selection-handle", 0_usize)),
        id.with(("selection-handle", 1_usize)),
    ];
    let regions = hit_regions(&h.store);
    ids.iter()
        .filter_map(|hid| regions.iter().find(|r| r.id == *hid).map(|r| r.rect))
        .collect()
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// A harness that has drawn `e` twice.
fn ready(e: &mut Edit) -> UiHarness {
    let mut h = UiHarness::new(WIN);
    e.frame(&mut h);
    e.frame(&mut h);
    h
}

/// A long press at `at` from app time `t0` (press, then the frame 600 ms later): that frame.
fn long_press(h: &mut UiHarness, e: &mut Edit, at: (f32, f32), t0: u64) -> (Response, FrameOutput) {
    h.at_ms(t0).input.touch_start(1, at.0, at.1);
    e.frame(h);
    h.at_ms(t0 + 600);
    e.frame(h)
}

#[test]
fn a_touch_press_changes_nothing_and_its_tap_places_the_caret_there() {
    for kind in KINDS {
        let mut e = Edit::new(kind);
        let mut h = ready(&mut e);
        let (_, before) = e.frame(&mut h);
        let start = selection(&before, e.id());
        let at = e.at(3);
        h.at_ms(0).input.touch_start(1, at.0, at.1);
        let (r, out) = e.frame(&mut h);
        assert!(
            r.press_started && !r.focused,
            "{kind:?}: no focus on the press"
        );
        assert_eq!(
            selection(&out, e.id()),
            start,
            "{kind:?}: the press moves nothing"
        );
        h.at_ms(50).input.touch_end(1, at.0, at.1);
        let (r, out) = e.frame(&mut h);
        assert!(r.clicked && r.focused, "{kind:?}: the tap focuses");
        assert_eq!(
            selection(&out, e.id()),
            (3, 3),
            "{kind:?}: the caret at the tap"
        );
        assert!(
            handles(&h, e.id()).is_empty(),
            "{kind:?}: no handles after a tap"
        );
    }
}

#[test]
fn a_double_tap_selects_the_word() {
    for kind in KINDS {
        let mut e = Edit::new(kind);
        let mut h = ready(&mut e);
        let at = e.at(8);
        h.at_ms(0).input.touch_start(1, at.0, at.1);
        e.frame(&mut h);
        h.at_ms(40).input.touch_end(1, at.0, at.1);
        e.frame(&mut h);
        h.at_ms(120).input.touch_start(1, at.0, at.1);
        let (_, out) = e.frame(&mut h);
        assert_eq!(
            selection(&out, e.id()),
            (8, 8),
            "{kind:?}: the second press moves nothing"
        );
        h.at_ms(160).input.touch_end(1, at.0, at.1);
        let (r, out) = e.frame(&mut h);
        assert!(r.double_clicked, "{kind:?}");
        assert_eq!(range(&out, e.id()), (6, 11), "{kind:?}: `world`");
        assert_eq!(
            handles(&h, e.id()).len(),
            2,
            "{kind:?}: a touch selection has handles"
        );
    }
}

#[test]
fn a_long_press_selects_the_word_reports_context_focuses_and_its_drag_extends() {
    for kind in KINDS {
        let mut e = Edit::new(kind);
        let mut h = ready(&mut e);
        let at = e.at(1);
        let (r, out) = long_press(&mut h, &mut e, at, 0);
        assert!(r.secondary_clicked && r.focused, "{kind:?}");
        assert_eq!(range(&out, e.id()), (0, 5), "{kind:?}: `hello`");
        assert_eq!(
            handles(&h, e.id()).len(),
            2,
            "{kind:?}: handles after a long press"
        );
        // The drag that follows extends the selection from the word.
        let to = e.at(14);
        h.at_ms(700).input.touch_move(1, (at.0 + to.0) / 2.0, at.1);
        e.frame(&mut h);
        h.at_ms(720).input.touch_move(1, to.0, to.1);
        let (r, out) = e.frame(&mut h);
        assert!(r.dragged, "{kind:?}");
        assert_eq!(
            range(&out, e.id()),
            (0, 14),
            "{kind:?}: extended to the finger"
        );
        h.at_ms(740).input.touch_end(1, to.0, to.1);
        let (r, out) = e.frame(&mut h);
        assert!(!r.clicked, "{kind:?}: the spent press clicks nothing");
        assert_eq!(
            range(&out, e.id()),
            (0, 14),
            "{kind:?}: the lift moves nothing"
        );
    }
}

#[test]
fn a_long_press_inside_the_selection_keeps_it() {
    for kind in KINDS {
        let mut e = Edit::new(kind);
        let mut h = ready(&mut e);
        // Select `hello world` by a long press and a drag.
        let at = e.at(1);
        long_press(&mut h, &mut e, at, 0);
        let to = e.at(11);
        h.at_ms(700).input.touch_move(1, to.0, to.1);
        e.frame(&mut h);
        h.at_ms(720).input.touch_end(1, to.0, to.1);
        let (_, out) = e.frame(&mut h);
        assert_eq!(range(&out, e.id()), (0, 11), "{kind:?}");
        // A long press inside it, clear of both handles' hits.
        let inside = e.at(5);
        let (r, out) = long_press(&mut h, &mut e, inside, 2000);
        assert!(r.secondary_clicked, "{kind:?}");
        assert_eq!(range(&out, e.id()), (0, 11), "{kind:?}: kept");
        // Outside it and clear of the end handle, on `again`: that word.
        h.at_ms(2700).input.touch_end(1, 0.0, 0.0);
        e.frame(&mut h);
        let outside = e.at(16);
        let (_, out) = long_press(&mut h, &mut e, outside, 4000);
        assert_eq!(range(&out, e.id()), (12, 17), "{kind:?}: `again`");
    }
}

#[test]
fn a_body_drag_without_a_long_press_moves_nothing() {
    for kind in KINDS {
        let mut e = Edit::new(kind);
        let mut h = ready(&mut e);
        // A tap first, so the caret is known.
        let at = e.at(2);
        h.at_ms(0).input.touch_start(1, at.0, at.1);
        e.frame(&mut h);
        h.at_ms(40).input.touch_end(1, at.0, at.1);
        e.frame(&mut h);
        let to = e.at(12);
        h.at_ms(1000).input.touch_start(1, at.0, at.1);
        e.frame(&mut h);
        for (k, x) in [(1, (at.0 + to.0) / 2.0), (2, to.0)] {
            h.at_ms(1000 + 20 * k).input.touch_move(1, x, at.1);
            let (_, out) = e.frame(&mut h);
            assert_eq!(
                selection(&out, e.id()),
                (2, 2),
                "{kind:?}: the drag selects nothing"
            );
        }
        h.at_ms(1100).input.touch_end(1, to.0, to.1);
        let (r, out) = e.frame(&mut h);
        assert!(!r.clicked, "{kind:?}");
        assert_eq!(selection(&out, e.id()), (2, 2), "{kind:?}");
        assert!(handles(&h, e.id()).is_empty(), "{kind:?}");
    }
}

#[test]
fn while_composing_a_touch_moves_nothing() {
    for kind in KINDS {
        let mut e = Edit::new(kind);
        let mut h = ready(&mut e);
        let at = e.at(2);
        h.at_ms(0).input.touch_start(1, at.0, at.1);
        e.frame(&mut h);
        h.at_ms(40).input.touch_end(1, at.0, at.1);
        e.frame(&mut h);
        h.input.ime(ImeEvent::Enabled);
        h.input.ime(ImeEvent::Preedit {
            text: "x".to_owned(),
            cursor: Some((1, 1)),
        });
        let (_, before) = e.frame(&mut h);
        let shown = selection(&before, e.id());
        let other = e.at(10);
        h.at_ms(1000).input.touch_start(1, other.0, other.1);
        e.frame(&mut h);
        h.at_ms(1040).input.touch_end(1, other.0, other.1);
        let (_, out) = e.frame(&mut h);
        assert_eq!(
            selection(&out, e.id()),
            shown,
            "{kind:?}: a tap moves nothing"
        );
        let (_, out) = long_press(&mut h, &mut e, other, 2000);
        assert_eq!(
            selection(&out, e.id()),
            shown,
            "{kind:?}: a long press moves nothing"
        );
    }
}

#[test]
fn a_long_press_on_a_password_selects_all_of_it() {
    let mut text = "secret".to_owned();
    let opts = FieldOptions {
        variant: Variant::Password,
        ..FieldOptions::default()
    };
    let rect = Rect::new(20.0, 20.0, 200.0, 32.0);
    let id = WidgetId::ROOT.with("f");
    let mut h = UiHarness::new(WIN);
    let mut run = |h: &mut UiHarness| {
        let mut r = None;
        let (_, out) = h.frame(|ui| {
            r = Some(ui.text_field_with("f", rect, &mut text, &opts).response);
            let actions = ui.edit_actions(id);
            assert!(
                actions.is_none_or(|a| !a.copy && !a.cut),
                "a mask copies nothing"
            );
        });
        (r.expect("ran"), out)
    };
    run(&mut h);
    h.at_ms(0).input.touch_start(1, 40.0, 36.0);
    run(&mut h);
    h.at_ms(600);
    let (r, out) = run(&mut h);
    assert!(r.secondary_clicked && r.focused);
    let _ = out;
    // All of it is selected: a typed char replaces it.
    h.at_ms(700).input.touch_end(1, 40.0, 36.0);
    run(&mut h);
    h.input.key_down(Key::Character("x".to_owned()), Some("x"));
    run(&mut h);
    assert_eq!(text, "x");
}

// A pan inside a scroll area.

const AREA: Rect = Rect::new(0.0, 0.0, 500.0, 400.0);

/// One frame of a vertical scroll area over `AREA` holding a field at y 90: the field's response,
/// the frame's output and the area's y offset.
fn form(h: &mut UiHarness, text: &mut String) -> (Response, FrameOutput, f64) {
    let mut res = None;
    let (_, out) = h.frame(|ui| {
        let a = ui.scroll_area("area", AREA, &ScrollOptions::default(), |ui, _| {
            let rect = Rect::new(20.0, 90.0, 200.0, 32.0);
            let r = ui.text_field_with("f", rect, text, &FieldOptions::default());
            (r.response, Extent::new(500.0, 1500.0))
        });
        res = Some((a.value, a.offset.1));
    });
    let (r, offset) = res.expect("ran");
    (r, out, offset)
}

fn form_id() -> WidgetId {
    WidgetId::ROOT.with("area").with("f")
}

/// Finds the field's id inside the area: the scope the area opens is the area's own.
fn field_in(out: &FrameOutput) -> WidgetId {
    let ids = [form_id(), WidgetId::ROOT.with("f")];
    *ids.iter()
        .find(|id| out.tree.get(**id).is_some())
        .expect("the field's node")
}

#[test]
fn a_pan_from_a_field_in_a_scroll_area_moves_no_caret_no_focus_and_no_selection() {
    let mut text = TEXT.to_owned();
    let mut h = UiHarness::new(WIN);
    form(&mut h, &mut text);
    let (_, out, _) = form(&mut h, &mut text);
    let id = field_in(&out);
    let before = selection(&out, id);
    h.at_ms(0).input.touch_start(1, 50.0, 106.0);
    let (r, out, _) = form(&mut h, &mut text);
    assert!(!r.focused);
    assert_eq!(selection(&out, id), before);
    let mut last = 0.0;
    for k in 1..=3 {
        h.at_ms(20 * k)
            .input
            .touch_move(1, 50.0, 106.0 - 30.0 * k as f32);
        let (r, out, offset) = form(&mut h, &mut text);
        assert!(!r.focused, "no focus during the pan");
        assert_eq!(selection(&out, id), before, "no caret moves during the pan");
        last = offset;
    }
    assert!(last > 0.0, "the pan scrolled the area");
    h.at_ms(80).input.touch_end(1, 50.0, 16.0);
    let (r, out, _) = form(&mut h, &mut text);
    assert!(!r.focused && !r.clicked);
    assert_eq!(selection(&out, id), before);
    assert_eq!(out.focused, None);
    assert!(handles(&h, id).is_empty());
}

#[test]
fn a_vertical_handle_drag_in_a_vertical_scroll_area_does_not_scroll_it() {
    let mut text = TEXT.to_owned();
    let mut h = UiHarness::new(WIN);
    form(&mut h, &mut text);
    let (_, out, _) = form(&mut h, &mut text);
    let id = field_in(&out);
    // A long press on `hello`, then the lift.
    h.at_ms(0).input.touch_start(1, 31.0 + GLYPH, 106.0);
    form(&mut h, &mut text);
    h.at_ms(600);
    let (r, out, _) = form(&mut h, &mut text);
    assert!(r.secondary_clicked);
    assert_eq!(range(&out, id), (0, 5));
    h.at_ms(700).input.touch_end(1, 31.0 + GLYPH, 106.0);
    form(&mut h, &mut text);
    let hs = handles(&h, id);
    assert_eq!(hs.len(), 2);
    let from = centre(hs[1]);
    h.at_ms(1000).input.touch_start(2, from.0, from.1);
    form(&mut h, &mut text);
    for k in 1..=3 {
        h.at_ms(1000 + 20 * k)
            .input
            .touch_move(2, from.0, from.1 + 30.0 * k as f32);
        let (r, _, offset) = form(&mut h, &mut text);
        assert_eq!(offset, 0.0, "the handle holds the drag");
        assert!(r.focused, "the field keeps the focus");
    }
    h.at_ms(1100).input.touch_end(2, from.0, from.1 + 90.0);
    let (_, _, offset) = form(&mut h, &mut text);
    assert_eq!(offset, 0.0);
}

// Handles.

/// A field with `hello` selected by a long press, lifted.
fn long_pressed_field() -> (Edit, UiHarness) {
    let mut e = Edit::new(Kind::Field);
    let mut h = ready(&mut e);
    let at = e.at(1);
    long_press(&mut h, &mut e, at, 0);
    h.at_ms(700).input.touch_end(1, at.0, at.1);
    e.frame(&mut h);
    (e, h)
}

#[test]
fn handles_sit_at_the_selection_ends_and_draw_in_the_caret_colour() {
    let (mut e, mut h) = long_pressed_field();
    let (scene, _) = h.frame(|ui| {
        e.draw(ui);
    });
    let hs = handles(&h, e.id());
    assert_eq!(hs.len(), 2);
    for r in &hs {
        assert_eq!((r.w, r.h), (44.0, 44.0), "a 44 x 44 hit");
    }
    // The start's knob above the line, the end's below it; at the two ends' x.
    let (s, t) = (centre(hs[0]), centre(hs[1]));
    assert!(s.1 < t.1);
    assert!((t.0 - s.0 - 5.0 * GLYPH).abs() < 0.01, "{s:?} {t:?}");

    // A field's caret colour, at scale 1: two knobs and two bars of it, besides the caret.
    let ink = lune_ui_core::color::premul(DARK.fg_primary);
    let over = &scene.layers[0].geometry.over;
    let knobs: Vec<[f32; 4]> = over
        .iter()
        .filter(|s| s.color == ink && s.rect[2] == 10.0 && s.rect[3] == 10.0)
        .map(|s| s.rect)
        .collect();
    assert_eq!(knobs.len(), 2, "two knobs");
    for (k, r) in knobs.iter().zip(&hs) {
        assert_eq!(
            (k[0] + 5.0, k[1] + 5.0),
            centre(*r),
            "each hit is centred on its knob"
        );
    }
    let bars = over
        .iter()
        .filter(|s| s.color == ink && s.rect[2] == 2.0)
        .count();
    assert_eq!(bars, 2, "two bars");
}

#[test]
fn a_mouse_press_hides_the_handles_and_a_key_edit_does_too() {
    let (mut e, mut h) = long_pressed_field();
    assert_eq!(handles(&h, e.id()).len(), 2);
    let at = e.at(14);
    h.at_ms(3000).input.pointer_to(at.0, at.1).press(P);
    let (r, out) = e.frame(&mut h);
    assert!(r.press_started);
    assert_eq!(selection(&out, e.id()), (14, 14));
    h.input.release(P);
    e.frame(&mut h);
    assert!(handles(&h, e.id()).is_empty(), "a mouse press");

    // Shift+Right grows the selection by key: still a selection, no handles.
    let (mut e, mut h) = long_pressed_field();
    h.input.set_modifiers(Modifiers {
        shift: true,
        ..Modifiers::default()
    });
    h.input.key_down(Key::Named(NamedKey::ArrowRight), None);
    h.input.set_modifiers(Modifiers::default());
    let (_, out) = e.frame(&mut h);
    assert_eq!(range(&out, e.id()), (0, 6));
    assert!(handles(&h, e.id()).is_empty(), "a key edit");
}

#[test]
fn an_end_handle_drag_extends_and_a_start_handle_drag_past_the_end_swaps() {
    // The end handle, two words on.
    let (mut e, mut h) = long_pressed_field();
    let from = centre(handles(&h, e.id())[1]);
    h.at_ms(1000).input.touch_start(2, from.0, from.1);
    e.frame(&mut h);
    let dx = GLYPH * 6.0;
    for k in 1..=3 {
        h.at_ms(1000 + 20 * k)
            .input
            .touch_move(2, from.0 + dx * k as f32 / 3.0, from.1);
        e.frame(&mut h);
    }
    h.at_ms(1100).input.touch_end(2, from.0 + dx, from.1);
    let (_, out) = e.frame(&mut h);
    assert_eq!(
        selection(&out, e.id()),
        (0, 11),
        "the start stays, the end follows"
    );
    assert_eq!(handles(&h, e.id()).len(), 2);

    // The start handle, dragged past the end: the old end is the anchor.
    let (mut e, mut h) = long_pressed_field();
    let from = centre(handles(&h, e.id())[0]);
    h.at_ms(1000).input.touch_start(2, from.0, from.1);
    e.frame(&mut h);
    let dx = GLYPH * 8.0;
    for k in 1..=3 {
        h.at_ms(1000 + 20 * k)
            .input
            .touch_move(2, from.0 + dx * k as f32 / 3.0, from.1);
        e.frame(&mut h);
    }
    h.at_ms(1100).input.touch_end(2, from.0 + dx, from.1);
    let (_, out) = e.frame(&mut h);
    assert_eq!(selection(&out, e.id()), (5, 8), "across the other end");
}

#[test]
fn mouse_input_never_registers_a_handle() {
    let mut e = Edit::new(Kind::Field);
    let mut h = ready(&mut e);
    let at = e.at(8);
    // A double click selects the word under a mouse: no handles.
    h.input.pointer_to(at.0, at.1).click(P);
    e.frame(&mut h);
    h.input.click(P);
    let (_, out) = e.frame(&mut h);
    assert_eq!(range(&out, e.id()), (6, 11));
    assert!(handles(&h, e.id()).is_empty());
}

// Edit commands and actions.

#[test]
fn edit_command_copy_cut_and_paste_take_the_clipboard_path_of_the_shortcuts() {
    let (mut e, mut h) = long_pressed_field();
    let id = e.id();
    let run = |h: &mut UiHarness, e: &mut Edit, cmd: Option<EditCommand>| {
        let mut r = None;
        let (_, out) = h.frame(|ui| {
            if let Some(cmd) = cmd.clone() {
                ui.edit_command(id, cmd);
            }
            r = Some(e.draw(ui));
        });
        (r.expect("ran"), out)
    };
    let (_, out) = run(&mut h, &mut e, Some(EditCommand::Copy));
    assert_eq!(out.clipboard, [ClipboardRequest::Write("hello".to_owned())]);
    assert_eq!(e.text, TEXT);
    let (_, out) = run(&mut h, &mut e, Some(EditCommand::Paste));
    assert_eq!(out.clipboard, [ClipboardRequest::Read]);
    h.input.clipboard_text("bye");
    let (r, out) = run(&mut h, &mut e, None);
    assert!(r.changed);
    assert_eq!(e.text, "bye world again");
    assert!(out.clipboard.is_empty());
    run(&mut h, &mut e, Some(EditCommand::SelectAll));
    let (r, out) = run(&mut h, &mut e, Some(EditCommand::Cut));
    assert!(r.changed);
    assert_eq!(
        out.clipboard,
        [ClipboardRequest::Write("bye world again".to_owned())]
    );
    assert_eq!(e.text, "");
}

#[test]
fn edit_command_cut_on_a_read_only_field_changes_nothing() {
    let mut text = TEXT.to_owned();
    let opts = FieldOptions {
        read_only: true,
        ..FieldOptions::default()
    };
    let id = WidgetId::ROOT.with("f");
    let rect = Rect::new(20.0, 20.0, 200.0, 32.0);
    let mut h = UiHarness::new(WIN);
    let mut run = |h: &mut UiHarness, cmd: Option<EditCommand>| {
        let mut actions = None;
        let (_, out) = h.frame(|ui| {
            if let Some(cmd) = cmd {
                ui.edit_command(id, cmd);
            }
            ui.text_field_with("f", rect, &mut text, &opts);
            actions = ui.edit_actions(id);
        });
        (out, actions.expect("a text widget"))
    };
    run(&mut h, None);
    run(&mut h, Some(EditCommand::SelectAll));
    let (out, actions) = run(&mut h, Some(EditCommand::Cut));
    assert!(out.clipboard.is_empty());
    assert_eq!(range(&out, id), (0, TEXT.len()));
    assert!(actions.copy && actions.select_all && !actions.cut && !actions.paste);
    let (out, _) = run(&mut h, Some(EditCommand::Paste));
    assert!(out.clipboard.is_empty(), "no read for a read-only field");
    assert_eq!(text, TEXT);
}

#[test]
fn a_command_queued_after_the_widget_ran_applies_next_frame_and_asks_for_it() {
    let mut e = Edit::new(Kind::Field);
    let mut h = ready(&mut e);
    let id = e.id();
    let (_, out) = h.frame(|ui| {
        e.draw(ui);
        ui.edit_command(id, EditCommand::SelectAll);
    });
    assert_eq!(range(&out, id), (TEXT.len(), TEXT.len()), "not yet");
    assert_ne!(
        out.repaint,
        lune_ui_core::input::Repaint::Idle,
        "a frame is asked for"
    );
    let (_, out) = e.frame(&mut h);
    assert_eq!(range(&out, id), (0, TEXT.len()));
}

#[test]
fn a_command_for_a_widget_not_drawn_in_its_frame_is_dropped() {
    let mut e = Edit::new(Kind::Field);
    let mut h = ready(&mut e);
    let id = e.id();
    // Queued in a frame that does not draw the field: due then, gone after it.
    h.frame(|ui| ui.edit_command(id, EditCommand::SelectAll));
    let (_, out) = e.frame(&mut h);
    assert_eq!(range(&out, id), (TEXT.len(), TEXT.len()));
    let (_, out) = e.frame(&mut h);
    assert_eq!(range(&out, id), (TEXT.len(), TEXT.len()));
    // A late one, then a frame without the field: dropped too.
    h.frame(|ui| {
        e.draw(ui);
        ui.edit_command(id, EditCommand::SelectAll);
    });
    h.frame(|_| {});
    let (_, out) = e.frame(&mut h);
    assert_eq!(range(&out, id), (TEXT.len(), TEXT.len()));
}

#[test]
fn a_command_in_a_disabled_scope_is_dropped() {
    let mut e = Edit::new(Kind::Field);
    let mut h = ready(&mut e);
    let id = e.id();
    let (_, out) = h.frame(|ui| {
        ui.edit_command(id, EditCommand::SelectAll);
        ui.with_enabled(false, |ui| {
            e.draw(ui);
        });
    });
    assert_eq!(range(&out, id), (TEXT.len(), TEXT.len()));
    let (_, out) = e.frame(&mut h);
    assert_eq!(
        range(&out, id),
        (TEXT.len(), TEXT.len()),
        "not applied later"
    );
    // The same command, enabled, applies.
    let (_, out) = h.frame(|ui| {
        ui.edit_command(id, EditCommand::SelectAll);
        e.draw(ui);
    });
    assert_eq!(range(&out, id), (0, TEXT.len()));
}

#[test]
fn edit_actions_follow_the_selection_and_are_none_for_an_undrawn_id() {
    for kind in KINDS {
        let mut e = Edit::new(kind);
        let mut h = UiHarness::new(WIN);
        let id = e.id();
        let read = |h: &mut UiHarness, e: &mut Edit, cmd: Option<EditCommand>| {
            let mut got = (None, None);
            h.frame(|ui| {
                got.0 = ui.edit_actions(id);
                if let Some(cmd) = cmd {
                    ui.edit_command(id, cmd);
                }
                e.draw(ui);
                got.1 = ui.edit_actions(id);
            });
            got
        };
        let (before, after) = read(&mut h, &mut e, None);
        assert_eq!(before, None, "{kind:?}: not drawn yet");
        let a = after.expect("drawn");
        assert!(
            !a.cut && !a.copy && a.paste && a.select_all,
            "{kind:?}: {a:?}"
        );
        let (before, after) = read(&mut h, &mut e, Some(EditCommand::SelectAll));
        assert_eq!(before, Some(a), "{kind:?}: last frame's");
        let b = after.expect("drawn");
        assert!(
            b.cut && b.copy && b.paste && b.select_all,
            "{kind:?}: {b:?}"
        );
        // Two frames without it: gone.
        h.frame(|_| {});
        let mut gone = Some(a);
        h.frame(|ui| gone = ui.edit_actions(id));
        assert_eq!(gone, None, "{kind:?}");
        assert_eq!(read(&mut h, &mut e, None).0, None);
    }
    let mut h = UiHarness::new(WIN);
    let mut none = Some(EditActions::default());
    h.frame(|ui| none = ui.edit_actions(WidgetId::ROOT.with("nothing")));
    assert_eq!(none, None);
}

// Selectable text.

const VALUE: Rect = Rect::new(20.0, 30.0, 300.0, 20.0);

/// One frame of selectable text drawing `shown` and copying `full`: its response and output.
fn selectable(
    h: &mut UiHarness,
    shown: &str,
    full: &str,
    cmd: Option<EditCommand>,
) -> (Response, FrameOutput, Option<EditActions>) {
    let spec = Spec::plain(shown, Style::ui(13.0, DARK.fg_primary));
    let id = WidgetId::ROOT.with("s");
    let mut got = (None, None);
    let (_, out) = h.frame(|ui| {
        if let Some(cmd) = cmd {
            ui.edit_command(id, cmd);
        }
        got.0 = Some(ui.selectable_text("s", VALUE, &spec, full));
        got.1 = ui.edit_actions(id);
    });
    (got.0.expect("ran"), out, got.1)
}

fn sel_id() -> WidgetId {
    WidgetId::ROOT.with("s")
}

#[test]
fn whole_selectable_text_selects_on_the_tap_not_on_the_press() {
    let (shown, full) = ("ab\u{2026}", "abcdef");
    let mut h = UiHarness::new(WIN);
    selectable(&mut h, shown, full, None);
    h.at_ms(0).input.touch_start(1, 24.0, 40.0);
    let (_, _, a) = selectable(&mut h, shown, full, None);
    assert!(!a.expect("drawn").copy, "the press selects nothing");
    h.at_ms(40).input.touch_end(1, 24.0, 40.0);
    let (r, _, a) = selectable(&mut h, shown, full, None);
    assert!(r.clicked && r.focused);
    assert!(a.expect("drawn").copy, "the tap selects all");
    let (_, out, _) = selectable(&mut h, shown, full, Some(EditCommand::Copy));
    assert_eq!(out.clipboard, [ClipboardRequest::Write(full.to_owned())]);
}

#[test]
fn whole_selectable_text_selects_all_on_a_long_press_and_takes_select_all() {
    let (shown, full) = ("ab\u{2026}", "abcdef");
    let mut h = UiHarness::new(WIN);
    selectable(&mut h, shown, full, None);
    h.at_ms(0).input.touch_start(1, 24.0, 40.0);
    selectable(&mut h, shown, full, None);
    h.at_ms(600);
    let (r, _, a) = selectable(&mut h, shown, full, None);
    assert!(r.secondary_clicked && r.focused);
    assert!(a.expect("drawn").copy);
    h.at_ms(700).input.touch_end(1, 24.0, 40.0);
    selectable(&mut h, shown, full, None);

    // The app's menu: Select all keeps it selected, Copy writes the whole value.
    let (_, _, a) = selectable(&mut h, shown, full, Some(EditCommand::SelectAll));
    let a = a.expect("drawn");
    assert!(a.copy && a.select_all && !a.cut && !a.paste);
    let (_, out, _) = selectable(&mut h, shown, full, Some(EditCommand::Copy));
    assert_eq!(out.clipboard, [ClipboardRequest::Write(full.to_owned())]);
}

#[test]
fn positioned_selectable_text_selects_the_word_on_a_long_press_and_copies_it() {
    let text = "alpha beta gamma";
    let mut h = UiHarness::new(WIN);
    selectable(&mut h, text, text, None);
    let press = selectable(&mut h, text, text, None);
    assert!(!press.0.focused);
    // On `beta`.
    let at = (20.0 + GLYPH * 7.0 + 1.0, 40.0);
    h.at_ms(0).input.touch_start(1, at.0, at.1);
    let (_, out, _) = selectable(&mut h, text, text, None);
    assert_eq!(
        range(&out, sel_id()).0,
        range(&out, sel_id()).1,
        "the press selects nothing"
    );
    h.at_ms(600);
    let (r, out, a) = selectable(&mut h, text, text, None);
    assert!(r.secondary_clicked && r.focused);
    assert_eq!(range(&out, sel_id()), (6, 10));
    assert!(a.expect("drawn").copy);
    assert_eq!(handles(&h, sel_id()).len(), 2);
    h.at_ms(700).input.touch_end(1, at.0, at.1);
    selectable(&mut h, text, text, None);
    let (_, out, _) = selectable(&mut h, text, text, Some(EditCommand::Copy));
    assert_eq!(out.clipboard, [ClipboardRequest::Write("beta".to_owned())]);
    let (_, out, _) = selectable(&mut h, text, text, Some(EditCommand::SelectAll));
    assert_eq!(range(&out, sel_id()), (0, text.len()));
}

#[test]
fn positioned_selectable_text_places_the_caret_on_a_tap_and_a_pan_selects_nothing() {
    let text = "alpha beta gamma";
    let mut h = UiHarness::new(WIN);
    selectable(&mut h, text, text, None);
    let at = (20.0 + GLYPH * 3.0 + 1.0, 40.0);
    h.at_ms(0).input.touch_start(1, at.0, at.1);
    selectable(&mut h, text, text, None);
    for k in 1..=3 {
        h.at_ms(20 * k)
            .input
            .touch_move(1, at.0 + 20.0 * k as f32, at.1);
        let (_, out, _) = selectable(&mut h, text, text, None);
        let (s, e) = range(&out, sel_id());
        assert_eq!(s, e, "a drag selects nothing");
    }
    h.at_ms(100).input.touch_end(1, at.0 + 60.0, at.1);
    selectable(&mut h, text, text, None);
    h.at_ms(1000).input.touch_start(1, at.0, at.1);
    selectable(&mut h, text, text, None);
    h.at_ms(1040).input.touch_end(1, at.0, at.1);
    let (r, out, _) = selectable(&mut h, text, text, None);
    assert!(r.clicked && r.focused);
    assert_eq!(selection(&out, sel_id()), (3, 3));
}
