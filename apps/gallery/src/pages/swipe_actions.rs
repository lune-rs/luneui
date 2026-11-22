//! The `swipe-actions` page: swipe row actions of a phone list (`widget.swipe-actions`, D374). A
//! navigation bar over a virtual list of mail rows, each a `Ui::swipe_actions` row: two trailing
//! buttons (More, and Delete in the error colour at the edge) and one leading (Read). The
//! `phone-swipe-*` scenes run in the [`PHONE`](crate::page::PHONE) window at scale 3 with a
//! `SafeArea` of 62 above and 34 below: every row closed, the third row opened to each edge by a
//! finger, a finger held past 60 % of the width (the full swipe), and the app's menu on a long
//! press (the mouse and keyboard way to the same actions). The knobs turn the full swipe and the
//! leading button.

use lune_ui::icon;
use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;
use lune_ui::widgets::menu::{MenuItem, MenuRow};
use lune_ui::widgets::nav_bar::{HEIGHT, NavBar};
use lune_ui::widgets::swipe_actions::{SwipeAction, SwipeActions};
use lune_ui::widgets::tone::Tone;
use lune_ui::widgets::virtual_list::{Heights, ListOptions, RowKey, SelectionMode};

use super::nav_bar::{SETUP, backdrop, phone};
use crate::page::{Ev, Page, PageKind, PageState, Stage, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "swipe-actions",
    title: "Swipe row actions",
    kind: PageKind::Ui(UiPage {
        state: live,
        scenes: SCENES,
        bench: &[],
    }),
};

/// A row's height.
const ROW: f32 = 64.0;

/// The rows: who it is from, and the line under it.
const MAIL: [(&str, &str); 11] = [
    ("Ada Lovelace", "Notes on the analytical engine"),
    ("Grace Hopper", "The compiler is ready for review"),
    ("Alan Turing", "Re: the halting problem, again"),
    ("Katherine Johnson", "Trajectory figures for Thursday"),
    ("Edsger Dijkstra", "A note on goto statements"),
    ("Margaret Hamilton", "Flight software, rev 4"),
    ("Donald Knuth", "Volume 4, fascicle 6 proofs"),
    ("Barbara Liskov", "Substitution, and when it holds"),
    ("Dennis Ritchie", "Re: the type of an array"),
    ("Radia Perlman", "Spanning trees over lunch"),
    ("Tim Berners-Lee", "A proposal, vague but exciting"),
];

const TRAILING: [SwipeAction<'static>; 2] = [
    SwipeAction::new("More").with_glyph(icon::ELLIPSIS),
    SwipeAction::new("Delete")
        .with_glyph(icon::CLOSE)
        .with_tone(Tone::Err),
];
const LEADING: [SwipeAction<'static>; 1] = [SwipeAction::new("Read")
    .with_glyph(icon::CHECK)
    .with_tone(Tone::Ok)];

/// What the app keeps: the knobs, and where its menu is open, if it is.
#[derive(Debug, Clone, Copy)]
struct Live {
    full: bool,
    leading: bool,
    menu: Option<(f32, f32)>,
}

fn live() -> Box<dyn PageState> {
    Box::new(Live {
        full: true,
        leading: true,
        menu: None,
    })
}

/// A row's content in `r`: an opaque background (the window's, so the row slides over the buttons
/// as one piece), an initial in a disc, the sender and the line under it, a separator.
fn mail(ui: &mut Ui<'_>, index: usize, r: Rect) {
    let t = *ui.theme();
    let tint = t.glass.window.tint;
    let (from, line) = MAIL[index % MAIL.len()];
    let p = ui.painter();
    p.fill(r, 0.0, [tint[0], tint[1], tint[2], 255]);
    let disc = Rect::new(r.x + 16.0, r.y + 12.0, 40.0, 40.0);
    p.fill(disc, 20.0, t.surface_active);
    let initial = Spec::plain(&from[..1], Style::ui(t.font_m, t.fg_primary).bold());
    p.text_centered(&initial, disc);
    let name = Spec::plain(from, Style::ui(t.font_m, t.fg_primary));
    let name = p.elide(&name, r.w - 68.0 - 16.0);
    p.text_in(
        &name,
        Rect::new(r.x + 68.0, r.y + 10.0, r.w - 84.0, 22.0),
        0.0,
    );
    let sub = Spec::plain(line, Style::ui(t.font_s, t.fg_secondary));
    let sub = p.elide(&sub, r.w - 68.0 - 16.0);
    p.text_in(
        &sub,
        Rect::new(r.x + 68.0, r.y + 34.0, r.w - 84.0, 20.0),
        0.0,
    );
    p.fill(
        Rect::new(r.x + 68.0, r.bottom() - 1.0, r.w - 68.0, 1.0),
        0.0,
        t.surface_separator,
    );
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let area = backdrop(ui, stage);
        let safe = ui.safe_area();
        let bar = Rect::new(area.x, area.y, area.w, safe.top + HEIGHT);
        let list = Rect::new(
            area.x,
            bar.bottom(),
            area.w,
            (area.bottom() - safe.bottom - bar.bottom()).max(0.0),
        );
        let leading: &[SwipeAction<'_>] = if self.leading { &LEADING } else { &[] };
        let actions = SwipeActions::new()
            .with_leading(leading)
            .with_trailing(&TRAILING)
            .with_full_swipe(self.full);
        let opts = ListOptions {
            count: MAIL.len(),
            heights: Heights::Fixed(ROW),
            selection: SelectionMode::None,
            index_of: None,
            label: "Mail",
        };
        let mut ask = false;
        ui.virtual_list(
            "list",
            list,
            &opts,
            |i| RowKey(i as u64),
            |ui, ctx| {
                let i = ctx.index;
                let res = ui.swipe_actions(("row", i), ctx.rect, &actions, |ui, r| {
                    mail(ui, i, r);
                });
                ask |= res.context;
                ROW
            },
        );
        ui.nav_bar("bar", bar, &NavBar::new("Inbox"));
        if ask {
            self.menu = ui.pointer_pos();
        }
        self.row_menu(ui);
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.toggle(ui, "Full swipe", &mut self.full);
        panel.toggle(ui, "Leading button", &mut self.leading);
    }
}

