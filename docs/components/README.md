# Component catalogue

One file per component the toolkit provides or must provide: what it is, what it needs, where the
code lives today, what is open. Requirements live in `../requirements.md`; this catalogue cites
them by id (`R-<area>-<n>`).

`INDEX.md` is **generated** by `scripts/gen-component-index.sh` from the front matter of the files
here. Never edit it by hand. The gate runs `scripts/gen-component-index.sh --check`, which fails
when `INDEX.md` is stale.

The **public API itself** (every item's signature, doc comment, fields, variants, methods and trait
implementations, a page per module) is generated from rustdoc JSON into `../api/`; start at
[`../api/INDEX.md`](../api/INDEX.md). It is not edited here either: `scripts/gen-api-docs.sh`
writes it and the gate runs its `--check`.

## Add a component

Start at [`adding.md`](adding.md): the whole path, from `scripts/new-component.sh` (it scaffolds a
component of the areas widget, overlay, layout and data and wires it into the build) to the check.
For another area, or to do it by hand:

1. Copy `_template.md` to `docs/components/<area>/<slug>.md`.
2. Fill the front matter and the body sections.
3. Run `scripts/gen-component-index.sh`.
4. Commit the new file and the regenerated `INDEX.md` together.

Parallel agents only ever create or edit their own component files; the one shared file is the
generated `INDEX.md`, and a merge conflict there is resolved by re-running the generator.

## New widget checklist

What a new widget must hold, one row per rule. The table is the one source: the scaffold and the
component check read it, so a rule lives here and nowhere else. `kind` says how it is held:
`auto:<id>` is checked by a function of that id, `test` by a test that runs in the gate, `review`
by a reviewer reading the diff (no test fails on it), `[mobile slot]` is a rule the mobile program
fills in and nothing enforces yet. Cells hold no `|`.

