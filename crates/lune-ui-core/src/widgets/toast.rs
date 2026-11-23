//! Toasts and the notification history (`overlay.toast-notifications`, D146).
//!
//! The app owns a [`Toasts`] model and pushes into it from anywhere; [`Ui::toasts`] draws it
//! straight on the toast layer and [`Ui::toast_history`] draws the archive. Time is the app's:
//! every rule reads the `now` the app passes or `ui.time()`.
//!
//! Rules (the shipped app's): at most 3 toasts show; a new one evicts the oldest that is not
//! loading and has no actions, without archiving it, and when none qualifies the list grows. An
//! info toast times out after 4 s, a success after 3 s, a warning after 6 s; an error, a loading
//! toast and a toast with actions never do. A deadline that passes while the toast is expanded
//! waits for the collapse. A toast that leaves (a timer, Dismiss, an action, [`Toasts::dismiss`])
//! is archived once at the front of the history, which keeps 50, and counts as unread. An evicted
//! toast leaves without being archived. The shipped app drops the newest history entry when the
//! archive is full, which reads as a slip; this one drops the oldest.
//!
//! Drawing: the stack sits bottom right (margin 16, gap 8, newest at the bottom), 320 wide or 720
//! expanded, without an overlay record and without a focus scope: a toast opened by an overlay
//! must not take Tab or Escape from the dialog under it, so only its buttons are tab stops, of the
//! scope the app called [`Ui::toasts`] in. A press on the body toggles the log; Dismiss and Cancel
//! do not. [`Ui::toasts_with`] places the stack in an area of the app's instead of the window. Motion (none in the shipped app, so D40's kinds at their defaults): appear is opacity
//! and a rise of 8 px, leave is opacity, a neighbour's move after an arrival or a removal is one
//! retargeted offset, and the expand animates the height with the text wrapped once at the target
//! width. Reduced fades instead of moving, Off snaps.
//!
//! A notice is a toast pushed with [`ToastExtras`] ([`Toasts::push_with`]); a toast pushed with
//! [`Toasts::push`] behaves exactly as above and nothing below applies to it. A notice may add a
//! header row (a source, an age, the buttons) and a title line, a glyph and an accent of its own, a
//! timeout of its own that applies with actions too, a drain line that shows the time left, and a
//! body press that reports an action instead of toggling the log. While the pointer (not a finger)
//! rests on a notice its timer is held and resumes with the time that was left. A notice with no
//! body action and no log has an inert body: a press on it does nothing, and its timer runs.
//!
//! The app can ask where the toasts are: [`Toasts::live`] and [`Toasts::contains`] read the rects
//! of the last [`Ui::toasts_with`] call, [`Toasts::is_live`] asks the model whether a toast has not
//! started leaving. Eviction is unchanged: a toast with actions is never evicted, so toasts with
//! actions stack without bound unless the app caps them.

use std::collections::VecDeque;
use std::hash::Hash;
use std::time::Duration;

use super::button::{self, ButtonKind, ButtonLook, FOCUS, HOVER, IconButton, PRESS};
use super::popover::glass_surface;
use super::progress::{self, ProgressOptions};
use super::scroll_area::{Bars, Extent, ScrollOptions};
use super::spinner::{self, SpinnerOptions};
use super::tone::Tone;
use crate::access::{Action, Live, Node, Role, State};
use crate::color::Rgba;
use crate::icon;
use crate::input::{CursorIcon, Event, NamedKey, PointerKind};
use crate::motion::{Kind, Motion, MotionTier, Time};
use crate::paint::{Painter, Rect, Xform};
use crate::scene::LayerId;
use crate::text::{Spec, Style};
use crate::theme::Theme;
use crate::ui::{Sense, Ui};

/// A toast appearing: opacity 0 to 1 and a rise from [`APPEAR_RISE`] px below.
pub const APPEAR: Motion = Motion::of(Kind::Appear);
/// A toast leaving: opacity 1 to 0.
pub const LEAVE: Motion = Motion::of(Kind::Dismiss);
/// A toast moving after an arrival or a removal above or below it.
pub const SHIFT: Motion = Motion::of(Kind::Move);
/// A toast growing or shrinking to show or hide its log.
pub const EXPAND: Motion = Motion::of(Kind::Expand);
/// How far below its place a toast starts.
pub const APPEAR_RISE: f32 = 8.0;

/// How many toasts show at once.
pub const MAX_VISIBLE: usize = 3;
/// How many archived toasts the history keeps.
pub const HISTORY_CAP: usize = 50;
/// When an info, a success and a warning toast time out.
pub const INFO_TIMEOUT: Duration = Duration::from_secs(4);
pub const SUCCESS_TIMEOUT: Duration = Duration::from_secs(3);
pub const WARNING_TIMEOUT: Duration = Duration::from_secs(6);

/// The collapsed and the expanded width.
pub const WIDTH: f32 = 320.0;
pub const WIDTH_EXPANDED: f32 = 720.0;
/// The distance from the window's right and bottom edge, and between toasts.
pub const MARGIN: f32 = 16.0;
pub const GAP: f32 = 8.0;
/// The corner radius, the border and the padding inside it.
pub const RADIUS: f32 = 8.0;
pub const BORDER: f32 = 1.0;
pub const PAD: f32 = 12.0;
/// The kind glyph and the gap after it; the detail's, the actions' and the log's left inset is
/// the glyph plus its gap.
pub const ICON: f32 = 16.0;
pub const ICON_GAP: f32 = 8.0;
pub const TEXT_INSET: f32 = ICON + ICON_GAP;
/// The Dismiss and Cancel glyph, the button around it (glyph plus 4 px each side), its corner
/// radius, and the space before Cancel and before Dismiss.
pub const BUTTON_GLYPH: f32 = 12.0;
pub const BUTTON: f32 = 20.0;
pub const BUTTON_RADIUS: f32 = 4.0;
pub const CANCEL_MARGIN: f32 = 8.0;
pub const DISMISS_MARGIN: f32 = 4.0;
/// The space above the detail, above the progress bar, the log block and the actions.
pub const DETAIL_TOP: f32 = 4.0;
pub const SECTION_TOP: f32 = 8.0;
/// The log block: its height, its padding, its corner radius and a line.
pub const LOG_HEIGHT: f32 = 200.0;
pub const LOG_PAD: f32 = 8.0;
pub const LOG_RADIUS: f32 = 4.0;
pub const LOG_LINE: f32 = 14.0;
/// An action button's padding, horizontal then vertical, and the space between buttons.
pub const ACTION_PAD: (f32, f32) = (16.0, 8.0);
pub const ACTION_GAP: f32 = 8.0;
/// A notice's drain line: its height along the card's bottom edge.
pub const DRAIN: f32 = 2.0;
/// The space between a notice's title and its message, and between the header's texts.
pub const TITLE_GAP: f32 = 2.0;
pub const HEADER_GAP: f32 = 8.0;

