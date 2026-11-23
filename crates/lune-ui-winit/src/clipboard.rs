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
//!
//! In a browser [`Clipboard::new`] is the page's asynchronous clipboard (`navigator.clipboard`): a
//! read settles some time after the frame that asked, so its reply is queued on the translator by
//! a later [`Clipboard::service`] call and the app is woken through [`Clipboard::with_wake`] to
//! run one. Every read gets exactly one reply, in request order, whatever order the browser
//! settles them in; the cap above applies to the asynchronous text too.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use lune_ui_core::input::{ClipboardBackend, ClipboardRequest, FrameOutput};

use crate::input::Translator;

/// The longest text the adapter reads from or writes to the OS clipboard, in bytes: 32 MiB, three
/// times the 11 MB document the code view measured (D213). It bounds the adapter, not the widget: a
/// text field and the text area refuse any insert over 1 MiB themselves (`text_edit::MAX_SHAPED_INSERT`, D232, D233);
/// only the code view, which draws just the lines in view, takes what the adapter lets through.
pub const MAX_PASTE_BYTES: usize = 33_554_432;

/// The old name of [`ClipboardBackend`], which moved to the core (D358). Kept so code written
/// against it still compiles.
pub use lune_ui_core::input::ClipboardBackend as Backend;

/// A clipboard whose read answers later: the browser's. Crate-private; an app that wants its own
/// source gives a synchronous [`ClipboardBackend`] to [`Clipboard::with_backend`].
#[allow(dead_code)] // built on the browser (and in the tests) only
pub(crate) trait AsyncSource {
    /// Writes `text`; a failure is silent.
    fn write(&mut self, text: &str);
    /// Starts a read and calls `done` once with the text or `Err`, now or later.
    fn read(&mut self, done: Box<dyn FnOnce(Result<String, ()>)>);
}

/// Where the clipboard's text lives: a synchronous backend, or an asynchronous source.
#[allow(dead_code)] // `Async` is built on the browser (and in the tests) only
pub(crate) enum Source {
    Sync(Box<dyn ClipboardBackend>),
    Async(Box<dyn AsyncSource>),
}

/// The reads asked and not yet delivered, in request order. A slot is `None` until its source
/// settles it; only a settled front slot is delivered, so a reply never overtakes an older one.
#[derive(Default)]
struct Replies {
    next: u64,
    slots: VecDeque<(u64, Option<Result<String, ()>>)>,
    /// Set while `service` runs: a reply settled inside it is delivered by that same call.
    servicing: bool,
}

impl Replies {
    /// A new slot for a read, and its number.
    fn open(&mut self) -> u64 {
        let seq = self.next;
        self.next += 1;
        self.slots.push_back((seq, None));
        seq
    }

    /// Settles slot `seq` (an unknown number is ignored) and says whether the front slot can now
    /// be delivered outside a `service` call, that is whether the app needs a frame.
    fn fill(&mut self, seq: u64, reply: Result<String, ()>) -> bool {
        if let Some(slot) = self.slots.iter_mut().find(|(n, _)| *n == seq) {
            slot.1 = Some(reply);
        }
        !self.servicing && matches!(self.slots.front(), Some((_, Some(_))))
    }

    /// Pops the settled slots from the front.
    fn take_ready(&mut self) -> Vec<Result<String, ()>> {
        let mut ready = Vec::new();
        while matches!(self.slots.front(), Some((_, Some(_)))) {
            if let Some((_, Some(reply))) = self.slots.pop_front() {
                ready.push(reply);
            }
        }
        ready
    }
}

/// Queues `replies` on `translator`: the text, or `ClipboardUnavailable` for an error or a text
/// over the cap. The one place every reply passes, so the cap holds for every source.
fn deliver(replies: Vec<Result<String, ()>>, translator: &mut Translator) -> usize {
    let n = replies.len();
    for reply in replies {
        match reply {
            Ok(text) if text.len() <= MAX_PASTE_BYTES => translator.clipboard_text(&text),
            _ => translator.clipboard_unavailable(),
        }
    }
    n
}

/// The app's clipboard service. See the module note.
pub struct Clipboard {
    source: Source,
    replies: Rc<RefCell<Replies>>,
    wake: Rc<dyn Fn()>,
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
    /// The OS clipboard (in a browser, the page's asynchronous one). Never fails and opens
    /// nothing yet.
    pub fn new() -> Self {
        Self::with_source(crate::platform::clipboard_source())
    }