<!-- checklist:begin -->
| rule | what | D-rows | enforced by | kind |
| --- | --- | --- | --- | --- |
| theme-colours | every colour comes from `ui.theme()` or the widget's options, never a literal or the preset; add the widget to `m10` of `c4.rs` | CLAUDE.md architecture rules | `crates/lune-ui-core/tests/c4.rs::no_fill_or_text_holds_a_preset_colour_under_a_theme_of_distinct_colours` | test |
| roving-focus | a composite with one tab stop uses `Sense.skip_tab`, `Ui::rove_stop`, `rove_keys`, `rove_focus` (the helper is tested in `ui_focus.rs`, its use by your widget is not) | D113 | by review | review |
| elide | text that must fit a width goes through `text::elide` or `Painter::elide`, not wrap or cut (the painter's elide is tested in `elide.rs`, its use by your widget is not) | D114 | by review | review |
| disabled-keys | a widget consumes its keys before it gates on `enabled`; check a subtree with `testing::disabled_violations` | D140, D159 | `crates/lune-ui-core/tests/widget_tab_strip.rs::a_disabled_strip_keeps_the_arrows_home_and_end_from_the_shortcuts` | test |
| selection-paint | a text selection is drawn by `widgets::selection::paint`, never filled by the widget | D272 | `apps/gallery/src/page.rs::no_widget_fills_a_selection_itself` | test |
| overlay-layers | a widget never names `LayerId::Raised` or `LayerId::Outside`; `Ui::overlay` picks the layer | D137, D255 to D265 | `crates/lune-ui-core/tests/widget_rules.rs::no_widget_names_the_raised_or_outside_layer`, `crates/lune-ui-core/tests/ui_overlays.rs::a_menu_opened_from_the_scope_stays_on_outside_above_it_even_under_a_modal` | test |
| wheel-per-axis | a composite with nothing to scroll on an axis disables that axis (`Bars::unless_fits`) so the wheel reaches the area around it | D176, D178 | `crates/lune-ui-core/tests/widget_kanban_board.rs::a_board_whose_columns_fit_leaves_the_wheel_to_the_page_around_it`, `crates/lune-ui-core/tests/widget_markdown.rs::the_wheel_over_a_document_that_fits_goes_to_the_page_and_one_that_overflows_keeps_it` | test |
| context-not-menu | a widget reports a secondary click and the show-menu action as `context` on its response; the app opens the menu with `Ui::menu` | D292 | `crates/lune-ui-core/tests/widget_rules.rs::no_widget_opens_a_menu`, `crates/lune-ui-core/tests/widget_tab_strip.rs::a_secondary_click_reports_the_tab_and_its_rect_and_opens_no_menu` | test |
| drag-ghost | a drag ghost draws one layer group above its source, out of any `record` (`Ui::drag_ghost`) | D168 | `crates/lune-ui-core/tests/widget_kanban_board.rs::the_ghost_is_above_the_board_cut_by_no_clip_and_hits_nothing` | test |
| access-node | every widget registers its accessibility node through `Ui::node`, which marks it disabled inside `with_enabled(false, ..)` | D140 | `crates/lune-ui-core/tests/ui_access.rs::a_node_in_a_disabled_scope_is_disabled_whatever_it_asked_for` | test |
| lints-and-docs | every public item has a doc comment and the workspace lints (`unreachable_pub`, `missing_debug_implementations`, no `unwrap`) are clean; the gate runs clippy with `-D warnings` and `scripts/gen-api-docs.sh --check` | hard rule 9, D242 | by review | review |
| catalogue-file | the widget has one catalogue file `docs/components/<area>/<id>.md` and its module is in some entry's `source:` | hard rule 10 | `apps/gallery/src/catalogue.rs::every_widget_module_is_claimed_by_an_entry` | auto:catalogue-file |
| gallery-scene | an entry of the areas widget, overlay, layout and data claims at least one scene the gallery has (n/a does not do); no entry claims a scene it lacks | D47 | `apps/gallery/src/catalogue.rs::every_scene_in_the_gallery_is_claimed_by_an_entry`, `apps/gallery/src/catalogue.rs::every_scene_an_entry_claims_is_in_the_gallery` | auto:gallery-scene |
| decision-row | an entry of the areas widget, overlay, layout and data cites its design choice as `[D<n>]`, each cited row is in `docs/DECISIONS-INDEX.md`, and `scripts/gen-decisions-index.sh` is run | D400 | by review | auto:decision-row |
| test-cited | a `done` entry cites at least one existing test as `<file>.rs::<fn>` on its Build line | D47 | `apps/gallery/src/catalogue.rs::every_done_entry_holds_its_evidence` | auto:test-cited |
| pixel-gate | a change to an existing component's drawing passes `scripts/pixel-gate.sh` with every changed scene named and a DECISIONS row | hard rule 8, D254 | by review | review |
| api-docs | `scripts/gen-api-docs.sh` is run after a public API or doc-comment change | D287 | by review | review |
| touch-hit-area | a control answers a touch pointer within a 44 pt area, in hit testing only (the drawing stays) | pending (luneui-rs/mobile-support) | by review | [mobile slot] |
| no-hover-only | no action or hint exists only on hover | pending (luneui-rs/mobile-support) | by review | [mobile slot] |
| touch-drag-arbitration | a scrolling composite inside another decides per axis who takes a touch drag, like the wheel | pending (luneui-rs/mobile-support) | by review | [mobile slot] |
| long-press-context | a long press reports `context` through `secondary_clicked`, the same path as D292 | pending (luneui-rs/mobile-support) | by review | [mobile slot] |
| safe-area-and-keyboard | a screen draws inside `Ui::safe_area()` and lifts by `Ui::keyboard_inset()` | pending (luneui-rs/mobile-support) | by review | [mobile slot] |
| pointer-kind | behaviour that differs for touch reads `Ui::pointer_kind()` | pending (luneui-rs/mobile-support) | by review | [mobile slot] |
| non-exhaustive-structs | a public options struct is `#[non_exhaustive]` so a field added later breaks no app | pending (luneui-rs/mobile-support) | by review | [mobile slot] |
<!-- checklist:end -->

To add a rule, add one row: a unique kebab-case `rule`, what it asks, its D-rows, what enforces it
(`<file>.rs::<fn>` in backticks, or `by review`) and its kind. A row of kind `auto:<id>` also needs a
check function of that id in the component check (`apps/gallery/src/catalogue.rs`); a test fails
when the table and the checks disagree. The mobile program fills its `[mobile slot]` rows here, in
this one table; each keeps its marker until a test or a check holds it.

## Id rule

Id = `<area>.<slug>`: lowercase letters and digits, single hyphens inside the slug, unique. The file
is `<area>/<slug>.md`, so the id and the path always agree (the generator checks it). No numeric
counters.

## Areas (closed list; extend it by editing this README and `AREAS` in the generator)

- `core`: colour, theme tokens, painter, icons, geometry shared by everything else
- `render`: GPU renderer, blur, frames, frame dump, the app-owned drawing hook
- `platform`: OS glass materials and window helpers (the only place for `unsafe`)
- `widget`: single controls (button, toggle, text field, ...)
- `overlay`: popups, dialogs, dropdowns, menus, toasts
- `layout`: containers, lists, trees, split panes, scrolling
- `data`: data-driven views (graphs, charts, boards, diff and code views)
- `text`: shaping, measuring, editing and viewing text
- `motion`: animation curves and timing
- `infra`: cross-cutting machinery (localization, shortcuts, theming reload, accessibility)
- `lunegit`: needs that are shaped like the Lunegit app and stay in it (`external` entries only)
- `luneterm`: the same for Luneterm (the terminal grid itself stays in Luneterm)

App areas hold app-shaped needs only. Since M3 they hold only `external` entries; a reusable shape
lives under a library area with a neutral id (DECISIONS D42). To generalise an app-area entry, move
it with `git mv` and change its id and area.

## Front matter

Between two `---` lines, one `key: value` per line, no nesting, every key present and non-empty:

| key | value |
| --- | --- |
| `id` | `<area>.<slug>` |
| `title` | short human title |
| `area` | one of the areas above, equal to the directory |
| `status` | see below |
| `consumers` | comma list: `luneterm`, `lunegit`, `wane`, `any` |
| `source` | existing code: a Luneterm-relative path for `in-luneterm`, `Lunegit: <path>` for a Lunegit stub, or `—`; for `done`, the repo paths (comma separated, from the workspace root) of the code, every one of which exists |
| `owner` | agent or task working on it, or `—` |
| `summary` | one line |

## Status

- `in-luneterm`: the code exists today in Luneterm's `lune-ui` (`source:` points at it) and moves in the extraction
- `planned`: a consumer has named the need; no code anywhere, or scaffolded by `scripts/new-component.sh` and not `done` yet
- `stub`: rough placeholder, not analysed; a planner must look before anyone builds (none left since M3: an analysed stub becomes `planned`, DECISIONS D49)
- `in-progress`: someone owns it (`owner:`)
- `done`: lives in this repo and holds the done standard (DECISIONS D47) by something that runs here. Its
  `### Build` carries one line, `- Done (D47, M11): tests `<path>.rs::<fn>`, ...; page `<id>` or n/a;
  unverified here: <list or none>.`, which cites at least one test by file and function (a docs-only
  entry says `tests: n/a — <reason>` instead), has `owner: —`, and holds no "still needs" or "Open for
  `done`". A gallery test (`apps/gallery/src/catalogue.rs`) checks the owner, the source paths, the absent
  "still needs" text, that the line has its `page` and `unverified here` parts, and that each cited
  function is in its file (one that is not fails the gate). It does not read what follows `page` or
  `unverified here:`: those words are the entry's. What only a real OS shows stays in the entry as `unverified here`
  and is never ticked.
- `external`: needed by a consumer but stays in the consumer's repo

Class A and B (D234). Class A: every D47 item of the entry holds by something that runs here, so it goes
`done`. Class B: the entry's own text ties its done standard to a run that cannot happen here (an OS
material on real machines, a pixel gate on a presented frame); it stays `in-progress`, `owner:` names who
runs it, and the reason is under `## Open questions`. An audit may add a Class B entry only by quoting that
line of the entry; it may not remove one by rewording the line.

