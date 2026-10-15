# Flow: tests, offscreen runs, profiling, releases

**Read this first.** Most behaviour is tested in the library without a window. Tests that
need the original's files read the player's install through `RAZDOR_DT_DIR` and **return
early without it** — so `cargo test` on a machine without the game (CI) passes them
silently. Locally, set `RAZDOR_DT_DIR` (a `.env` file works, values without quotes in
the shell) before trusting a green run. To *see* a screen, run Razdor on an invisible X
display with a scripted scene; never open a visible window unless asked.

## Test layers

| Layer | Where | Needs the install | What it pins |
|---|---|---|---|
| Unit tests | `#[cfg(test)]` modules and `src/rules/battle/tests.rs` | no | rules on synthetic data, decoders on synthetic bytes, the demo content |
| Real-data tests | e.g. `src/rules/ai/real_maps.rs`, real-file tests in `src/dt/gfx.rs`, `src/rules/save.rs` (`rk1_roundtrip_after_walking_and_a_battle`) | yes (skip without) | the shipped maps, art and data load and behave |
| Difftests | `src/difftest/tests.rs` | yes | Razdor's state after scripted actions equals numbers recorded from the original (clock, generator state, mana, armies …) |
| Replays | `src/rules/replay.rs` | yes | whole scenarios played headless; dumps (`RAZDOR_DUMP=<map prefix>` prints a map's buildings, armies and events) |
| Binary tests | `src/ui/**` `#[cfg(test)]` | some | UI logic that has no macroquad state (layouts, labels, catalogs) |

The difftest **action list** (`difftest::Action`, JSON lines with an `op`: new_game, click_map,
wait, answer, ok, cast, …) is played by `difftest::Runner`; `razdor --replay
<list.jsonl>` runs it headless from the command line (`difftest::cli`). The pipeline that
plays the same list in the original under Wine and diffs the states is in
`tools/difftest/` (see its `README.md`); the original's side needs a local Wine setup and
is not part of `cargo test`.

Commands (pin to the cores you are allowed to use; see `CLAUDE.local.md` if present):

```sh
cargo test --release                     # everything
cargo test --release --lib <name>        # one library test
cargo test --release --bins              # the game binary's tests (src/ui)
cargo clippy --all-targets
```

`cargo test` stops at the first failing target: when the library fails, the binary tests
did not run — run `--bins` separately.

Known slow-machine failure: `rules::ai::real_maps::thirty_days_on_every_map` has a time
budget per map (`RELEASE_BUDGET_SECS`); on slow cores scale it with
`RAZDOR_PERF_BUDGET_SCALE=2` (РК6 takes about 8 s on E-cores).

## Offscreen runs and snapshots

`src/ui/snapshot.rs`. The game itself, on an invisible display:

```sh
RAZDOR_SIZE=1280x800 RAZDOR_SCENE='building:<map>:0' \
RAZDOR_SNAPSHOT=/tmp/shot.png \
xvfb-run -a -s "-screen 0 1280x800x24" target/release/razdor
```

- `RAZDOR_SCENE` sets a screen up (`snapshot::stage`): `title`, `options[:advanced]`,
  `load[:manual|auto[:<row>]]`, `map:<map>[:x,y]`, `walk:<map>:dx,dy`,
  `building:<map>:<n>[:barracks|garrison|market|sanctuary]`, `army:<map>[:<n>…]`,
  `journal:<map>`, `spells:<map>`, `menu:<map>`, `settings:<map>[:advanced]`,
  `battle:<map>:<n>`, `save:<map>`, `victory:<map>[:<gold>]`, `defeat:<map>`, `custom`,
  `custom-battle[:watch]`, `replay:<step>` (with `RAZDOR_REPLAY=<actions.jsonl>`). The full
  list is the module's doc comment. A window pushed by the scene (victory, defeat, a hall
  entry taken with `RAZDOR_SCENE_TAKE=1`) needs `RAZDOR_SCENE_QUIET` off: quiet drops every
  dialog each frame.
