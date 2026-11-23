//! The pure rules of the web text bridge: where a key of the hidden textarea goes, what a
//! composition or an input event means as winit `Ime` values, and the focus bookkeeping that keeps
//! the canvas-to-textarea hand-over from reaching the toolkit as a lost focus. No browser type here,
//! so the rules compile for the tests on every host (the bridge itself, `wasm.rs`, is wasm only).

use winit::event::Ime;

/// Where a `keydown` of the textarea goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyRoute {
    /// A printable key: the textarea's own `input` event carries the character, once.
    Text,
    /// Not text (a named key, or a shortcut chord): re-dispatched on the canvas, so winit's key
    /// mapping stays the only one.
    Forward,
    /// The input method owns the key: nothing is done with it.
    Ignore,
}

/// The route of one `keydown`: `key` is `KeyboardEvent.key`, `key_code` its (deprecated) `keyCode`,
/// `composing` its `isComposing`, `alt_graph` whether the `AltGraph` modifier is on (on Windows an
/// AltGr chord also reports Ctrl and Alt). Alt alone changes nothing: Option on a Mac types
/// characters, and Alt with a named key (Alt+Backspace, Alt+Arrow) is forwarded like any named key.
pub(crate) fn route_keydown(
    key: &str,
    ctrl: bool,
    meta: bool,
    alt_graph: bool,
    composing: bool,
    key_code: u32,
) -> KeyRoute {
    // 229 is the code a browser gives every key the input method handles; the names are what the
    // spec gives a key that has none to report (a dead key, an Android soft keyboard's key).
    if composing || key_code == 229 || matches!(key, "Process" | "Dead" | "Unidentified") {
        return KeyRoute::Ignore;
    }
    let printable = key.chars().count() == 1;
    if alt_graph && printable {
        return KeyRoute::Text;
    }
    if ctrl || meta {
        return KeyRoute::Forward;
    }
    if printable {
        KeyRoute::Text
    } else {
        KeyRoute::Forward
    }
}

/// True for a forwarded chord that the browser will not release: a Mac gives no `keyup` for a key
/// pressed with Cmd held, so the release is sent with the press.
pub(crate) fn releases_at_once(key: &str, meta: bool) -> bool {
    meta && key.chars().count() == 1
}

/// The `Ime` values of a `compositionupdate` with `data`: the preedit, its cursor at the end (a
/// byte offset, the unit winit and the toolkit use), or its clearing when `data` is empty.
pub(crate) fn composition_update(data: &str) -> Vec<Ime> {
    if data.is_empty() {
        vec![Ime::Preedit(String::new(), None)]
    } else {
        vec![Ime::Preedit(
            data.to_owned(),
            Some((data.len(), data.len())),
        )]
    }
}

/// The `Ime` values of a `compositionend` with the committed `data`: the preedit is cleared, then
/// the text is committed (nothing is committed for an empty `data`, a cancelled composition).
pub(crate) fn composition_end(data: &str) -> Vec<Ime> {
    let mut out = vec![Ime::Preedit(String::new(), None)];
    if !data.is_empty() {
        out.push(Ime::Commit(data.to_owned()));
    }
    out
}

/// The `Ime` values of an `input` event outside a composition: its `data` committed when the
/// `input_type` inserts typed or replaced text. Everything else (a composition's own `input`, a
/// paste, a delete) gives nothing: the composition events and the forwarded keys carry those.
pub(crate) fn input_text(data: Option<&str>, input_type: &str, composing: bool) -> Vec<Ime> {
    match (composing, input_type, data) {
        (false, "insertText" | "insertReplacementText", Some(text)) if !text.is_empty() => {
            vec![Ime::Commit(text.to_owned())]
        }
        _ => Vec::new(),
    }
}

/// The key (`KeyboardEvent.key` and `code`) an editing `beforeinput` stands for when the soft
/// keyboard sent no usable `keydown` for it (Backspace, Delete and Enter on a phone); `None` for
/// every other `input_type`.
pub(crate) fn edit_key(input_type: &str) -> Option<&'static str> {
    match input_type {
        "deleteContentBackward" => Some("Backspace"),
        "deleteContentForward" => Some("Delete"),
        "insertLineBreak" | "insertParagraph" => Some("Enter"),
        _ => None,
    }
}

/// Who holds the window's focus, as the toolkit must see it. The window is focused while its
/// canvas or its textarea holds the DOM focus: moving it from one to the other is not a loss, and
/// the focus events winit sends for the canvas alone would otherwise say it was.
///
/// winit's `Focused` events and the textarea's own focus and blur events feed it; the app reads
/// [`take`](Self::take) and gives the toolkit that value only when it changed.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FocusTracker {
    /// What winit last said about the canvas (or the textarea's blur said it is about to).
    canvas: bool,
    /// Whether the textarea holds the focus, or is about to.
    text: bool,
    /// What the toolkit was last told.
    reported: bool,
}

