//! The `nav-bar` page: the navigation bar of a phone screen (`widget.nav-bar`, D370). The bar sits
//! at the window's top, `Ui::safe_area().top + HEIGHT` tall so its row is under the notch, over
//! a few plain rows. The `phone-nav-bar-*` scenes run in the [`PHONE`] window at scale 3 with a
//! `SafeArea` of 62 above and 34 below; the knobs turn the back button, the actions and the title.

use lune_ui::icon;
use lune_ui::layout::Insets;
use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::scene::LayerId;
use lune_ui::ui::Ui;
use lune_ui::widgets::nav_bar::{HEIGHT, NavAction, NavBar};

use crate::page::{Ev, Expect, Page, PageKind, PageState, Stage, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "nav-bar",
    title: "Navigation bar",
    kind: PageKind::Ui(UiPage {
        state: plain,
        scenes: SCENES,
        bench: &[],
    }),
};

/// The safe area the phone scenes tell the page: a notch above, a home indicator below.
pub(super) const SAFE: Insets = Insets::new(0.0, 62.0, 0.0, 34.0);

/// The first step of every phone scene: the window's safe area.
pub(super) const SETUP: Step = Step {
    at_ms: 0,
    events: &[Ev::SafeArea(SAFE.left, SAFE.top, SAFE.right, SAFE.bottom)],
};

/// A phone scene at scale 3 and the full motion tier.
pub(super) const fn phone(
    id: &'static str,
    state: fn() -> Box<dyn PageState>,
    script: &'static [Step],
    capture_ms: u32,
) -> UiScene {
    UiScene {
        id,
        scale: 3.0,
        tier: MotionTier::Full,
        state,
        script,
        capture_ms,
        expect: Expect::Settled,
    }
}

/// The window background of a phone page: the glass tint, over the area the page owns. Returns it.
pub(super) fn backdrop(ui: &mut Ui<'_>, stage: &Stage) -> Rect {
    let area = stage.area(ui);
    let tint = ui.theme().glass.window.tint;
    ui.painter().on(LayerId::Base);
    ui.painter()
        .fill(area, 0.0, [tint[0], tint[1], tint[2], 255]);
    ui.painter().on(LayerId::Page);
    area
}

/// Plain rows of a list, a dot and two lines each, in `from..to` of `area`'s width: what a screen
/// shows between its bars, so the bars read as chrome.
pub(super) fn plain_rows(ui: &mut Ui<'_>, area: Rect, from: f32, to: f32) {
    const ROW: f32 = 64.0;
    let t = *ui.theme();
    let p = ui.painter();
    p.push_clip(Rect::new(area.x, from, area.w, (to - from).max(0.0)));
    let mut y = from + 8.0;
    while y < to {
        p.fill(
            Rect::new(area.x + 16.0, y + 12.0, 40.0, 40.0),
            20.0,
            t.surface_hover,
        );
        p.fill(
            Rect::new(area.x + 68.0, y + 16.0, 150.0, 12.0),
            6.0,
            t.surface_active,
        );
        p.fill(
            Rect::new(area.x + 68.0, y + 36.0, 240.0, 10.0),
            5.0,
            t.surface_hover,
        );
        p.fill(
            Rect::new(area.x + 68.0, y + ROW - 0.5, area.w - 68.0, 1.0),
            0.0,
            t.surface_separator,
        );
        y += ROW;
    }
    p.pop_clip();
}

const LONG: &str = "A title far too long to fit between the back button and the actions";

/// What the bar shows: the app's choice, which the knobs turn.
#[derive(Debug, Clone, Copy)]
struct Sheet {
    back: bool,
    actions: bool,
    long: bool,
}

fn plain() -> Box<dyn PageState> {
    Box::new(Sheet {
        back: false,
        actions: false,
        long: false,
    })
}

fn back_actions() -> Box<dyn PageState> {
    Box::new(Sheet {
        back: true,
        actions: true,
        long: false,
    })
}

fn long_title() -> Box<dyn PageState> {
    Box::new(Sheet {
        back: true,
        actions: true,
        long: true,
    })
}

impl PageState for Sheet {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let area = backdrop(ui, stage);
        let safe = ui.safe_area();
        let bar = Rect::new(area.x, area.y, area.w, safe.top + HEIGHT);
        plain_rows(ui, area, bar.bottom(), area.bottom() - safe.bottom);
        let actions = [
            NavAction::new(icon::SEARCH, "Search"),
            NavAction::new(icon::ELLIPSIS, "More"),
        ];
        let mut nav = NavBar::new(if self.long { LONG } else { "Inbox" });
        if self.back {
            nav = nav.with_back("Mail");
        }
        if self.actions {
            nav = nav.with_actions(&actions);
        }
        ui.nav_bar("nav-bar", bar, &nav);
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.toggle(ui, "Back button", &mut self.back);
        panel.toggle(ui, "Actions", &mut self.actions);
        panel.toggle(ui, "Long title", &mut self.long);
    }
}

const SCENES: &[UiScene] = &[
    phone("phone-nav-bar-title", plain, &[SETUP], 300),
    phone("phone-nav-bar-back-actions", back_actions, &[SETUP], 300),
    phone("phone-nav-bar-long-title", long_title, &[SETUP], 300),
];
