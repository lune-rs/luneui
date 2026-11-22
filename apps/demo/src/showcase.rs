//! The demo's screens: a three-tab phone app built only from the toolkit's public components.
//!
//! Home is a navigation stack: a list of messages (pull to refresh, swipe row actions, a search
//! field) and, one level down, a detail page whose "Options" action opens a bottom sheet. Cards is
//! a pager of five cards. Settings is two toggles and a slider. A tab bar sits at the bottom.
//!
//! Everything here is a pure function of the app's state and the frame's input, shared by the
//! window shell (`lib.rs`), the headless `--dump` (`dump.rs`) and the tests: [`Layout`] is where
//! things sit (a script aims at it), [`Showcase`] is the state and its screens, [`Demo`] is what
//! one frame needs besides the GPU, and [`Headless`] drives frames in app time with no window.

use std::time::Duration;

use lune_ui::icon;
use lune_ui::input::{Event, FrameOutput, RawInput, keymap};
use lune_ui::layout::Insets;
use lune_ui::lunegit_dark::THEME;
use lune_ui::motion::{Cadence, MotionTier};
use lune_ui::paint::Rect;
use lune_ui::scene::{LayerId, Scene};
use lune_ui::shortcuts::Shortcuts;
use lune_ui::strings::{DisplayFormat, StringTable};
use lune_ui::text::{Spec, Style, TextCache, TextSystem};
use lune_ui::ui::{FrameConfig, Sense, Ui, UiStore};
use lune_ui::widgets::bottom_sheet::{BottomSheet, Detent};
use lune_ui::widgets::button::ButtonKind;
use lune_ui::widgets::field::FieldState;
use lune_ui::widgets::nav_bar::{self, NavAction, NavBar};
use lune_ui::widgets::pager::PagerOptions;
use lune_ui::widgets::scroll_area::{Extent, ScrollOptions};
use lune_ui::widgets::slider::SliderOptions;
use lune_ui::widgets::swipe_actions::{SwipeAction, SwipeActions, SwipeEdge};
use lune_ui::widgets::tab_bar::{self, TabBarItem};
use lune_ui::widgets::tone::Tone;

/// A list row's height.
pub const ROW_H: f32 = 64.0;
/// The margin between the window's edge and content.
pub const MARGIN: f32 = 16.0;
/// The search field's height, and the room above and below it.
pub const SEARCH_H: f32 = 36.0;
const SEARCH_GAP: f32 = 8.0;
const LIST_GAP: f32 = 12.0;
/// How long a refresh takes, in app time, before it adds a row.
pub const REFRESH_WORK: Duration = Duration::from_millis(1500);
/// The pager's card count.
pub const CARDS: usize = 5;
/// The room the pager's dots take below a card, and the gap above a card.
const DOTS_ROOM: f32 = 36.0;
const CARD_GAP: f32 = 12.0;
/// A settings row's height, and the slider row's.
const SETTINGS_ROW: f32 = 52.0;
const SLIDER_ROW: f32 = 76.0;
const SETTINGS_TOP: f32 = 24.0;
const PANEL_RADIUS: f32 = 12.0;
/// A sheet's list row.
const SHEET_ROW: f32 = 48.0;
/// The id of the search field's key, in the Home list's scope.
const SEARCH_KEY: &str = "search";

/// The three tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// The message list and its detail.
    #[default]
    Home,
    /// The pager of cards.
    Cards,
    /// Two toggles and a slider.
    Settings,
}

impl Tab {
    /// The tabs in bar order.
    pub const ALL: [Tab; 3] = [Tab::Home, Tab::Cards, Tab::Settings];

    /// Its place in the tab bar.
    pub fn index(self) -> usize {
        match self {
            Tab::Home => 0,
            Tab::Cards => 1,
            Tab::Settings => 2,
        }
    }

    /// The tab at bar place `i` (the last one for a place past the end).
    pub fn from_index(i: usize) -> Tab {
        Tab::ALL[i.min(Tab::ALL.len() - 1)]
    }
}

/// Where everything sits for a window of `size` logical points with `safe` the edges the system
/// covers. The screens lay out from it and a script aims at it, so one script fits every size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// The window's size.
    pub size: (f32, f32),
    /// The safe area.
    pub safe: Insets,
    /// The whole window.
    pub window: Rect,
    /// The tab bar, running down through the bottom inset.
    pub tab_bar: Rect,
    /// The area above the tab bar: what a tab's content fills.
    pub content: Rect,
    /// A level's navigation bar: the top inset and the bar's row.
    pub nav_bar: Rect,
    /// The area between the navigation bar and the tab bar.
    pub body: Rect,
    /// Home's search field.
    pub search: Rect,
    /// Home's list, from under the search field to the tab bar.
    pub list: Rect,
}