/// The history panel: the space above the header, its row, the space below it and above the list
/// (the shipped panel's margins), the clear-all button, its glyph, and a row's padding, margin and
/// glyph.
pub const HISTORY_HEADER_TOP: f32 = 5.0;
pub const HISTORY_HEADER: f32 = 24.0;
pub const HISTORY_HEADER_BOTTOM: f32 = 4.0;
pub const HISTORY_LIST_TOP: f32 = 4.0;
pub const CLEAR_GLYPH: f32 = 14.0;
pub const HISTORY_PAD: (f32, f32) = (8.0, 6.0);
pub const HISTORY_MARGIN: f32 = 1.0;
pub const HISTORY_ICON: f32 = 14.0;
pub const HISTORY_GAP: f32 = 8.0;
pub const HISTORY_DETAIL_INSET: f32 = 22.0;

/// A toast's identity, stable from [`Toasts::push`] until it is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ToastId(pub u64);

/// What a toast says it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
    /// A running operation: a turning glyph and a bar, determinate with a fraction or not.
    Loading {
        /// The fraction 0 to 1; `None` is an indeterminate bar.
        progress: Option<f32>,
        /// Shows the Cancel button.
        cancellable: bool,
    },
}

/// An action button of a toast (a prompt): the app's label and its own number for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastAction {
    pub label: String,
    /// The look: the filled `surface_input` button; the others are ghosts. Position is the app's order.
    pub primary: bool,
    pub id: u64,
}

/// What a toast shows. Texts are the app's.
#[derive(Debug, Clone, PartialEq)]
pub struct ToastSpec {
    pub kind: ToastKind,
    pub message: String,
    pub detail: Option<String>,
    /// The lines of the log block an expanded toast shows.
    pub log: Vec<String>,
    pub actions: Vec<ToastAction>,
}

impl ToastSpec {
    /// A toast of `kind` that says `message`, with no detail, log or actions.
    pub fn new(kind: ToastKind, message: impl Into<String>) -> Self {
        ToastSpec {
            kind,
            message: message.into(),
            detail: None,
            log: Vec::new(),
            actions: Vec::new(),
        }
    }

    /// How long this toast lives, or `None` when it stays until it is dismissed.
    fn timeout(&self) -> Option<Duration> {
        if !self.actions.is_empty() {
            return None;
        }
        match self.kind {
            ToastKind::Info => Some(INFO_TIMEOUT),
            ToastKind::Success => Some(SUCCESS_TIMEOUT),
            ToastKind::Warning => Some(WARNING_TIMEOUT),
            ToastKind::Error | ToastKind::Loading { .. } => None,
        }
    }

    /// Whether an eviction may take this toast: not loading, no actions.
    fn evictable(&self) -> bool {
        !matches!(self.kind, ToastKind::Loading { .. }) && self.actions.is_empty()
    }
}

/// What a notice adds to a [`ToastSpec`] ([`Toasts::push_with`]). Build it with [`Self::new`] and
/// the `with_*` methods; new fields stay additive.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct ToastExtras {
    /// The header row's text: who the notice is from. A header row (the glyph, this, the age and
    /// the buttons) shows only when this is `Some`.
    pub source: Option<String>,
    /// The age text, right-aligned in the header row; the app writes it and refreshes it with
    /// [`Toasts::update_with`]. Shown only with a `source`.
    pub age: Option<String>,
    /// A bold line above the message, one line, elided. It names the toast's node, and the message
    /// describes it.
    pub title: Option<String>,
    /// The glyph for the kind's own (also for a loading toast, which then shows no spinner).
    pub glyph: Option<&'static str>,
    /// The colour of the glyph and the drain line, for the kind's own.
    pub accent: Option<Rgba>,
    /// How long the toast lives, for the kind's rule; it applies with actions too. `None` keeps
    /// the kind's rule.
    pub timeout: Option<Duration>,
    /// Draws the [`DRAIN`] line along the card's bottom edge while a timeout runs (Full tier).
    pub drain: bool,
    /// A press on the body (not on a button) reports `ToastResponse::action` with this number and
    /// the toast leaves, as an action button does, instead of toggling the log.
    pub body_action: Option<u64>,
}

impl ToastExtras {
    /// No extras: a toast pushed with it is a notice with the kind's own look and timeout, and an
    /// inert body unless it has a log.
    pub fn new() -> Self {
        Self::default()
    }

    /// The header row's text.
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// The age text of the header row.
    #[must_use]
    pub fn with_age(mut self, age: impl Into<String>) -> Self {
        self.age = Some(age.into());
        self
    }

    /// The title line.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The glyph, for the kind's own.
    #[must_use]
    pub fn with_glyph(mut self, glyph: &'static str) -> Self {
        self.glyph = Some(glyph);
        self
    }

    /// The accent colour, for the kind's own.
    #[must_use]
    pub fn with_accent(mut self, accent: Rgba) -> Self {
        self.accent = Some(accent);
        self
    }

    /// The timeout, for the kind's rule.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Whether the drain line shows.
    #[must_use]
    pub fn with_drain(mut self, drain: bool) -> Self {
        self.drain = drain;
        self
    }

    /// The number a press on the body reports.
    #[must_use]
    pub fn with_body_action(mut self, id: u64) -> Self {
        self.body_action = Some(id);
        self
    }
}

/// Where [`Ui::toasts_with`] puts the stack. Build it with [`Self::new`] and the `with_*` methods.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct ToastStackOptions {
    /// The rect the stack sits at the bottom right of; `None` is the overlay bounds (the window).
    pub area: Option<Rect>,
    /// The distance from the area's right and bottom edge.
    pub margin: f32,
}

impl Default for ToastStackOptions {
    fn default() -> Self {
        ToastStackOptions {
            area: None,
            margin: MARGIN,
        }
    }
}

impl ToastStackOptions {
    /// The window's bottom right with [`MARGIN`].
    pub fn new() -> Self {
        Self::default()
    }

    /// The area the stack sits in.
    #[must_use]
    pub fn with_area(mut self, area: Rect) -> Self {
        self.area = Some(area);
        self
    }

    /// The distance from the area's edges.
    #[must_use]
    pub fn with_margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }
}

/// The timeout of a toast with `extras`: the notice's own, else the kind's rule.
fn timeout_of(spec: &ToastSpec, extras: Option<&ToastExtras>) -> Option<Duration> {
    extras.and_then(|x| x.timeout).or_else(|| spec.timeout())
}

/// When a toast times out.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Timer {
    /// Never.
    Off,
    /// It should run, from the next frame (an update turned a toast into a timed one).
    Pending,
    At(Time),
    /// The pointer rests on a notice: the time that was left, which resumes when it leaves.
    Held(Duration),
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    id: ToastId,
    spec: ToastSpec,
    /// A notice's extras; `None` for a toast pushed with [`Toasts::push`].
    extras: Option<Box<ToastExtras>>,
    timer: Timer,
    /// The timeout the timer started with, the drain line's whole.
    total: Option<Duration>,
    expanded: bool,
    /// Fading out; already archived (or evicted), gone once the fade ends.
    leaving: bool,
}

/// The toasts an app has up and the archive of those that left. The app owns it, pushes from
/// anywhere and hands it to [`Ui::toasts`] and [`Ui::toast_history`] every frame.
#[derive(Debug, Clone, Default)]
pub struct Toasts {
    entries: Vec<Entry>,
    history: VecDeque<ToastSpec>,
    next: u64,
    unread: usize,
    /// The live toasts of the last `toasts` call and the rects they take input in.
    live: Vec<(ToastId, Rect)>,
}

