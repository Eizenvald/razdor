# Flow: the install's files, decoding and drawing

**Read this first.** Razdor ships no original content. Every picture, sound, text and map is
read at runtime from the player's Discord Times install (`RAZDOR_DT_DIR` or the located
folder) and decoded in memory; nothing decoded is ever written to the repo. The pure
decoders live in `src/dt/` (no macroquad, fully unit-tested); `src/ui/` turns their RGBA
images into textures and caches them. Formats are specified in
`docs/reference/graphics-formats.md` and `docs/reference/dtm-format.md`.

## From install file to pixels

```
install file ──► src/dt/gfx.rs decode_file ──► gfx::Image (RGBA, not premultiplied)
                    │ (.lit / .ugs / .spi / .bmp)
                    ▼
     src/ui/chrome.rs art / win / win_fx  (window art, cached per path × Fx)
     src/ui/dt_art.rs DtArt               (portraits, figures, items, terrain, map objects)
                    ▼
     macroquad Texture2D ──► chrome::tex / chrome::tex_src / draw_texture_ex
```

- `src/dt/install.rs`: `DtInstall::load` reads the data inis (units, items, spells), lists
  the maps (`list_maps`, `MapEntry`) and the play options (`PlayOptions`). `find_path`
  matches file names case-insensitively (the install is a Windows folder).