impl Layout {
    /// The layout of a window of `size` with `safe` insets.
    pub fn new(size: (f32, f32), safe: Insets) -> Self {
        let (w, h) = size;
        let tab_h = tab_bar::HEIGHT + safe.bottom;
        let tab_rect = Rect::new(0.0, h - tab_h, w, tab_h);
        let content = Rect::new(0.0, 0.0, w, tab_rect.y.max(0.0));
        let nav = Rect::new(0.0, 0.0, w, safe.top + nav_bar::HEIGHT);
        let body = Rect::new(0.0, nav.bottom(), w, (tab_rect.y - nav.bottom()).max(0.0));
        let search = Rect::new(
            MARGIN + safe.left,
            nav.bottom() + SEARCH_GAP,
            (w - 2.0 * MARGIN - safe.left - safe.right).max(0.0),
            SEARCH_H,
        );
        let list_y = search.bottom() + LIST_GAP;
        Self {
            size,
            safe,
            window: Rect::new(0.0, 0.0, w, h),
            tab_bar: tab_rect,
            content,
            nav_bar: nav,
            body,
            search,
            list: Rect::new(0.0, list_y, w, (tab_rect.y - list_y).max(0.0)),
        }
    }

    /// The middle of tab `i`'s item in the tab bar.
    pub fn tab_item_center(&self, i: usize) -> (f32, f32) {
        let w = self.tab_bar.w / Tab::ALL.len() as f32;
        (
            self.tab_bar.x + w * (i as f32 + 0.5),
            self.tab_bar.y + tab_bar::HEIGHT / 2.0,
        )
    }

    /// The middle of Home's `n`-th shown row with the list scrolled by `offset` (and no refresh
    /// band open).
    pub fn row_center(&self, n: usize, offset: f32) -> (f32, f32) {
        (
            self.list.x + self.list.w / 2.0,
            self.list.y + (n as f32 + 0.5) * ROW_H - offset,
        )
    }

    /// The navigation bar's back button, at its left end.
    pub fn back_center(&self) -> (f32, f32) {
        (
            self.nav_bar.x + 32.0,
            self.nav_bar.bottom() - nav_bar::HEIGHT / 2.0,
        )
    }

    /// The navigation bar's last action (a bar's actions are 44 wide each, at its right end).
    pub fn action_center(&self) -> (f32, f32) {
        (
            self.nav_bar.right() - 30.0,
            self.nav_bar.bottom() - nav_bar::HEIGHT / 2.0,
        )
    }

    /// The pager's card (every page has one in the same place).
    pub fn card(&self) -> Rect {
        Rect::new(
            self.body.x + MARGIN + self.safe.left,
            self.body.y + CARD_GAP,
            (self.body.w - 2.0 * MARGIN - self.safe.left - self.safe.right).max(0.0),
            (self.body.h - CARD_GAP - DOTS_ROOM).max(0.0),
        )
    }

    /// Settings row `i` (0 and 1 the toggles, 2 the slider), inside the group.
    pub fn settings_row(&self, i: usize) -> Rect {
        let x = self.body.x + MARGIN + self.safe.left;
        let w = (self.body.w - 2.0 * MARGIN - self.safe.left - self.safe.right).max(0.0);
        let y = self.body.y + SETTINGS_TOP;
        match i {
            0 => Rect::new(x, y, w, SETTINGS_ROW),
            1 => Rect::new(x, y + SETTINGS_ROW, w, SETTINGS_ROW),
            _ => Rect::new(x, y + 2.0 * SETTINGS_ROW, w, SLIDER_ROW),
        }
    }

    /// The middle of toggle `i`'s switch (right-aligned in its row).
    pub fn toggle_center(&self, i: usize) -> (f32, f32) {
        let r = self.settings_row(i);
        (r.right() - MARGIN - 20.0, r.y + r.h / 2.0)
    }

    /// The rect the slider is drawn in, and so the track it spans left to right.
    pub fn slider_rect(&self) -> Rect {
        let r = self.settings_row(2);
        Rect::new(
            r.x + MARGIN,
            r.y + 40.0,
            (r.w - 2.0 * MARGIN).max(0.0),
            24.0,
        )
    }

