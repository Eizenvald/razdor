//! «Максимум», Razdor's strongest battle AI (the owner's request; a deviation from the
//! original, main only, never on `dt-original`, off by default). The original's AI
//! ([`Battle::plan`]) stays as it is and keeps playing everywhere else: both sides of
//! off-screen battles, the pre-simulation, the world AI's simulated battles, the hero's
//! auto-battle and the rollouts here.
//!
//! A decision is a minimax search with alpha-beta pruning over the battle's real actions:
//! every legal action of the active unit (each target with its cell's action, each step,
//! a pass) is played on a clone by the game's own rules, and so on for whoever acts next
//! (a node maximises when a unit of the searching side acts, minimises when the other
//! side's does; the order is the battle's own initiative queue, several actions of a unit
//! in a row included). Battles have no randomness, so each line is exact. A leaf is scored
//! by a rollout: the rest of the battle played by the original's AI on both sides, then a
//! win or loss term that outweighs everything plus each side's remaining worth (tactical
//! value × the share of its HP left) of the searching side minus the other's. Leaves deep
//! in the tree are scored statically (the same worth, no rollout).
//!
//! Iterative deepening runs until the time (or, in tests, the node) budget is spent; the
//! deepest completed iteration decides. The original's own pick is searched first at every
//! node (the move ordering for the pruning, then the per-target damage) and at the root it
//! keeps the choice unless another action scores strictly higher, so Max never picks what
//! its own evaluation rates below the original's choice, and ties stay predictable.

use std::time::{Duration, Instant};

use super::*;

/// The time one decision may take on screen (the search returns the best found by then).
pub const MAX_AI_BUDGET: Duration = Duration::from_millis(150);
/// Iterative deepening stops at this many actions ahead.
const MAX_DEPTH: u32 = 24;
/// Leaves more than this many actions ahead are scored without a rollout.
const ROLLOUT_PLIES: u32 = 8;
/// A battle won or lost outweighs any strength left.
const WIN: i64 = 1 << 40;
const INF: i64 = i64::MAX / 4;
/// A unit's worth is its tactical value × this × its share of HP left.
const WORTH_SCALE: i64 = 1000;
/// Rollout steps between two looks at the clock.
const CLOCK_EVERY: usize = 8;

/// How long a decision may search: a time limit (the game's), or none and a node limit
/// (deterministic, for tests); both stop at `max_depth` actions ahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaxBudget {
    pub time: Option<Duration>,
    pub max_depth: u32,
    pub max_nodes: u64,
}

impl MaxBudget {
    /// The battle on screen: [`MAX_AI_BUDGET`] per decision.
    pub const GAME: MaxBudget = MaxBudget { time: Some(MAX_AI_BUDGET), max_depth: MAX_DEPTH, max_nodes: u64::MAX };

    /// No clock: at most `max_depth` actions ahead and `max_nodes` positions, the same
    /// choice on any machine.
    pub const fn fixed(max_depth: u32, max_nodes: u64) -> MaxBudget {
        MaxBudget { time: None, max_depth, max_nodes }
    }
}

/// One decision for the active unit: an action, or (`None`) the original AI's skip of the
/// unit's turn when the rules refuse its plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaxMove(Option<Plan>);

/// What a decision found.
#[derive(Clone, Copy, Debug)]
pub struct MaxReport {
    pub choice: MaxMove,
    /// The original AI's choice in the same position.
    pub original: MaxMove,
    /// The deepest completed iteration's scores of the choice and of the original's (0 and
    /// 0 when not even depth 1 completed: the original's choice is kept).
    pub value: i64,
    pub original_value: i64,
    /// Actions ahead of the deepest completed iteration.
    pub depth: u32,
    pub nodes: u64,
    pub rollouts: u64,
    pub elapsed: Duration,
}

impl Battle {
    /// The enemy of the battle on screen plays by «Максимум» (Razdor's, [`Battle::max_choice`]);
    /// the interface reads it, the rules never do.
    pub fn set_max_ai(&mut self, on: bool) {
        self.max_ai = on;
    }

    pub fn max_ai(&self) -> bool {
        self.max_ai
    }

    /// Every action the active unit `id` may take: its cells' actions, its steps, a pass.
    fn max_moves(&self, id: usize) -> Vec<Plan> {
        let mut v: Vec<Plan> = self.all_options(id).into_iter().map(|(t, k)| Plan::Act(t, k)).collect();
        v.extend(self.moves(id).into_iter().map(Plan::Move));
        v.push(Plan::Pass);
        v
    }

