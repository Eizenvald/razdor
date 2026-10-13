# Flow: the battle AI

**Read this first.** The battle AI is a set of pure scoring functions on `Battle`
(`src/rules/battle.rs`, section "AI"), copied from the original's code (486bb9 and on,
`docs/reference/original-mechanics/battle.md` §4, §3 for mage targets, §2 for the reserve). It
picks one action for the active unit; it never looks ahead except through `killable`. The
same AI plays the enemy in the player's battles, both sides of off-screen battles between AI
armies (`src/rules/ai.rs`), the pre-simulation in `Battle::begin`, and, with two changes, the
player's side in Razdor's auto-battle. The world-map AI that decides *whether* to fight is a
different thing: `docs/reference/original-mechanics/ai.md` §4 (simulated battles), §10 (AI
against AI).

## 1. Call chain

```
BattleView::advance (computer's turn)          src/ui/battle_view.rs
  └ BattleView::ai_move → Battle::hero_auto_step
       └ auto_step_by(hero) → step_by(hero) → plan(hero) → Plan
                                             └ act_with | move_active | pass   (rules)
Battle::ai_step = step_by(false)               (original AI on both sides)
Battle::auto_play_to_end / hero_auto_play_to_end: begin() then step until the outcome
```

`Plan` is `Act(target, kind)`, `Move(slot)` or `Pass`. `step_by` turns it into a `Step` for
the UI. If a plan fails, `auto_step_by` calls `skip` (the unit passes its remaining actions),
so an auto-battle always terminates (`AUTO_PLAY_STEPS` caps it).

## 2. `Battle::plan`, in order