impl Toasts {
    /// Shows `spec` at the bottom of the stack. While 3 toasts show, the oldest that is not
    /// loading and has no actions leaves first (not archived); when none qualifies, the stack
    /// grows. The timer of a timed toast starts at `now`.
    pub fn push(&mut self, spec: ToastSpec, now: Time) -> ToastId {
        self.push_entry(spec, None, now)
    }

    /// [`Self::push`] for a notice: `extras` adds the header, title, glyph, accent, timeout, drain
    /// and body action (see the module docs). The eviction rule reads only `spec`.
    pub fn push_with(&mut self, spec: ToastSpec, extras: ToastExtras, now: Time) -> ToastId {
        self.push_entry(spec, Some(Box::new(extras)), now)
    }

    fn push_entry(
        &mut self,
        spec: ToastSpec,
        extras: Option<Box<ToastExtras>>,
        now: Time,
    ) -> ToastId {
        while self.entries.iter().filter(|e| !e.leaving).count() >= MAX_VISIBLE {
            let Some(oldest) = self
                .entries
                .iter_mut()
                .find(|e| !e.leaving && e.spec.evictable())
            else {
                break;
            };
            oldest.leaving = true;
            oldest.timer = Timer::Off;
        }
        self.next += 1;
        let id = ToastId(self.next);
        let timeout = timeout_of(&spec, extras.as_deref());
        let timer = timeout.map_or(Timer::Off, |d| Timer::At(now + d));
        self.entries.push(Entry {
            id,
            spec,
            extras,
            timer,
            total: timeout,
            expanded: false,
            leaving: false,
        });
        id
    }

    /// Replaces what toast `id` shows (a loading toast that finished, a new message). A toast
    /// that was not timed and now is starts its timer at the next frame; one already timed keeps
    /// its deadline; one that is not timed any more loses it. Does nothing once the toast is
    /// leaving or gone.
    ///
    /// A notice keeps its extras; a held one stays held with the time it has left.
    pub fn update(&mut self, id: ToastId, spec: ToastSpec) {
        self.update_inner(id, spec, None);
    }

    /// [`Self::update`] with new `extras` (a new age text, a new title). The timer follows the
    /// same rule: a timed toast keeps its deadline and its drain line's whole (a fresh timeout is
    /// [`Self::dismiss`] and a new push), a held one stays held, one that becomes timed starts at
    /// the next frame, and one that is not timed any more loses it.
    pub fn update_with(&mut self, id: ToastId, spec: ToastSpec, extras: ToastExtras) {
        self.update_inner(id, spec, Some(extras));
    }

    fn update_inner(&mut self, id: ToastId, spec: ToastSpec, extras: Option<ToastExtras>) {
        let Some(e) = self.entries.iter_mut().find(|e| e.id == id && !e.leaving) else {
            return;
        };
        if let Some(x) = extras {
            e.extras = Some(Box::new(x));
        }
        e.timer = match (timeout_of(&spec, e.extras.as_deref()), e.timer) {
            (None, _) => {
                e.total = None;
                Timer::Off
            }
            (Some(_), Timer::At(t)) => Timer::At(t),
            (Some(_), Timer::Held(r)) => Timer::Held(r),
            (Some(_), _) => Timer::Pending,
        };
        e.spec = spec;
    }

    /// Dismisses toast `id`: it fades out and is archived. Does nothing for a toast that is
    /// leaving or gone.
    pub fn dismiss(&mut self, id: ToastId, _now: Time) {
        if let Some(i) = self.entries.iter().position(|e| e.id == id && !e.leaving) {
            self.leave(i, true);
        }
    }

    /// Whether toast `id` shows its log.
    pub fn expanded(&self, id: ToastId) -> bool {
        self.entries.iter().any(|e| e.id == id && e.expanded)
    }

    /// Expands or collapses toast `id` (a press on its body toggles it).
    pub fn set_expanded(&mut self, id: ToastId, expanded: bool) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id && !e.leaving) {
            e.expanded = expanded;
        }
    }

    /// The archived toasts, newest first.
    pub fn history(&self) -> impl Iterator<Item = &ToastSpec> {
        self.history.iter()
    }

    /// How many toasts were archived since [`Self::mark_read`] or [`Self::clear_history`].
    pub fn unread(&self) -> usize {
        self.unread
    }

    /// Sets the unread count to 0.
    pub fn mark_read(&mut self) {
        self.unread = 0;
    }

    /// Empties the history and the unread count.
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.unread = 0;
    }

    /// The toasts drawn by the last [`Ui::toasts`] or [`Ui::toasts_with`] call that had not started
    /// leaving, top to bottom, each with the window-space rect its body takes input in (a press
    /// anywhere on the card, a button too, is inside it). Valid until the next call; empty before
    /// the first one.
    pub fn live(&self) -> &[(ToastId, Rect)] {
        &self.live
    }

    /// Whether `pos` (window space) is inside a toast of [`Self::live`].
    pub fn contains(&self, pos: (f32, f32)) -> bool {
        self.live.iter().any(|(_, r)| r.contains(pos))
    }

    /// Whether toast `id` is up: true from [`Self::push`] until it starts leaving (a timeout, its
    /// x, an action, [`Self::dismiss`] or an eviction), read from the model, not the last frame.
    pub fn is_live(&self, id: ToastId) -> bool {
        self.entries.iter().any(|e| e.id == id && !e.leaving)
    }

    /// Whether no toast shows and none is still fading out.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Starts entry `i` leaving and archives its spec once when `archive`.
    fn leave(&mut self, i: usize, archive: bool) {
        let e = &mut self.entries[i];
        if e.leaving {
            return;
        }
        e.leaving = true;
        e.timer = Timer::Off;
        if archive {
            let spec = e.spec.clone();
            self.history.push_front(spec);
            self.history.truncate(HISTORY_CAP);
            self.unread += 1;
        }
    }

    /// The frame's timers at `now`: starts the pending ones and dismisses the expired ones that
    /// are not expanded.
    fn tick(&mut self, now: Time) {
        for i in 0..self.entries.len() {
            let e = &mut self.entries[i];
            if e.leaving {
                continue;
            }
            if e.timer == Timer::Pending
                && let Some(d) = timeout_of(&e.spec, e.extras.as_deref())
            {
                e.timer = Timer::At(now + d);
                e.total = Some(d);
            }
            if let Timer::At(t) = e.timer
                && t <= now
                && !e.expanded
            {
                self.leave(i, true);
            }
        }
    }

    /// The earliest deadline a frame is due at: a timed toast that is not expanded.
    fn next_deadline(&self) -> Option<Time> {
        self.entries
            .iter()
            .filter(|e| !e.leaving && !e.expanded)
            .filter_map(|e| match e.timer {
                Timer::At(t) => Some(t),
                _ => None,
            })
            .min()
    }
}

/// What [`Ui::toasts`] reports for a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToastResponse {
    /// The action button pressed: the toast and the action's own number. The toast leaves too.
    pub action: Option<(ToastId, u64)>,
    /// The toast whose Cancel button was pressed. The toast stays; the app updates it.
    pub cancelled: Option<ToastId>,
}