    /// The slider's track as `(left, right, y)`, where the knob's centre can be.
    pub fn slider_track(&self) -> (f32, f32, f32) {
        let r = self.slider_rect();
        (r.x, r.right(), r.y + r.h / 2.0)
    }
}

/// One message in Home's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Its key: it stays with the row when the rows move.
    pub id: u32,
    /// The sender, the row's title.
    pub title: String,
    /// The line under it.
    pub preview: String,
}

/// What a row's swipe button did last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    /// The leading button: moved the row to the top.
    Pin,
    /// The trailing button next to the row's content: removed the row.
    Archive,
    /// The outermost trailing button, also the full swipe: removed the row.
    Delete,
}

/// The messages the list starts with: the sender and a line under it. The first has a title long
/// enough to elide in a navigation bar; several previews are long enough to elide in a row.
const MESSAGES: [(&str, &str); 30] = [
    (
        "Katherine Johnson and the Apollo guidance team",
        "Trajectory figures for Thursday's launch window, checked twice by hand and once by machine",
    ),
    ("Grace Hopper", "The compiler is ready for review"),
    (
        "Alan Turing",
        "Re: the halting problem, again, with a counter-example this time",
    ),
    ("Ada Lovelace", "Notes on the analytical engine"),
    ("Edsger Dijkstra", "A note on goto statements"),
    ("Margaret Hamilton", "Flight software, rev 4"),
    ("Donald Knuth", "Volume 4, fascicle 6 proofs"),
    ("Barbara Liskov", "Substitution, and when it holds"),
    ("Dennis Ritchie", "Re: the type of an array"),
    ("Radia Perlman", "Spanning trees over lunch"),
    ("Tim Berners-Lee", "A proposal, vague but exciting"),
    (
        "Hedy Lamarr",
        "Frequency hopping: the patent drawings are attached",
    ),
    ("Claude Shannon", "A mathematical theory of the weekend"),
    ("John McCarthy", "Lisp, the next fifty years"),
    ("Frances Allen", "Optimising compilers: the reading list"),
    (
        "Ken Thompson",
        "Trusting trust, revisited on a long train ride home",
    ),
    ("Leslie Lamport", "The time at which things happen"),
    ("Shafi Goldwasser", "Zero knowledge, one coffee"),
    ("Vint Cerf", "Packets for the interplanetary network"),
    ("Linus Torvalds", "Re: a small patch that grew rather large"),
    ("Anita Borg", "The conference dinner seating plan"),
    ("Niklaus Wirth", "Lean software, a short reminder"),
    ("Sophie Wilson", "Instruction set, minimal"),
    (
        "Bjarne Stroustrup",
        "Templates, and what they were meant for",
    ),
    ("Joan Clarke", "The crossword was solved before lunch"),
    ("Seymour Cray", "Cooling, mostly"),
    (
        "Jean Sammet",
        "A history of programming languages, chapter two",
    ),
    ("Guido van Rossum", "Indentation, one more time"),
    ("Mary Allen Wilkes", "The home terminal is installed"),
    ("Brian Kernighan", "Hello, world: the original listing"),
];

/// The titles and previews of rows a refresh adds.
const FRESH: [(&str, &str); 4] = [
    ("Morning briefing", "Four new items since you last looked"),
    ("Weather office", "Clear, then a little rain by evening"),
    (
        "Library desk",
        "Your hold on the almanac is ready to collect",
    ),
    ("Ferry timetable", "The winter service starts on Monday"),
];

/// The cards: a tone for the disc, a title and a line.
const CARD_TEXT: [(Tone, &str, &str); CARDS] = [
    (Tone::Neutral, "Welcome", "Swipe sideways for the next card"),
    (
        Tone::Ok,
        "Everything in place",
        "Every screen here is drawn by the toolkit",
    ),
    (
        Tone::Data,
        "Made of components",
        "Bars, lists, sheets and pagers",
    ),
    (Tone::Warn, "Touch first", "Taps, swipes, drags and pulls"),
    (Tone::Err, "That is all", "Pull the other way to go back"),
];

/// The sheet's rows.
const SHEET_ITEMS: [&str; 4] = ["Share", "Duplicate", "Move to folder", "Add to favourites"];

/// The paragraph under a detail's title.
const BODY: &str = "Sent from the luneui demo. The bars, the list, the sheet and the gestures on this screen are drawn by the toolkit's own components, on the GPU, with nothing from the platform's widget set.";