1. **Reserve** unit → `ai_move` directly (4864e0), a Ghost's cast included.
2. **Retreat** (`ai_retreat`, skipped for the hero's units): a front-row non-warrior with more
   than one action whose AB is not above both its power and AS steps back behind the
   healthiest own front unit, if another own unit stands in front (or it is the side's lone
   mage); with no back cell, the original's edge rule scores the corner front cells.
3. **Melee** (486bb9) on enemy front cells: `dmg × round((R + 1) × M)` (R = the target's
   answer `return_threat`, M = its Manevres, half its negative actions; Community 0-Manevres
   factor `MANEVRES_0_FACTOR` wraps in 32 bits); ×2 if it poisons (`poisons`); a kill
   (`killable`) replaces it with `100 × round(...)`.
4. **Shot** (486f90): `round((power + 1) × dmg × M')`, ×2 poison, ×4 kill, then a back-row
   target is ÷3 (warrior), ×1.5 (shooter) or ×1.75 (mage, halved again for same-nature
   hostile-only casters and single-Manevres ones).
5. **Magic** (`ai_magic`) by school: Life (486bb9: heal the biggest wound not below a quarter
   of the power, not an undead's; or strike), Elemental (`ai_elemental`, 487e99: haste and
   the side's mean initiative), Death (488928, `life_strike_score`). It only picks a cell;
   the cell's own action (heal/bless, strike/curse) is what is cast.
6. Hero's side only: a healer/blesser with nothing to cast → `own_cell_plan` (keeps its cell).
7. **Move** (`ai_move`, 489549): back-row pure warrior steps forward (support score); front
   row scores columns by nearby enemies (and may pick an ally's cell to heal/bless); reserve:
   mages tend the most wounded reserve unit, warriors come out (front 2, back 1), others to the
   nearest back cell (Community skips column 0 with 6 columns). The AI **never** moves into
   the reserve. Nothing scores → `own_cell_plan` (self-cast or pass).

The picker `Battle::pick` (4860cc): rows front to back, columns in `Formation::col_order`,
the first **strictly** higher score wins, 0 or less never does. Ties go to the earlier cell;
this ordering is why a seemingly equal choice differs between columns.

## 3. Inputs the AI reads

- `Fighter::ai_power` / `ai_role` (`ai_power_role`, 4836cc): the best of AB, AS, MP plus a third
  of the other two; mage if MP ≥ ⅔ of it, else shooter if AS ≥ ⅔, else warrior. Set once in
  `begin` from the setup's stats.
- `physical_damage`, `strike_power`, `hits_to_kill` — the same damage code as the real action.
- `Battle::ai_level` (`set_improved_ai`): 1 normal, 2 "improved enemy AI" (`OptValue9`), 0
  between AI armies. With 2 (or the player's side at 1) `killable` counts all actions left
  (`HP ≤ actions × dmg`), else one blow.
- `mean_initiative` per side (turn start), for the Elemental school.
- Stale records (`set_side_records`, `stale_reads`): the original's killable test reads a
  wrong-side record; off-screen battles keep their static sides' old HP. Razdor reproduces this
  for the AI's simulated battles and fixes it for the battle on screen (comment in `killable`).

## 4. Hero auto-battle (Razdor extra, main only)

Commit a375a60. `plan(hero = true)` applies to the player's units only: no `ai_retreat`, and
a friendly caster with nothing to cast stays put. Everything else is the original's AI, and
the enemy always plays by it (`hero_auto_battle_leaves_the_enemy_to_the_original_s_ai`). It
is what Q (quick battle) and W (watch) use in `BattleView`; AI-vs-AI, the pre-simulation, the
difftests and the cheats use `auto_play_to_end` / `ai_step`.

## 5. Off-screen battles (world AI)

`src/rules/ai.rs`: `fight` builds a `Battle` between two `Side`s (units, defence, strength
defence, bare strengths), `set_simulation`, optionally `skip_prediction`, auto-arranges both
sides, `set_side_records`, `begin`, then steps to the end; `simulate` wraps it into a
`SimResult` and caches by a `SimKey` (FxHash). The world AI's scores come from there
(`ai.md` §4). Changing anything in the battle AI changes the world AI's choices too —
run the real-maps AI test (`src/rules/ai/real_maps.rs`, needs `RAZDOR_DT_DIR`).

## 6. Traps

- Scores are integers with the original's rounding (`experience::round_half_even`) and 32-bit
  wrapping where the exe wraps; do not "clean up" to floats.
- The AI must stay deterministic: the difftests replay recorded battles action by action
  (`rk1_battle_by_actions`), and the AI cache assumes the same inputs give the same battle.
- A plan the rules refuse (`act_with` error) silently becomes a skip in auto-play; in
  `ai_step` it returns `None` — check both when a unit "does nothing".
- The player reported supports "running to the reserve" in auto-battle: it was `ai_retreat`
  stepping them to the back row; the AI never enters the reserve itself.

## 7. Tests

`src/rules/battle/tests.rs`: `row30_ai_leaves_the_reserve_and_never_enters_it`,
`auto_battles_terminate_deterministically_and_never_use_the_reserve`,
`hero_auto_battle_keeps_a_front_row_blesser_in_its_cell`,
`hero_auto_battle_leaves_the_enemy_to_the_original_s_ai`, `real_armies_auto_battle_terminates`.

---
Verify every `path`, symbol and test named in the battle flow docs (run from the repo root):

```sh
python3 -I - docs/flows/battle.md docs/flows/battle-ai.md <<'EOF'
import re, subprocess, sys
src = subprocess.run(['git', 'grep', '-hE', r'(fn|struct|enum|const|static|type) [A-Za-z_0-9]+', '--', 'src'], capture_output=True, text=True).stdout
defined = set(re.findall(r'(?:fn|struct|enum|const|static|type) ([A-Za-z_0-9]+)', src))
bad = []
for doc in sys.argv[1:]:
    text = open(doc, encoding='utf-8').read()
    for p in set(re.findall(r'`((?:src|docs|data|tools)/[^`\s]+?)`', text)):
        if subprocess.run(['git', 'ls-files', '--error-unmatch', p.rstrip('/')], capture_output=True).returncode and not subprocess.run(['test', '-e', p]).returncode == 0:
            bad.append((doc, p))
    for s in set(re.findall(r'`(?:[A-Z][A-Za-z]+::)?([a-z_][a-z_0-9]+|[A-Z][A-Za-z0-9]+|[A-Z_0-9]{4,})`', text)):
        if s not in defined and not subprocess.run(['git', 'grep', '-qw', s, '--', 'src', 'data'], capture_output=True).returncode == 0:
            bad.append((doc, s))
print('\n'.join(f'{d}: {s}' for d, s in sorted(bad)) or 'all references found')
EOF
```
