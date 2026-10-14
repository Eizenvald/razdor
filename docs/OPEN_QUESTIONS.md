# Open questions

Parity questions and known differences from the original that are not settled yet, newest
first. Every agent that leaves something unresolved adds it here (what, where, what is known,
what would settle it); whoever settles it moves it to **Settled** with the commit. The specs'
"Razdor now → original" tables stay the long-term parity list; this file is for loose ends
from reports and recent work. `docs/PLAYTEST_NOTES.md` keeps the player-facing story.

## Open

- **AI step clock: the tick banked at its start** (2026-10-14). `Game::ai_move` adds a hero
  tick's minutes to every army's bank at the tick's start, so a step can start at once; the
  original banks each frame's `dt` (0x4ade3c → 0x4a399c) and starts a step only in the frame
  its bank covers it. Even in the limit of short frames, which Razdor aims at, the bank then
  grows through the tick: on C1014-181146 the 7.5-minute step after an arrival with 5 minutes
  banked would start 2.5 minutes in, Razdor's about 1. Small, and every AI difftest leans on
  the present model (FINDINGS §5), so changing it needs a run of all of them; the explorer's
  candidates of 2026-10-14 are the original's frame noise either way.
- **The band under the barracks and garrison pictures** (2026-10-14, UI audit pairs 51, 53).
  The original's strip between the building picture and the army cards is ochre with black
  ornaments at its ends; Razdor draws the brown text box. Find which picture the original
  draws there (`HSTexture` is a scrollbar's; `HintFrame` is the main hall's dark divider) and
  use it.
- **Battle screen parity not on dt-original** (2026-10-14). fa63292 (the original's card
  highlight `chrome::card_highlight`, the red bold ExitHint, the leave-battle window as
  4d3620) conflicts with dt-original's older `battle_view.rs` (8 hunks), `saves.rs`,
  `chrome.rs` and `snapshot.rs`; port it by hand (on dt-original the exit window's warning can
  be the install's `[ExitBattle] Warning`, its restart being the original's).
- **Battle strip art** (2026-10-14). The original's strip between the formations spans the
  right side between two silver lines (4d1fb0: y 319 and 346, x 266…894) with dark scroll
  ornaments at both ends; Razdor draws `chrome::text_box` (parchment with corner pieces), a
  little shorter. Find the strip label's (667a74) background art and use it.
- **Highlights on the AI's turn** (2026-10-14). The original also lights, on the AI's turn,
  the cells its actor can act on (its side "can act", the player's "enemy", 4c57bc with
  `local_1c == 2`); Razdor lights only the actor and, while the action plays, its target.
  Needs the AI actor's options before its action (the rules apply it first).
- **Battle layout in the original's pixels** (2026-10-14). `battle_view::Layout` still uses the
  960×720 reference × k (836×600 window, 244-wide panel); the original's window is 896×640
  (4daa80), the panel 264 wide, cards 94×133 at a 102 × 141 pitch from (278, 37) / (278, 356).
  At 1024×768 the two are within a few pixels; a relayout like the event window's would make
  them equal at every scale.
- **Battle button labels** (2026-10-14). The leave-battle window's button labels come out a
  little larger and heavier than the original's (its buttons' font); check the button font.

- **Button art picked by screen width** (2026-10-14). `chrome::defeat_button` and
  `marble_button` choose Btn1/2/3 by the screen's width, so on large screens a 95-px button is
  drawn from Btn3. Check which art the original uses for which button size and pick by the
  button's own size.

## Settled

- **Event window text size, position, red button; victory with no next map** (2026-10-14).
  From 0x4a8ae8, 0x4dba90 → 0x4768f8 and 0x4aba30, matched against the original captured under
  Wine at 1024×768: the event window laid out in the original's pixels times the interface
  scale, centred over the map view, its text in Benguiat at the sheet's own size (the lines
  break as the original's); the red button darkened by (0, 64, 128) inside its 1 px rim; a
  victory with no next map goes to the main menu. main 1866ec2, dt-original a1f2444 (ported by
  hand: that branch has no scrolling in the window).
- **Market goods order** (2026-10-14). Measured under Wine on «Проклятое озеро» (78 stocks): the
  window lists the stock's places in order and the draws walk down the price window, so the
  original shows most expensive first (economy.md §2, «Display order»). The player's original
  that showed cheapest first is likely another build. Razdor keeps cheapest first by the
  owner's choice (35374ff). Recorded in 1866ec2 / a1f2444.
- **Original fonts on dt-original** (2026-10-14). Ported as dt-original's only mode (sheets,
  shadow sheets, scale rule, Benguiat for event and village texts; no «Шрифт» setting):
  dt-original 8b94705.