/// What the app keeps: every value a screen reads or a check asks about.
#[derive(Debug, Clone)]
pub struct Showcase {
    /// The selected tab.
    pub tab: Tab,
    /// Home's navigation stack: 1 is the list, 2 the detail.
    pub depth: usize,
    /// Whether the options sheet is open (or opening).
    pub sheet_open: bool,
    /// The sheet's detent while it rests or goes to one (0 half, 1 full); `None` closed.
    pub sheet_detent: Option<usize>,
    /// The pager's current card.
    pub page: usize,
    /// How far Home's list is scrolled, in points.
    pub list_offset: f64,
    /// How many refreshes have finished.
    pub refreshes: u32,
    /// When the running refresh started, in app time.
    pub refresh_since: Option<Duration>,
    /// Home's rows, in order.
    pub rows: Vec<Row>,
    /// The row the detail shows.
    pub selected: Option<u32>,
    /// The search field's text.
    pub search: String,
    /// The settings' two toggles.
    pub toggles: [bool; 2],
    /// The settings' slider, 0 to 1.
    pub slider: f32,
    /// What a row's swipe button did last, with the row's title.
    pub last_action: Option<(RowAction, String)>,
    /// Focus the search field when it is next drawn (the shell's `--focus-field`).
    pub want_focus: bool,
    next_id: u32,
}

impl Default for Showcase {
    fn default() -> Self {
        Self::new()
    }
}

/// What a frame's screens asked of the app, applied after they are drawn.
#[derive(Debug, Default)]
struct Asked {
    open: Option<u32>,
    back: bool,
    options: bool,
    refresh: bool,
    action: Option<(u32, SwipeEdge, usize)>,
}

/// `r` moved right by `dx`.
fn shift(r: Rect, dx: f32) -> Rect {
    Rect::new(r.x + dx, r.y, r.w, r.h)
}

/// The window's colour, opaque.
fn tint(ui: &Ui<'_>) -> [u8; 4] {
    let t = ui.theme().glass.window.tint;
    [t[0], t[1], t[2], 255]
}

impl Showcase {
    /// The app as it starts: Home, its list at the top, nothing open, the slider at 0.3.
    pub fn new() -> Self {
        let rows = MESSAGES
            .iter()
            .enumerate()
            .map(|(i, (t, p))| Row {
                id: i as u32,
                title: (*t).to_owned(),
                preview: (*p).to_owned(),
            })
            .collect();
        Self {
            tab: Tab::Home,
            depth: 1,
            sheet_open: false,
            sheet_detent: None,
            page: 0,
            list_offset: 0.0,
            refreshes: 0,
            refresh_since: None,
            rows,
            selected: None,
            search: String::new(),
            toggles: [true, false],
            slider: 0.3,
            last_action: None,
            want_focus: false,
            next_id: MESSAGES.len() as u32,
        }
    }

    /// The app in a fixed state for a dump: `tab`, Home at `depth` (2 shows the first row's detail)
    /// and the sheet open or not.
    pub fn with_state(tab: Tab, depth: usize, sheet_open: bool) -> Self {
        let mut s = Self::new();
        s.tab = tab;
        s.depth = depth.clamp(1, 2);
        s.sheet_open = sheet_open;
        s.selected = s.rows.first().map(|r| r.id);
        s
    }

    /// The titles of the rows, in order.
    pub fn row_titles(&self) -> Vec<&str> {
        self.rows.iter().map(|r| r.title.as_str()).collect()
    }

    /// The row the detail shows.
    fn selected_row(&self) -> Option<&Row> {
        let id = self.selected?;
        self.rows.iter().find(|r| r.id == id)
    }

    /// Draws one frame of the whole app: the tab bar, the selected tab and the sheet.
    pub fn draw(&mut self, ui: &mut Ui<'_>, l: &Layout) {
        let tint = tint(ui);
        let p = ui.painter();
        p.on(LayerId::Base);
        p.fill(l.window, 0.0, tint);
        p.on(LayerId::Page);

        let items = [
            TabBarItem::new(icon::NOTE, "Home"),
            TabBarItem::new(icon::STAR_FULL, "Cards"),
            TabBarItem::new(icon::GEAR, "Settings"),
        ];
        let mut selected = self.tab.index();
        ui.tab_bar("tabs", l.tab_bar, &items, &mut selected);
        self.tab = Tab::from_index(selected);

        match self.tab {
            Tab::Home => self.home(ui, l),
            Tab::Cards => self.cards(ui, l),
            Tab::Settings => self.settings(ui, l),
        }
        self.sheet(ui);
    }