/// The measured parts of a toast at one width.
#[derive(Debug, Clone, PartialEq)]
struct Parts {
    width: f32,
    /// The width inside the border and the padding.
    inner: f32,
    msg_w: f32,
    msg_h: f32,
    /// A notice with a source: the header row's height and the space after it, else 0.
    head_h: f32,
    /// The title line and the space under it, else 0.
    title_h: f32,
    /// The row of the glyph, the message column and the buttons (without a header), or the
    /// message column alone (under a header).
    row: f32,
    detail_h: f32,
    bar_h: f32,
    actions_h: f32,
    action_widths: Vec<f32>,
    total: f32,
}

/// The bar is 3 px high in both states (`MainWindow.axaml:369`, `Height="3"`).
fn bar_height(_kind: ToastKind) -> f32 {
    progress::HEIGHT_INDETERMINATE
}

fn message_style(t: &Theme) -> Style {
    Style::ui(t.font_base, t.fg_primary)
}

fn detail_style(t: &Theme) -> Style {
    Style::ui(t.font_s, t.fg_secondary)
}

fn title_style(t: &Theme) -> Style {
    Style::ui(t.font_base, t.fg_bright).bold()
}

fn source_style(t: &Theme) -> Style {
    Style::ui(t.font_base, t.fg_primary).medium()
}

fn age_style(t: &Theme) -> Style {
    Style::ui(t.font_s, t.fg_muted)
}

/// Whether a notice has the header row.
fn has_header(extras: Option<&ToastExtras>) -> bool {
    extras.is_some_and(|x| x.source.is_some())
}

/// The left inset of the detail: under the message text, which starts at the glyph's right
/// unless a header row holds the glyph.
fn text_inset(header: bool) -> f32 {
    if header { 0.0 } else { TEXT_INSET }
}

fn parts<H: Copy + PartialEq>(
    p: &mut Painter<'_, H>,
    spec: &ToastSpec,
    extras: Option<&ToastExtras>,
    expanded: bool,
) -> Parts {
    let t = p.theme;
    let width = if expanded { WIDTH_EXPANDED } else { WIDTH };
    let inner = width - 2.0 * (PAD + BORDER);
    let header = has_header(extras);
    let cancel = match spec.kind {
        ToastKind::Loading {
            cancellable: true, ..
        } => CANCEL_MARGIN + BUTTON,
        _ => 0.0,
    };
    let msg_w = if header {
        inner
    } else {
        (inner - TEXT_INSET - cancel - DISMISS_MARGIN - BUTTON).max(0.0)
    };
    let msg_h = p
        .measure(&Spec::plain(spec.message.as_str(), message_style(&t)).wrapped(msg_w))
        .1;
    let title_h = extras.and_then(|x| x.title.as_deref()).map_or(0.0, |s| {
        p.measure(&Spec::plain(s, title_style(&t))).1 + TITLE_GAP
    });
    let column = title_h + msg_h;
    let head_h = if header { BUTTON + SECTION_TOP } else { 0.0 };
    let row = if header { column } else { column.max(BUTTON) };
    let detail_h = spec.detail.as_deref().map_or(0.0, |d| {
        DETAIL_TOP
            + p.measure(
                &Spec::plain(d, detail_style(&t)).wrapped((inner - text_inset(header)).max(0.0)),
            )
            .1
    });
    let bar_h = if matches!(spec.kind, ToastKind::Loading { .. }) {
        SECTION_TOP + bar_height(spec.kind)
    } else {
        0.0
    };
    let action_widths: Vec<f32> = spec
        .actions
        .iter()
        .map(|a| button::button_width(p, &a.label))
        .collect();
    let actions_h = if spec.actions.is_empty() {
        0.0
    } else {
        SECTION_TOP + action_height(&t)
    };
    let log = if expanded {
        SECTION_TOP + LOG_HEIGHT
    } else {
        0.0
    };
    Parts {
        width,
        inner,
        msg_w,
        msg_h,
        head_h,
        title_h,
        row,
        detail_h,
        bar_h,
        actions_h,
        action_widths,
        total: 2.0 * (PAD + BORDER) + head_h + row + detail_h + bar_h + log + actions_h,
    }
}

/// An action button's height: a line of the label and the padding.
fn action_height(t: &Theme) -> f32 {
    message_style(t).line + 2.0 * ACTION_PAD.1
}

/// The size of a toast with `spec` as it settles, collapsed or `expanded`. A notice's header and
/// title add to it; this is the size of a toast pushed with [`Toasts::push`].
pub fn toast_size<H: Copy + PartialEq>(
    p: &mut Painter<'_, H>,
    spec: &ToastSpec,
    expanded: bool,
) -> (f32, f32) {
    let m = parts(p, spec, None, expanded);
    (m.width, m.total)
}

fn glyph_of(kind: ToastKind) -> &'static str {
    match kind {
        ToastKind::Info => icon::INFO,
        ToastKind::Success => icon::CHECK,
        ToastKind::Warning => icon::WARNING,
        ToastKind::Error => icon::ERROR,
        ToastKind::Loading { .. } => icon::SYNC,
    }
}

fn accent_of(t: &Theme, kind: ToastKind) -> Rgba {
    match kind {
        ToastKind::Info | ToastKind::Loading { .. } => t.toast_info,
        ToastKind::Success => t.toast_success,
        ToastKind::Warning => t.toast_warning,
        ToastKind::Error => t.toast_error,
    }
}

/// What the user did to one toast this frame.
#[derive(Debug, Default)]
struct Outcome {
    toggle: bool,
    dismiss: bool,
    cancel: bool,
    action: Option<u64>,
    /// The rect the body took input in (window space), for a live toast.
    rect: Option<Rect>,
}

/// What a toast remembers between frames.
#[derive(Debug, Default)]
struct Mem {
    /// Drawn before: its appear has begun.
    seen: bool,
    /// Where its slot was last frame, and the toasts below it then.
    slot_y: Option<f32>,
    below: Vec<u64>,
}

/// What one toast's paint reads.
#[derive(Debug)]
struct ToastView<'a> {
    spec: &'a ToastSpec,
    extras: Option<&'a ToastExtras>,
    expanded: bool,
    m: &'a Parts,
    card: Rect,
    /// False while it fades out: it draws as it was, takes no input and emits no node.
    live: bool,
    /// The drain line's remaining fraction, when it shows.
    drain: Option<f32>,
}

impl<'a> Ui<'a> {
    /// Draws `toasts` on the toast layer: the stack at the window's bottom right, newest at the
    /// bottom. Applies the frame's timers from `ui.time()`, asks for a frame at the next deadline,
    /// and animates arrivals, removals and expansion. Draws on the toast layer wherever it is
    /// called, outside any overlay and focus scope, and does not take part in a surrounding
    /// [`Ui::record`].
    ///
    /// Reports the action button or Cancel pressed this frame. Dismiss and the body press act on
    /// the model themselves. A toast is a `Status` node (`Alert` for an error) named by its
    /// message and described by its detail; its default action toggles the log, `Dismiss`
    /// dismisses it. Buttons are tab stops of the scope below; nothing takes focus on its own.
    /// [`Self::toasts_with`] with the default options.
    pub fn toasts(&mut self, key: impl Hash, toasts: &mut Toasts) -> ToastResponse {
        self.toasts_with(key, toasts, &ToastStackOptions::default())
    }