- `src/dt/ini.rs` (`Ini::from_cp1251`), `src/dt/text.rs` (cp1251), `src/dt/markup.rs`
  (the original's text marks), `src/dt/dtm.rs` (maps, `Scenario`), `src/dt/container.rs`
  (the `AIpf` container), `src/dt/data.rs` (units/items/spells/battle effects).

## Image formats (`src/dt/gfx.rs`)

- **LIT** (`decode_lit`): a DCT image — header, then Y, Cb, Cr[, A] planes, each with a
  quantisation table and a coefficient rank table (`lit_plane`). Two traps found by players:
  - every coefficient byte is **signed**, the DC slot too (the original's `CBW` at
    0x4778b4). Reading slot 0 unsigned drew grey stripes on the mana icon, the battle's
    sword cells (`wb1.lit`), the selection ring and 11 more pictures (commit 84a84fd;
    test `lit_dct_slot_zero_is_signed`);
  - the colour path is the original's byte pipeline: planes rounded to bytes
    (`plane_bytes`), chroma block edges smoothed (`deblock`), 4:2:0 chroma doubled with a
    1-2-1 filter (`upsample`) — commit 325ecd1, test `lit_chroma_smoothing`. Grey pictures
    must come out exactly grey.
  `real_files_match_reference_decoder` pins checksums of real files (needs the install).
- **UGS** (`decode_ugs`): sprite sequences of 16-bit pixels (`ugs_pixel`); `Objects.ugs`
  has its own record layout (`decode_objects_ugs`, `ObjectSprite`, `ObjectSprites`).
- **SPI** (`decode_spi`, editor icons) and **BMP** (`decode_bmp`, water frames; some
  `.lit` files are really BMPs).
- Install locations as constants: `UNIT_PORTRAITS`, `UNIT_FIGURES`, `ITEM_ICONS`,
  `MAP_OBJECTS`, `TERRAIN_TEXTURES`, `WATER_FRAMES` / `water_frame_file`.

## Caching and decoding ahead

- `chrome::art(rel, fx)` decodes a file once and caches the texture per path and per
  `chrome::Fx` (Plain, KeyBlack, Glow, Grey, Shade, SwapGreenBlue, ShadeStrong,
  KeyPurple). `chrome::win(name)` = `Graphics/Windows/<name>.lit` drawn plainly with its own
  alpha. Prefer `Fx::Plain`: a black key was once used to hide the decoder's sign bug and
  ate dark pixels (commit 66d29ff).
- `DtArt` (`src/ui/dt_art.rs`) caches by `Key` (portrait, figure, item, terrain, object,
  building) and builds one `Atlas` for every map object and building sprite so the map
  draws in few batches.
- Decoding ahead (commit c04fa1b): `chrome::set_install` calls `gfx::prefetch` with
  `WINDOWS_AHEAD` (the paper, portraits and icon sheets, the load window's book, building
  tabs) on a thread named "art ahead". `gfx::decode_file` takes a picture decoded ahead
  from the `Ahead` map; if the game asks first it decodes itself and marks it `wanted`
  so the thread skips it. Slow decodes are logged ("slow load") with `RAZDOR_PROFILE=1`.
  Add a file to `WINDOWS_AHEAD` when profiling shows a window's first open decoding it.
- Textures are created on the main thread only (macroquad/OpenGL); threads may only make
  `gfx::Image`s.

## Drawing helpers (`src/ui/chrome.rs`)

- `chrome::tex(t, rect, color)`, `chrome::tex_src(t, src, dst, color)` (a sub-rectangle),
  `chrome::tile`, `chrome::three_slice`.
- Blend modes as closures: `chrome::premultiplied`, `chrome::additive` (spell effects),
  `chrome::multiply`, `chrome::subtractive`, `chrome::silhouette`.
- Composite widgets: `chrome::empty_cell` (battle/army cells; front-row swords `wb1`,
  bow `wb2`), `chrome::wounds`, `chrome::glow_frame`, `chrome::badge`,
  `chrome::spell_icon`, `chrome::effect` / `chrome::animation` (multi-frame effects),
  `chrome::battle_effect`.
- Pixel transforms on `gfx::Image` before upload: `chrome::subtract`,
  `chrome::scale_channels`, `chrome::lock_portrait`, `chrome::transform`.

## The world map

`src/ui/world_view.rs`: `draw_world` draws only the visible cells — `draw_terrain`
(`TerrainLayer::draw` in `src/ui/terrain.rs`: one shader quad mixing the four nearest cell
textures from a 4×4 atlas, water frames multiplied in), then decorations
(`draw_object`), buildings (`draw_building`), armies (`draw_army`, `draw_figure`,
`draw_ship`), the hero (`draw_hero`), the route (`draw_route`), and the tooltip
(`draw_tooltip`). Buildings are anchored like the original (right edge on the cell; see
PLAYTEST_NOTES "Мосты сдвинуты"). The minimap is `src/ui/minimap.rs`.

## Sound and music

- `src/dt/sound.rs`: `SoundTable::from_ini` (`_Sounds.ini`: `[Backgrounds]` music keys,
  `[SFX-Effects]`), `parse_wav`, `.raw` music as 16-bit PCM at an assumed 22050 Hz
  (`RAZDOR_MUSIC_RATE` overrides; `raw_to_wav`).
- `src/ui/audio.rs`: screens call `audio::cue(Cue::…)` (a per-frame queue, de-duplicated);
  `Audio::frame` plays them and drives the music. `audio::cue_on_release` repeats a cue on
  the mouse release, as the original's hire button does.
- `src/ui/jukebox.rs`: which track plays for a `Mood` (menu, map rotation, battle,
  triumph, defeat), crossfades (`Fade`, `SCREEN_FADE`, `ROTATION_FADE`). The map
  rotation's picks are draws of the game's generator (`rules::music`, `App::rotate_music`),
  so they matter for parity with the original's random state.
- `RAZDOR_NO_AUDIO=1` silences; `RAZDOR_AUDIO_LOG=1` logs every sound.

## Fonts

TrueType faces bundled in `data/fonts/` (open licences, loaded by `widgets::load_font`)
for Razdor's text; the original's bitmap fonts are decoded from the install by
`src/ui/dt_font.rs` (`BitmapFont`, glyph order `ORDER`) and scaled so their capitals match
the TrueType size; missing glyphs fall back to TrueType.

## How to add a picture

1. Find the file and how the original draws it (Ghidra / `docs/reference/graphics-formats.md`;
   `parity-check` skill): which file, frame, blend, alpha.
2. Load it through `chrome::win` / `chrome::art` (window art) or a `DtArt` method (unit,
   item, map art). Do not add a new cache.
3. If it is big and a window needs it on first open, add it to `WINDOWS_AHEAD`.
4. Check it offscreen (`RAZDOR_SCENE` + `RAZDOR_SNAPSHOT`, see
   [testing-and-tools.md](testing-and-tools.md)); a decoder change gets a unit test on
   synthetic bytes plus a real-file test gated on `RAZDOR_DT_DIR`.

## Traps

- Never commit decoded output, screenshots of original art, or install files.
- The Python reference decoder in the local notes still has the two LIT bugs; trust
  `src/dt/gfx.rs` and its tests.
- Alpha is not premultiplied in `gfx::Image`; use `chrome::premultiplied` when a texture is.

<!-- refs verified by: python3 -I docs/flows/verify.py docs/flows/*.md -->
