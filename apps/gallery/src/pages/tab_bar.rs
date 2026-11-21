//! The `tab-bar` page: the bottom tab bar of a phone screen (`widget.tab-bar`, D370). The bar sits
//! at the window's bottom, `HEIGHT + Ui::safe_area().bottom` tall so its colour runs down behind
//! the home indicator, under a few plain rows. The `phone-tab-bar-*` scenes run in the [`PHONE`]
//! window at scale 3 with a `SafeArea` of 62 above and 34 below; `phone-tab-bar-pressed` holds a
//! finger on an item that is not selected. The knobs turn the badges and the selected item.

use lune_ui::icon;
use lune_ui::paint::Rect;
use lune_ui::ui::Ui;
use lune_ui::widgets::tab_bar::{HEIGHT, TabBarItem};

use super::nav_bar::{SAFE, SETUP, backdrop, phone, plain_rows};
use crate::page::{Ev, Expect, PHONE, Page, PageKind, PageState, Stage, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "tab-bar",
    title: "Bottom tab bar",
    kind: PageKind::Ui(UiPage {
        state: five,
        scenes: SCENES,
        bench: &[],
    }),
};

const NAMES: [&str; 5] = ["Home", "Search", "Library", "Inbox", "Settings"];
const GLYPHS: [&str; 5] = [
    icon::NOTE,
    icon::SEARCH,
    icon::CALENDAR,
    icon::SYNC,
    icon::GEAR,
];

/// What the app keeps: the selected item and whether two items carry badges.
#[derive(Debug, Clone, Copy)]
struct Live {
    selected: usize,
    badges: bool,
}

fn five() -> Box<dyn PageState> {
    Box::new(Live {
        selected: 0,
        badges: false,
    })
}

fn badged() -> Box<dyn PageState> {
    Box::new(Live {
        selected: 2,
        badges: true,
    })
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let area = backdrop(ui, stage);
        let safe = ui.safe_area();
        let bar = Rect::new(
            area.x,
            area.bottom() - safe.bottom - HEIGHT,
            area.w,
            HEIGHT + safe.bottom,
        );
        plain_rows(ui, area, area.y + safe.top, bar.y);
        let mut items: Vec<TabBarItem<'_>> = (0..5)
            .map(|i| TabBarItem::new(GLYPHS[i], NAMES[i]))
            .collect();
        if self.badges {
            items[1] = items[1].with_badge("3");
            items[3] = items[3].with_badge("99+");
        }
        ui.tab_bar("tab-bar", bar, &items, &mut self.selected);
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.toggle(ui, "Badges", &mut self.badges);
        panel.choice(ui, "Selected", &NAMES, &mut self.selected);
    }
}

/// Where item 3's centre is in the phone window: five equal items over the 402 wide bar, the row
/// 49 high above the 34 px home-indicator band.
const PRESSED_AT: (f32, f32) = (
    PHONE.0 as f32 * 3.5 / 5.0,
    PHONE.1 as f32 - SAFE.bottom - HEIGHT / 2.0,
);

const PRESS: &[Step] = &[
    SETUP,
    Step {
        at_ms: 16,
        events: &[Ev::TouchStart(1, PRESSED_AT.0, PRESSED_AT.1)],
    },
];

const SCENES: &[UiScene] = &[
    phone("phone-tab-bar-five", five, &[SETUP], 300),
    phone("phone-tab-bar-badge", badged, &[SETUP], 300),
    // The finger is still down, so the long press is due at 516 ms: the frame says so.
    UiScene {
        expect: Expect::Waiting,
        ..phone("phone-tab-bar-pressed", five, PRESS, 400)
    },
];