    /// A clipboard over `backend` instead of the OS one: a phone app's
    /// `lune_ui_platform::mobile::Pasteboard`, or a fake in a test. The cap [`MAX_PASTE_BYTES`] and
    /// the replies are the same for every backend.
    pub fn with_backend(backend: Box<dyn ClipboardBackend>) -> Self {
        Self::with_source(Source::Sync(backend))
    }

    pub(crate) fn with_source(source: Source) -> Self {
        Self {
            source,
            replies: Rc::new(RefCell::new(Replies::default())),
            wake: Rc::new(|| {}),
        }
    }

    /// Calls `wake` when a reply completes after [`Clipboard::service`] returned, and once more
    /// when a later `service` has queued it on the translator, so the app runs a frame to take it.
    /// The browser's clipboard needs it (a read settles on a later task); a synchronous backend
    /// (the OS, a phone's pasteboard, a `with_backend` one) answers inside `service`, so `wake`
    /// is never called for it. `wake` runs on the caller's thread and must only ask for a frame
    /// (a window's `request_redraw`, an event-loop proxy's `send_event`).
    #[must_use]
    pub fn with_wake(mut self, wake: impl Fn() + 'static) -> Self {
        self.wake = Rc::new(wake);
        self
    }

    /// Executes `out`'s clipboard requests in order. A `Write` goes to the OS (so of two writes in
    /// a frame the last stays), unless it is over the cap. A `Read` queues exactly one reply on
    /// `translator`: the text, or `ClipboardUnavailable` when there is none, it cannot be read or
    /// it is over the cap. The replies come in request order. The reply reaches the app in the
    /// translator's next `take`.
    pub fn service(&mut self, out: &FrameOutput, translator: &mut Translator) {
        // Replies the source settled since the last call (the browser's).
        let late = deliver(self.replies.borrow_mut().take_ready(), translator);
        self.replies.borrow_mut().servicing = true;
        for request in &out.clipboard {
            match request {
                ClipboardRequest::Write(text) => {
                    if text.len() <= MAX_PASTE_BYTES {
                        match &mut self.source {
                            Source::Sync(backend) => {
                                let _ = backend.write(text);
                            }
                            Source::Async(source) => source.write(text),
                        }
                    }
                }
                ClipboardRequest::Read => {
                    let seq = self.replies.borrow_mut().open();
                    match &mut self.source {
                        Source::Sync(backend) => {
                            let reply = backend.read();
                            self.replies.borrow_mut().fill(seq, reply);
                        }
                        Source::Async(source) => {
                            // No borrow of `replies` is held across `read`: a source may answer
                            // at once.
                            let (replies, wake) = (self.replies.clone(), self.wake.clone());
                            source.read(Box::new(move |reply| {
                                let ready = replies.borrow_mut().fill(seq, reply);
                                if ready {
                                    wake();
                                }
                            }));
                        }
                    }
                }
            }
        }
        // Replies settled inside this call land in this same call, as a synchronous read always did.
        let now = {
            let mut replies = self.replies.borrow_mut();
            replies.servicing = false;
            replies.take_ready()
        };
        deliver(now, translator);
        if late > 0 {
            // They wait on the translator for a frame's `take`.
            (self.wake)();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use lune_ui_core::input::Event;
    use winit::dpi::PhysicalSize;

    type Done = Box<dyn FnOnce(Result<String, ()>)>;

    /// An asynchronous source that holds its `done` callbacks until the test settles them.
    #[derive(Default)]
    struct Held {
        pending: Vec<Done>,
        writes: Vec<String>,
        /// Answers inside `read` instead of holding.
        at_once: Option<Result<String, ()>>,
    }

    struct Fake(Rc<RefCell<Held>>);

    impl AsyncSource for Fake {
        fn write(&mut self, text: &str) {
            self.0.borrow_mut().writes.push(text.to_owned());
        }

        fn read(&mut self, done: Done) {
            let at_once = self.0.borrow().at_once.clone();
            match at_once {
                Some(reply) => done(reply),
                None => self.0.borrow_mut().pending.push(done),
            }
        }
    }

    struct Bench {
        clipboard: Clipboard,
        translator: Translator,
        held: Rc<RefCell<Held>>,
        wakes: Rc<Cell<u32>>,
    }

    fn bench() -> Bench {
        let held = Rc::new(RefCell::new(Held::default()));
        let wakes = Rc::new(Cell::new(0));
        let w = wakes.clone();
        Bench {
            clipboard: Clipboard::with_source(Source::Async(Box::new(Fake(held.clone()))))
                .with_wake(move || w.set(w.get() + 1)),
            translator: Translator::new(PhysicalSize::new(100, 100), 1.0),
            held,
            wakes,
        }
    }

    fn reads(n: usize) -> FrameOutput {
        FrameOutput {
            clipboard: vec![ClipboardRequest::Read; n],
            ..FrameOutput::default()
        }
    }

    impl Bench {
        /// Settles the `i`th held read.
        fn settle(&mut self, i: usize, reply: Result<String, ()>) {
            let done = self.held.borrow_mut().pending.remove(i);
            done(reply);
        }

        fn replies(&mut self) -> Vec<Option<String>> {
            self.translator
                .take(std::time::Duration::ZERO)
                .events
                .into_iter()
                .filter_map(|e| match e {
                    Event::ClipboardText(t) => Some(Some(t)),
                    Event::ClipboardUnavailable => Some(None),
                    _ => None,
                })
                .collect()
        }
    }

    #[test]
    fn reads_settled_in_reverse_order_reply_in_request_order() {
        let mut b = bench();
        b.clipboard.service(&reads(2), &mut b.translator);
        assert!(b.replies().is_empty());
        b.settle(1, Ok("second".into()));
        assert_eq!(
            b.wakes.get(),
            0,
            "the newer reply cannot overtake the older"
        );
        b.clipboard.service(&reads(0), &mut b.translator);
        assert!(b.replies().is_empty());
        b.settle(0, Ok("first".into()));
        assert_eq!(b.wakes.get(), 1);
        b.clipboard.service(&reads(0), &mut b.translator);
        assert_eq!(b.replies(), [Some("first".into()), Some("second".into())]);
    }

    #[test]
    fn a_late_reply_lands_on_the_next_service_and_wakes_once() {
        let mut b = bench();
        b.clipboard.service(&reads(1), &mut b.translator);
        assert_eq!(b.wakes.get(), 0);
        b.settle(0, Ok("late".into()));
        assert_eq!(
            b.wakes.get(),
            1,
            "the settled reply asks for a frame to run `service`"
        );
        assert!(
            b.replies().is_empty(),
            "nothing reaches the translator before `service`"
        );
        b.clipboard.service(&reads(0), &mut b.translator);
        assert_eq!(
            b.wakes.get(),
            2,
            "`service` wakes once for the frame that takes it"
        );
        assert_eq!(b.replies(), [Some("late".into())]);
        b.clipboard.service(&reads(0), &mut b.translator);
        assert_eq!(b.wakes.get(), 2, "a reply is delivered once");
        assert!(b.replies().is_empty());
    }

    #[test]
    fn an_over_cap_async_text_answers_unavailable_whole() {
        let mut b = bench();
        b.clipboard.service(&reads(2), &mut b.translator);
        b.settle(0, Ok("x".repeat(MAX_PASTE_BYTES + 1)));
        b.settle(0, Ok("x".repeat(MAX_PASTE_BYTES)));
        b.clipboard.service(&reads(0), &mut b.translator);
        let replies = b.replies();
        assert_eq!(replies.len(), 2);
        assert_eq!(replies[0], None);
        assert_eq!(replies[1].as_ref().map(String::len), Some(MAX_PASTE_BYTES));
    }

    #[test]
    fn an_over_cap_write_is_dropped_and_a_rejected_read_is_unavailable() {
        let mut b = bench();
        let out = FrameOutput {
            clipboard: vec![
                ClipboardRequest::Write("x".repeat(MAX_PASTE_BYTES + 1)),
                ClipboardRequest::Write("kept".into()),
                ClipboardRequest::Read,
            ],
            ..FrameOutput::default()
        };
        b.clipboard.service(&out, &mut b.translator);
        assert_eq!(b.held.borrow().writes, ["kept"]);
        b.settle(0, Err(()));
        b.clipboard.service(&reads(0), &mut b.translator);
        assert_eq!(b.replies(), [None]);
    }

    #[test]
    fn a_source_that_answers_inside_read_does_not_panic_and_lands_in_the_same_call() {
        let mut b = bench();
        b.held.borrow_mut().at_once = Some(Ok("now".into()));
        b.clipboard.service(&reads(2), &mut b.translator);
        assert_eq!(b.replies(), [Some("now".into()), Some("now".into())]);
        assert_eq!(
            b.wakes.get(),
            0,
            "a reply delivered by the call that asked needs no frame"
        );
        b.held.borrow_mut().at_once = Some(Err(()));
        b.clipboard.service(&reads(1), &mut b.translator);
        assert_eq!(b.replies(), [None]);
    }
}
