# Open questions

Parity questions and known differences from the original that are not settled yet, newest
first. Every agent that leaves something unresolved adds it here (what, where, what is known,
what would settle it); whoever settles it moves it to **Settled** with the commit. The specs'
"Razdor now → original" tables stay the long-term parity list; this file is for loose ends
from reports and recent work. `docs/PLAYTEST_NOTES.md` keeps the player-facing story.

## Open

- **Market goods order** (2026-10-14). The original shows the stock unsorted, in restock order
  (0x4be9a0 jumps over its sort; 0x4be178 draws the stock). A player's screenshots showed the
  same goods in reverse order between Razdor and the original. Razdor now sorts cheapest first
  at the owner's request (35374ff), so this only matters for the spec: settle the original's
  order with a run under Wine on «Проклятое озеро» and record it in economy.md.
- **Event window text size** (2026-10-14). Razdor's event-window text is larger than the
  original's (wraps to 3 lines where the original uses 2; seen on the defeat window). Applies
  to every event window. Settle against the original's font sheet and box size (0x4a8ae8,
  0x4aa76c) and screenshots under Wine.
- **Event and defeat window position** (2026-10-14). Razdor centres the event window on the
  whole screen; the original centres it over the map view above the bottom bar.
- **Red button overlay** (2026-10-14). The defeat window's red button lacks the original's thin
  1 px overlay (0x4dba90 → 0x48dd40).
- **Victory with no next map** (2026-10-14). The original goes to the main menu; Razdor shows
  its own victory screen.

## Settled

- **Original fonts on dt-original** (2026-10-14). Ported as dt-original's only mode (sheets,
  shadow sheets, scale rule, Benguiat for event and village texts; no «Шрифт» setting):
  dt-original 8b94705.
