# Design recipes

How to put a screen together from what the toolkit already has. Seven recipes, each a function you
copy from `apps/gallery/src/pages/recipes.rs` and a gallery scene you can dump at any size. Draw
with these, then judge the dump with `docs/screen-review.md`. The smallest whole app is
`apps/starter/src/screen.rs`.

Every recipe lays out from the rect it is given and takes every colour from `ui.theme()`.

## View a recipe at another size

```
cargo run -p gallery -- --dump <dir> --only recipe-form
cargo run -p gallery -- --dump <dir> --only recipe-form --window 402x874 --scale 3
```

The second writes `recipe-form@3x-402x874.png`. The window page `Recipes` (the end of the sidebar's Data group) shows the
same functions live, one at a time, with a knob to pick the recipe.

## Layout primitives

`crates/lune-ui-core/src/layout.rs`, re-exported as `lune_ui::layout`. They draw nothing and return
rects.

- `stack` (`layout.rs:181`), `row` (`:223`) and `column` (`:228`): lay `Child` values along an axis
  inside a parent shrunk by `Insets`, with a gap and an `Align`.
- `grid` (`:283`) resolves column and row `Track`s into a `Grid` and `Grid::cell(col, row)` (`:249`)
  gives a cell's rect.
- `Size` (`:28`): `Fixed`, `Hug` (a size you measured) or `Fill(weight)`. `Track` (`:77`): `Fixed`,
  `Auto` or `Star(weight)`. `Align` (`:38`): `Start`, `Center`, `End`, `Stretch`. `Insets` (`:48`):
  `new`, `all`, `ZERO`. `Child` (`:87`): a `Size` along the axis and a cross size.

A recipe is a `grid` or a `row` of the rect it is given, with each cell handed to a composite.

## Sizes that exist

The theme holds colours and four font sizes, and nothing else: `font_s` 11, `font_base` 13,
`font_m` 14 and `font_title` 18 on a 24 px line (`crates/lune-ui-core/src/theme.rs:121-124`,
values at `:374-377`). **There are no shared spacing, radius or control-height tokens.** Each
composite holds its own consts; quote them, do not invent a second scale. The ones the recipes
meet:

| composite | consts |
| --- | --- |
| page card (`widgets/page.rs`) | `MARGIN` 0 10 10 6 `:35`, `RADIUS` 8 `:37`, `PAD` 16 `:39`, `TITLE_LINE` 24 `:41`, `SUBTITLE_LINE` 20 `:42`, `FOOTER_ROW` 32 `:47`, `FOOTER_H` 56 `:48` |
| panel (`widgets/panel.rs`) | `RADIUS` 8 `:30`, `TEXT_INSET` 8 `:32`, `HEADER_PAD` 12 16 `:34`, `TITLE_LINE` 20 `:36`, `BUTTON_ROW` 32 `:45`, `CARD_PAD` 12 `:47` |
| list row (`widgets/list_row.rs`) | `SIDE_PANEL_PADDING` 8 6 `:27`, `MASTER_PADDING` 12 6 `:29`, `NAME_LINE` 20 `:31`, `SMALL_LINE` 16 `:32`, `LINE_GAP` 4 `:34` |
| list (`widgets/virtual_list.rs`) | `HIGHLIGHT_INSET` 4 1 `:46` |
| table (`widgets/table.rs`) | `HEADER` 28 `:39`, `ROW` 28 `:41`, `PAD` 8 `:43`, `EDGE_PAD` 16 `:45` |
| sidebar (`widgets/sidebar.rs`) | `WIDTH` 208 `:25`, `NAV_PAD` 8 `:27`, `FOOTER_H` 40 `:34` |
| nav item (`widgets/nav_item.rs`) | `ROW_HEIGHT` 32 `:32`, `PITCH` 34 `:34`, `RADIUS` 6 `:36` |
| side panel (`widgets/side_panel.rs`) | `RADIUS` 8 `:32`, `PAD` 10 `:34` |
| form field (`widgets/form_field.rs`) | `LABEL_LINE` 20 `:18`, `NOTE_LINE` 16 `:19`, `GAP` 4 `:21`, `INLINE_GAP` 8 `:23` |
| button (`widgets/button.rs`) | `SMALL_HEIGHT` 24 `:30`; a normal button is as tall as the rect you give it, 32 by convention |
| dialog (`widgets/dialog.rs`) | confirm card 448 `:70`, form card 600 `:72` (private; fixed) |

The recipes themselves use three literals, named at the top of `recipes.rs`: `MARGIN` 16 (window
edge to content), `GAP` 12 (between blocks) and `CONTROL_H` 32.

## Job, composite, recipe

| job | composite | recipe scene | function |
| --- | --- | --- | --- |
| pick from many rows | `Ui::virtual_list` + `Ui::list_row` | `recipe-list` | `recipe_list` |
| compare rows by column | `Ui::table` | `recipe-table` | `recipe_table` |
| group related content | `Ui::panel` (+ `Ui::stat_strip`) | `recipe-panel` | `recipe_panel` |
| one titled screen with actions | `Ui::page` | `recipe-page` | `recipe_page` |
| move between screens | `Ui::sidebar` | `recipe-sidebar` | `recipe_sidebar` |
| collect values | `Ui::form_field` + `Ui::text_field` | `recipe-form` | `recipe_form` |
| confirm or interrupt | `Ui::dialog_with` | `recipe-dialog` | `recipe_dialog` |

Also see the pages `settings-page` (a settings screen with banners and rows) and `side-panel` (a
panel that slides in beside the content), which have no recipe of their own.

## The recipes

Each: when to use it, the function to copy, what bites.

### List: `recipe_list`

Use for a set the user picks one from, where each item has a name and a line or two of detail.
Copy `recipe_list`: a title, a `FieldOptions` search field and a `virtual_list` whose rows are
`ListRow { name, sub, badge, dot }` at the fixed height `list_row::height` gives. It draws a line
for the empty state.

- A list takes the wheel on the vertical axis only while it scrolls (D178); keep its rect to the
  space the screen leaves, not to its content's height.
- One Tab stop with roving focus: arrow keys move inside (D113); do not add a Tab stop per row.
- A row that needs a menu reports `context` on its response; the app opens `Ui::menu` (D292).
- Long names are elided by the row; do not wrap them.

### Table: `recipe_table`

Use to compare several values per row. Copy `recipe_table`: one `Fill` column and fixed numeric
columns (`num: true` puts the cell right-aligned in mono). Cells return `Cell::text(..)` with a
`CellTone`; a control in a cell draws itself and returns `Cell::Control`.

- The table elides text to the column; give the name column the fill so the fixed ones keep their
  width (`ColumnWidth::Fill` / `Fixed`).
- It scrolls vertically and keeps its header; it takes the wheel per axis like a list (D178).
- Sorting is the app's: the response says which column, the app orders the rows.

### Panel: `recipe_panel`

Use to group figures or a short list under a title. Copy `recipe_panel`: a `Strip` panel of
`StatOptions` cells over a `Flush` panel holding rows. A panel is an outline, not a fill; its
body kind (`Plain`, `Rows`, `Flush`, `Strip`, `Card`) sets the padding.

- A flush body's text is yours to inset by `TEXT_INSET` (the rounded clip cuts fills, not text).
- Elide what you draw by hand with `Painter::elide` (D114); the recipe does it for the names.
- A mono text sits a little higher in a row than a sans one: the recipe drops it `MONO_DROP`.

### Page: `recipe_page`

Use for the main screen of an area: a title, a subtitle, actions at the right, a body that
scrolls, an optional footer. Copy `recipe_page`: the `PageOptions`, one closure per `PageSlot`.

- The glass layers come from `ui.theme().glass`; the card names no preset.
- The body closure returns its content's height; the page scrolls when it is taller.
- The header's actions are aligned to the top of their slot.

### Sidebar: `recipe_sidebar`

Use to move between a few screens. Copy `recipe_sidebar`: a `row` of a `Fixed(WIDTH)` column and a
`Fill` content area; `SidebarBody::Nav` holds the entries and the selected key, and the status
footer is optional.

- The column is 208 px and does not narrow. Under about 560 px the content is left too little
  (the recipe then keeps one figure); an app on a narrow window uses a different pattern.
- Navigation rows are one roving Tab stop (D113).

### Form: `recipe_form`

Use to collect values. Copy `recipe_form`: `FormField` per field (label above or inline, a hint, an
error that replaces the hint), a `text_field` in its closure, and Cancel and Save in a `row`. Put
`FieldState { error: true }` on the field in error as well as the message on the `FormField`.

- `Ui::form_field` returns the height it drew; advance `y` by it.
- A control that cannot act is disabled with `with_enabled(false, ..)`; the widget consumes its
  keys first (D159).
- The form is at most 480 px wide: a wider window leaves empty space rather than a long field.

### Dialog: `recipe_dialog`

Use to confirm or to interrupt. Copy `recipe_dialog`: `Dialog { title, message, extra_h, confirm,
cancel }`, `DialogGlass::of(ui.theme())` and a body closure. It reports `confirmed` or `cancelled`
once; stop calling it the next frame to close it.

- It is an overlay: `Ui::dialog_with` puts it on the modal layer and dims and blurs what is behind
  (D137, D255). Never name a layer yourself.
- Anything the body opens (a combo list, a menu, a tooltip) draws above it on its own.
- **Limit:** the card is 448 px wide (600 for `DialogWidth::Form`) whatever the window. At 402 px
  it is wider than the window and its text and buttons are cut at the edges (dump
  `recipe-dialog` at `--window 402x874 --scale 3` to see it). Until the dialog adapts to a narrow
  window, a phone screen uses a page or a panel for the same job. `[mobile slot]`

## Phone layout

The mobile program has decided some of this (the first two lines) and not the rest; a line still
marked `[mobile slot]` is open, and the rules live in `docs/components/README.md`, the checklist
table.

- Safe area and keyboard inset (D344, D345; checklist row `safe-area-and-keyboard`, held by review):
  a recipe's `area` is the window less `MARGIN`; on a phone it is the safe area
  (`Ui::safe_area()`), and a form lifts by `Ui::keyboard_inset()`.
- `[mobile slot]` Touch hit area: every control answers a 44 pt touch even where it draws 32 px;
  the recipes' 32 px rows and buttons are drawn size, not the touch target.
- `[mobile slot]` No hover-only affordance: the list's hover highlight is not the only sign a row
  is pressable.
- Touch drag arbitration (D349, D350; checklist row `touch-drag-arbitration`, held by test): a
  scrolling list or table inside a page that also scrolls decides per axis who takes the drag (as
  the wheel does, D178); a drag widget claims the axes it uses with `Ui::set_drag_axes`, and the
  scroll area takes the other pan, so do not write a mechanism of your own.

Where the dumps at 402x874 at 3x show a limit today it is named under the recipe: the sidebar
(208 px column) and the dialog (448 px card).
