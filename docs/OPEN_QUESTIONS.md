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