- `RAZDOR_SNAPSHOT=<png>` saves the frame after `RAZDOR_SNAPSHOT_FRAMES` (default 20) and
  quits. Helpers: `RAZDOR_MOUSE=x,y` (hover looks, tooltips), `RAZDOR_SCENE_QUIET=1` (drop
  the scenario's messages), `RAZDOR_SCENE_DEBUG=1`, `RAZDOR_SCENE_SPELLS`,
  `RAZDOR_SCENE_POTION`, `RAZDOR_SCENE_FILTER`, `RAZDOR_SCENE_CONSOLE`,
  `RAZDOR_SCENE_ITEMS=<id>,…` (items in the hero's pack).
- A map scene starts its map as a new game: later campaign maps may end at once by their
  own defeat event (nothing carried over) — not a bug.
- Add a scene for every new window (`try_stage` in `src/ui/snapshot.rs`).

Use the `offscreen-snapshot` skill for the full recipe.

## Profiling a hitch

1. Build with symbols: `CARGO_PROFILE_RELEASE_DEBUG=true cargo build --release`.
2. Use a scratch save folder (`RAZDOR_SAVE_DIR=<scratch>`) — never the player's.
3. Time frames: `RAZDOR_PROFILE=1` logs frames over 100 ms as
   `slow frame: N ms = work W + render R (screen -> screen)` and slow decodes as
   "slow load" (`razdor::diag::profiling`, `diag::timed`).
   `RAZDOR_SCENE_OPEN=<frame>:<window>[,…]` (`snapshot::open_later`: `load`, `army`,
   `spells`, `journal`, `menu`, `building:<n>`, `map`) opens a window at a set frame and
   logs the frames after it.
4. `perf record -g` the offscreen run, `perf report --no-children`; `strace -c -f` for file
   I/O. Mesa's software renderer under Xvfb inflates render times; compare *work* times.
5. Fix on the main thread's work: cache, decode ahead (`gfx::prefetch`), read on threads
   (`save::want_metas`). Report before/after per case.

Other diagnostics: `razdor.log` in the data folder (`razdor::diag`, `RAZDOR_LOG=<file>`),
`RAZDOR_CRASH_TEST=panic|crash`, `RAZDOR_QUIT_AFTER=<frames>`, `RAZDOR_NO_AUDIO=1`.

## Releases and updates

- `.github/workflows/release.yml`: a pushed tag `vX.Y.Z` checks the tag against
  `Cargo.toml` and the changelog, runs the tests (macOS and Linux), builds
  `razdor` + `Razdor.exe` with `scripts/dist.sh` (pinned toolchain and llvm-mingw, paths
  remapped so the SHA-256 is reproducible) and `razdor-macos` with
  `scripts/dist-macos.sh`, and publishes «Раздор X.Y.Z» with the version's changelog
  section and the SHA-256s. Run by hand it only builds artifacts.
- The procedure (version bump, changelog files, tag, follow-up commit with the SHA-256)
  is the `release` skill.
- `src/update.rs`: the game checks GitHub releases on a background thread
  (`update::Mode`: Ask / Always / Off), downloads through the system `curl`, accepts a file
  only if its SHA-256 matches `SHA256SUMS`, and replaces itself by rename. A `cargo`
  build checks nothing unless `RAZDOR_UPDATE_AS=<X.Y.Z>`. The update window shows the
  release notes up to their first `## ` heading.

## The map editor (briefly)

`src/editor/` is the model (pure, tested): `doc` (the scenario document), `command`
(undoable edits), `validate`, `files` (writes `.DTm` byte-identical for an unchanged map).
The window is `src/ui/editor/`. Started with `--editor` or from the main menu. Its
parity work with the original DTMapEdit is paused.

## The map translator (`razdor-translate`)

Not part of the original game (asked for by the owner): a second program that writes English
copies of the install's maps (`Name.DTm` → `Name_en.DTm` in `Maps_Rus`) with any
OpenAI-compatible model. The window (`src/bin/razdor-translate/view.rs`) reuses the game's
`ui` module through `#[path]`; with `--map <name>` it runs without a window (`--help`).
When every string is translated, both write the English copies into the maps folder of the
install they run against (`razdor::dt::install::locate`), replacing older copies
(`translate::write_maps`).

- `src/translate/job.rs`: a job is one map, or a whole campaign (`job::campaign` follows the
  next-map links), and its distinct Russian strings. Names (buildings, owners, armies,
  leaders, named characters, titles) go first, then prose with the names they mention as a
  glossary (`Job::glossary`). The army-name tail from `#` and the event-title flag script
  from `%` are never sent. `Job::english` rebuilds each map and relinks the next map to the
  next chapter's copy. Progress is saved after every answer to
  `<data dir>/razdor/translate/<first map>.json` and taken over by a later job.
- `src/translate/text.rs`: what the model must keep (`#\`, CR LF, `#` words as `{N}`, the
  markup marks) and the checks on each answer; a refused string is retried alone, given up
  after `job::MAX_TRIES`.
- `src/translate/llm.rs`: `chat` through the system `curl` (the key in a private curl config
  file), batches sized by the settings' context (`Settings::batch_chars`); settings in
  `translate.json` next to the remembered install path. the API's reasoning_effort and
  response_format options are dropped by themselves when a server refuses them.

## Traps

- A test that returns early without the install looks green: when a change touches maps,
  art, events or AI, run with `RAZDOR_DT_DIR` set.
- `.env` values may be quoted; strip the quotes when you export them by hand.
- Do not commit screenshots of original art, saves, or install files from offscreen runs;
  keep scratch output outside the repo.

<!-- refs verified by: python3 -I docs/flows/verify.py docs/flows/*.md -->
