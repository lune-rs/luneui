//! The desktop OS clipboard: `arboard`, opened on first use.

use lune_ui_core::input::ClipboardBackend;

use crate::clipboard::Source;

/// The source `Clipboard::new` starts from.
pub(crate) fn source() -> Source {
    Source::Sync(Box::new(Os::default()))
}

/// `arboard`, opened on first use.
#[derive(Default)]
pub(crate) struct Os {
    inner: Option<arboard::Clipboard>,
}

impl Os {
    fn open(&mut self) -> Result<&mut arboard::Clipboard, ()> {
        if self.inner.is_none() {
            self.inner = Some(arboard::Clipboard::new().map_err(|_| ())?);
        }
        self.inner.as_mut().ok_or(())
    }
}

impl ClipboardBackend for Os {
    fn read(&mut self) -> Result<String, ()> {
        // `ContentNotAvailable` (no text on it) and every other error are the same answer.
        self.open()?.get_text().map_err(|_| ())
    }

    fn write(&mut self, text: &str) -> Result<(), ()> {
        self.open()?.set_text(text).map_err(|_| ())
    }
}
