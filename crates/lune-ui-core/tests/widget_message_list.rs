//! The `message-list` widget (`layout.message-list`): rows with an unread dot, a title, an age, a
//! body cut to two lines and a meta line, in a scroll area; one roving tab stop; a click, Enter and
//! Space report `clicked`, a secondary click `context`; the header and its action.

use lune_ui_core::access::{Action, Role, SemanticTree};
use lune_ui_core::color::{premul, with_alpha};
use lune_ui_core::input::{FrameOutput, Key, NamedKey, PointerButton};
use lune_ui_core::paint::Rect;
use lune_ui_core::scene::Scene;
use lune_ui_core::testing::{FakeText, UiHarness, disabled_violations};
use lune_ui_core::text::{Spec, Style, TextSystem};
use lune_ui_core::theme::DARK;
use lune_ui_core::ui::WidgetId;
use lune_ui_core::widgets::message_list::{
    HEADER_HEIGHT, MessageListOptions, MessageListResponse, MessageRow, height, row_height,
};

const WIN: (f32, f32) = (400.0, 700.0);
const AT: Rect = Rect::new(10.0, 10.0, 300.0, 600.0);
const P: PointerButton = PointerButton::Primary;

fn list_id() -> WidgetId {
    WidgetId::ROOT.with("ml")
}

fn row_id(i: usize) -> WidgetId {
    list_id().with(i)
}

fn titles(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("Message {i}")).collect()
}

fn plain_rows(titles: &[String]) -> Vec<MessageRow<'_>> {
    titles.iter().map(|t| MessageRow::new(t)).collect()
}

struct Seen {
    scene: Scene,
    out: FrameOutput,
    resp: MessageListResponse,
}

