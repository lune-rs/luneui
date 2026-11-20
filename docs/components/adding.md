# Adding a component

The path of a new component from nothing to merged, in the order to do it. It names no rule: the
rules are the table in [`README.md`](README.md#new-widget-checklist), which the scaffold and the
check read, so a new rule needs no edit here. Read the table once before step 3.

Everything below runs from the repository root.

## 1. Scaffold

```
scripts/new-component.sh <area> <slug> [--title "<Title>"] [--owner <name>] [--dry-run]
```

`<area>` is `widget`, `overlay`, `layout` or `data` (the components `Ui` draws); `<slug>` is
lowercase letters and digits with single hyphens, starting with a letter, and the entry's id is
`<area>.<slug>`. Run it with `--dry-run` first to see what it would write. It exits 2 and writes
nothing when a target exists, a name is taken or one of the files it edits has changed shape.

It writes:

- `docs/components/<area>/<slug>.md`: the catalogue entry from `_template.md`, `status: planned`,
  `owner` the `--owner` value (default `scaffold`), the Spec subsections still to fill;
- `crates/lune-ui-core/src/widgets/<module>.rs`: an options struct and one `Ui` method that
  draws a themed box and registers its node, to be replaced;
- `crates/lune-ui-core/tests/widget_<module>.rs`: one test that the node exists;
- `apps/gallery/src/pages/<module>.rs`: a gallery page with one scene, `<slug>-default`;

and adds one line each to `widgets/mod.rs`, `pages/mod.rs` (the module and `PAGES`) and the area's
group in `showcase/nav.rs`. It prints the commands to run next and the checklist rows left to a
person.

It does not design the component, fill the entry, write a decision row, run a generator or add
the touch rules. A scaffolded entry is `planned`, so a commit right after it keeps the gate green.

**Another area** (`core`, `render`, `platform`, `text`, `motion`, `infra`): the scaffold exits 2.
Do by hand what it does: copy `_template.md` to `docs/components/<area>/<slug>.md` and fill its
front matter; add the module in the crate of that area (the seam table in `docs/architecture.md`
says which); write its test file; and, if it has something to look at, add a gallery page (a file
under `apps/gallery/src/pages/`, a `mod` line in `pages/mod.rs`, an element of `PAGES` and a page
in a group of `showcase/nav.rs`). Then continue with step 3; the check runs the checks that apply to its area.

## 2. Design it

Pick the composite for the job from [`../design-recipes.md`](../design-recipes.md): its table says
which composite does a list, a table, a panel, a page, a sidebar, a form or a dialog, and the
recipe function to copy. `apps/starter/src/screen.rs` is the smallest whole screen and shows how
an app calls the toolkit; write the component so the app's code stays that short.

## 3. Build it against the checklist

Open the table ([New widget checklist](README.md#new-widget-checklist)) and hold each row. Its
`enforced by` column says what fails when you do not (a test, an automatic check) or that a
reviewer reads for it. Build from the stub the scaffold left; replace its drawing and keep its
shape.

## 4. Tests

Put the component's acceptance cases in the scaffolded test file
`crates/lune-ui-core/tests/widget_<module>.rs`. Add the widget to the theme-colour test (`m10` in
`crates/lune-ui-core/tests/c4.rs`) so a loaded theme is seen to reach it. Run only your tests while
you work: `cargo test -p lune-ui-core --test widget_<module>`.

## 5. Gallery scene, dump, self-review

Make the page draw every state the entry will list, one scene each (scale 2, settled). Then look
at it, as nobody else will:

```
cargo run -p gallery -- --dump <dir> --only <slug>-default
cargo run -p gallery -- --dump <dir> --only <slug>-default --window 402x874 --scale 3
```

Open each PNG with the image-reading tool and go through the rubric in
[`../screen-review.md`](../screen-review.md); record one verdict per item (pass, or fail and what),
in your task notes. Fix what fails, or write it into the entry as a limit. `cargo test -p gallery
showcase` checks the page is in a sidebar group, has a knob (the stub has one; make it a real one) and
draws inside its stage.

## 6. Fill the catalogue entry

Fill the twelve `### ` subsections of `## Spec` in order (a subsection that does not apply reads
`n/a — <reason>`; none is left out), the `### Gallery scenes` bullets for every scene, and the
rest of the body, as the section "Body sections" of [`README.md`](README.md) describes.

When it holds, set `status: done`, `owner: —`, and give `### Build` its Done line:

```
- Done (D47, M11): tests `<path>.rs::<fn>`, ...; page `<id>` or n/a; unverified here: <list or none>.
```

The Done line cites at least one test by file and function, and the check opens the file to find
it.

## 7. Its decision row

A design choice gets a row at the end of `docs/DECISIONS.md`, at the next free number (the highest
id in `docs/DECISIONS-INDEX.md` plus one, unless a task has reserved a range for you). Cite it in
the entry as `[D<n>]`. Then regenerate the index: `scripts/gen-decisions-index.sh`. Never edit
`docs/DECISIONS-INDEX.md` by hand.

## 8. Generated files

```
scripts/gen-api-docs.sh
scripts/gen-component-index.sh
```

The first rewrites `docs/api/` for the new public items (the gate fails when it is stale); the
second rewrites `docs/components/INDEX.md`.

## 9. Check, then the gate

```
scripts/check-component.sh <area>.<slug>
```

It runs both index checks and then every automatic check on the entry, whatever its status, and
prints what is still to do by hand. Run it until it exits 0. Then the gate of `CLAUDE.md`, which
also runs the same check for every `done` component as a test. The pixel gate is only for a change
to an existing component's drawing (D254); a new component has no earlier look to compare.

## 10. Touch and phone

The mobile program has not set its rules yet. The checklist's `[mobile slot]` rows are their
places; each states what is wanted and says its D-rows are pending. While they are pending:

- lay the component out from the rect it is given, so the dump at `402x874` in step 5 is a fair
  look at it;
- read each `[mobile slot]` row and note in the entry's `## Open questions` which of them apply to
  this component, in a line each, so the mobile program finds them;
- do not invent a touch mechanism of your own.

The mobile program fills the slots in the table, in that one place; when it does, the scaffold's
output and the check pick the new rows up and this guide needs no change.