impl Live {
    /// The app's menu on a row's `context` (D292): the row's own actions, for a mouse or a keyboard.
    fn row_menu(&mut self, ui: &mut Ui<'_>) {
        let Some(at) = self.menu else {
            return;
        };
        let item = |glyph, text, danger| {
            MenuRow::Entry(MenuItem {
                glyph: Some(glyph),
                text,
                danger,
                ..MenuItem::default()
            })
        };
        let rows = [
            item(icon::CHECK, "Read", false),
            item(icon::ELLIPSIS, "More", false),
            item(icon::CLOSE, "Delete", true),
        ];
        let response = ui.menu("row-menu", &rows, at);
        if response.picked.is_some() || response.dismissed {
            self.menu = None;
        }
    }
}

/// The third row's vertical centre: the list starts under the bar (62 + 44), two rows down.
const Y: f32 = 106.0 + 2.0 * ROW + ROW / 2.0;

/// A step with no events: a frame at `ms`.
const fn frame(ms: u32) -> Step {
    Step {
        at_ms: ms,
        events: &[],
    }
}

/// A finger down at `x`, moving by `dx` every 16 ms for `moves` frames from 1.0 s. A `const`
/// table per scene, since a step's events are `'static`.
const OPEN_TRAILING: &[Step] = &[
    SETUP,
    frame(500),
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, 360.0, Y)],
    },
    Step {
        at_ms: 1016,
        events: &[Ev::TouchMove(1, 330.0, Y)],
    },
    Step {
        at_ms: 1033,
        events: &[Ev::TouchMove(1, 300.0, Y)],
    },
    Step {
        at_ms: 1050,
        events: &[Ev::TouchMove(1, 270.0, Y)],
    },
    Step {
        at_ms: 1066,
        events: &[Ev::TouchMove(1, 240.0, Y)],
    },
    Step {
        at_ms: 1083,
        events: &[Ev::TouchMove(1, 210.0, Y)],
    },
    frame(1300),
    Step {
        at_ms: 1316,
        events: &[Ev::TouchEnd(1, 210.0, Y)],
    },
];

const OPEN_LEADING: &[Step] = &[
    SETUP,
    frame(500),
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, 40.0, Y)],
    },
    Step {
        at_ms: 1016,
        events: &[Ev::TouchMove(1, 70.0, Y)],
    },
    Step {
        at_ms: 1033,
        events: &[Ev::TouchMove(1, 100.0, Y)],
    },
    Step {
        at_ms: 1050,
        events: &[Ev::TouchMove(1, 130.0, Y)],
    },
    Step {
        at_ms: 1066,
        events: &[Ev::TouchMove(1, 160.0, Y)],
    },
    frame(1300),
    Step {
        at_ms: 1316,
        events: &[Ev::TouchEnd(1, 160.0, Y)],
    },
];

/// A finger held 290 px left of where it landed: past 60 % of the row's 402.
const FULL_MID: &[Step] = &[
    SETUP,
    frame(500),
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, 396.0, Y)],
    },
    Step {
        at_ms: 1016,
        events: &[Ev::TouchMove(1, 340.0, Y)],
    },
    Step {
        at_ms: 1033,
        events: &[Ev::TouchMove(1, 280.0, Y)],
    },
    Step {
        at_ms: 1050,
        events: &[Ev::TouchMove(1, 220.0, Y)],
    },
    Step {
        at_ms: 1066,
        events: &[Ev::TouchMove(1, 160.0, Y)],
    },
    Step {
        at_ms: 1083,
        events: &[Ev::TouchMove(1, 100.0, Y)],
    },
];

/// A finger held still on the row's content for 600 ms: the long press asks for the menu.
const LONG_PRESS: &[Step] = &[
    SETUP,
    frame(500),
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, 170.0, Y)],
    },
    frame(1520),
    frame(1620),
    frame(1800),
    frame(2000),
];

const SCENES: &[UiScene] = &[
    phone("phone-swipe-closed", live, &[SETUP], 300),
    phone("phone-swipe-trailing-open", live, OPEN_TRAILING, 2400),
    phone("phone-swipe-leading-open", live, OPEN_LEADING, 2400),
    phone("phone-swipe-full-mid", live, FULL_MID, 1100),
    phone("phone-swipe-context-menu", live, LONG_PRESS, 2100),
];