    /// The original AI's decision as the auto-battle carries it out: its plan, or a skip
    /// when the rules would refuse that plan.
    fn original_move(&self, legal: &[Plan]) -> MaxMove {
        MaxMove(self.plan(false).filter(|p| legal.contains(p)))
    }

    /// The order to search `id`'s actions in: the original's first, then the most damage
    /// (in tactical worth, a kill counting its whole worth again), heals, the rest of the
    /// cells' actions, steps, a pass.
    fn max_order(&self, id: usize) -> Vec<MaxMove> {
        let legal = self.max_moves(id);
        let original = self.original_move(&legal);
        let key = |p: &Plan| -> i64 {
            match *p {
                Plan::Act(t, k) => {
                    let tf = &self.fighters[t];
                    let worth = (tf.tactical.max(1) as i64) * WORTH_SCALE / tf.max_hp().max(1) as i64;
                    match self.preview(id, t, k) {
                        Preview::Damage(d) => 2 + d.max(0) as i64 * worth + if d >= tf.hp { tf.tactical.max(1) as i64 * WORTH_SCALE } else { 0 },
                        Preview::Heal(h) => 2 + h.max(0) as i64 * worth,
                        Preview::Buff(_) => 2,
                    }
                }
                Plan::Move(_) => 1,
                Plan::Pass => 0,
            }
        };
        let mut rest: Vec<Plan> = legal.iter().copied().filter(|&p| MaxMove(Some(p)) != original).collect();
        rest.sort_by_key(|p| std::cmp::Reverse(key(p)));
        std::iter::once(original).chain(rest.into_iter().map(|p| MaxMove(Some(p)))).collect()
    }

    /// Carries out a decision for the active unit; a refused action ends its turn, as the
    /// auto-battle's (`auto_step`).
    pub fn play_max_move(&mut self, m: MaxMove) -> Option<Step> {
        let actor = self.active()?;
        let step = m.0.and_then(|p| self.step_plan(p));
        step.or_else(|| {
            self.skip();
            Some(Step::Wait { actor })
        })
    }

    /// One action of the active unit by «Максимум».
    pub fn max_step(&mut self, budget: MaxBudget) -> Option<Step> {
        let report = self.max_choice(budget)?;
        self.play_max_move(report.choice)
    }

    /// `side`'s score of the battle as it stands: a win or loss, then the worth left of each
    /// side, `side`'s minus the other's.
    fn max_eval(&self, side: Team) -> i64 {
        let worth = |team: Team| -> i64 {
            self.fighters
                .iter()
                .filter(|f| f.team == team && f.alive() && !f.surrendered)
                .map(|f| f.tactical.max(1) as i64 * WORTH_SCALE * f.hp as i64 / f.max_hp().max(1) as i64 + f.hp as i64)
                .sum()
        };
        let won = match self.outcome() {
            Outcome::Ongoing => 0,
            Outcome::Victory if side == Team::Player => WIN,
            Outcome::Defeat if side == Team::Enemy => WIN,
            _ => -WIN,
        };
        won + worth(side) - worth(side.other())
    }

    /// The search for the active unit's best action (the module's comment). The battle is
    /// left as it is, the Community patch's globals too. `None` when nobody is to act.
    pub fn max_choice(&self, budget: MaxBudget) -> Option<MaxReport> {
        let id = self.active()?;
        let t0 = Instant::now();
        let saved = patch_globals();
        let mut root = self.clone();
        root.log.clear();
        root.stale_reads.borrow_mut().clear();
        let mut moves = root.max_order(id);
        let original = moves[0];
        let mut s = Search {
            side: root.fighters[id].team,
            deadline: budget.time.map(|t| t0 + t),
            max_nodes: budget.max_nodes,
            nodes: 0,
            rollouts: 0,
            aborted: false,
            cut: false,
        };
        let mut done = None;
        for depth in 1..=budget.max_depth.max(1) {
            s.cut = false;
            let Some((best, value, original_value, values)) = s.root(&root, &moves, depth) else { break };
            done = Some((best, value, original_value, depth));
            if depth == 1 {
                // Only the actions whose rollout does at least as well as the original's
                // choice's stay in the deeper search: against the original's AI, whose next
                // moves the rollout plays, every decision then keeps at least the score the
                // last one expected, so Max ends no worse than the original would have.
                moves = moves.iter().zip(&values).filter(|&(_, &v)| v >= original_value).map(|(&m, _)| m).collect();
            }
            // The previous best next after the original's.
            if let Some(i) = moves.iter().skip(1).position(|&m| m == best) {
                let m = moves.remove(i + 1);
                moves.insert(1, m);
            }
            // Every line ended within the depth (a deeper search finds the same), or the
            // original's choice is the only one left.
            if !s.cut || moves.len() == 1 {
                break;
            }
        }
        set_patch_globals(saved);
        let (choice, value, original_value, depth) = done.unwrap_or((original, 0, 0, 0));
        Some(MaxReport { choice, original, value, original_value, depth, nodes: s.nodes, rollouts: s.rollouts, elapsed: t0.elapsed() })
    }
}