    // ---- Home ----

    fn home(&mut self, ui: &mut Ui<'_>, l: &Layout) {
        let mut asked = Asked::default();
        let depth = self.depth;
        let resp = ui.nav_stack("stack", l.content, depth, |ui, level, r| {
            if level == 0 {
                self.list_level(ui, r, l, &mut asked);
            } else {
                self.detail_level(ui, r, l, &mut asked);
            }
        });
        if asked.back {
            ui.nav_stack_pop("stack");
        }
        if resp.popped {
            self.depth = 1;
        }
        if asked.options {
            self.sheet_open = true;
        }
        if let Some(id) = asked.open {
            self.selected = Some(id);
            self.depth = 2;
        }
        if asked.refresh && self.refresh_since.is_none() {
            self.refresh_since = Some(ui.time());
        }
        if let Some((id, edge, i)) = asked.action {
            self.apply_action(id, edge, i);
        }
    }

    /// What a swipe button of row `id` does: edge and index as the buttons are listed.
    fn apply_action(&mut self, id: u32, edge: SwipeEdge, i: usize) {
        let Some(at) = self.rows.iter().position(|r| r.id == id) else {
            return;
        };
        let kind = match (edge, i) {
            (SwipeEdge::Leading, _) => RowAction::Pin,
            (_, 0) => RowAction::Archive,
            _ => RowAction::Delete,
        };
        let row = self.rows.remove(at);
        self.last_action = Some((kind, row.title.clone()));
        if kind == RowAction::Pin {
            self.rows.insert(0, row);
        }
    }

    /// Home's level 0 in `r`: the bar, the search field and the list.
    fn list_level(&mut self, ui: &mut Ui<'_>, r: Rect, l: &Layout, asked: &mut Asked) {
        let dx = r.x - l.content.x;
        let background = tint(ui);
        ui.painter().fill(r, 0.0, background);
        let now = ui.time();

        // A refresh that has run its time adds a row at the top.
        match self.refresh_since {
            Some(since) if now.saturating_sub(since) >= REFRESH_WORK => {
                self.refresh_since = None;
                self.refreshes += 1;
                let (title, preview) = FRESH[self.refreshes as usize % FRESH.len()];
                self.rows.insert(
                    0,
                    Row {
                        id: self.next_id,
                        title: format!("{title} {}", self.refreshes),
                        preview: preview.to_owned(),
                    },
                );
                self.next_id += 1;
            }
            Some(since) => ui.request_repaint_at(since + REFRESH_WORK),
            None => {}
        }

        if std::mem::take(&mut self.want_focus) {
            let id = ui.id(SEARCH_KEY);
            ui.request_focus(id);
        }
        ui.text_field(
            SEARCH_KEY,
            shift(l.search, dx),
            &mut self.search,
            "Search",
            FieldState::default(),
        );

        let query = self.search.to_lowercase();
        let shown: Vec<&Row> = self
            .rows
            .iter()
            .filter(|row| {
                query.is_empty()
                    || row.title.to_lowercase().contains(&query)
                    || row.preview.to_lowercase().contains(&query)
            })
            .collect();
        let leading = [SwipeAction::new("Pin")
            .with_glyph(icon::ARROW_UP)
            .with_tone(Tone::Ok)];
        let trailing = [
            SwipeAction::new("Archive").with_glyph(icon::ARROW_DOWN),
            SwipeAction::new("Delete")
                .with_glyph(icon::CLOSE)
                .with_tone(Tone::Err),
        ];
        let actions = SwipeActions::new()
            .with_leading(&leading)
            .with_trailing(&trailing)
            .with_full_swipe(true);
        let list = shift(l.list, dx);
        let refreshing = self.refresh_since.is_some();
        let res = ui.pull_to_refresh(
            "list",
            list,
            refreshing,
            &ScrollOptions::default(),
            |ui, view| {
                for (n, row) in shown.iter().enumerate() {
                    let rr = Rect::new(
                        view.viewport.x,
                        view.viewport.y + n as f32 * ROW_H,
                        view.viewport.w,
                        ROW_H,
                    );
                    let swipe = ui.swipe_actions(("row", row.id), rr, &actions, |ui, c| {
                        message_row(ui, row, c)
                    });
                    if swipe.value {
                        asked.open = Some(row.id);
                    }
                    if let Some((edge, i)) = swipe.action {
                        asked.action = Some((row.id, edge, i));
                    }
                }
                if shown.is_empty() {
                    let t = *ui.theme();
                    let spec = Spec::plain("No messages", Style::ui(t.font_m, t.fg_secondary));
                    ui.painter().text_centered(&spec, view.viewport);
                }
                (
                    (),
                    Extent::new(
                        f64::from(view.viewport.w),
                        f64::from(ROW_H) * shown.len() as f64,
                    ),
                )
            },
        );
        self.list_offset = res.scroll.offset.1;
        if res.refresh {
            asked.refresh = true;
        }
        let refresh = [NavAction::new(icon::SYNC, "Refresh")];
        let bar = ui.nav_bar(
            "bar",
            shift(l.nav_bar, dx),
            &NavBar::new("Home").with_actions(&refresh),
        );
        if bar.action.is_some() {
            asked.refresh = true;
        }
    }

