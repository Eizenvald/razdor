//! The spell book on the world map (key B): the hero's learned spells in the original's
//! 3 × 5 cells, what each costs this hero (mana and casting time), how long it lasts and what
//! it does; cast on the own army or on an enemy army within reach; the spells on the army
//! now, with the time they have left.


use macroquad::prelude::*;

use razdor::i18n::{n_, tr};
use razdor::trf;

use razdor::rules::clock::duration_label;
use razdor::rules::content::SpellDef;
use razdor::rules::game::Game;
use razdor::rules::magic::{self, CastError, CastOutcome, CastTarget, Duration, CASTABLE};

use super::audio::{cue, Cue};
use super::widgets::*;
use super::world_view;
use super::Screen;

const COLS: usize = 3;

/// "heals 30 hits", "defence +5, hits +20%", "-15 hits".
pub fn effect_summary(s: &SpellDef) -> String {
    use razdor::rules::items::stat_label;
    let mut effects = Vec::new();
    if let Some(h) = s.delta_fixed_hits {
        effects.push(if h >= 0 { trf!("heals {h} hits", h) } else { trf!("{h} hits", h) });
    }
    if let Some(p) = s.delta_percent_hits {
        effects.push(trf!("hits {p}% at once", p = format!("{p:+}")));
    }
    for (&st, &v) in &s.add {
        effects.push(format!("{} {v:+}", stat_label(st)));
    }
    for (&st, &v) in &s.percent {
        effects.push(format!("{} {v:+}%", stat_label(st)));
    }
    match s.life_lose_percent {
        Some(p) if p < 0 => effects.push(trf!("max hits {p}%", p)),
        Some(_) => effects.push(tr("lifts life-draining curses").into()),
        None => {}
    }
    if effects.is_empty() {
        tr("no effect").into()
    } else {
        effects.join(", ")
    }
}

pub fn duration_text(s: &SpellDef) -> String {
    match Duration::of(s) {
        Duration::Instant => tr("instant").into(),
        Duration::Minutes(m) => trf!("lasts {time}", time = duration_label(m as f64)),
    }
}

/// The spell `s` read on `t` landed with `outcome`: its sound, and the line telling it.
pub fn landed(s: &SpellDef, t: CastTarget, outcome: CastOutcome) -> String {
    if matches!(outcome, CastOutcome::Done { .. }) {
        cue(if matches!(t, CastTarget::Own) { Cue::SpellGood } else { Cue::SpellEvil });
    }
    match outcome {
        CastOutcome::Done { hits, killed, destroyed } => {
            let mut m = s.name.clone();
            if hits != 0 {
                m += &trf!(": {hits} hits", hits = format!("{hits:+}"));
            }
            if killed > 0 {
                m += &trf!(", {killed} fell", killed);
            }
            if destroyed {
                m += tr(". The army is no more.");
            } else if magic::is_lasting(s) {
                m += &format!(" ({})", duration_text(s));
            }
            m
        }
        CastOutcome::Interrupted => tr("An enemy fell on you while you were casting: the spell is lost.").into(),
        CastOutcome::TargetLost => tr("The target left the map before the spell was ready.").into(),
    }
}

fn cast_error(e: CastError) -> &'static str {
    match e {
        CastError::NotInBook | CastError::NoSuchSpell => tr("That spell is not in your book."),
        CastError::NotEnoughMana => tr("Not enough mana."),
        CastError::WrongTarget => tr("That spell does not work on that target."),
        CastError::NotATarget => tr("That army cannot be targeted."),
        CastError::Busy => tr("Not now: a battle is coming."),
    }
}

/// A text of the install (`[Magic] <key>`) in Russian, else ours.
fn own(key: &str, ours: &'static str) -> String {
    let t = (razdor::i18n::lang() == razdor::i18n::Lang::Ru).then(|| super::chrome::ui_text("Magic", key)).flatten();
    t.unwrap_or_else(|| tr(ours).to_string())
}

/// Whole hours of `minutes`, as the book writes them ("2 час", "6 часов" in the original).
fn hours(minutes: u64, many: bool) -> String {
    let h = (minutes as f64 / 60.0).round() as u64;
    let word = if many { own("Hours", n_("hours")) } else { own("Hour", n_("h")) };
    format!("{h} {word}")
}