## Body sections

Five sections, in this order, with `## Spec` after `## Existing code` (DECISIONS D49):

1. `## What it is`
2. `## Needs` (bullets, each citing an `R-` id)
3. `## Existing code` (`path:line`, or "none")
4. `## Spec` with exactly these twelve `###` subsections in this order: `Evidence`, `Anatomy`, `States`,
   `Transitions`, `Input and focus`, `Retained state`, `Accessibility`, `Strings`, `Gallery scenes`,
   `Painter primitives`, `Acceptance`, `Build`. A subsection that does not apply reads
   `n/a — <reason>`; none is omitted. An entry may add `### Unverified` (what only a real OS shows) and
   `### Limits` (what the entry knowingly does not do, each with its DECISIONS row) after `Build`.
5. `## Open questions` (only what is still open, else `- None.`) and `## Notes for the planner`

An `external` entry (status `external`) has a short `## Spec` of three subsections only: `Evidence`,
`Reusable parts` (the library entry ids that cover the reusable pieces, with cites) and `Build`
(`- Milestone: external — stays in <app>`).

Rules for the spec subsections:

- **Evidence**: first line `Pinned: Lunegit <rev>, Wane <rev>, this repo <rev>`; then the files and line
  ranges read; then every conflict between sources and its winner. Precedence: this repo's moved
  code (pixel-gated), then Lunegit, then Wane; for a component only Wane draws, Wane.