/// One decision's search state.
struct Search {
    side: Team,
    deadline: Option<Instant>,
    max_nodes: u64,
    nodes: u64,
    rollouts: u64,
    /// The budget ran out: the iteration under way is dropped.
    aborted: bool,
    /// A line was cut by the depth while the battle went on.
    cut: bool,
}

impl Search {
    fn out_of_time(&mut self) -> bool {
        if !self.aborted && (self.nodes >= self.max_nodes || self.deadline.is_some_and(|d| Instant::now() >= d)) {
            self.aborted = true;
        }
        self.aborted
    }

    /// The root of one iteration: the original's move first and exactly; another move
    /// replaces it only by scoring strictly higher. Gives the best move, its score, the
    /// original's and every move's score (exact at depth 1, where nothing is pruned). `None`
    /// if the budget ran out.
    fn root(&mut self, b: &Battle, moves: &[MaxMove], depth: u32) -> Option<(MaxMove, i64, i64, Vec<i64>)> {
        let (mut best, mut alpha, mut original_value) = (moves[0], -INF, 0);
        let mut values = Vec::with_capacity(moves.len());
        for (i, &m) in moves.iter().enumerate() {
            let v = self.child(b, m, depth - 1, 1, alpha, INF);
            if self.aborted {
                return None;
            }
            values.push(v);
            if i == 0 {
                (best, alpha, original_value) = (m, v, v);
            } else if v > alpha {
                (best, alpha) = (m, v);
            }
        }
        Some((best, alpha, original_value, values))
    }

    /// The score of `m` played in `b`; the patch's globals are put back after it.
    fn child(&mut self, b: &Battle, m: MaxMove, depth: u32, ply: u32, alpha: i64, beta: i64) -> i64 {
        let globals = patch_globals();
        let mut c = b.clone();
        c.play_max_move(m);
        let v = self.value(&c, depth, ply, alpha, beta);
        set_patch_globals(globals);
        v
    }

    fn value(&mut self, b: &Battle, depth: u32, ply: u32, mut alpha: i64, mut beta: i64) -> i64 {
        self.nodes += 1;
        if self.out_of_time() {
            return 0;
        }
        let Some(id) = b.active() else { return self.leaf(b, ply) };
        if depth == 0 {
            self.cut = true;
            return self.leaf(b, ply);
        }
        let maxing = b.fighters[id].team == self.side;
        let mut best = if maxing { -INF } else { INF };
        for m in b.max_order(id) {
            let v = self.child(b, m, depth - 1, ply + 1, alpha, beta);
            if self.aborted {
                return 0;
            }
            if maxing {
                best = best.max(v);
                alpha = alpha.max(v);
            } else {
                best = best.min(v);
                beta = beta.min(v);
            }
            if alpha >= beta {
                break;
            }
        }
        best
    }

    /// A leaf: the rest of the battle by the original's AI on both sides, then scored; deep
    /// in the tree, or with the battle over, scored as it stands.
    fn leaf(&mut self, b: &Battle, ply: u32) -> i64 {
        if b.outcome() != Outcome::Ongoing || ply > ROLLOUT_PLIES {
            return b.max_eval(self.side);
        }
        self.rollouts += 1;
        let mut r = b.clone();
        for i in 0..AUTO_PLAY_STEPS {
            if r.outcome() != Outcome::Ongoing {
                break;
            }
            if i % CLOCK_EVERY == 0 && self.deadline.is_some_and(|d| Instant::now() >= d) {
                self.aborted = true;
                return 0;
            }
            r.auto_step();
        }
        r.max_eval(self.side)
    }
}

#[cfg(test)]
mod tests;