    /// Home's level 1 in `r`: the bar with its back button and Options, and the message.
    fn detail_level(&mut self, ui: &mut Ui<'_>, r: Rect, l: &Layout, asked: &mut Asked) {
        let dx = r.x - l.content.x;
        let t = *ui.theme();
        let background = tint(ui);
        ui.painter().fill(r, 0.0, background);
        let nav = shift(l.nav_bar, dx);
        let (title, preview) = self
            .selected_row()
            .map_or(("", ""), |row| (row.title.as_str(), row.preview.as_str()));
        let x = nav.x + MARGIN + l.safe.left;
        let w = (nav.w - 2.0 * MARGIN - l.safe.left - l.safe.right).max(0.0);
        let mut y = nav.bottom() + MARGIN;
        let p = ui.painter();
        let heading = Spec::plain(title, Style::ui(t.font_title, t.fg_primary).bold()).wrapped(w);
        let (_, h) = p.text(&heading, x, y);
        y += h + 8.0;
        let lead = Spec::plain(preview, Style::ui(t.font_m, t.fg_secondary)).wrapped(w);
        let (_, h) = p.text(&lead, x, y);
        y += h + MARGIN;
        let body = Spec::plain(BODY, Style::ui(t.font_base, t.fg_primary)).wrapped(w);
        p.text(&body, x, y);

        let options = [NavAction::new(icon::ELLIPSIS, "Options")];
        let bar = ui.nav_bar(
            "bar",
            nav,
            &NavBar::new(title).with_back("Home").with_actions(&options),
        );
        if bar.back {
            asked.back = true;
        }
        if bar.action.is_some() {
            asked.options = true;
        }
    }

    /// The options sheet: a list of labels and Done.
    fn sheet(&mut self, ui: &mut Ui<'_>) {
        let mut open = self.sheet_open;
        let mut done = false;
        let detents = [Detent::Fraction(0.5), Detent::Fraction(1.0)];
        let sheet = BottomSheet::new("Options")
            .with_detents(&detents)
            .with_grabber(true);
        let res = ui.bottom_sheet("sheet", &mut open, &sheet, |ui, rect| {
            let t = *ui.theme();
            let x = rect.x + MARGIN;
            let w = (rect.w - 2.0 * MARGIN).max(0.0);
            let mut y = rect.y;
            for item in SHEET_ITEMS {
                let spec = Spec::plain(item, Style::ui(t.font_m, t.fg_primary));
                let p = ui.painter();
                p.text_in(&spec, Rect::new(x, y, w, SHEET_ROW), 0.0);
                p.fill(
                    Rect::new(x, y + SHEET_ROW - 1.0, w, 1.0),
                    0.0,
                    t.surface_separator,
                );
                y += SHEET_ROW;
            }
            let button = Rect::new(x, y + 12.0, w, 44.0);
            done = ui
                .button("done", button, "Done", ButtonKind::Primary)
                .clicked;
        });
        if done {
            open = false;
        }
        self.sheet_open = open;
        self.sheet_detent = res.detent;
    }

    // ---- Cards ----

