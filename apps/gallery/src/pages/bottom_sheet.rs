//! The `bottom-sheet` page: the bottom sheet of a phone screen (`overlay.bottom-sheet`, D371) over
//! a page with a navigation bar, a few plain rows and a tab bar. The `phone-sheet-*` scenes run in
//! the [`PHONE`] window at scale 3 with a `SafeArea` of 62 above and 34 below. The sheet opens with
//! the scene and has settled when it is captured, except `phone-sheet-dragging` (a finger holds it
//! between its detents) and `phone-sheet-scrolling-content` (a finger holds a list that scrolls
//! under it); the knobs turn the sheet, its detents and its content.

use lune_ui::icon;
use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;
use lune_ui::widgets::bottom_sheet::{BottomSheet, Detent};
use lune_ui::widgets::button::ButtonKind;
use lune_ui::widgets::field::FieldState;
use lune_ui::widgets::nav_bar::{HEIGHT as NAV_HEIGHT, NavBar};
use lune_ui::widgets::scroll_area::{Extent, ScrollOptions};
use lune_ui::widgets::tab_bar::{HEIGHT as TAB_HEIGHT, TabBarItem};

use super::nav_bar::{SETUP, backdrop, phone, plain_rows};
use crate::page::{Ev, Expect, PHONE, Page, PageKind, PageState, Stage, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "bottom-sheet",
    title: "Bottom sheet",
    kind: PageKind::Ui(UiPage {
        state: closed,
        scenes: SCENES,
        bench: &[],
    }),
};

/// What the sheet holds.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Body {
    /// A title, three switches and a note: content that fits.
    Filters,
    /// A long list under the grabber: content that scrolls.
    List,
    /// A title, a text field and rows that run on below it.
    Field,
}

const BODIES: [&str; 3] = ["Filters", "List", "Field"];
const MEDIUM: &[Detent] = &[Detent::Fraction(0.5), Detent::Fraction(1.0)];
const LARGE: &[Detent] = &[Detent::Fraction(1.0)];
const SMALL_MEDIUM_LARGE: &[Detent] = &[
    Detent::Height(180.0),
    Detent::Fraction(0.5),
    Detent::Fraction(1.0),
];

/// What the app keeps: whether the sheet is open, its detents and its content.
#[derive(Debug)]
struct Live {
    open: bool,
    detents: &'static [Detent],
    body: Body,
    on: [bool; 3],
    name: String,
    tab: usize,
}

fn live(open: bool, detents: &'static [Detent], body: Body) -> Box<dyn PageState> {
    Box::new(Live {
        open,
        detents,
        body,
        on: [true, false, true],
        name: String::new(),
        tab: 0,
    })
}

fn closed() -> Box<dyn PageState> {
    live(false, MEDIUM, Body::Filters)
}

fn medium() -> Box<dyn PageState> {
    live(true, MEDIUM, Body::Filters)
}

fn large() -> Box<dyn PageState> {
    live(true, LARGE, Body::Filters)
}

fn list() -> Box<dyn PageState> {
    live(true, MEDIUM, Body::List)
}

fn field() -> Box<dyn PageState> {
    live(true, LARGE, Body::Field)
}

fn title(ui: &mut Ui<'_>, text: &str, x: f32, y: f32) {
    let t = *ui.theme();
    let spec = Spec::plain(text, Style::ui(t.font_title, t.fg_primary).bold());
    ui.painter().text(&spec, x, y);
}