    /// [`Self::toasts`] with `opts`: the stack sits at the bottom right of `opts.area` (else the
    /// window), inset by `opts.margin`. A notice (see the module docs) takes its hold, its drain
    /// line and its body action here, and the call refills [`Toasts::live`].
    pub fn toasts_with(
        &mut self,
        key: impl Hash,
        toasts: &mut Toasts,
        opts: &ToastStackOptions,
    ) -> ToastResponse {
        toasts.tick(self.time());
        let mut resp = ToastResponse::default();
        let saved = self.painter().save_state();
        let paused = self.painter().pause_recording();
        self.painter().enter(LayerId::Toast, 0);
        if !self.is_enabled() {
            self.painter().push_xform(Xform {
                origin: (0.0, 0.0),
                scale: 1.0,
                dx: 0.0,
                dy: 0.0,
                alpha: crate::ui::DISABLED_ALPHA,
            });
        }
        self.scope(key, |ui| ui.toast_stack(toasts, opts, &mut resp));
        self.painter().resume_recording(paused);
        self.painter().restore_state(saved);
        if let Some(t) = toasts.next_deadline() {
            self.request_repaint_at(t);
        }
        resp
    }

    fn toast_stack(
        &mut self,
        toasts: &mut Toasts,
        opts: &ToastStackOptions,
        resp: &mut ToastResponse,
    ) {
        let tier = self.tier();
        let area = opts.area.unwrap_or_else(|| self.overlay_bounds());
        let margin = opts.margin;
        let now = self.time();
        let finger = self.pointer_kind() == PointerKind::Touch;
        let pointer = self.pointer_pos();
        // A toast that left before it was ever drawn has nothing to fade.
        let unseen: Vec<ToastId> = toasts
            .entries
            .iter()
            .filter(|e| e.leaving)
            .map(|e| e.id)
            .filter(|id| {
                let mem_id = self.id(id.0).with("mem");
                !self.transient::<Mem>(mem_id).seen
            })
            .collect();
        toasts.entries.retain(|e| !unseen.contains(&e.id));

        // Measure, and settle each height on its own transition.
        let mut sizes = Vec::with_capacity(toasts.entries.len());
        for e in &toasts.entries {
            let m = parts(self.painter(), &e.spec, e.extras.as_deref(), e.expanded);
            let h_id = self.id(e.id.0).with("h");
            let h = self.animate_f32(h_id, m.total, &EXPAND);
            sizes.push((m, h));
        }
        // Lay out from the bottom edge up.
        let mut bottom = area.bottom() - margin;
        let mut slots = vec![0.0f32; sizes.len()];
        for (slot, (_, h)) in slots.iter_mut().zip(&sizes).rev() {
            *slot = bottom - h;
            bottom = *slot - GAP;
        }

        let appear = self
            .theme()
            .motion
            .resolve(&APPEAR, tier)
            .is_some_and(|r| r.transforms);
        let mut outcomes: Vec<(ToastId, Outcome)> = Vec::new();
        let mut holds: Vec<(ToastId, bool)> = Vec::new();
        let mut finished: Vec<ToastId> = Vec::new();
        for i in 0..toasts.entries.len() {
            let (id, leaving, expanded) = {
                let e = &toasts.entries[i];
                (e.id, e.leaving, e.expanded)
            };
            let (m, h) = &sizes[i];
            let slot_y = slots[i];
            let eid = self.id(id.0);
            let below: Vec<u64> = toasts.entries[i + 1..].iter().map(|e| e.id.0).collect();
            let mem_id = eid.with("mem");
            let (seen, prev_slot, prev_below) = {
                let mem = self.transient::<Mem>(mem_id);
                (mem.seen, mem.slot_y, std::mem::take(&mut mem.below))
            };
            // Opacity and the rise: a toast starts invisible and low; one that leaves fades from
            // where it is.
            let (alpha_id, rise_id, shift_id) =
                (eid.with("alpha"), eid.with("rise"), eid.with("shift"));
            if !seen {
                self.snap_f32(alpha_id, 0.0);
                if appear {
                    self.snap_f32(rise_id, APPEAR_RISE);
                }
            }
            let alpha = if leaving {
                self.animate_f32(alpha_id, 0.0, &LEAVE)
            } else {
                self.animate_f32(alpha_id, 1.0, &APPEAR)
            };
            let rise = if appear {
                self.animate_f32(rise_id, 0.0, &APPEAR)
            } else {
                0.0
            };
            // A slot that changed because a toast below it arrived or went keeps the toast where
            // it is and moves it to its new place: the offset takes the difference and settles.
            let mut shift = self.animate_f32(shift_id, 0.0, &SHIFT);
            if let Some(prev) = prev_slot
                && prev_below != below
            {
                self.snap_f32(shift_id, shift + prev - slot_y);
                shift = self.animate_f32(shift_id, 0.0, &SHIFT);
            }
            {
                let mem = self.transient::<Mem>(mem_id);
                mem.seen = true;
                mem.slot_y = Some(slot_y);
                mem.below = below;
            }
            if leaving && alpha <= 0.0 {
                finished.push(id);
                continue;
            }
            let card = Rect::new(area.right() - margin - m.width, slot_y, m.width, *h);
            let entry = &toasts.entries[i];
            let extras = entry.extras.as_deref();
            // The hold reads the pointer against the settled card, not the one still rising or
            // sliding, and only a mouse holds.
            if extras.is_some() && !leaving {
                let over = !finger && pointer.is_some_and(|q| card.contains(q));
                holds.push((id, over));
            }
            let drain = match extras {
                Some(x) if x.drain && tier == MotionTier::Full && !leaving => {
                    drain_fraction(entry, now)
                }
                _ => None,
            };
            // A running line moves with the time: ask for frames at the busy cadence until the
            // deadline, which the repaint request at that time covers. A held line is still.
            if drain.is_some() && matches!(entry.timer, Timer::At(t) if t > now) {
                self.request_busy_frame(card);
            }
            self.painter().push_xform(Xform {
                origin: (0.0, 0.0),
                scale: 1.0,
                dx: 0.0,
                dy: shift + rise,
                alpha,
            });
            let view = ToastView {
                spec: &entry.spec,
                extras,
                expanded,
                m,
                card,
                live: !leaving,
                drain,
            };
            let outcome = self.scope(id.0, |ui| ui.paint_toast(&view));
            self.painter().pop_xform();
            outcomes.push((id, outcome));
        }

        // A notice's timer holds while the pointer rests on it and resumes with what was left.
        for (id, over) in holds {
            if let Some(e) = toasts.entries.iter_mut().find(|e| e.id == id && !e.leaving) {
                e.timer = match (over, e.timer) {
                    (true, Timer::At(t)) => Timer::Held(t.saturating_sub(now)),
                    (false, Timer::Held(r)) => Timer::At(now + r),
                    (_, timer) => timer,
                };
            }
        }

        let mut rects: Vec<(ToastId, Rect)> = Vec::new();
        for (id, o) in outcomes {
            let Some(i) = toasts.entries.iter().position(|e| e.id == id) else {
                continue;
            };
            if let Some(r) = o.rect {
                rects.push((id, r));
            }
            if o.toggle {
                toasts.entries[i].expanded = !toasts.entries[i].expanded;
            }
            if o.cancel && resp.cancelled.is_none() {
                resp.cancelled = Some(id);
            }
            if let Some(a) = o.action
                && resp.action.is_none()
            {
                resp.action = Some((id, a));
            }
            if o.dismiss || o.action.is_some() {
                toasts.leave(i, true);
            }
            if o.toggle || o.dismiss || o.cancel || o.action.is_some() {
                self.request_catch_up();
            }
        }
        toasts.entries.retain(|e| !finished.contains(&e.id));
        rects.retain(|(id, _)| toasts.is_live(*id));
        toasts.live = rects;
    }

