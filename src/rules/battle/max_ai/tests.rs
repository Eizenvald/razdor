//! «Максимум» against the original's AI: the same positions played with the original on
//! both sides and with Max on one, each army on each side.
use std::time::Duration;

use super::*;
use crate::rules::custom::{self, Pick};

/// The deterministic budget of the harness: a few actions ahead, a node cap.
const TEST_BUDGET: MaxBudget = MaxBudget::fixed(4, 600);

/// One battle played to its end, `max` (if any) playing by «Максимум».
struct Played {
    outcome: Outcome,
    battle: Battle,
    decisions: Vec<Duration>,
}

fn play(mut b: Battle, max: Option<Team>, budget: MaxBudget) -> Played {
    b.begin();
    let mut decisions = Vec::new();
    for _ in 0..AUTO_PLAY_STEPS {
        let Some(id) = b.active() else { break };
        if Some(b.fighters[id].team) == max {
            let r = b.max_choice(budget).unwrap();
            decisions.push(r.elapsed);
            b.play_max_move(r.choice);
        } else {
            b.auto_step();
        }
    }
    Played { outcome: b.outcome(), battle: b, decisions }
}

fn won(outcome: Outcome, team: Team) -> bool {
    matches!((outcome, team), (Outcome::Victory, Team::Player) | (Outcome::Defeat, Team::Enemy))
}

/// `team`'s worth left at the end as a share (0..1) of its worth at the start.
fn worth_left(b: &Battle, team: Team) -> f64 {
    let worth = |alive: bool| -> f64 {
        b.fighters
            .iter()
            .filter(|f| f.team == team && (!alive || (f.alive() && !f.surrendered)))
            .map(|f| f.tactical.max(1) as f64 * if alive { f.hp as f64 / f.max_hp().max(1) as f64 } else { 1.0 })
            .sum()
    };
    worth(true) / worth(false).max(1.0)
}

/// The tally of a harness run.
#[derive(Default)]
struct Tally {
    games: u32,
    max_wins: u32,
    orig_wins: u32,
    max_left: f64,
    orig_left: f64,
    decisions: Vec<Duration>,
    /// Positions the original's side wins and Max's loses, or where Max keeps much less.
    worse: Vec<String>,
    /// Positions where Max ends below the original by its own score (none expected: each
    /// decision keeps at least what the original's rollout from there gives).
    below: Vec<String>,
}

impl Tally {
    /// Plays `b` with Max on each side against the original's AI, and the same position
    /// with the original on both sides.
    fn add(&mut self, name: &str, b: &Battle, budget: MaxBudget) {
        let base = play(b.clone(), None, budget);
        for team in Team::BOTH {
            let m = play(b.clone(), Some(team), budget);
            self.games += 1;
            let (mw, ow) = (won(m.outcome, team), won(base.outcome, team));
            self.max_wins += u32::from(mw);
            self.orig_wins += u32::from(ow);
            let (ml, ol) = (worth_left(&m.battle, team), worth_left(&base.battle, team));
            self.max_left += ml;
            self.orig_left += ol;
            if (ow && !mw) || (mw == ow && ol - ml > 0.25) {
                self.worse.push(format!("{name}, Max as {team:?}: original {:?} {ol:.2} left, Max {:?} {ml:.2} left", base.outcome, m.outcome));
            }
            if m.battle.max_eval(team) < base.battle.max_eval(team) {
                self.below.push(format!("{name}, Max as {team:?}"));
            }
            self.decisions.extend(m.decisions);
        }
    }

