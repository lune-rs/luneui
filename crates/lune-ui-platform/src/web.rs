//! Typing into a page's canvas. A canvas has no text input of its own: an input method (Japanese,
//! Chinese and Korean composition, accents typed with dead keys) and a phone's soft keyboard talk
//! to a focused editable element, and winit 0.30's web backend sends no IME events. [`TextInput`]
//! owns one hidden `<textarea>` for it, tells it what winit's window would be told, and gives back
//! the `winit::event::Ime` values an app feeds to `lune_ui_winit::input::Translator::ime`.
//!
//! Every item exists on every target; off the web the value is inert (as
//! [`KeyboardInset`](crate::mobile::KeyboardInset) is off the phones): it holds nothing, `take` is
//! always empty, the setters do nothing and every window event passes. The DOM glue lives under
//! `platform/web/`, in the platform layer, and the app joins it to the winit adapter, as it does for
//! the keyboard inset.
//!
//! One frame of an app, in order (the demo's `apps/demo/src/lib.rs` is the model):
//!
//! 1. each `WindowEvent` goes to [`passes`](TextInput::passes) first and to the translator only
//!    when it answers true;
//! 2. before the translator's `take`: [`take_focus`](TextInput::take_focus) (a `Some` goes to the
//!    translator as `Translator::focused`) and [`take`](TextInput::take) (each value to
//!    `Translator::ime`);
//! 3. after the frame, from the output plan (`OutputState::plan_output`): `set_allowed(true)` and
//!    `set_area(ime_area_physical(..))` when an editing area appears or moves, `set_allowed(false)`
//!    when it goes away.

use winit::event::{Ime, WindowEvent};
use winit::window::Window;

use crate::platform::web as os;

/// The page's text input bridge: a hidden `<textarea>` that takes the focus while the toolkit
/// edits text.
///
/// While allowed ([`set_allowed`](Self::set_allowed)) the textarea holds the DOM focus; the canvas
/// does not, so a printable key reaches the field exactly once, through the textarea's `input`
/// event. A key that is not text (Enter, Backspace, Tab, the arrows, any Ctrl or Cmd chord such as
/// select all, copy and paste) is re-dispatched on the canvas as a synthetic `KeyboardEvent`, so
/// winit's own key mapping stays the only one and the toolkit's shortcuts and focus keys work as
/// ever. Keys the input method owns (a composition in progress, key code 229) are left to it, and
/// its composition becomes `Ime::Preedit` (the cursor a byte offset) and `Ime::Commit`. A soft
/// keyboard that sends no usable `keydown` for Backspace, Delete or Enter (Android's) is mapped from
/// the textarea's `beforeinput` to those keys; phone keyboards are unverified on a device.
///
/// A pointer release on the canvas gives the textarea the focus back while allowed (inside the
/// gesture, which is what makes a phone show its keyboard), and winit moves the focus to the canvas
/// on every pointer press; the toolkit must not see that round trip as a lost focus. winit sends
/// `Focused(false)` for the canvas when the textarea takes the focus, so [`passes`](Self::passes)
/// keeps every `Focused` event from the app's translator and [`take_focus`](Self::take_focus)
/// gives the focus of the window as a whole: gained when the canvas or the textarea has it, lost
/// when neither does, reported only when it changed.
///
/// Dropping the value removes the element and every listener it added; a second [`attach`](Self::attach)
/// after that does not double a keystroke. It owns a DOM element: create, use and drop it on the
/// main thread (`!Send`). The element carries `data-luneui="text-input"`.
#[derive(Debug)]
pub struct TextInput(os::TextInput);

impl TextInput {
    /// Attaches a bridge to `window`'s canvas. `on_event` runs, from a browser event, whenever the
    /// bridge has something to give the app (a composition or typed text, a focus change): it must
    /// only wake the app (an event-loop proxy's `send_event`, `Window::request_redraw`), never draw
    /// or call the bridge. Off the web, and on a window that has no canvas, the value is inert.
    pub fn attach(window: &Window, on_event: impl Fn() + 'static) -> TextInput {
        TextInput(os::TextInput::attach(window, on_event))
    }

    /// The toolkit wants text: `true` gives the textarea the focus and queues `Ime::Enabled`;
    /// `false` gives it to the canvas and queues `Ime::Disabled`. The same value again does
    /// nothing. This is the call winit's `Window::set_ime_allowed` takes on a desktop.
    pub fn set_allowed(&mut self, allowed: bool) {
        self.0.set_allowed(allowed);
    }

    /// Where the text is edited, `(x, y, width, height)` in physical pixels of the canvas, the tuple
    /// `lune_ui_winit::input::ime_area_physical` returns, which winit's
    /// `Window::set_ime_cursor_area` takes on a desktop. The textarea moves there (a phone scrolls
    /// to it, and an input method opens its candidate window beside it).
    pub fn set_area(&mut self, area: (f32, f32, f32, f32)) {
        self.0.set_area(area);
    }

    /// The `Ime` values since the last call, in order (empty off the web).
    pub fn take(&mut self) -> Vec<Ime> {
        self.0.take()
    }

    /// Whether `event` is for the app's translator. `Focused` events are not (on the web): the
    /// bridge keeps the focus of the canvas and the textarea together and gives the window's own
    /// through [`take_focus`](Self::take_focus). Everything else, and everything off the web, is.
    pub fn passes(&mut self, event: &WindowEvent) -> bool {
        self.0.passes(event)
    }

    /// The window's focus when it changed since the last call (`Some(true)` gained, `Some(false)`
    /// lost), else `None`; always `None` off the web, where `passes` lets winit's events through.
    pub fn take_focus(&mut self) -> Option<bool> {
        self.0.take_focus()
    }
}
