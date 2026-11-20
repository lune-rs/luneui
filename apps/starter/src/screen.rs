//! The screen to copy: title, field, Add button and list, laid out from any rect, theme colours.

use lune_ui::layout::{Insets, Track::Fixed, Track::Star, grid};
use lune_ui::ui::Ui;
use lune_ui::widgets::button::ButtonKind;
use lune_ui::widgets::label::LabelRole;
use lune_ui::widgets::list_row::{ListRow, SIDE_PANEL_PADDING as PAD, height};
use lune_ui::widgets::virtual_list::{Heights, ListOptions, RowKey, SelectionMode};

/// What the app keeps between frames: the field's text and the list's items.
pub(crate) struct Screen {
    pub(crate) text: String,
    pub(crate) items: Vec<String>,
}

impl Screen {
    pub(crate) fn new() -> Self {
        let items = ["Inbox", "Today", "Upcoming", "Done", "Archive"].map(String::from);
        Self {
            text: String::new(),
            items: items.to_vec(),
        }
    }

    /// Draws a frame of the screen in `window` (logical pixels) and applies what the user did.
    pub(crate) fn draw(&mut self, ui: &mut Ui<'_>, window: lune_ui::paint::Rect) {
        let background = ui.theme().glass.window.tint;
        ui.painter().fill(window, 0.0, background);
        let rows = [Fixed(24.0), Fixed(32.0), Star(1.0)];
        let page = grid(window, &[Star(1.0)], &rows, 0.0, 12.0, Insets::all(16.0));
        let at = page.cell(0, 0);
        ui.label("title", (at.x, at.y), "starter", LabelRole::Title);
        let cols = [Star(1.0), Fixed(72.0)];
        let form = grid(page.cell(0, 1), &cols, &[Star(1.0)], 8.0, 0.0, Insets::ZERO);
        let (field, add) = (form.cell(0, 0), form.cell(1, 0));
        ui.text_field("field", field, &mut self.text, "Item", Default::default());
        if ui.button("add", add, "Add", ButtonKind::Primary).clicked && !self.text.is_empty() {
            self.items.push(std::mem::take(&mut self.text));
        }
        let opts = ListOptions {
            count: self.items.len(),
            heights: Heights::Fixed(height(&ListRow::default(), PAD)),
            selection: SelectionMode::Single,
            index_of: None,
            label: "Items",
        };
        let key = |i| RowKey(i as u64);
        ui.virtual_list("list", page.cell(0, 2), &opts, key, |ui, ctx| {
            let name = &self.items[ctx.index];
            ui.list_row(
                ctx,
                &ListRow {
                    name,
                    ..ListRow::default()
                },
                PAD,
            )
        });
    }
}