/// The spell book window as the original's (the video, 36:59): 3 × 5 brown cells, each
/// with the spell's round picture in its silver frame, its name, what it does, its mana and
/// reading time and how long it lasts; empty cells are dark with the knot and "Нет изученных
/// заклинаний". A click casts the spell; an enemy spell's target is then picked on the map
/// (`world_view::begin_targeting`).
pub fn frame(
    game: &mut Game,
    assets: &super::assets::Assets,
    selected: &mut usize,
    message: &mut Option<String>,
) -> Option<Screen> {
    use super::chrome::{self, CREAM};
    let bar_pick = world_view::window_backdrop(game, assets, Some(super::game_bar::BarButton::Spells));
    let k = chrome::k();
    // The army screen's window: 836 × 600 video pixels over the map.
    let r = super::window_layout::window();
    let title = own("Title", n_("The hero's spell book"));
    let (inner, closed) = chrome::window(r, &title, chrome::Skin::Marble, true);
    let book: Vec<SpellDef> = game.book().into_iter().cloned().collect();
    let gap = 8.0 * k;
    let (cw, ch) = ((inner.w - 4.0 * gap) / 3.0, (inner.h - 6.0 * gap) / 5.0 + gap);
    let mut cast = None;
    for cell in 0..CASTABLE {
        let (c, row) = (cell % COLS, cell / COLS);
        let cr = Rect::new(inner.x + gap + c as f32 * (cw + gap), inner.y + gap + row as f32 * ch, cw, ch - gap);
        let spell = book.get(cell);
        // The cell: brown parchment; an empty one is the original's (0x4dc118 makes it,
        // 0x49bde4 draws it): `Win-paper` from (20, 20) less (180, 140, 113), an olive, inside
        // a 1 px steel line (0x48d1d4).
        match (spell, chrome::win_scaled("Win-paper", [1024; 3], [-180, -140, -113])) {
            (None, Some(paper)) => {
                chrome::tex_src(&paper, Rect::new(20.0, 20.0, 286.0, 112.0), cr, WHITE);
                draw_rectangle_lines(cr.x, cr.y, cr.w, cr.h, k.max(1.0), chrome::SILVER);
            }
            (None, None) => {
                chrome::text_box(cr);
                chrome::multiply(|| draw_rectangle(cr.x, cr.y, cr.w, cr.h, Color::new(0.62, 0.78, 0.5, 1.0)));
            }
            (Some(_), _) => chrome::text_box(cr),
        }
        let hover = !input_blocked() && cr.contains(crate::ui::widgets::pointer().into());
        if spell.is_some() && (*selected == cell || hover) {
            draw_rectangle_lines(cr.x, cr.y, cr.w, cr.h, k.max(1.0), Color::new(1.0, 0.85, 0.5, 1.0));
        }
        let icon = Rect::new(cr.x + 4.0 * k, cr.y + 4.0 * k, cr.h - 8.0 * k, cr.h - 8.0 * k);
        let tx = icon.x + icon.w + (cr.x + cr.w - icon.x - icon.w) / 2.0;
        let room = cr.x + cr.w - icon.x - icon.w - 8.0 * k;
        let Some(s) = spell else {
            // The knot stamped in black: `MSign` less 192 taken off the paper (0x4dc118: 4768f8
            // then 476790 in mode 1 at (6, 6)).
            match chrome::win_scaled("MSign", [1024; 3], [-192; 3]) {
                Some(t) => chrome::subtractive(|| chrome::tex(&t, Rect::new(cr.x + 6.0 * k, cr.y + 6.0 * k, icon.w, icon.h), WHITE)),
                None => {
                    if let Some(t) = chrome::win_fx("MSign", chrome::Fx::Shade) {
                        chrome::tex(&t, icon, WHITE);
                    }
                }
            }
            let none = own("NoSpell", n_("No\nspells\nlearnt"));
            let lines: Vec<&str> = none.split('\n').collect();
            let size = 15.0 * k;
            let top = cr.y + cr.h / 2.0 - (lines.len() as f32 - 1.0) * 8.0 * k + size * 0.36;
            // Pressed into the paper as the original's (0x49bde4): white Benguiat taken off the
            // paper (the text box's mode 5, which leaves black) 2 px down and right and 1 px up
            // and left, then its font 0xae24b4 (Benguiat less 125, 140, 170) drawn over them.
            super::dt_font::with_face(super::dt_font::Face::Subtitle, || {
                for (i, l) in lines.iter().enumerate() {
                    let y = top + i as f32 * 16.0 * k;
                    text_centered(l, tx + 2.0 * k, y + 2.0 * k, size, BLACK);
                    text_centered(l, tx - 1.0 * k, y - 1.0 * k, size, BLACK);
                    text_centered(l, tx, y, size, Color::from_rgba(130, 115, 85, 255));
                }
            });
            continue;
        };
        chrome::spell_icon(s, icon);
        let cost = game.cast_cost(s);
        let size = 12.0 * k;
        super::dt_font::with_face(super::dt_font::Face::Subtitle, || {
            let ns = fit_size(&s.name, room, 17.0 * k);
            chrome::shadow_centered(&ellipsize(&s.name, room, ns), tx, cr.y + 26.0 * k, ns, CREAM);
        });
        let effect = effect_summary(s);
        // The card shows the cost function of `TimeCast` in hours, not the real wait (0x49bbe0).
        let card_hours = game.card_cast_hours(s).max(0) as u64;
        let mana_line = format!("{} {}, {} {}", own("Mana", n_("Mana:")), cost.mana, own("Reading", n_("Reading:")), hours(card_hours * 60, false));
        let last = match Duration::of(s) {
            Duration::Instant => own("MomentaryEffect", n_("Instant effect")),
            Duration::Minutes(m) => format!("{} {}", own("TimeOfEffect", n_("Lasts:")), hours(m, true)),
        };
        let mana_color = if game.mana >= cost.mana { Color::new(1.0, 0.72, 0.3, 1.0) } else { chrome::RED_TEXT };
        let mut y = cr.y + 50.0 * k;
        for line in wrap(&effect, room, size).iter().take(2) {
            chrome::shadow_centered(line, tx, y, size, chrome::BLUE_TEXT);
            y += 13.0 * k;
        }
        chrome::shadow_centered(&ellipsize(&mana_line, room, size), tx, y, size, mana_color);
        chrome::shadow_centered(&ellipsize(&last, room, size), tx, y + 13.0 * k, size, Color::new(1.0, 0.72, 0.3, 1.0));
        if hover {
            if game.mana < cost.mana {
                tooltip(&[(own("NoManaForSpell", n_("Not enough mana\nto cast this spell!")).replace('\n', " "), chrome::RED_TEXT)]);
            }
            if clicked() {
                *selected = cell;
                cast = Some(s.clone());
            }
        }
    }
    // A spell on the hero's army is cast at once; an enemy spell closes the book and the map
    // asks for its target (0x4c2e34 → 0x4c2f39). Without the mana the click does nothing.
    let mut next = None;
    if let Some(s) = cast.filter(|s| game.mana >= game.cast_cost(s).mana) {
        if magic::targets_enemy(&s) {
            if game.foe.is_none() {
                world_view::begin_targeting(s.id);
                next = Some(Screen::WorldMap);
            }
        } else {
            next = self::cast(game, s.id, CastTarget::Own, message);
        }
    }
    if closed || key(KeyCode::Escape) || key(KeyCode::B) {
        next = next.or(Some(Screen::WorldMap));
    }
    if let Some(m) = message.as_ref().filter(|_| next.is_none()) {
        chrome::shadow_centered(m, r.center().x, r.y + r.h - 8.0 * k, 14.0 * k, ACCENT);
    }
    next.or(bar_pick)
}

