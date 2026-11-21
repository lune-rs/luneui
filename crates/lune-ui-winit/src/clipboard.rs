//! The OS clipboard as text. [`Clipboard`] executes the frame's [`ClipboardRequest`]s: a write goes
//! to the OS, a read is answered with an event on the translator, which the next frame's input
//! carries. One per app. Text only; the core never touches the OS.
//!
//! Pasted text comes from outside the process, so it is capped at [`MAX_PASTE_BYTES`]: a longer
//! text is refused whole (a read answers `ClipboardUnavailable`, a write is dropped). Nothing is
//! ever truncated. A failed write is silent. The OS backend is opened on first use,
//! so a process that never copies or pastes opens nothing; a backend that cannot open answers
//! `ClipboardUnavailable` to a read and drops a write. [`Clipboard::new`] has no OS clipboard on iOS and
//! Android: there every read answers `ClipboardUnavailable` and every write is dropped. An iOS app
//! gives the clipboard its backend with [`Clipboard::with_backend`] (`lune_ui_platform::mobile::Pasteboard`).

use lune_ui_core::input::{ClipboardBackend, ClipboardRequest, FrameOutput};

use crate::input::Translator;
use crate::platform::Os;

/// The longest text the adapter reads from or writes to the OS clipboard, in bytes: 32 MiB, three
/// times the 11 MB document the code view measured (D213). It bounds the adapter, not the widget: a
/// text field and the text area refuse any insert over 1 MiB themselves (`text_edit::MAX_SHAPED_INSERT`, D232, D233);
/// only the code view, which draws just the lines in view, takes what the adapter lets through.
pub const MAX_PASTE_BYTES: usize = 33_554_432;

/// The old name of [`ClipboardBackend`], which moved to the core (D358). Kept so code written
/// against it still compiles.
pub use lune_ui_core::input::ClipboardBackend as Backend;

/// The app's clipboard service. See the module note.
pub struct Clipboard {
    backend: Box<dyn ClipboardBackend>,
}

impl std::fmt::Debug for Clipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clipboard").finish_non_exhaustive()
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard {
    /// The OS clipboard. Never fails and opens nothing yet.
    pub fn new() -> Self {
        Self::with_backend(Box::new(Os::default()))
    }

    /// A clipboard over `backend` instead of the OS one: a phone app's
    /// `lune_ui_platform::mobile::Pasteboard`, or a fake in a test. The cap [`MAX_PASTE_BYTES`] and
    /// the replies are the same for every backend.
    pub fn with_backend(backend: Box<dyn ClipboardBackend>) -> Self {
        Self { backend }
    }

    /// Executes `out`'s clipboard requests in order. A `Write` goes to the OS (so of two writes in
    /// a frame the last stays), unless it is over the cap. A `Read` queues exactly one reply on
    /// `translator`: the text, or `ClipboardUnavailable` when there is none, it cannot be read or
    /// it is over the cap. The reply reaches the app in the translator's next `take`.
    pub fn service(&mut self, out: &FrameOutput, translator: &mut Translator) {
        for request in &out.clipboard {
            match request {
                ClipboardRequest::Write(text) => {
                    if text.len() <= MAX_PASTE_BYTES {
                        let _ = self.backend.write(text);
                    }
                }
                ClipboardRequest::Read => match self.backend.read() {
                    Ok(text) if text.len() <= MAX_PASTE_BYTES => translator.clipboard_text(&text),
                    _ => translator.clipboard_unavailable(),
                },
            }
        }
    }
}