fn frame_with(h: &mut UiHarness, rows: &[MessageRow<'_>], opts: &MessageListOptions<'_>) -> Seen {
    let mut resp = MessageListResponse::default();
    let (scene, out) = h.frame(|ui| resp = ui.message_list("ml", AT, rows, opts));
    Seen { scene, out, resp }
}

fn frame(h: &mut UiHarness, rows: &[MessageRow<'_>]) -> Seen {
    frame_with(h, rows, &MessageListOptions::new("Inbox"))
}

fn press(h: &mut UiHarness, key: NamedKey) {
    h.input.key_down(Key::Named(key), None);
}

fn row_node(tree: &SemanticTree, i: usize) -> Rect {
    tree.get(row_id(i)).expect("row node").bounds
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Clicks the centre of row `i` as the last frame drew it.
fn click_row(h: &mut UiHarness, rows: &[MessageRow<'_>], i: usize, button: PointerButton) -> Seen {
    let at = centre(row_node(&frame(h, rows).out.tree, i));
    h.input.pointer_to(at.0, at.1);
    frame(h, rows);
    h.input.click(button);
    frame(h, rows)
}

/// Focuses the list's tab stop (its first row) with one Tab.
fn enter(h: &mut UiHarness, rows: &[MessageRow<'_>]) {
    frame(h, rows);
    press(h, NamedKey::Tab);
    frame(h, rows);
}

#[test]
fn a_click_on_a_row_reports_its_index() {
    let t = titles(4);
    let rows = plain_rows(&t);
    let mut h = UiHarness::new(WIN);
    assert_eq!(click_row(&mut h, &rows, 2, P).resp.clicked, Some(2));
    // Once: the frame after reports nothing.
    assert_eq!(frame(&mut h, &rows).resp.clicked, None);
}

#[test]
fn the_arrows_and_the_end_keys_move_focus_and_enter_and_space_activate() {
    let t = titles(4);
    let rows = plain_rows(&t);
    let mut h = UiHarness::new(WIN);
    enter(&mut h, &rows);
    assert_eq!(frame(&mut h, &rows).out.focused, Some(row_id(0)));
    for _ in 0..2 {
        press(&mut h, NamedKey::ArrowDown);
        frame(&mut h, &rows);
    }
    assert_eq!(frame(&mut h, &rows).out.focused, Some(row_id(2)));
    press(&mut h, NamedKey::Enter);
    assert_eq!(frame(&mut h, &rows).resp.clicked, Some(2));
    press(&mut h, NamedKey::End);
    frame(&mut h, &rows);
    press(&mut h, NamedKey::Space);
    assert_eq!(frame(&mut h, &rows).resp.clicked, Some(3));
    press(&mut h, NamedKey::Home);
    frame(&mut h, &rows);
    assert_eq!(frame(&mut h, &rows).out.focused, Some(row_id(0)));
}

#[test]
fn the_list_is_one_tab_stop() {
    let t = titles(4);
    let rows = plain_rows(&t);
    let mut h = UiHarness::new(WIN);
    enter(&mut h, &rows);
    press(&mut h, NamedKey::Tab);
    // The only stop of the frame: the next Tab leaves the list, to nothing or around to it.
    let after = frame(&mut h, &rows).out.focused;
    assert!(
        after.is_none() || after == Some(row_id(0)),
        "Tab left the list on {after:?}"
    );
}

/// The `fg_bright` title items of `text` in `scene`, by alpha.
fn drawn(scene: &Scene, text: &str, style: Style) -> usize {
    let key = Spec::plain(text, style).cache_key(1.0);
    scene.layers[0]
        .texts
        .iter()
        .filter(|t| t.key.0 == key)
        .count()
}

fn title_style(alpha: f32) -> Style {
    Style::ui(DARK.font_base, with_alpha(DARK.fg_bright, alpha)).medium()
}

/// The bounds of the attention fills in `scene`.
fn dots(scene: &Scene) -> Vec<Rect> {
    let want = premul(DARK.attention());
    scene
        .layers
        .iter()
        .flat_map(|l| l.geometry.under.iter())
        .filter(|s| s.color == want)
        .map(|s| Rect::new(s.rect[0], s.rect[1], s.rect[2], s.rect[3]))
        .collect()
}

#[test]
fn a_read_row_draws_at_alpha_six_tenths_and_an_unread_one_has_a_dot() {
    let rows = [
        MessageRow::new("Unread one").with_unread(true),
        MessageRow::new("Read one"),
    ];
    let mut h = UiHarness::new(WIN);
    let s = frame(&mut h, &rows);
    assert_eq!(drawn(&s.scene, "Unread one", title_style(1.0)), 1);
    assert_eq!(drawn(&s.scene, "Read one", title_style(0.6)), 1);
    assert_eq!(drawn(&s.scene, "Read one", title_style(1.0)), 0);
    let fills = dots(&s.scene);
    assert_eq!(fills.len(), 1, "one dot, for the unread row: {fills:?}");
    assert_eq!((fills[0].w, fills[0].h), (6.0, 6.0));
    // No dot for a list of read rows.
    let read = [MessageRow::new("a"), MessageRow::new("b")];
    let s = frame(&mut UiHarness::new(WIN), &read);
    let none = dots(&s.scene);
    assert!(none.is_empty(), "{none:?}");
}

const BODY: &str = "one two three four five six seven eight nine ten eleven twelve thirteen \
fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three";

#[test]
fn a_long_body_is_drawn_as_two_lines_the_second_ending_in_an_ellipsis() {
    let rows = [MessageRow::new("t").with_body(BODY).with_unread(true)];
    let mut h = UiHarness::new(WIN);
    let s = frame(&mut h, &rows);
    let style = Style::ui(DARK.font_base, DARK.fg_primary);
    let room = 300.0 - 24.0 - 6.0 - 10.0;
    let lines = FakeText.lines(&Spec::plain(BODY, style).wrapped(room), 1.0);
    assert!(lines.len() > 2, "the body needs more than two lines here");
    let first = BODY[lines[0].start..lines[0].end].trim_end();
    let second = format!("{}\u{2026}", BODY[lines[1].start..lines[1].end].trim_end());
    assert_eq!(drawn(&s.scene, first, style), 1, "first line {first:?}");
    assert_eq!(drawn(&s.scene, &second, style), 1, "second line {second:?}");
    // The third line is not drawn.
    let third = BODY[lines[2].start..lines[2].end].trim_end();
    assert_eq!(drawn(&s.scene, third, style), 0);
    // A short body stays one line with no ellipsis.
    let rows = [MessageRow::new("t").with_body("short").with_unread(true)];
    let s = frame(&mut UiHarness::new(WIN), &rows);
    assert_eq!(drawn(&s.scene, "short", style), 1);
}

#[test]
fn row_height_adds_the_lines_and_height_is_their_sum_under_the_cap() {
    let rows = [
        MessageRow::new("a"),
        MessageRow::new("b").with_body("body").with_meta("meta"),
        MessageRow::new("c").with_body(BODY).with_age("now"),
    ];
    let mut h = UiHarness::new(WIN);
    let mut sum = 0.0;
    let mut hs = Vec::new();
    h.frame(|ui| {
        for r in &rows {
            let v = row_height(ui.painter(), r, 300.0);
            hs.push(v);
            sum += v;
        }
        let all = height(ui.painter(), &rows, 300.0, f32::INFINITY);
        assert!((all - sum).abs() < 0.001, "{all} vs {sum}");
        let capped = height(ui.painter(), &rows, 300.0, 50.0);
        assert_eq!(capped, 50.0);
        assert_eq!(height(ui.painter(), &[], 300.0, 50.0), 0.0);
    });
    // A row with a body and a meta line is taller than a bare one by their lines and gaps.
    assert!(hs[1] > hs[0] + 4.0);
    assert!(hs[2] > hs[1]);
    // The list reports the same height it was sized by.
    let r = frame(&mut h, &rows).resp;
    assert!((r.height - sum).abs() < 0.001, "{} vs {sum}", r.height);
}

#[test]
fn a_capped_list_scrolls_and_keeps_the_focused_row_in_view() {
    let t = titles(50);
    let rows = plain_rows(&t);
    let opts = MessageListOptions::new("Inbox").with_max_height(300.0);
    // Each frame is 16 ms on, so the scroll's own animation runs.
    let mut ms = 0;
    let mut next = |h: &mut UiHarness| {
        ms += 16;
        frame_with(h.at_ms(ms), &rows, &opts)
    };
    let mut h = UiHarness::new(WIN);
    let s = next(&mut h);
    assert_eq!(s.resp.height, 300.0);
    let top_before = row_node(&s.out.tree, 10).y;
    // A wheel over the list moves the rows.
    h.input.pointer_to(100.0, 100.0);
    next(&mut h);
    h.input.scroll_pixels(0.0, -120.0);
    for _ in 0..30 {
        next(&mut h);
    }
    let top_after = row_node(&next(&mut h).out.tree, 10).y;
    assert!(top_after < top_before, "{top_after} vs {top_before}");

    // Down from the first row, one key at a time, past the visible bottom: the focused row stays
    // inside the list's 300 px.
    let mut h = UiHarness::new(WIN);
    next(&mut h);
    press(&mut h, NamedKey::Tab);
    next(&mut h);
    for step in 1..=30 {
        press(&mut h, NamedKey::ArrowDown);
        for _ in 0..30 {
            next(&mut h);
        }
        let s = next(&mut h);
        let at = row_node(&s.out.tree, step);
        assert_eq!(s.out.focused, Some(row_id(step)));
        assert!(
            at.y >= AT.y - 0.5 && at.bottom() <= AT.y + 300.0 + 0.5,
            "step {step}: the focused row {at:?} is inside the list"
        );
    }
    // End goes to the last row, and it shows.
    press(&mut h, NamedKey::End);
    for _ in 0..40 {
        next(&mut h);
    }
    let s = next(&mut h);
    let last = row_node(&s.out.tree, 49);
    let which = (0..50).find(|i| s.out.focused == Some(row_id(*i)));
    assert_eq!(which, Some(49));
    assert!(
        last.y >= AT.y - 0.5 && last.bottom() <= AT.y + 300.0 + 0.5,
        "{last:?}"
    );
}

#[test]
fn a_secondary_click_reports_context_not_clicked() {
    let t = titles(3);
    let rows = plain_rows(&t);
    let mut h = UiHarness::new(WIN);
    let s = click_row(&mut h, &rows, 1, PointerButton::Secondary);
    assert_eq!(s.resp.context, Some(1));
    assert_eq!(s.resp.clicked, None);
}

#[test]
fn the_header_reports_its_action_and_with_none_draws_no_button() {
    let mut h = UiHarness::new(WIN);
    let at = Rect::new(10.0, 10.0, 300.0, HEADER_HEIGHT);
    let mut clicked = false;
    let (_, out) = h.frame(|ui| {
        clicked = ui.message_list_header("hd", at, "Notifications", Some("Clear all"));
    });
    assert!(!clicked);
    let button = out
        .tree
        .nodes
        .iter()
        .find(|n| n.role == Role::Button && n.name == "Clear all")
        .expect("the action button")
        .bounds;
    assert!(
        button.right() <= at.right() && button.x > at.x + 100.0,
        "{button:?}"
    );
    let c = centre(button);
    h.input.pointer_to(c.0, c.1);
    h.frame(|ui| {
        ui.message_list_header("hd", at, "Notifications", Some("Clear all"));
    });
    h.input.click(P);
    h.frame(|ui| {
        clicked = ui.message_list_header("hd", at, "Notifications", Some("Clear all"));
    });
    assert!(clicked);
    let (_, out) = UiHarness::new(WIN).frame(|ui| {
        assert!(!ui.message_list_header("hd", at, "Notifications", None));
    });
    assert!(out.tree.nodes.iter().all(|n| n.role != Role::Button));
}

#[test]
fn the_nodes_name_the_list_and_describe_each_row() {
    let rows = [
        MessageRow::new("Build")
            .with_body("Finished")
            .with_meta("main")
            .with_unread(true),
        MessageRow::new("Deploy").with_body("Started"),
        MessageRow::new("Bare"),
    ];
    let mut h = UiHarness::new(WIN);
    let s = frame(&mut h, &rows);
    let list = s.out.tree.get(list_id()).expect("list node");
    assert_eq!((list.role, list.name.as_str()), (Role::List, "Inbox"));
    let desc = |i: usize| {
        let n = s.out.tree.get(row_id(i)).expect("row");
        assert_eq!(n.role, Role::ListItem);
        assert!(n.actions.contains(Action::Default));
        (n.name.clone(), n.description.clone())
    };
    // The string table is empty in the harness, so the key itself names the state.
    let unread = desc(0);
    assert_eq!(unread.0, "Build");
    let d = unread.1.expect("description");
    assert!(d.ends_with("Finished, main") && d.contains(", "), "{d}");
    assert_eq!(desc(1), ("Deploy".to_owned(), Some("Started".to_owned())));
    assert_eq!(desc(2), ("Bare".to_owned(), None));
}

#[test]
fn no_rows_draws_nothing_and_has_no_stop() {
    let mut h = UiHarness::new(WIN);
    let s = frame(&mut h, &[]);
    assert_eq!(s.resp, MessageListResponse::default());
    assert!(s.out.tree.get(list_id()).is_none());
    press(&mut h, NamedKey::Tab);
    assert_eq!(frame(&mut h, &[]).out.focused, None);
}

#[test]
fn a_disabled_list_reports_nothing_and_marks_every_node() {
    let t = titles(3);
    let rows = plain_rows(&t);
    let mut h = UiHarness::new(WIN);
    let run = |h: &mut UiHarness| {
        let mut resp = MessageListResponse::default();
        let (_, out) = h.frame(|ui| {
            ui.with_enabled(false, |ui| {
                resp = ui.message_list("ml", AT, &rows, &MessageListOptions::new("Inbox"));
            });
        });
        (resp, out)
    };
    run(&mut h);
    for key in [NamedKey::Enter, NamedKey::Space, NamedKey::ArrowDown] {
        press(&mut h, key);
        let (resp, out) = run(&mut h);
        assert_eq!(resp.clicked, None, "{key:?}");
        assert!(out.fired.is_empty());
    }
    let (_, out) = run(&mut h);
    assert_eq!(disabled_violations(&out.tree, list_id()), []);
    let at = centre(row_node(&out.tree, 1));
    h.input.pointer_to(at.0, at.1);
    run(&mut h);
    h.input.click(P);
    assert_eq!(run(&mut h).0.clicked, None);
}
