//! The `pager` page: the pager of a phone screen (`layout.pager`, D373) under a navigation bar.
//! Four pages, each a full-bleed card of its own colour with a big number, side by side, so the
//! page edges show; the dots sit over the pages' bottom edge, above the home indicator. The
//! `phone-pager-*` scenes run in the [`PHONE`](crate::page::PHONE) window at scale 3 with a
//! `SafeArea` of 62 above and 34 below: the first page, a finger half way through a swipe (two
//! pages joined at one edge), a finger pulling past the last page (the rubber band) and the
//! first page without dots. The knobs turn the dots and pick the page.

use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;
use lune_ui::widgets::nav_bar::{HEIGHT, NavBar};
use lune_ui::widgets::pager::PagerOptions;

use super::nav_bar::{SETUP, backdrop, phone};
use crate::page::{Ev, Page, PageKind, PageState, Stage, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "pager",
    title: "Pager",
    kind: PageKind::Ui(UiPage {
        state: first,
        scenes: SCENES,
        bench: &[],
    }),
};

/// The pages: a title and the card's colour. The scene's own colours: the pager takes none.
const CARDS: [(&str, [u8; 4]); 4] = [
    ("Summer", [38, 70, 128, 255]),
    ("Autumn", [128, 78, 30, 255]),
    ("Winter", [30, 104, 100, 255]),
    ("Spring", [100, 50, 112, 255]),
];

/// The big number's and the title's colour on every card.
const INK: [u8; 4] = [255, 255, 255, 235];

/// What the app keeps: the current page and whether the dots show.
#[derive(Debug, Clone, Copy)]
struct Live {
    page: usize,
    dots: bool,
}

fn first() -> Box<dyn PageState> {
    Box::new(Live {
        page: 0,
        dots: true,
    })
}

fn last() -> Box<dyn PageState> {
    Box::new(Live {
        page: CARDS.len() - 1,
        dots: true,
    })
}

fn no_dots() -> Box<dyn PageState> {
    Box::new(Live {
        page: 0,
        dots: false,
    })
}

/// Page `index` in `r`: its card, the number and its title.
fn card(ui: &mut Ui<'_>, index: usize, r: Rect) {
    let (title, fill) = CARDS[index];
    let t = *ui.theme();
    let p = ui.painter();
    p.fill(r, 0.0, fill);
    let number = Spec::plain((index + 1).to_string(), Style::ui(120.0, INK).bold());
    let h = 140.0;
    p.text_centered(&number, Rect::new(r.x, r.y + r.h / 2.0 - h, r.w, h));
    let name = Spec::plain(title, Style::ui(t.font_title, INK));
    p.text_centered(&name, Rect::new(r.x, r.y + r.h / 2.0 + 8.0, r.w, 32.0));
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let area = backdrop(ui, stage);
        let safe = ui.safe_area();
        let bar = Rect::new(area.x, area.y, area.w, safe.top + HEIGHT);
        let rect = Rect::new(
            area.x,
            bar.bottom(),
            area.w,
            (area.bottom() - safe.bottom - bar.bottom()).max(0.0),
        );
        ui.pager(
            "pager",
            rect,
            CARDS.len(),
            &mut self.page,
            &PagerOptions::new("Seasons").with_dots(self.dots),
            card,
        );
        ui.nav_bar("bar", bar, &NavBar::new("Seasons"));
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.toggle(ui, "Dots", &mut self.dots);
        panel.choice(
            ui,
            "Page",
            &["Summer", "Autumn", "Winter", "Spring"],
            &mut self.page,
        );
    }
}

/// A finger down at 1.0 s and 210 px left by 1.05 s, held: a little over half a page across.
const SWIPE: &[Step] = &[
    SETUP,
    Step {
        at_ms: 500,
        events: &[],
    },
    Step {
        at_ms: 1000,
        events: &[Ev::TouchStart(1, 330.0, 420.0)],
    },
    Step {
        at_ms: 1016,
        events: &[Ev::TouchMove(1, 260.0, 420.0)],
    },
    Step {
        at_ms: 1033,
        events: &[Ev::TouchMove(1, 190.0, 420.0)],
    },
    Step {
        at_ms: 1050,
        events: &[Ev::TouchMove(1, 120.0, 420.0)],
    },
];

/// The same pull on the last page: 210 px of travel, which the rubber band shows as far less.
const RUBBER: &[Step] = SWIPE;

const SCENES: &[UiScene] = &[
    phone("phone-pager-first", first, &[SETUP], 300),
    phone("phone-pager-mid-swipe", first, SWIPE, 1100),
    phone("phone-pager-last-rubber", last, RUBBER, 1100),
    phone("phone-pager-no-dots", no_dots, &[SETUP], 300),
];