impl FocusTracker {
    /// winit's `Focused(on)` for the canvas.
    pub(crate) fn canvas_focus(&mut self, on: bool) {
        self.canvas = on;
    }

    /// The textarea is about to take the focus (set before the `focus()` call, whose canvas blur
    /// is delivered before the textarea's own focus event).
    pub(crate) fn text_focusing(&mut self) {
        self.text = true;
    }

    /// The textarea gained the focus.
    pub(crate) fn text_gained(&mut self) {
        self.text = true;
    }

    /// The textarea lost the focus; `to_canvas` when it went to the canvas (`relatedTarget`), which
    /// winit reports with its own `Focused(true)` later.
    pub(crate) fn text_lost(&mut self, to_canvas: bool) {
        self.text = false;
        if to_canvas {
            self.canvas = true;
        }
    }

    /// The focus to give the toolkit now, `None` while it already has it.
    pub(crate) fn take(&mut self) -> Option<bool> {
        let now = self.canvas || self.text;
        (now != self.reported).then(|| {
            self.reported = now;
            now
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(key: &str) -> KeyRoute {
        route_keydown(key, false, false, false, false, 0)
    }

    #[test]
    fn printable_keys_route_to_text() {
        for key in ["a", "Z", "5", " ", "é", "日", "!"] {
            assert_eq!(route(key), KeyRoute::Text, "{key:?}");
        }
    }

    #[test]
    fn named_keys_forward() {
        for key in [
            "Enter",
            "Backspace",
            "Delete",
            "Tab",
            "Escape",
            "ArrowLeft",
            "Home",
            "End",
            "F5",
            "Shift",
            "Control",
        ] {
            assert_eq!(route(key), KeyRoute::Forward, "{key:?}");
        }
    }

    #[test]
    fn ctrl_and_meta_chords_forward() {
        assert_eq!(
            route_keydown("a", true, false, false, false, 65),
            KeyRoute::Forward
        );
        assert_eq!(
            route_keydown("c", false, true, false, false, 67),
            KeyRoute::Forward
        );
        assert_eq!(
            route_keydown("Backspace", true, false, false, false, 8),
            KeyRoute::Forward
        );
    }

    #[test]
    fn a_composing_key_or_code_229_is_ignored() {
        assert_eq!(
            route_keydown("a", false, false, false, true, 65),
            KeyRoute::Ignore
        );
        assert_eq!(
            route_keydown("a", false, false, false, false, 229),
            KeyRoute::Ignore
        );
        assert_eq!(
            route_keydown("Process", false, false, false, false, 0),
            KeyRoute::Ignore
        );
        // A composing Enter is the input method's, not a submit.
        assert_eq!(
            route_keydown("Enter", false, false, false, true, 13),
            KeyRoute::Ignore
        );
        // A dead key and a soft keyboard's nameless key.
        assert_eq!(route("Dead"), KeyRoute::Ignore);
        assert_eq!(route("Unidentified"), KeyRoute::Ignore);
    }

    #[test]
    fn enter_forwards() {
        assert_eq!(route("Enter"), KeyRoute::Forward);
    }

    #[test]
    fn alt_types_characters_but_forwards_named_keys() {
        // Option+e on a Mac, AltGr+q on a German layout (Windows reports Ctrl and Alt with it).
        assert_eq!(
            route_keydown("é", false, false, false, false, 69),
            KeyRoute::Text
        );
        assert_eq!(
            route_keydown("@", true, false, true, false, 81),
            KeyRoute::Text
        );
        assert_eq!(
            route_keydown("Backspace", false, false, false, false, 8),
            KeyRoute::Forward
        );
    }

    #[test]
    fn a_meta_chord_is_released_with_its_press() {
        assert!(releases_at_once("a", true));
        assert!(!releases_at_once("a", false));
        assert!(!releases_at_once("Meta", true));
        assert!(!releases_at_once("ArrowLeft", true));
    }

    #[test]
    fn a_preedit_cursor_is_a_byte_offset() {
        assert_eq!(
            composition_update("日本"),
            vec![Ime::Preedit("日本".into(), Some((6, 6)))]
        );
        assert_eq!(
            composition_update("abc"),
            vec![Ime::Preedit("abc".into(), Some((3, 3)))]
        );
    }

    #[test]
    fn an_empty_composition_update_clears_the_preedit() {
        assert_eq!(
            composition_update(""),
            vec![Ime::Preedit(String::new(), None)]
        );
    }

    #[test]
    fn a_composition_end_clears_the_preedit_then_commits() {
        assert_eq!(
            composition_end("日本"),
            vec![
                Ime::Preedit(String::new(), None),
                Ime::Commit("日本".into())
            ]
        );
    }

    #[test]
    fn a_cancelled_composition_commits_nothing() {
        assert_eq!(composition_end(""), vec![Ime::Preedit(String::new(), None)]);
    }

    #[test]
    fn typed_text_commits_once_outside_a_composition() {
        assert_eq!(
            input_text(Some("h"), "insertText", false),
            vec![Ime::Commit("h".into())]
        );
        assert_eq!(
            input_text(Some("hello"), "insertReplacementText", false),
            vec![Ime::Commit("hello".into())]
        );
    }

    #[test]
    fn an_input_inside_or_of_another_kind_gives_nothing() {
        assert!(input_text(Some("h"), "insertText", true).is_empty());
        assert!(input_text(Some("日"), "insertCompositionText", false).is_empty());
        assert!(input_text(Some("x"), "insertFromPaste", false).is_empty());
        assert!(input_text(None, "insertText", false).is_empty());
        assert!(input_text(Some(""), "insertText", false).is_empty());
        assert!(input_text(None, "deleteContentBackward", false).is_empty());
    }

    #[test]
    fn a_soft_keyboards_edits_map_to_keys() {
        assert_eq!(edit_key("deleteContentBackward"), Some("Backspace"));
        assert_eq!(edit_key("deleteContentForward"), Some("Delete"));
        assert_eq!(edit_key("insertLineBreak"), Some("Enter"));
        assert_eq!(edit_key("insertParagraph"), Some("Enter"));
        assert_eq!(edit_key("insertText"), None);
        assert_eq!(edit_key("deleteCompositionText"), None);
    }

    /// The focus the toolkit is given over a sequence, one `take` after each step.
    fn told(steps: &[Step]) -> Vec<Option<bool>> {
        let mut f = FocusTracker::default();
        steps
            .iter()
            .map(|s| {
                match s {
                    Step::Canvas(on) => f.canvas_focus(*on),
                    Step::Focusing => f.text_focusing(),
                    Step::Gained => f.text_gained(),
                    Step::Lost(to_canvas) => f.text_lost(*to_canvas),
                }
                f.take()
            })
            .collect()
    }

    enum Step {
        Canvas(bool),
        Focusing,
        Gained,
        Lost(bool),
    }

    #[test]
    fn a_click_into_the_field_tells_the_toolkit_focus_once() {
        use Step::*;
        // The pointer focuses the canvas; the toolkit allows the IME; the textarea takes the focus
        // (the canvas blur arrives after, delivered by winit).
        let got = told(&[Canvas(true), Focusing, Canvas(false), Gained]);
        assert_eq!(got, [Some(true), None, None, None]);
    }

    #[test]
    fn each_later_click_in_the_field_tells_the_toolkit_nothing() {
        use Step::*;
        let mut steps = vec![Canvas(true), Focusing, Canvas(false), Gained];
        // Pointerdown: the textarea blurs to the canvas, winit says the canvas is focused;
        // pointerup: the textarea takes it back and winit says the canvas lost it.
        for _ in 0..3 {
            steps.extend([Lost(true), Canvas(true), Focusing, Canvas(false), Gained]);
        }
        let got = told(&steps);
        assert_eq!(got.iter().flatten().count(), 1, "{got:?}");
    }

    #[test]
    fn winit_events_arriving_late_and_together_still_tell_nothing() {
        use Step::*;
        // A quick tap: both of winit's events are delivered after both DOM moves happened.
        let got = told(&[
            Canvas(true),
            Focusing,
            Canvas(false),
            Gained,
            Lost(true),
            Focusing,
            Gained,
            Canvas(true),
            Canvas(false),
        ]);
        assert_eq!(got.iter().flatten().count(), 1, "{got:?}");
    }

    #[test]
    fn the_textarea_losing_the_focus_to_the_page_is_a_loss() {
        use Step::*;
        let got = told(&[Canvas(true), Focusing, Canvas(false), Gained, Lost(false)]);
        assert_eq!(got, [Some(true), None, None, None, Some(false)]);
    }

    #[test]
    fn the_canvas_losing_the_focus_alone_is_a_loss() {
        use Step::*;
        assert_eq!(
            told(&[Canvas(true), Canvas(false)]),
            [Some(true), Some(false)]
        );
    }

    #[test]
    fn the_textarea_regaining_the_focus_is_a_gain() {
        use Step::*;
        let got = told(&[
            Canvas(true),
            Focusing,
            Canvas(false),
            Gained,
            Lost(false),
            Gained,
        ]);
        assert_eq!(got[5], Some(true));
    }

    #[test]
    fn the_hand_back_to_the_canvas_is_not_a_loss() {
        use Step::*;
        // The toolkit turned the IME off: focus goes textarea to canvas.
        let got = told(&[
            Canvas(true),
            Focusing,
            Canvas(false),
            Gained,
            Lost(true),
            Canvas(true),
        ]);
        assert_eq!(got.iter().flatten().count(), 1, "{got:?}");
    }
}
