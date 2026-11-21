//! Running as a phone app. On iOS the toolkit's window must belong to a `UIWindowScene`: since the
//! iOS 27 SDK, UIKit refuses to launch an app that does not adopt the scene life cycle, and winit
//! 0.30 creates its `UIWindow` without a scene. An iOS app declares the scene manifest in its
//! `Info.plist` (`docs/mobile.md` has the keys) and calls [`attach_to_scene`] once its window
//! exists.
//!
//! Every item exists on every target; off iOS each one does nothing and says so.

use lune_ui_core::input::ClipboardBackend;
use lune_ui_core::layout::Insets;
use winit::window::Window;

use crate::platform::mobile as os;

/// What [`attach_to_scene`] found or did.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneAttach {
    /// The window belongs to a window scene: it was attached now or before.
    Attached,
    /// No window scene to attach to yet. On iOS the window is attached when one connects (an
    /// observer waits for it); this is also the answer off the main thread, or when the window
    /// has no `UIWindow` behind it, and then nothing was done.
    Waiting,
    /// This platform has no scenes (every target but iOS); nothing was done.
    NotNeeded,
}

/// Puts `window` in the app's window scene, so UIKit shows it under the scene life cycle (iOS).
///
/// On iOS, called on the main thread (the event loop's): it remembers the window and registers
/// one observer for the life of the process that attaches the window to each scene that connects
/// later, while the window has none (a scene the OS discards in the background reconnects as a
/// new one). It does this on every path, so the one call at start-up also covers a reconnect. Then,
/// when the window already has a scene it answers [`SceneAttach::Attached`]; otherwise it
/// attaches the window to the first connected `UIWindowScene` and answers
/// [`SceneAttach::Attached`], or, with none connected yet, answers [`SceneAttach::Waiting`]. A
/// later call never registers a second observer, so calling again is how an app asks for the
/// state. An attach makes the window key and visible
/// only when it is not hidden: an app that keeps it hidden (to create its accessibility bridge
/// first) shows it itself.
///
/// Off iOS it does nothing and answers [`SceneAttach::NotNeeded`].
pub fn attach_to_scene(window: &Window) -> SceneAttach {
    os::attach_to_scene(window)
}

/// The window edges the system covers (the status bar and the notch above, the home indicator
/// below, the rounded corners and the camera housing in landscape), in logical pixels: the
/// `Event::SafeArea` an app pushes after `Translator::take` every frame.
///
/// On iOS it is derived from `window`'s four position and size queries, which winit reads live from
/// UIKit's `safeAreaInsets`: the left and top insets are the offset of the inner (safe) rectangle
/// inside the outer (screen) one, the right and bottom the rest, each clamped to at least 0. A value
/// that cannot be read, a non-finite one or a failed position query gives [`Insets::ZERO`].
///
/// Everywhere else it is [`Insets::ZERO`]: a desktop window's frame (a title bar) is not an area the
/// content must keep clear of.
pub fn safe_area(window: &Window) -> Insets {
    os::safe_area(window)
}

/// The part of the window the soft keyboard covers, kept current for an app to push as
/// `Event::KeyboardInset` (iOS).
///
/// On iOS it observes `UIKeyboardWillChangeFrameNotification` and `UIKeyboardWillHideNotification`
/// and keeps the overlap of the keyboard's end frame with the window's view as a bottom inset, in
/// logical pixels (the frame converted from screen space, clamped to the view's height; a missing
/// or non-finite frame, and a hide, give zero). `on_change` runs when the value changes, never when
/// it stays the same, on the main thread and from inside the notification: it must only wake the
/// app (for example `Window::request_redraw`), never draw. Dropping the value removes both
/// observers. It is `!Send`: create and read it on the main thread.
///
/// Elsewhere it is inert: [`inset`](Self::inset) is always [`Insets::ZERO`] and `on_change` never
/// runs.
#[derive(Debug)]
pub struct KeyboardInset(os::KeyboardInset);

impl KeyboardInset {
    /// Starts observing the keyboard for `window`, calling `on_change` when the inset changes. A
    /// window with no UIKit view (or a call off the main thread) gives an inert value that reads
    /// zero.
    pub fn new(window: &Window, on_change: impl Fn() + 'static) -> KeyboardInset {
        KeyboardInset(os::KeyboardInset::new(window, on_change))
    }

    /// The bottom inset the keyboard covers now, in logical pixels; `Insets::ZERO` while it is
    /// hidden.
    pub fn inset(&self) -> Insets {
        self.0.inset()
    }
}

/// The phone's general pasteboard as a clipboard backend (iOS: `UIPasteboard`), for an app to give
/// to `lune_ui_winit::clipboard::Clipboard::with_backend(Box::new(Pasteboard::new()))`. The
/// toolkit's own `Clipboard::new()` has no clipboard on iOS (the calls go to UIKit directly, which the
/// winit adapter does not do), and the adapter's cap on what it reads or writes still applies.
///
/// `read` answers the pasteboard's text, `Err(())` when it holds none or the call is not on the
/// main thread; `write` sets the text, `Err(())` off the main thread. On iOS 16 and later reading
/// text another app put there can raise the system's "Allow Paste" prompt. `!Send`.
///
/// Off iOS both calls answer `Err(())`: a desktop app uses `Clipboard::new()`.
#[derive(Debug, Default)]
pub struct Pasteboard(os::Pasteboard);

impl Pasteboard {
    /// A handle on the general pasteboard.
    pub fn new() -> Pasteboard {
        Pasteboard::default()
    }
}

impl ClipboardBackend for Pasteboard {
    fn read(&mut self) -> Result<String, ()> {
        self.0.read()
    }

    fn write(&mut self, text: &str) -> Result<(), ()> {
        self.0.write(text)
    }
}