- **Cites**: every claim in Anatomy, States, Transitions and Input and focus carries a tag:
  `[L path:n]` Lunegit, `[W path:n]` Wane, `[R path:n]` this repo, `[LT path:n]` Luneterm (`:n-m` and
  `:n,m` allowed), `[D<n>]` a DECISIONS row, `[derived: <from what>]` only when no source has the
  value, `[core.ui]`, `[core.focus]`, `[infra.accessibility]` or `[motion]` for toolkit-model
  statements, or `[<entry id>]` for another entry. A claim with no source and no derivation is an
  open question.
- **States**: table `| state | look | cite |`. **Transitions**: table
  `| change | property | kind | duration | easing | reduced | cite |` using the vocabulary of
  `../motion.md` (15 kinds, 5 durations, 7 easings; reduced is `keep`, `fade` or `snap`); a transition
  Lunegit lacks cites `[D40]`; name what never animates.
- **Input and focus**: pointer senses, keys, Tab stop, layer (`Base` < `Page` < `Popup` < `Modal`, and
  `Toast` above them by D54), and for overlays the focus scope, dismiss rules and pointer capture.
- **Retained state**: Transient, Persistent (owned by the container, survives `ui.keep_alive`),
  App-owned, Exit retention.
- **Accessibility**: role, name, description and value sources, states and actions from
  `infra.accessibility`'s enums; a role or action the enums lack is an open question `role-gap: ...`.
- **Strings**: keys `luneui.<slug>.<name>` with the English default and the Lunegit `en.json` line it
  comes from, or `None: <why>`.
- **Gallery scenes**: `- `<id>`: <setup>, t = <ms or settled>`; ids are lowercase-hyphen and unique
  across the catalogue except the existing scene ids, which are reused by name. Every state appears in
  a scene; every transition gets a mid-transition scene at half its Full duration.
- **Painter primitives**: `- Existing: <list>` and `- New: <list or none>` using the primitive names in
  `core.painter`; each new item cites the drawing that needs it and is added to `core.painter`'s
  aggregate table.
- **Acceptance**: behaviour tests with scripted input and injected time, pixel probes at scale 2, one
  conformance line per transition, the semantic-node test. Infrastructure entries with no UI name the
  existing tests by path.
- **Build**: `- Milestone: <M4..M11>` and `- Depends on: <entry ids>`.

State only what is verified; write unknowns as open questions.