/// Casts `spell` on `target`: the hero reads it on the map, and it lands when the reading is
/// done. The map is next, or nothing when it cannot be cast (the message says why).
pub(super) fn cast(game: &mut Game, spell: u32, target: CastTarget, message: &mut Option<String>) -> Option<Screen> {
    let s = game.spell(spell)?.clone();
    match game.begin_cast(s.id, target) {
        Ok(outcome) => {
            cue(Cue::CastSpell);
            // The camera goes to the army the spell is for: the hero's own before the
            // reading, an enemy's after it (0x4c2e34, 0x4cc148); then the effect on it.
            let own = matches!(target, CastTarget::Own);
            if own {
                world_view::look_at(target);
            }
            if let Some(CastOutcome::Done { .. }) = outcome {
                if !own {
                    world_view::look_at(target);
                }
                world_view::spell_effect(&s, target);
            }
            razdor::diag::play(&game.clock.label(), &format!("CAST «{}» on {target:?}: {:.0} min", s.name, game.reading().map_or(0.0, |r| r.1)));
            *message = Some(match outcome {
                Some(o) => landed(&s, target, o),
                None => trf!("Reading «{name}»…", name = s.name),
            });
            Some(Screen::WorldMap)
        }
        Err(e) => {
            *message = Some(cast_error(e).into());
            None
        }
    }
}