    fn cards(&mut self, ui: &mut Ui<'_>, l: &Layout) {
        let t = *ui.theme();
        let card = l.card();
        let pager = Rect::new(l.body.x, l.body.y, l.body.w, l.body.h);
        ui.pager(
            "pager",
            pager,
            CARDS,
            &mut self.page,
            &PagerOptions::new("Cards").with_dots(true),
            |ui, i, r| {
                let (tone, title, line) = CARD_TEXT[i];
                let c = Rect::new(
                    r.x + (card.x - pager.x),
                    r.y + (card.y - pager.y),
                    card.w,
                    card.h,
                );
                let p = ui.painter();
                p.fill(c, 16.0, t.surface_overlay);
                p.ring(c, 16.0, 1.0, t.border_subtle, false);
                let disc = Rect::new(c.x + MARGIN, c.y + MARGIN, 44.0, 44.0);
                p.fill(disc, 22.0, tone.color(&t));
                let title = Spec::plain(title, Style::ui(t.font_title, t.fg_primary).bold());
                let title = p.elide(&title, c.w - 2.0 * MARGIN);
                p.text_in(
                    &title,
                    Rect::new(c.x + MARGIN, c.y + MARGIN + 60.0, c.w - 2.0 * MARGIN, 28.0),
                    0.0,
                );
                let line = Spec::plain(line, Style::ui(t.font_m, t.fg_secondary));
                let line = p.elide(&line, c.w - 2.0 * MARGIN);
                p.text_in(
                    &line,
                    Rect::new(c.x + MARGIN, c.y + MARGIN + 92.0, c.w - 2.0 * MARGIN, 22.0),
                    0.0,
                );
            },
        );
        ui.nav_bar("bar", l.nav_bar, &NavBar::new("Cards"));
    }

    // ---- Settings ----

    fn settings(&mut self, ui: &mut Ui<'_>, l: &Layout) {
        let t = *ui.theme();
        let (first, second, third) = (l.settings_row(0), l.settings_row(1), l.settings_row(2));
        let group = Rect::new(first.x, first.y, first.w, third.bottom() - first.y);
        let p = ui.painter();
        p.fill(group, PANEL_RADIUS, t.surface_overlay);
        for sep in [first.bottom(), second.bottom()] {
            p.fill(
                Rect::new(group.x + MARGIN, sep - 0.5, group.w - MARGIN, 1.0),
                0.0,
                t.surface_separator,
            );
        }
        for (i, name) in ["Notifications", "Sounds"].into_iter().enumerate() {
            let row = l.settings_row(i);
            let spec = Spec::plain(name, Style::ui(t.font_m, t.fg_primary));
            ui.painter().text_in(
                &spec,
                Rect::new(row.x + MARGIN, row.y, row.w - 80.0, row.h),
                0.0,
            );
            let switch = Rect::new(row.right() - MARGIN - 40.0, row.y, 40.0, row.h);
            ui.toggle(("toggle", i), switch, &mut self.toggles[i], name);
        }
        let label = Spec::plain("Volume", Style::ui(t.font_m, t.fg_primary));
        let value = format!("{}%", (self.slider * 100.0).round() as i32);
        let value = Spec::plain(value, Style::ui(t.font_m, t.fg_secondary));
        let p = ui.painter();
        p.text_in(
            &label,
            Rect::new(third.x + MARGIN, third.y + 8.0, third.w - 80.0, 28.0),
            0.0,
        );
        p.text_in(
            &value,
            Rect::new(third.right() - MARGIN - 56.0, third.y + 8.0, 56.0, 28.0),
            0.0,
        );
        let opts = SliderOptions {
            min: 0.0,
            max: 1.0,
            step: 0.0,
            name: "Volume",
        };
        ui.slider("volume", l.slider_rect(), &mut self.slider, &opts);
        ui.nav_bar("bar", l.nav_bar, &NavBar::new("Settings"));
    }
}

/// One message in `r` (the row's content, which moves with a swipe): an opaque background, an
/// initial in a disc, the sender, a line under it, a separator. Returns whether it was tapped.
fn message_row(ui: &mut Ui<'_>, row: &Row, r: Rect) -> bool {
    let t = *ui.theme();
    let id = ui.id("tap");
    let tap = ui.interact(id, r, Sense::CLICK);
    let background = tint(ui);
    let p = ui.painter();
    p.fill(r, 0.0, background);
    if tap.pressed {
        p.fill(r, 0.0, t.surface_hover);
    }
    let disc = Rect::new(r.x + MARGIN, r.y + 12.0, 40.0, 40.0);
    p.fill(disc, 20.0, t.surface_active);
    let initial: String = row.title.chars().take(1).collect();
    let initial = Spec::plain(initial, Style::ui(t.font_m, t.fg_primary).bold());
    p.text_centered(&initial, disc);
    let room = r.w - 68.0 - MARGIN;
    let name = Spec::plain(row.title.as_str(), Style::ui(t.font_m, t.fg_primary));
    let name = p.elide(&name, room);
    p.text_in(&name, Rect::new(r.x + 68.0, r.y + 10.0, room, 22.0), 0.0);
    let sub = Spec::plain(row.preview.as_str(), Style::ui(t.font_s, t.fg_secondary));
    let sub = p.elide(&sub, room);
    p.text_in(&sub, Rect::new(r.x + 68.0, r.y + 34.0, room, 20.0), 0.0);
    p.fill(
        Rect::new(r.x + 68.0, r.bottom() - 1.0, r.w - 68.0, 1.0),
        0.0,
        t.surface_separator,
    );
    tap.enabled && tap.clicked
}