fn line(ui: &mut Ui<'_>, text: &str, x: f32, y: f32) {
    let t = *ui.theme();
    let spec = Spec::plain(text, Style::ui(t.font_base, t.fg_secondary));
    ui.painter().text(&spec, x, y);
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let area = backdrop(ui, stage);
        let safe = ui.safe_area();
        let bar = Rect::new(area.x, area.y, area.w, safe.top + NAV_HEIGHT);
        let tabs = Rect::new(
            area.x,
            area.bottom() - safe.bottom - TAB_HEIGHT,
            area.w,
            TAB_HEIGHT + safe.bottom,
        );
        plain_rows(ui, area, bar.bottom() + 52.0, tabs.y);
        if ui
            .button(
                "show",
                Rect::new(area.x + 16.0, bar.bottom() + 8.0, 140.0, 36.0),
                "Show sheet",
                ButtonKind::Primary,
            )
            .clicked
        {
            self.open = true;
        }
        ui.nav_bar("page-nav", bar, &NavBar::new("Photos"));
        let items = [
            TabBarItem::new(icon::NOTE, "Library"),
            TabBarItem::new(icon::SEARCH, "Search"),
            TabBarItem::new(icon::GEAR, "Settings"),
        ];
        ui.tab_bar("page-tabs", tabs, &items, &mut self.tab);

        let sheet = BottomSheet::new("Sheet").with_detents(self.detents);
        let (body, on, name) = (self.body, &mut self.on, &mut self.name);
        ui.bottom_sheet("sheet", &mut self.open, &sheet, |ui, rect| match body {
            Body::Filters => {
                title(ui, "Filters", rect.x + 16.0, rect.y + 6.0);
                for (i, label) in ["Favourites", "Videos", "Screenshots"].iter().enumerate() {
                    let y = rect.y + 52.0 + 52.0 * i as f32;
                    line(ui, label, rect.x + 16.0, y + 12.0);
                    ui.toggle(
                        ("filter", i),
                        Rect::new(rect.right() - 16.0 - 40.0, y + 6.0, 40.0, 24.0),
                        &mut on[i],
                        label,
                    );
                }
                line(
                    ui,
                    "Drag the grabber, or tap it, to change the height.",
                    rect.x + 16.0,
                    rect.y + 52.0 + 52.0 * 3.0 + 12.0,
                );
            }
            Body::List => {
                let o = ScrollOptions::default();
                ui.scroll_area("rows", rect, &o, |ui, _| {
                    plain_rows(ui, rect, rect.y, rect.y + 1280.0);
                    ((), Extent::new(f64::from(rect.w), 1280.0))
                });
            }
            Body::Field => {
                title(ui, "New album", rect.x + 16.0, rect.y + 6.0);
                ui.text_field(
                    "name",
                    Rect::new(rect.x + 16.0, rect.y + 52.0, rect.w - 32.0, 36.0),
                    name,
                    "Album name",
                    FieldState::default(),
                );
                for i in 0..12 {
                    let y = rect.y + 104.0 + 56.0 * i as f32;
                    line(ui, "A suggestion below the field", rect.x + 16.0, y + 16.0);
                }
            }
        });
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.toggle(ui, "Open", &mut self.open);
        let mut which = match self.detents.len() {
            1 => 1,
            3 => 2,
            _ => 0,
        };
        panel.choice(
            ui,
            "Detents",
            &["Medium, large", "Large", "Three"],
            &mut which,
        );
        self.detents = [MEDIUM, LARGE, SMALL_MEDIUM_LARGE][which];
        let mut body = match self.body {
            Body::Filters => 0,
            Body::List => 1,
            Body::Field => 2,
        };
        panel.choice(ui, "Content", &BODIES, &mut body);
        self.body = [Body::Filters, Body::List, Body::Field][body];
    }
}

/// Where the medium detent's grabber is in the phone window: the largest height is 874 - 62 - 10,
/// half of it is 401, so the top edge is at 473.
const GRABBER_AT: (f32, f32) = (PHONE.0 as f32 / 2.0, PHONE.1 as f32 - 401.0 + 10.0);

/// A finger down on the grabber at 1.0 s, 150 px up by 1.05 s and held: between the detents.
const DRAG: &[Step] = &[
    SETUP,
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, GRABBER_AT.0, GRABBER_AT.1)],
    },
    Step {
        at_ms: 1016,
        events: &[Ev::TouchMove(1, GRABBER_AT.0, GRABBER_AT.1 - 50.0)],
    },
    Step {
        at_ms: 1033,
        events: &[Ev::TouchMove(1, GRABBER_AT.0, GRABBER_AT.1 - 110.0)],
    },
    Step {
        at_ms: 1050,
        events: &[Ev::TouchMove(1, GRABBER_AT.0, GRABBER_AT.1 - 160.0)],
    },
];

/// A finger down on the list at 1.0 s and 120 px up by 1.05 s, held: the list scrolled, the sheet
/// where it was.
const SCROLL: &[Step] = &[
    SETUP,
    Step {
        at_ms: 500,
        events: &[],
    },
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, 201.0, 700.0)],
    },
    Step {
        at_ms: 1016,
        events: &[Ev::TouchMove(1, 201.0, 660.0)],
    },
    Step {
        at_ms: 1033,
        events: &[Ev::TouchMove(1, 201.0, 620.0)],
    },
    Step {
        at_ms: 1050,
        events: &[Ev::TouchMove(1, 201.0, 580.0)],
    },
];

/// Tab from the grabber (the sheet's first tab stop) to the field at 1.0 s, then the keyboard
/// 300 px up at 1.2 s: the field keeps its ring and the content ends above the keyboard.
const KEYBOARD: &[Step] = &[
    SETUP,
    Step {
        at_ms: 500,
        events: &[],
    },
    Step {
        at_ms: 1000,
        events: &[Ev::Tab],
    },
    Step {
        at_ms: 1200,
        events: &[Ev::KeyboardInset(300.0)],
    },
];

/// The sheet opens at the first frame and has taken its focus and settled by the time it is shot.
const OPEN: &[Step] = &[
    SETUP,
    Step {
        at_ms: 400,
        events: &[],
    },
    Step {
        at_ms: 800,
        events: &[],
    },
];

const SCENES: &[UiScene] = &[
    phone("phone-sheet-medium", medium, OPEN, 1500),
    phone("phone-sheet-large", large, OPEN, 1500),
    // The finger is still on the grabber, whose pressed fill keeps asking for frames.
    UiScene {
        expect: Expect::Moving,
        ..phone("phone-sheet-dragging", medium, DRAG, 1100)
    },
    // The list's scroll bar is fading in under the held finger: a frame is due.
    UiScene {
        expect: Expect::Moving,
        ..phone("phone-sheet-scrolling-content", list, SCROLL, 1100)
    },
    phone("phone-sheet-keyboard", field, KEYBOARD, 1700),
];