/// A spell's card as the sanctuary shows it (0x49bde4): a box with the spell's picture and
/// its silver frame on the left (100 px, 6 px in), and to its right, centred and wrapped
/// into the rest of the box, the name, a blank line, what it does, the mana and reading
/// time and how long it lasts, the block centred in the box's height.
pub fn spell_card(game: &Game, s: &SpellDef, r: Rect) {
    super::typography::Role::Body.with(|| spell_card_in_face(game, s, r));
}

fn spell_card_in_face(game: &Game, s: &SpellDef, r: Rect) {
    use super::chrome::{self, CREAM};
    let k = chrome::k();
    chrome::surface(r, chrome::Skin::Brown);
    chrome::silver_frame(r, k.max(1.0));
    let side = (r.h - 12.0 * k).min(100.0 * k);
    let icon = Rect::new(r.x + 6.0 * k, r.y + (r.h - side) / 2.0, side, side);
    chrome::spell_icon(s, icon);
    let left = icon.x + icon.w + 2.0 * k;
    let room = r.x + r.w - left - 8.0 * k;
    let tx = left + (r.x + r.w - left) / 2.0;
    let cost = game.cast_cost(s);
    let card_hours = game.card_cast_hours(s).max(0) as u64;
    let mana_line = format!("{} {}, {} {}", own("Mana", n_("Mana:")), cost.mana, own("Reading", n_("Reading:")), hours(card_hours * 60, false));
    let last = match Duration::of(s) {
        Duration::Instant => own("MomentaryEffect", n_("Instant effect")),
        Duration::Minutes(m) => format!("{} {}", own("TimeOfEffect", n_("Lasts:")), hours(m, true)),
    };
    let name_size = super::typography::Role::Name.size(17.0 * k);
    let names = super::typography::Role::Name.with(|| wrap(&s.name, room, name_size));
    let name_lh = super::typography::Role::Name.line_height(18.0 * k);
    let size = super::typography::Role::Body.size(12.0 * k);
    let gold = Color::from_rgba(235, 165, 75, 255);
    let effect_ink = if magic::targets_enemy(s) { Color::from_rgba(255, 255, 155, 255) } else { Color::from_rgba(75, 185, 255, 255) };
    let mut lines: Vec<(String, Color)> = wrap(&effect_summary(s), room, size).into_iter().map(|l| (l, effect_ink)).collect();
    lines.extend(wrap(&mana_line, room, size).into_iter().map(|l| (l, gold)));
    lines.extend(wrap(&last, room, size).into_iter().map(|l| (l, gold)));
    let line_h = super::typography::Role::Body.line_height(13.0 * k);
    // A long name wraps at the same font size, as the two-line title in the reference.
    let block = names.len() as f32 * name_lh + line_h * 0.5 + lines.len() as f32 * line_h;
    let top = r.y + ((r.h - block) / 2.0).max(3.0 * k);
    super::typography::Role::Name.with(|| {
        for (n, name) in names.iter().enumerate() {
            chrome::shadow_centered(name, tx, top + n as f32 * name_lh + name_size * 0.72, name_size, CREAM);
        }
    });
    let mut y = top + names.len() as f32 * name_lh + line_h * 0.5 + size * 0.72;
    for (line, color) in lines {
        if y > r.y + r.h - 2.0 * k {
            break;
        }
        chrome::shadow_centered(&line, tx, y, size, color);
        y += line_h;
    }
}