/// What the shell adds to a frame beyond the screens: the probes' clipboard requests and a wake.
#[derive(Debug, Clone, Default)]
pub struct Extras {
    /// Text to write to the clipboard in this frame.
    pub clipboard_write: Option<String>,
    /// Ask for a clipboard read in this frame.
    pub clipboard_read: bool,
    /// Ask for a frame at this app time.
    pub repaint_at: Option<Duration>,
}

/// One frame's result.
#[derive(Debug)]
pub struct Rendered {
    /// The display list to draw.
    pub scene: Scene,
    /// What the frame asks of the app: repaint, cursor, clipboard, accessibility.
    pub out: FrameOutput,
    /// The safe area the frame read.
    pub safe: Insets,
}

/// What one frame needs besides the GPU: the toolkit's store, the two tables every frame reads and
/// the app's state.
#[derive(Debug, Default)]
pub struct Demo {
    store: UiStore,
    strings: StringTable,
    shortcuts: Shortcuts,
    /// The app's state.
    pub showcase: Showcase,
}

impl Demo {
    /// A demo at its start.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs one frame over `input` and returns what it drew and asked for. The input carries the
    /// size, the scale, the time and the events (the safe area among them).
    pub fn frame(
        &mut self,
        input: &RawInput,
        text: &mut dyn TextSystem,
        extras: &Extras,
    ) -> Rendered {
        let config = FrameConfig {
            theme: THEME,
            tier: MotionTier::Full,
            busy_cadence: Cadence::BUSY,
            strings: &self.strings,
            arg_format: &DisplayFormat,
            keymap: keymap(),
            shortcuts: &self.shortcuts,
        };
        let mut ui = Ui::begin(&mut self.store, input, config, text);
        let safe = ui.safe_area();
        let layout = Layout::new(input.size, safe);
        if let Some(text) = &extras.clipboard_write {
            ui.clipboard_write(text.clone());
        }
        if extras.clipboard_read {
            ui.clipboard_read();
        }
        if let Some(at) = extras.repaint_at {
            ui.request_repaint_at(at);
        }
        self.showcase.draw(&mut ui, &layout);
        let (scene, out) = ui.finish();
        Rendered { scene, out, safe }
    }
}

/// Drives a [`Demo`] in app time with no window and no GPU: the dump, the tests and the script's
/// headless run all go through it.
#[derive(Debug)]
pub struct Headless {
    /// The app.
    pub demo: Demo,
    /// The fonts.
    pub text: TextCache,
    /// The window's size in logical points.
    pub size: (f32, f32),
    /// Device pixels per logical point.
    pub scale: f32,
    /// The safe area pushed with every frame.
    pub safe: Insets,
    /// App time: the time of the last frame.
    pub time: Duration,
}

impl Headless {
    /// A demo at time zero in a window of `size` at `scale` with `safe` insets.
    pub fn new(size: (f32, f32), scale: f32, safe: Insets) -> Self {
        Self {
            demo: Demo::new(),
            text: crate::text_cache(),
            size,
            scale,
            safe,
            time: Duration::ZERO,
        }
    }

    /// The layout of this window.
    pub fn layout(&self) -> Layout {
        Layout::new(self.size, self.safe)
    }

    /// One frame `dt` after the last, with `events` after the safe area.
    pub fn step(&mut self, dt: Duration, mut events: Vec<Event>) -> Rendered {
        self.time += dt;
        events.insert(0, Event::SafeArea(self.safe));
        let input = RawInput {
            size: self.size,
            scale: self.scale,
            time: self.time,
            focused: true,
            events,
        };
        self.demo.frame(&input, &mut self.text, &Extras::default())
    }

    /// `frames` frames `dt` apart with no events; returns the last.
    pub fn run(&mut self, frames: u32, dt: Duration) -> Option<Rendered> {
        let mut last = None;
        for _ in 0..frames {
            last = Some(self.step(dt, Vec::new()));
        }
        last
    }
}