    /// One toast at `card`. `live` is false while it fades out: it draws as it was, takes no
    /// input and emits no node.
    fn paint_toast(&mut self, v: &ToastView<'_>) -> Outcome {
        let eid = self.scope_id();
        let mut out = Outcome::default();
        {
            let p = self.painter();
            glass_surface(p, v.card, RADIUS);
            p.push_clip_rounded(v.card, RADIUS);
        }
        let spec = v.spec;
        let assertive = spec.kind == ToastKind::Error;
        // A title names the node and the message describes it.
        let title = v.extras.and_then(|x| x.title.as_deref());
        let (name, description) = match title {
            Some(title) => (
                title,
                Some(match spec.detail.as_deref() {
                    Some(d) => format!("{}\n{d}", spec.message),
                    None => spec.message.clone(),
                }),
            ),
            None => (spec.message.as_str(), spec.detail.clone()),
        };
        let mut node = Node::new(
            eid,
            if assertive { Role::Alert } else { Role::Status },
            name,
            v.card,
        );
        node.description = description;
        node.live = Some(if assertive {
            Live::Assertive
        } else {
            Live::Polite
        });
        if v.expanded {
            node.states.insert(State::Expanded);
        }
        let (acts, toggles) = body_rules(spec, v.extras);
        if acts {
            node.actions.insert(Action::Default);
        }
        node.actions.insert(Action::Dismiss);
        if toggles {
            node.actions.insert(if v.expanded {
                Action::Collapse
            } else {
                Action::Expand
            });
        }
        let by_access = v.live
            && self
                .previous_tree()
                .get(eid)
                .is_some_and(|n| n.actions.contains(Action::Dismiss))
            && self.input().events.iter().any(|ev| {
                matches!(ev, Event::Access(a) if a.target == eid && a.action == Action::Dismiss)
            });
        if by_access {
            out.dismiss = true;
        }
        if v.live {
            self.node_container(node, |ui| ui.toast_content(v, &mut out));
        } else {
            self.toast_content(v, &mut out);
        }
        self.painter().pop_clip();
        out
    }

    fn toast_content(&mut self, v: &ToastView<'_>, out: &mut Outcome) {
        let (spec, m, card, live) = (v.spec, v.m, v.card, v.live);
        let t = *self.theme();
        let eid = self.scope_id();
        let (x0, w) = (card.x + PAD + BORDER, m.inner);
        let mut y = card.y + PAD + BORDER;
        let (acts, toggles) = body_rules(spec, v.extras);
        if live {
            let body = self.interact(eid, card, Sense::CLICK);
            out.rect = Some(body.rect);
            if body.hovered && acts {
                self.set_cursor(CursorIcon::Pointer);
            }
            if body.clicked {
                if toggles {
                    out.toggle = true;
                } else if let Some(n) = v.extras.and_then(|x| x.body_action) {
                    out.action = Some(n);
                }
            }
        }

        // Row 0: the glyph, the message (without a header), Cancel and Dismiss.
        let header = m.head_h > 0.0;
        let head_row = if header { BUTTON } else { m.row };
        let centre = (x0 + ICON / 2.0, y + head_row / 2.0);
        let accent = v
            .extras
            .and_then(|x| x.accent)
            .unwrap_or_else(|| accent_of(&t, spec.kind));
        let glyph = v.extras.and_then(|x| x.glyph);
        match (spec.kind, glyph) {
            (ToastKind::Loading { .. }, None) if live => {
                self.spinner(
                    "icon",
                    centre,
                    &SpinnerOptions {
                        busy: true,
                        size: ICON,
                        color: Some(accent),
                        name: "",
                    },
                );
            }
            (ToastKind::Loading { .. }, None) => {
                spinner::paint(self.painter(), centre, ICON, accent, 0.0);
            }
            (kind, glyph) => {
                let g = icon::glyph(glyph.unwrap_or_else(|| glyph_of(kind)), ICON, accent);
                self.painter()
                    .text_centered(&g, Rect::new(x0, y + (head_row - ICON) / 2.0, ICON, ICON));
            }
        }

        let dismiss = Rect::new(
            x0 + w - BUTTON,
            y + (head_row - BUTTON) / 2.0,
            BUTTON,
            BUTTON,
        );
        let cancel = Rect::new(
            dismiss.x - DISMISS_MARGIN - BUTTON,
            dismiss.y,
            BUTTON,
            BUTTON,
        );
        let cancellable = matches!(
            spec.kind,
            ToastKind::Loading {
                cancellable: true,
                ..
            }
        );
        if let Some(x) = v.extras.filter(|_| header) {
            let right = if cancellable { cancel.x } else { dismiss.x } - CANCEL_MARGIN;
            self.toast_header(x, (x0, y), right, &t);
        } else {
            let column = m.title_h + m.msg_h;
            self.toast_column(v, x0 + TEXT_INSET, y + (m.row - column) / 2.0);
        }
        let button_of = |glyph, name| IconButton {
            glyph,
            glyph_size: BUTTON_GLYPH,
            color: None,
            radius: BUTTON_RADIUS,
            name,
        };
        if cancellable {
            let name = self.tr("luneui.dialog.cancel");
            let b = button_of(icon::CIRCLE_SLASH, name);
            if live {
                out.cancel = self.icon_button("cancel", cancel, &b).clicked;
            } else {
                let look = ButtonLook::of(&t, ButtonKind::Ghost, Default::default());
                button::paint_icon(self.painter(), cancel, &b, &look);
            }
        }
        let name = self.tr("luneui.toast-notifications.dismiss");
        let b = button_of(icon::CLOSE, name);
        if live {
            out.dismiss |= self.icon_button("dismiss", dismiss, &b).clicked;
        } else {
            let look = ButtonLook::of(&t, ButtonKind::Ghost, Default::default());
            button::paint_icon(self.painter(), dismiss, &b, &look);
        }
        y += head_row;
        if header {
            // Under the header row the title and the message start at the inner left edge.
            y += SECTION_TOP;
            self.toast_column(v, x0, y);
            y += m.row;
        }

        // The detail, under the message.
        if let Some(d) = spec.detail.as_deref() {
            y += DETAIL_TOP;
            let inset = text_inset(header);
            let s = Spec::plain(d, detail_style(&t)).wrapped((w - inset).max(0.0));
            let h = self.painter().measure(&s).1;
            self.painter().text(&s, x0 + inset, y);
            y += h;
        }

        // The bar of a running operation.
        if let ToastKind::Loading { progress, .. } = spec.kind {
            y += SECTION_TOP;
            let track = Rect::new(x0, y, w, 0.0);
            if live {
                self.progress_sized(
                    "progress",
                    track,
                    &ProgressOptions {
                        value: progress,
                        tone: Tone::Neutral,
                        head: None,
                        name: spec.message.as_str(),
                    },
                    bar_height(spec.kind),
                );
            } else {
                let bar = Rect::new(x0, y, w, bar_height(spec.kind));
                match progress {
                    Some(v) => progress::paint(self.painter(), bar, v, Tone::Neutral),
                    None => {
                        let (tier, now) = (self.tier(), self.time());
                        progress::paint_indeterminate(
                            self.painter(),
                            bar,
                            Tone::Neutral,
                            progress::indeterminate_at(tier, now),
                        );
                    }
                }
            }
            y += bar_height(spec.kind);
        }

        // The log block.
        if v.expanded {
            y += SECTION_TOP;
            self.toast_log(Rect::new(x0, y, w, LOG_HEIGHT), &spec.log, live);
            y += LOG_HEIGHT;
        }

        // The actions in the app's order (`MainWindow.axaml:373-392` sorts nothing), right-aligned.
        if !spec.actions.is_empty() {
            y += SECTION_TOP;
            let height = action_height(&t);
            let order: Vec<usize> = (0..spec.actions.len()).collect();
            let total: f32 = order.iter().map(|&i| m.action_widths[i]).sum::<f32>()
                + ACTION_GAP * (order.len() - 1) as f32;
            let mut x = x0 + w - total;
            for i in order {
                let a = &spec.actions[i];
                let r = Rect::new(x, y, m.action_widths[i], height);
                if live {
                    let key = format!("action-{i}");
                    if self.toast_action(&key, r, &a.label, a.primary) && out.action.is_none() {
                        out.action = Some(a.id);
                    }
                } else {
                    let look = action_look(&t, a.primary, false, false);
                    button::paint(self.painter(), r, &a.label, ButtonKind::Secondary, &look);
                }
                x += m.action_widths[i] + ACTION_GAP;
            }
        }

        // The drain line, on top, along the bottom edge inside the card's rounded clip.
        if let Some(frac) = v.drain {
            let accent = v
                .extras
                .and_then(|x| x.accent)
                .unwrap_or_else(|| accent_of(&t, spec.kind));
            let line = Rect::new(card.x, card.bottom() - DRAIN, card.w * frac, DRAIN);
            if line.w > 0.0 {
                self.painter().fill(line, 0.0, accent);
            }
        }
    }

