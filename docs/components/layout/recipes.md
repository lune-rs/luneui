---
id: layout.recipes
title: Screen design recipes
area: layout
status: done
consumers: any
source: apps/gallery/src/pages/recipes.rs, docs/design-recipes.md
owner: —
summary: Seven screens (list, table, panel, page, sidebar, form, dialog) built from the composites and the layout functions, each a function to copy and a scene to dump at any window size
---

## What it is

A gallery page `recipes` and a doc, `docs/design-recipes.md`, that show how to assemble a screen
from what the toolkit has: one function per pattern (`recipe_list`, `recipe_table`, `recipe_panel`,
`recipe_page`, `recipe_sidebar`, `recipe_form`, `recipe_dialog` in `apps/gallery/src/pages/recipes.rs`),
each laid out from the rect it is given with `lune_ui::layout`, every colour from `ui.theme()`. It
adds no library item and no theme token [D404].

## Needs

- Layout containers that lay out from a given rect, so one screen serves any window size
  (R-layout-1).
- An agent can find the composite for a job and copy a working function for it, and see it at a
  phone size, without a person looking [D404].

## Existing code

`apps/gallery/src/pages/recipes.rs` (the page, `recipe_*`, the doc test), `docs/design-recipes.md`.
The composites it uses are their own entries (`layout.panel`, `layout.page`, `layout.sidebar`,
`layout.virtualized-list`, `data.table`, `widget.form-field`, `overlay.dialog`).

## Spec

### Evidence

Pinned: this repo at the task `luneui-rs/ai-usability`. Read: the composites' sources for their
consts (`widgets/page.rs:35-48`, `panel.rs:30-49`, `list_row.rs:27-36`, `table.rs:39-51`,
`sidebar.rs:25-36`, `form_field.rs:18-25`), `layout.rs` for the primitives, and the gallery pages
`panel`, `sidebar`, `page`, `table`, `form-field` and `dialog` for how each composite is called.
The theme holds no spacing, radius or control-height token (`theme.rs:121-124` has the four font
sizes); the doc says so and quotes each composite's own consts instead of adding tokens.

### Anatomy

- `Recipes` page state: one recipe at a time, `Data` holds what the recipes keep (the filter, the
  selected screen, the form values, the dialog open flag). The knob picks the recipe.
- Each recipe takes the content rect and draws through a composite or a `grid`/`row`; none holds a
  coordinate that assumes the 1040x660 gallery window.
- `list`: title, search field, fixed-height rows with a dot, a name, an address line and a status
  badge; an empty state line. `table`: one fill column and two fixed, numbers in mono. `panel`: a
  strip panel over a flush panel of rows. `page`: the page card with a header action, a body and a
  footer. `sidebar`: the 208 px navigation column and a content strip. `form`: three fields (one in
  error, one inline) and Cancel and Save. `dialog`: a confirm dialog over the list.

### States

A rest state per recipe, plus what each shows on purpose: the list's empty line (a filter that
matches nothing), the form's error field and disabled Save, the sidebar's selected row, the
dialog open. Hover, press and focus are the composites' own and are drawn in their entries.

### Transitions

None of its own: the dialog's open is the dialog's (`overlay.dialog`) and the scene captures it
after 700 ms, settled.

### Input and focus

The recipes add no key handling. The list, the sidebar and the table are the composites' roving
Tab stops; the form's fields are ordinary Tab stops in call order.

### Retained state

- Transient: none of its own. App-owned: `Data`, passed every frame.
- Exit retention: no.

### Accessibility

No node of its own; every node is the composites' (`list`, `table`, `navigation`, `form` groups, the
dialog) [infra.accessibility].

### Strings

Demo text in English, literal in the page (a gallery fixture); the composites' own strings come
from the string table.

### Gallery scenes

- `recipe-list`: the list recipe at scale 2, settled.
- `recipe-table`: the table recipe, settled.
- `recipe-panel`: the strip panel over the flush panel, settled.
- `recipe-page`: the page card with header action, body and footer, settled.
- `recipe-sidebar`: the navigation column and the content strip, settled.
- `recipe-form`: the form with an error field and a disabled Save, settled.
- `recipe-dialog`: the confirm dialog over the list, captured at 700 ms, settled.

Any of them at another size: `gallery --dump <dir> --only recipe-form --window 402x874 --scale 3`.

### Painter primitives

- Existing: `fill`, `text`, `measure` and `elide`, only to draw a zebra row by hand in `recipe_panel`
  and `recipe_page`; everything else is a composite.
- New: none.

### Acceptance

- The doc and the scenes agree: every `recipe-*` scene and every `recipe_<pattern>` function is
  named in `docs/design-recipes.md`, the function exists in the source, and the doc names no scene
  that does not exist.
- Self-review against `docs/screen-review.md` at 1040x660 at scale 2 and at 402x874 at scale 3, a
  verdict per rubric item, recorded in the task's notes.
- Limits found at phone size are named in the doc and here (below), not hidden.
- Conformance lines: none; the recipes animate nothing of their own.
- Semantic-node test: n/a, no node of its own.

Limits at 402x874: the sidebar column is 208 px and does not narrow, so the recipe shows one figure
below 560 px; the dialog card is 448 px (600 for the form width) and is cut by a 402 px window.
Both are the composites' (`widgets/sidebar.rs:25`, `widgets/dialog.rs:70`), not this page's.

### Build

- Milestone: AI usability (task `luneui-rs/ai-usability`, step 5, D404).
- Depends on: layout.stack
- Done (D47, M11): tests `apps/gallery/src/pages/recipes.rs::every_recipe_scene_is_in_the_design_doc`, `apps/gallery/src/pages/recipes.rs::the_scan_for_named_ids_finds_a_known_one_and_stops_at_punctuation`; page `recipes` (scenes `recipe-*`); unverified here: the Windows and Linux run time.

## Open questions

- Whether the dialog and the sidebar should adapt to a narrow window is the mobile program's to
  decide (`[mobile slot]` lines in the doc); not done here because it changes the library.

## Notes for the planner

New in the AI usability task. A recipe for a new composite is a function in `recipes.rs`, a scene,
a row of the doc's table and a line here.
