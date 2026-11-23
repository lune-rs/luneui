//! The browser clipboard: the page's `navigator.clipboard`, whose `readText` and `writeText` answer
//! with promises. A read settles on a later task, so the source is asynchronous (the shared
//! `clipboard.rs` holds the ordering, the cap and the wake). It needs a secure context and the
//! browser's clipboard permission: with no `navigator.clipboard`, a rejected promise or a value
//! that is not a string, a read is `Err` (the service answers `ClipboardUnavailable`), and a
//! failed write is dropped silently. The text is never logged.

use js_sys::Reflect;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::Clipboard as BrowserClipboard;

use crate::clipboard::{AsyncSource, Source};

/// The source `Clipboard::new` starts from.
pub(crate) fn source() -> Source {
    Source::Async(Box::new(Web))
}

struct Web;

/// The page's `navigator.clipboard`, or none (an insecure context has no such property).
fn browser_clipboard() -> Option<BrowserClipboard> {
    let navigator = web_sys::window()?.navigator();
    let value = Reflect::get(&navigator, &JsValue::from_str("clipboard")).ok()?;
    if value.is_undefined() || value.is_null() {
        return None;
    }
    value.dyn_into::<BrowserClipboard>().ok()
}

impl AsyncSource for Web {
    fn write(&mut self, text: &str) {
        if let Some(clipboard) = browser_clipboard() {
            // The promise is dropped: a failed write is silent, and a rejection is a handled
            // value, not an unhandled error, because `JsFuture` is never made from it.
            let _ = clipboard.write_text(text);
        }
    }

    fn read(&mut self, done: Box<dyn FnOnce(Result<String, ()>)>) {
        let Some(clipboard) = browser_clipboard() else {
            done(Err(()));
            return;
        };
        let promise = clipboard.read_text();
        spawn_local(async move {
            let reply = JsFuture::from(promise)
                .await
                .ok()
                .and_then(|value| value.as_string())
                .ok_or(());
            done(reply);
        });
    }
}
