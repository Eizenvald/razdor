# Interface typography audit

## Evidence and scope

The reference is Discord Times Community Update 1.1. Its executable's SHA-256 is
`B09AE25904F68C074401771EEC8A9B83094CA1F50772069C44094E14AC9FB0BB`.
The font-loading palette (4dbb0c), unit text list (491fa4), stat comparison (491a58)
and unit panel (492f24) were inspected. The installed SanSerif sheets have white
foreground pixels; Benguiat includes grey edge pixels. No original assets or machine
code are bundled by this change.

The project audit covered the shared drawing/measurement functions and the game's
uses of font faces, fitting, ellipsis and wrapping (128 references before this change).
It does not establish visual equivalence of every screen to the original: the original's
individual screens still need image comparisons, particularly at small interface scales.

## Shared roles

`src/ui/typography.rs` defines the roles and shared palette. Font selection and native
metrics remain in `dt_font.rs`; measurement, drawing and overflow remain in `widgets.rs`.

| Role | Original sheet | Uses |
| --- | --- | --- |
| Body | SanSerif | Characteristics, item descriptions and properties, spell effects |
| Bold | SanSerif_Bold | Time ribbon, scenario description |
| Title | Benguiat | Window title bars |
| Name | Benguiat | Unit/item/spell names, market lists, building descriptions and quest entries |
| Button | Benguiat | Normal rectangular button labels |
| Hint | SanSerif | Army middle strip, generic tooltips |

Small hire/heal pills use the body face. Bottom resource captions use SanSerif,
their numbers Benguiat. Marked event text uses Benguiat and the markup's own palette.
The project's editor retains its existing presentation; it is not a screen of the game.

## Size and overflow

In the original font mode, every face uses its sheet's native capital height and cell
height multiplied by the common interface scale. Drawing, measuring and shadow drawing
resolve the same size. A string's length cannot change that size; fractional interface
scales no longer snap to a different integer font scale.

- Names, list cells, window headings, buttons and one-line hints shorten with an ellipsis.
- Paragraphs wrap at the available width. Existing dialog and journal scrolling is retained;
  their visible-line calculations use the same line height as their drawing.
- Unit statistic labels reserve space for the value, then shorten the label if necessary.
- Item descriptions reserve room for the properties before allocating description lines.
  Their fonts do not shrink when another item has a longer description.
- A box too narrow even for an ellipsis draws an empty string.

Modern fonts and explicit TrueType overrides keep their fallback sizing behaviour. This
change does not claim that those substitute fonts have the original's glyph metrics.

## Palette

Shared colours are resolved by text role/face in the original mode. Nominal RGB on a white
glyph: plain 255/255/255, yellow 255/255/155, body blue 75/185/255, Benguiat blue
55/180/255, lowered characteristics 255/103/96, body orange 235/165/75,
Benguiat orange 255/175/85, muted 155/155/155, green 155/255/155.

Only shared legacy text constants are mapped; arbitrary colours are not quantized. The
player's explicit choices remain separate: market headings 156/255/156, building and
item names 211/228/161, quest heading 189/190/90, quest ink 97/0/0, and the chosen item
name/class restriction/description/properties/bonus colours.

This standardizes the role palette, not the original executable's per-pixel font-colour
loader. Bitmap anti-aliasing still uses the project's existing coverage renderer.

## Screen audit

| Area | Applied rule |
| --- | --- |
| Main menu, settings, custom battle | Shared fixed native metrics; common button and title overflow |
| Scenario and hero selection | Name/bold roles; centered scenario description uses bold metrics, with scrolling when long |
| Save/load | Existing name/bold/body faces; fixed-size names with ellipsis |
| Buildings, market, sanctuary | Native metrics, explicit name roles, fixed item descriptions, bounded list names |
| Army and battle | Shared unit sheet/card metrics and role palette; label/value separation |
| Dialogs and village offers | Benguiat narrative; wrapping, scrolling and geometry share line height |
| Journal | Name heading, fixed body paragraphs, line-height-based scrolling |
| Spell book/cards | Stable names and effect text; names shorten rather than change size |
| Lower bar and tooltips | Body captions, Benguiat values, bold time; native hint metrics |
| Editor and modern font fallback | Existing presentation retained |

## Validation

`cargo build --release --locked` succeeds. The library suite passed 973 of 975 tests;
the interface binary suite passed 152 of 153. Windows test runs exposed three failures
outside the typography changes: path separators in install
discovery, a UTF-8 boundary in the translation-catalog scanner, and the input-import scanner
reporting `saves.rs`. These files are not changed by this audit. A passing build is not a
visual comparison with the original.

1920×1080 snapshots of the army, market, scenario selection, tutorial, spell book, journal
and battle/settings screens were rendered and visually inspected. This caught and fixed
market names touching their prices and inaccessible long scenario descriptions. Snapshots
were saved successfully; their automated process shutdown reports a gamepad-library
(`gilrs`) thread-join panic after the quit message. Gameplay shutdown and a side-by-side
comparison of every original screen are not validated here.