    /// The title (one line, elided) over the message, from `x` at `top`, each at the column's
    /// width.
    fn toast_column(&mut self, v: &ToastView<'_>, x: f32, top: f32) {
        let t = *self.theme();
        let m = v.m;
        let mut y = top;
        if let Some(title) = v.extras.and_then(|e| e.title.as_deref()) {
            let p = self.painter();
            let spec = p.elide(&Spec::plain(title, title_style(&t)), m.msg_w);
            p.text(&spec, x, y);
            y += m.title_h;
        }
        let msg = Spec::plain(v.spec.message.as_str(), message_style(&t)).wrapped(m.msg_w);
        self.painter().text(&msg, x, y);
    }

    /// The header row's source and age: the age right-aligned to `right`, the source elided to the
    /// room before it. `at` is the row's top left corner.
    fn toast_header(&mut self, x: &ToastExtras, at: (f32, f32), right: f32, t: &Theme) {
        let left = at.0 + TEXT_INSET;
        let mut end = right;
        if let Some(age) = x.age.as_deref() {
            let s = Spec::plain(age, age_style(t));
            let p = self.painter();
            let (aw, ah) = p.measure(&s);
            p.text(&s, right - aw, at.1 + (BUTTON - ah) / 2.0);
            end = right - aw - HEADER_GAP;
        }
        if let Some(source) = x.source.as_deref() {
            let p = self.painter();
            let s = p.elide(&Spec::plain(source, source_style(t)), (end - left).max(0.0));
            let h = p.measure(&s).1;
            p.text(&s, left, at.1 + (BUTTON - h) / 2.0);
        }
    }

    /// The log block at `block`: the lines in the mono face on the log fill, scrolled, following
    /// the tail. Only the lines in view are shaped.
    fn toast_log(&mut self, block: Rect, lines: &[String], live: bool) {
        let t = *self.theme();
        self.painter().fill(block, LOG_RADIUS, t.toast_log_bg);
        let inner = block.inset(LOG_PAD, LOG_PAD);
        let style = Style::mono(t.font_s, LOG_LINE, t.toast_log_fg);
        if !live {
            // The tail, as far as it fits.
            let fit = (inner.h / LOG_LINE).floor() as usize;
            let from = lines.len().saturating_sub(fit);
            let p = self.painter();
            p.push_clip(inner);
            for (k, line) in lines[from..].iter().enumerate() {
                p.text(
                    &Spec::plain(line.as_str(), style),
                    inner.x,
                    inner.y + k as f32 * LOG_LINE,
                );
            }
            p.pop_clip();
            return;
        }
        let opts = ScrollOptions {
            follow_tail: true,
            ..ScrollOptions::default()
        };
        self.scroll_area("log", inner, &opts, |ui, view| {
            let first =
                ((view.offset.1 / f64::from(LOG_LINE)).floor().max(0.0) as usize).min(lines.len());
            let last = (((view.offset.1 + f64::from(view.viewport.h)) / f64::from(LOG_LINE)).ceil()
                as usize
                + 1)
            .min(lines.len());
            let mut widest = view.viewport.w;
            let p = ui.painter();
            for (k, line) in lines[first..last].iter().enumerate() {
                let spec = Spec::plain(line.as_str(), style);
                widest = widest.max(p.measure(&spec).0);
                p.text(
                    &spec,
                    view.viewport.x,
                    view.viewport.y + (first + k) as f32 * LOG_LINE,
                );
            }
            (
                (),
                Extent::new(f64::from(widest), lines.len() as f64 * f64::from(LOG_LINE)),
            )
        });
    }

    /// An action button: the shipped app's `toast-action` look, `surface_input` for the primary
    /// one and a muted ghost for the rest. True when pressed this frame.
    fn toast_action(&mut self, key: &str, rect: Rect, label: &str, primary: bool) -> bool {
        let t = *self.theme();
        let id = self.id(key);
        let mut r = self.interact(id, rect, Sense::CLICK_FOCUS);
        if self.activation_keys(id, r.enabled, &[NamedKey::Enter, NamedKey::Space]) {
            r.clicked = true;
        }
        let target = action_look(&t, primary, r.hovered, r.pressed);
        let look = ButtonLook {
            fill: self.animate_color(id.with("fill"), target.fill, &HOVER),
            label: target.label,
            scale: self.animate_f32(
                id.with("press"),
                if r.pressed { button::PRESS_SCALE } else { 1.0 },
                &PRESS,
            ),
            ring: self.animate_f32(
                id.with("ring"),
                if r.focus_visible { 1.0 } else { 0.0 },
                &FOCUS,
            ),
        };
        button::paint(self.painter(), rect, label, ButtonKind::Secondary, &look);
        let mut node = Node::new(id, Role::Button, label, rect);
        if r.enabled {
            node.states.insert(State::Focusable);
            node.actions.insert(Action::Default);
        } else {
            node.states.insert(State::Disabled);
        }
        self.node(node);
        r.clicked
    }

