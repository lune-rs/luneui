# Screen review

How an agent looks at a screen it built and judges it against the toolkit's rules, with no person
in the loop (nobody checks by eye, so the dump and this list are the check). Draw first, with the
recipes (`docs/design-recipes.md`), then review here.

## Dump one scene

`gallery --dump <dir>` writes all scenes, which is slow and writes hundreds of files. To look at
one:

```
cargo run -p gallery -- --dump <dir> --only <name> [--window <W>x<H>] [--scale <S>]
```

- `<name>` is a listed name (`ui-controls@1.5x`, see `cargo run -p gallery -- --list`) or a bare
  id (`ui-controls`: every listed scene with that id, in list order). Without `--window` and
  `--scale` the file is exactly what the full dump writes for it (`<id>@<scale>x.png`).
- `--window <W>x<H>` runs a Ui scene in a window of W by H logical pixels (each 1 to 4096) and
  `--scale <S>` at S device pixels per logical pixel (0.5 to 4.0); the one you leave out keeps the
  scene's own. Flags come in any order, each at most once. The file is `<id>@<S>x-<W>x<H>.png`
  (`recipe-form@3x-402x874.png`), so it never shadows a file of the pixel gate; with a bare id
  that has several scales, only its first listed scene is dumped. Legacy scenes (the first pages
  in `--list`) take no override. A physical side (`W * S`, `H * S`) is at most 8192.
- Desktop: no override, or `--window 1040x660 --scale 2`. Phone: `--window 402x874 --scale 3` is
  the size the mobile program's simulator uses, a parameter and not a rule; change it for another
  device. `[mobile slot]`
- Exit 2 names the usage problem (an unknown scene says to run `gallery --list`). A scene's
  script aims at fixed points of its own window, so under an override the capture frame may not
  be the one the scene expects: the dump prints `warning: ...` and writes the file anyway. Read a
  scene that warns with that in mind (a hover that missed its target is the script, not the
  screen).

## View it

Open the PNG with the image-reading tool (the `Read` tool shows an image). A 3x phone frame is
1206 by 2622 pixels: crop the part you judge, or dump a smaller window, so the detail is not lost
in the downscale (macOS: `sips -c <h> <w> --cropOffset <y> <x> in.png --out crop.png`). Compare
with the component's own scene (the gallery page of the widget, `--only <its id>`) to see what
the widget alone looks like. Dump at 1x, 2x and 3x for the edge check below. Look at the whole
frame once, then at each region.

## The rubric

Go through every item. Each is a check, what to look at, and what fails.

1. **Theme colours.** Look: every fill, stroke and text colour. Fails: a colour that is not in
   `ui.theme()` or a widget's options (a literal in your code, the preset where a theme is set).
   The test `c4::m10` draws the library's controls under a theme of distinct colours and fails
   on a preset colour; for your own screen, read each colour in the dump against the theme
   tokens and grep your code for colour literals.
2. **Text fits.** Look: every label, value and title. Fails: a clipped glyph, text spilling out
   of its box, a long text wrapped where one line was meant. Long text that must fit a width is
   elided with `text::elide` / `Painter::elide` (an ellipsis), not wrapped and not cut.
3. **Alignment and spacing.** Look: left edges, baselines, gaps between neighbours. Fails: edges
   a pixel or two off each other, uneven gaps between siblings, a padding that differs from the
   composite's own (`PAD`, `GAP`, `RADIUS` in its source; the recipes quote them). Lay out with
   `layout::row`, `column` and `grid`, not with hand-placed rects.
4. **Hierarchy.** Look: title, body, secondary text. Fails: two things that read as equally
   important when one is the title, a secondary line in the body colour, sizes outside the
   theme's `font_s`, `font_base`, `font_m` and `font_title`.
5. **Focus and selection.** Look: tab to a control (dump a scene with the focus script) and
   select a row or some text. Fails: no visible focus ring, a selection that hides its text, a
   selection drawn by hand (`widgets::selection::paint` draws every one).
6. **Disabled state.** Look: a disabled control beside an enabled one. Fails: text that is hard
   to read, a disabled control that looks live, or one that still reacts.
7. **Overlays.** Look: a menu, popover, tooltip, dialog. Fails: an overlay below what it covers,
   an unfrosted panel (every floating surface goes through `popover::glass_surface`), a dialog
   that does not dim what is under it, a widget that opened a context menu itself (it reports
   `context`; the app opens the menu).
8. **Inside the window.** Look: all four edges, the corners, the scroll regions. Fails: anything
   cut off at the window edge, a control past the bottom, a scroll region that reaches nothing
   of what it holds. Re-dump at a smaller window to see it.
9. **Crisp edges.** Look: 1x, 2x and 3x crops of a border, an icon and a text baseline. Fails:
   a blurred border (a stroke on a half pixel), an icon that moves between scales, text that
   looks softer than its neighbours.
10. **Empty and overflow states.** Look: the screen with no rows, with one row, with a hundred,
    and with a very long name. Fails: a blank area with no hint, a header with nothing under it,
    a list that grows past its pane instead of scrolling, a table whose columns collide.

### Touch and phone

The mobile program has decided the first line (D344, D345); the others stay `[mobile slot]` until it
decides them.

- The screen draws inside the safe area (`Ui::safe_area()`) and moves with the
  keyboard (`Ui::keyboard_inset()`); dump a phone window and look at the top, the bottom and
  the focused field. Fails: content under the notch or the home bar, a focused field behind the
  keyboard.
- `[mobile slot]` Every control has a 44 pt touch hit area, even where the drawing is smaller.
  Fails: a hit area under 44 by 44, two targets that overlap.
- `[mobile slot]` Nothing needs hover. Fails: an action or a hint that only shows on hover.

## Record the verdict

One line per item, in the task's `notes.md` or the reply, for example:

```
screen-review: recipe-form@3x-402x874.png
1 theme colours: pass
2 text fits: fail - the hint under "Branch" is clipped at x=388; elide it
3 alignment and spacing: pass
...
```

`pass`, or `fail` with what and where (the pixel or the element), or `n/a` with why. A fail is
fixed and the scene dumped again; the verdict is of the last dump. Do not write `pass` for an
item you did not look at.
