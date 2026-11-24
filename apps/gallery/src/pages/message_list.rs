//! The `message-list` page: the Message list component (`layout.message-list`) in the popover an
//! app opens from a bell button, as an inbox: the header with its "Clear all" action above the
//! rows. The page sizes the popover with `message_list::height`, as an app does. The scenes are the
//! inbox at rest (four rows, one read) and a long inbox scrolled.

use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::ui::Ui;
use lune_ui::widgets::button::ButtonKind;
use lune_ui::widgets::message_list::{
    HEADER_HEIGHT, MessageListOptions, MessageRow, height as list_height,
};
use lune_ui::widgets::popover::PopoverOptions;

use super::{card, heading, on_card};
use crate::page::Stage;
use crate::page::{Ev, Expect, Page, PageKind, PageState, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "message-list",
    title: "Message list",
    kind: PageKind::Ui(UiPage {
        state: rest,
        scenes: SCENES,
        bench: &[],
    }),
};

/// The bell button that opens the inbox, in window coordinates: inside the card, under the heading.
const BUTTON: Rect = Rect::new(34.0, 82.0, 140.0, 32.0);
/// The inbox's width.
const WIDTH: f32 = 360.0;
/// The room the popover leaves round its content.
const PAD: f32 = 4.0;
/// Where the pointer rests over the long inbox's rows, in window coordinates.
const OVER_LIST: (f32, f32) = (110.0, 250.0);
/// Away from every row.
const AWAY: Ev = Ev::PointerAt(900.0, 600.0);

/// One message the page owns.
struct Row {
    title: String,
    age: String,
    body: String,
    meta: String,
    unread: bool,
}

fn row(title: &str, age: &str, body: &str, meta: &str, unread: bool) -> Row {
    Row {
        title: title.to_owned(),
        age: age.to_owned(),
        body: body.to_owned(),
        meta: meta.to_owned(),
        unread,
    }
}

/// The four rows of the mockup: three unread (the second has a body that takes two lines), one
/// read.
fn four() -> Vec<Row> {
    vec![
        row(
            "Build finished",
            "now",
            "All 214 tests passed in 38 s.",
            "main \u{b7} luneterm",
            true,
        ),
        row(
            "Deploy needs approval",
            "2 m",
            "The staging deploy is waiting for a reviewer before it can go on to the next stage.",
            "staging",
            true,
        ),
        row(
            "Tab 3 needs attention",
            "14 m",
            "The command exited with status 1.",
            "zsh",
            true,
        ),
        row("Backup complete", "1 h", "42 files copied.", "nas", false),
    ]
}

/// Twenty rows, every third one read.
fn twenty() -> Vec<Row> {
    (1..=20)
        .map(|n| {
            row(
                &format!("Message {n}"),
                &format!("{n} h"),
                "A line of what happened, long enough to take a second line in the list.",
                "inbox",
                n % 3 != 0,
            )
        })
        .collect()
}

fn rest() -> Box<dyn PageState> {
    Box::new(Inbox {
        rows: four(),
        open: true,
        max: 480.0,
    })
}

fn long() -> Box<dyn PageState> {
    Box::new(Inbox {
        rows: twenty(),
        open: true,
        max: 320.0,
    })
}

/// The page plays the app: it owns the rows and whether the inbox is open.
struct Inbox {
    rows: Vec<Row>,
    open: bool,
    /// The most height the list uses.
    max: f32,
}

impl PageState for Inbox {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let card = card(ui, stage, "Message list");
        heading(ui, "MESSAGE LIST", card.x + 24.0, card.y + 24.0);
        let anchor = on_card(card, BUTTON);
        let bell = ui.button("bell", anchor, "Notifications", ButtonKind::Secondary);
        if bell.clicked {
            self.open = true;
        }
        if !self.open {
            return;
        }
        let rows: Vec<MessageRow<'_>> = self
            .rows
            .iter()
            .map(|r| {
                MessageRow::new(&r.title)
                    .with_age(&r.age)
                    .with_body(&r.body)
                    .with_meta(&r.meta)
                    .with_unread(r.unread)
            })
            .collect();
        let inner = WIDTH - 2.0 * PAD;
        let used = list_height(ui.painter(), &rows, inner, self.max);
        let size = (WIDTH, 2.0 * PAD + HEADER_HEIGHT + used);
        let options = PopoverOptions {
            anchor_id: Some(bell.id),
            ..PopoverOptions::new(anchor, size)
        };
        let mut clear = false;
        let mut opened = None;
        let (frame, _placement, ()) = ui.popover("inbox", &options, |ui, at| {
            let head = Rect::new(at.x + PAD, at.y + PAD, inner, HEADER_HEIGHT);
            clear = ui.message_list_header("head", head, "Notifications", Some("Clear all"));
            let list = Rect::new(at.x + PAD, head.bottom(), inner, used);
            let response = ui.message_list(
                "list",
                list,
                &rows,
                &MessageListOptions::new("Notifications").with_max_height(self.max),
            );
            opened = response.clicked;
        });
        if frame.dismissed.is_some() {
            self.open = false;
        }
        if clear {
            self.rows.clear();
        }
        if let Some(i) = opened {
            // A click opens the message and reads it.
            self.rows[i].unread = false;
        }
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.number(ui, "Max height", &mut self.max, (100.0, 600.0, 1.0));
        if panel.button(ui, "Add a message") {
            let n = self.rows.len() + 1;
            self.rows.insert(
                0,
                row(
                    &format!("New message {n}"),
                    "now",
                    "Something just happened.",
                    "inbox",
                    true,
                ),
            );
        }
        if panel.button(ui, "Fill the list") {
            self.rows = twenty();
        }
        // Last: an open popover is dismissed by a press outside it.
        panel.toggle(ui, "Open", &mut self.open);
    }
}

const fn scene(
    id: &'static str,
    state: fn() -> Box<dyn PageState>,
    script: &'static [Step],
    capture_ms: u32,
) -> UiScene {
    UiScene {
        id,
        scale: 2.0,
        tier: MotionTier::Full,
        state,
        script,
        capture_ms,
        expect: Expect::Settled,
    }
}

const SCENES: &[UiScene] = &[
    // The open is `fast` (180 ms): settled by 500 ms.
    scene("message-list-rest", rest, &[], 500),
    // The wheel scrolls the rows 130 px; the pointer leaves so no row is lit.
    scene(
        "message-list-scrolled",
        long,
        &[
            Step {
                at_ms: 300,
                events: &[Ev::PointerAt(OVER_LIST.0, OVER_LIST.1)],
            },
            Step {
                at_ms: 350,
                events: &[Ev::Scroll {
                    x: 0.0,
                    y: -130.0,
                    precise: true,
                }],
            },
            Step {
                at_ms: 450,
                events: &[AWAY],
            },
        ],
        1000,
    ),
];