    /// The notification history in `rect`: the header with its clear-all button, and the archived
    /// toasts newest first in a scrolling list. Returns the height it uses (`rect.h`). The list
    /// is a `List` of `ListItem` rows named by their messages; clear-all is a button.
    pub fn toast_history(&mut self, key: impl Hash, rect: Rect, toasts: &mut Toasts) -> f32 {
        let t = *self.theme();
        let header = self.tr("luneui.toast-notifications.history-header");
        let clear = self.tr("luneui.toast-notifications.clear-all");
        let id = self.id(&key);
        let top = rect.y + HISTORY_HEADER_TOP;
        let title = Spec::plain(header, Style::ui(t.font_m, t.on_accent).bold());
        let th = self.painter().measure(&title).1;
        self.painter()
            .text(&title, rect.x, top + (HISTORY_HEADER - th) / 2.0);

        let list_top = top + HISTORY_HEADER + HISTORY_HEADER_BOTTOM + HISTORY_LIST_TOP;
        let area = Rect::new(
            rect.x,
            list_top,
            rect.w,
            (rect.bottom() - list_top).max(0.0),
        );
        let msg_w = (rect.w - 2.0 * HISTORY_PAD.0 - HISTORY_ICON - HISTORY_GAP).max(0.0);
        let rows: Vec<(f32, f32)> = toasts
            .history
            .iter()
            .map(|s| {
                let p = self.painter();
                let msg = p
                    .measure(&Spec::plain(s.message.as_str(), message_style(&t)).wrapped(msg_w))
                    .1;
                let detail = s.detail.as_deref().map_or(0.0, |d| {
                    2.0 + p
                        .measure(&Spec::plain(d, detail_style(&t)).wrapped(
                            (msg_w + HISTORY_ICON + HISTORY_GAP - HISTORY_DETAIL_INSET).max(0.0),
                        ))
                        .1
                });
                let content = msg.max(HISTORY_ICON) + detail;
                (msg, 2.0 * (HISTORY_MARGIN + HISTORY_PAD.1) + content)
            })
            .collect();
        let total: f32 = rows.iter().map(|r| r.1).sum();

        let mut list = Node::new(id, Role::List, header, area);
        list.description = None;
        let history = &toasts.history;
        self.node_container(list, |ui| {
            ui.scope(&key, |ui| {
                let opts = ScrollOptions {
                    bars: (
                        Bars::Disabled,
                        Bars::Auto.unless_fits(f64::from(total), f64::from(area.h)),
                    ),
                    ..ScrollOptions::default()
                };
                ui.scroll_area("list", area, &opts, |ui, view| {
                    let mut y = view.viewport.y;
                    for (n, (spec, (msg_h, h))) in history.iter().zip(&rows).enumerate() {
                        let row = Rect::new(view.viewport.x, y, view.viewport.w, *h);
                        let x = row.x + HISTORY_PAD.0;
                        let top = row.y + HISTORY_MARGIN + HISTORY_PAD.1;
                        let content = msg_h.max(HISTORY_ICON);
                        let g = icon::glyph(
                            glyph_of(spec.kind),
                            HISTORY_ICON,
                            accent_of(&t, spec.kind),
                        );
                        let p = ui.painter();
                        p.text_centered(
                            &g,
                            Rect::new(
                                x,
                                top + (content - HISTORY_ICON) / 2.0,
                                HISTORY_ICON,
                                HISTORY_ICON,
                            ),
                        );
                        let tx = x + HISTORY_ICON + HISTORY_GAP;
                        p.text(
                            &Spec::plain(spec.message.as_str(), message_style(&t)).wrapped(msg_w),
                            tx,
                            top + (content - msg_h) / 2.0,
                        );
                        if let Some(d) = spec.detail.as_deref() {
                            p.text(
                                &Spec::plain(d, detail_style(&t)).wrapped(
                                    (msg_w + HISTORY_ICON + HISTORY_GAP - HISTORY_DETAIL_INSET)
                                        .max(0.0),
                                ),
                                x + HISTORY_DETAIL_INSET,
                                top + content + 2.0,
                            );
                        }
                        let mut item =
                            Node::new(ui.id("row").with(n), Role::ListItem, &spec.message, row);
                        item.description = spec.detail.clone();
                        ui.node(item);
                        y += h;
                    }
                    (
                        (),
                        Extent::new(f64::from(view.viewport.w), f64::from(total)),
                    )
                });
            });
        });

        let button = IconButton {
            glyph: icon::CLEAR_ALL,
            glyph_size: CLEAR_GLYPH,
            color: Some(t.fg_dim),
            radius: BUTTON_RADIUS,
            name: clear,
        };
        let at = Rect::new(
            rect.right() - HISTORY_HEADER,
            top,
            HISTORY_HEADER,
            HISTORY_HEADER,
        );
        let cleared = self.scope(&key, |ui| ui.icon_button("clear", at, &button).clicked);
        if cleared {
            toasts.clear_history();
            self.request_catch_up();
        }
        rect.h
    }
}

/// Whether a press on a toast's body does something, and whether that is the log's toggle: a
/// toast pushed with [`Toasts::push`] always toggles it; a notice toggles it only with a log and no
/// body action, and has an inert body with neither.
fn body_rules(spec: &ToastSpec, extras: Option<&ToastExtras>) -> (bool, bool) {
    match extras {
        None => (true, true),
        Some(x) => {
            let toggles = x.body_action.is_none() && !spec.log.is_empty();
            (toggles || x.body_action.is_some(), toggles)
        }
    }
}

/// The fraction of its timeout a notice has left, for the drain line; `None` when no timeout runs.
fn drain_fraction(e: &Entry, now: Time) -> Option<f32> {
    let total = e.total.filter(|d| !d.is_zero())?;
    let left = match e.timer {
        Timer::At(t) => t.saturating_sub(now),
        Timer::Held(r) => r,
        Timer::Pending | Timer::Off => return None,
    };
    Some((left.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0))
}

/// The look of an action button at a state.
fn action_look(t: &Theme, primary: bool, hovered: bool, pressed: bool) -> ButtonLook {
    let [r, g, b, _] = t.surface_hover;
    let fill = match (pressed, hovered, primary) {
        (true, _, _) => t.surface_active,
        (false, true, _) => t.surface_hover,
        (false, false, true) => t.surface_input,
        (false, false, false) => [r, g, b, 0],
    };
    ButtonLook {
        fill,
        label: if primary {
            t.fg_primary
        } else {
            t.fg_secondary
        },
        scale: if pressed { button::PRESS_SCALE } else { 1.0 },
        ring: 0.0,
    }
}