    fn report(&self, title: &str) {
        let n = self.games.max(1) as f64;
        let mut d = self.decisions.clone();
        d.sort();
        let at = |q: f64| d.get(((d.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or_default();
        println!(
            "{title}: {} games; wins Max {} ({:.0}%) vs original {} ({:.0}%); worth left Max {:.2} vs original {:.2}; {} decisions, median {:?}, 95% {:?}, max {:?}",
            self.games,
            self.max_wins,
            100.0 * self.max_wins as f64 / n,
            self.orig_wins,
            100.0 * self.orig_wins as f64 / n,
            self.max_left / n,
            self.orig_left / n,
            d.len(),
            at(0.5),
            at(0.95),
            d.last().copied().unwrap_or_default()
        );
        for w in &self.below {
            println!("  below the original's score: {w}");
        }
        for w in &self.worse {
            println!("  worse: {w}");
        }
    }
}

/// A small deterministic generator for the synthetic armies.
fn lcg(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *seed >> 33
}

/// A custom battle's armies (`rules::custom`), as the setup screen builds them.
fn custom_battle(c: &Content, armies: [Vec<Pick>; 2], wide: bool) -> Battle {
    let setup = custom::Setup { armies, control: [custom::Control::Ai; 2], wide };
    setup.battle(c, true).unwrap()
}

/// Custom battles of `c`'s unit types: armies of 2 to `most` units at levels 1 to 6, both
/// formations.
fn custom_battles(c: &Content, label: &str, n: usize, most: usize, mut seed: u64) -> Vec<(String, Battle)> {
    let choices = custom::choices(c);
    (0..n)
        .map(|i| {
            let army = |seed: &mut u64| -> Vec<Pick> {
                let len = 2 + lcg(seed) as usize % (most - 1);
                (0..len).map(|_| Pick { unit: choices[lcg(seed) as usize % choices.len()], level: 1 + lcg(seed) as i32 % 6, items: Vec::new() }).collect()
            };
            let armies = [army(&mut seed), army(&mut seed)];
            (format!("{label} #{i}"), custom_battle(c, armies, i % 2 == 0))
        })
        .collect()
}

/// Battles of the demo's units.
fn synthetic_battles(n: usize) -> Vec<(String, Battle)> {
    custom_battles(&Content::builtin(), "demo", n, 6, 7)
}

#[test]
fn max_keeps_the_original_choice_unless_it_finds_better() {
    for (name, mut b) in synthetic_battles(6) {
        b.begin();
        let r = b.max_choice(TEST_BUDGET).unwrap();
        assert!(r.depth >= 1, "{name}: depth 1 completes within the budget");
        assert!(r.value >= r.original_value, "{name}: never below the original's choice");
        if r.choice != r.original {
            assert!(r.value > r.original_value, "{name}: a tie keeps the original's choice");
        }
    }
}

#[test]
fn max_leaves_the_battle_and_the_patch_globals_alone_and_is_deterministic() {
    let (_, mut b) = synthetic_battles(1).remove(0);
    b.begin();
    let globals = patch_globals();
    let before = (b.log.clone(), b.fighters.iter().map(|f| (f.hp, f.slot, f.actions)).collect::<Vec<_>>());
    let first = b.max_choice(TEST_BUDGET).unwrap();
    assert_eq!(patch_globals(), globals);
    assert_eq!((b.log.clone(), b.fighters.iter().map(|f| (f.hp, f.slot, f.actions)).collect::<Vec<_>>()), before);
    let again = b.max_choice(TEST_BUDGET).unwrap();
    assert_eq!((first.choice, first.value, first.nodes), (again.choice, again.value, again.nodes));
}

#[test]
fn max_finishes_every_battle() {
    for (name, b) in synthetic_battles(4) {
        let p = play(b, Some(Team::Enemy), TEST_BUDGET);
        assert_ne!(p.outcome, Outcome::Ongoing, "{name}");
    }
}

/// The off-screen battles and the auto-battles never use Max, even with it chosen.
#[test]
fn only_the_interface_asks_for_max() {
    let (_, mut b) = synthetic_battles(1).remove(0);
    let mut plain = b.clone();
    b.set_max_ai(true);
    assert!(b.max_ai());
    assert_eq!(b.auto_play_to_end(), plain.auto_play_to_end());
    assert_eq!(b.log, plain.log);
}

/// Max against the original's AI on the demo's units, Max on each side of each position.
#[test]
fn max_beats_the_original_on_synthetic_battles() {
    let mut t = Tally::default();
    for (name, b) in synthetic_battles(24) {
        t.add(&name, &b, TEST_BUDGET);
    }
    t.report("synthetic");
    assert!(t.below.is_empty(), "Max never ends below the original's score");
    assert!(t.max_wins >= t.orig_wins, "Max wins at least as often");
    assert!(t.max_left >= t.orig_left, "and keeps at least as much");
}

/// [`MAX_AI_BUDGET`] × `RAZDOR_PERF_BUDGET_SCALE` (2 on the loaded E-cores), with a quarter
/// over for the clock checks between rollout steps.
fn decision_limit() -> Duration {
    let scale = std::env::var("RAZDOR_PERF_BUDGET_SCALE").ok().and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| *s > 0.0);
    MAX_AI_BUDGET.mul_f64(1.25 * scale.unwrap_or(1.0))
}

/// The game's own budget: every decision returns within its time.
#[test]
fn a_decision_keeps_to_its_time() {
    if cfg!(debug_assertions) {
        return;
    }
    let mut longest = Duration::ZERO;
    for (_, b) in synthetic_battles(3) {
        let p = play(b, Some(Team::Enemy), MaxBudget::GAME);
        longest = longest.max(p.decisions.iter().copied().max().unwrap_or_default());
    }
    assert!(longest <= decision_limit(), "{longest:?}");
}

/// The armies of the player's maps (`RAZDOR_DT_DIR`): each map's first armies against the
/// hero's army as the game sets up the battle, armies of near strength against each other,
/// and custom battles of the install's unit types; Max on each side of each. Prints the
/// tally; Max must win at least as often as the original and never end below its score.
#[test]
fn max_beats_the_original_on_real_armies() {
    real_armies(TEST_BUDGET);
}

/// The same with the game's time budget per decision (slow: run by hand with `--ignored`).
#[test]
#[ignore]
fn max_beats_the_original_on_real_armies_in_game_time() {
    real_armies(MaxBudget::GAME);
}

fn real_armies(budget: MaxBudget) {
    use crate::dt::install::DtInstall;
    use crate::rules::content::HeroClass;
    use crate::rules::game::{Foe, Game};
    let Some(dir) = std::env::var_os(crate::dt::install::ENV_VAR) else { return };
    let dt = DtInstall::load(std::path::Path::new(&dir)).unwrap();
    let c = Arc::new(Content::from_dt(&dt));
    let mut hero = Vec::new();
    let mut armies: Vec<(String, i64, Vec<Unit>)> = Vec::new();
    for m in &dt.maps {
        let s = m.load().unwrap();
        let g0 = Game::from_scenario(c.clone(), &s, HeroClass::Knight);
        for (i, a) in g0.world.armies.iter().enumerate() {
            let units = crate::rules::ai::army_units(&c, a);
            if units.is_empty() {
                continue;
            }
            if i < 2 {
                let mut g = Game::from_scenario(c.clone(), &s, HeroClass::Knight);
                g.drain_events();
                g.improved_ai = true;
                g.foe = Some(Foe::Army(i));
                hero.push((format!("{} hero vs army {i}", m.name), g.start_battle()));
            }
            let strength = units.iter().map(|u| u.tactical(&c, 0) as i64).sum();
            armies.push((format!("{} army {i}", m.name), strength, units));
        }
    }
    // Neighbours by strength, every third pair, up to 30, each army on each side.
    armies.sort_by_key(|a| a.1);
    let mut pairs = Vec::new();
    for w in armies.windows(2).step_by(3).take(30) {
        for (p, e) in [(&w[0], &w[1]), (&w[1], &w[0])] {
            let squad: Vec<(usize, &Unit)> = p.2.iter().enumerate().map(|(k, u)| (k + 1, u)).collect();
            let mut b = Battle::new(c.clone(), &squad, &e.2, Team::Player);
            b.set_improved_ai(true);
            b.auto_arrange(Team::Player);
            b.auto_arrange(Team::Enemy);
            pairs.push((format!("{} vs {}", p.0, e.0), b));
        }
    }
    let customs = custom_battles(&c, "custom", 20, 8, 11);
    let mut all = Tally::default();
    for (title, set) in [("real: hero vs map armies", hero), ("real: map armies of near strength", pairs), ("real: custom battles", customs)] {
        let mut t = Tally::default();
        for (name, b) in &set {
            t.add(name, b, budget);
        }
        t.report(title);
        all.games += t.games;
        all.max_wins += t.max_wins;
        all.orig_wins += t.orig_wins;
        all.max_left += t.max_left;
        all.orig_left += t.orig_left;
        all.decisions.extend(t.decisions);
        all.below.extend(t.below);
    }
    all.report("real: all");
    if budget.time.is_some() && !cfg!(debug_assertions) {
        let longest = all.decisions.iter().copied().max().unwrap_or_default();
        assert!(longest <= decision_limit(), "{longest:?}");
    }
    assert!(all.below.is_empty(), "Max never ends below the original's score");
    assert!(all.max_wins >= all.orig_wins, "Max wins at least as often");
}
