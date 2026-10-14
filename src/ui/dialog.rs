//! Modal message windows in the original's style: a title bar, a parchment text box, a row
//! of resource icons and an OK button (video notes §5: the noon report, the victory window,
//! story and quest dialogs). A scenario question has Yes and No instead of OK.

use crate::ui::input::{is_mouse_button_down, is_mouse_button_pressed};
use macroquad::prelude::*;

use razdor::i18n::tr;
use razdor::rules::battle::Team;
use razdor::trf;
use razdor::rules::content::{ItemId, UnitId};
use razdor::rules::game::{BattleResult, DayReport, Game};

use super::assets::Assets;
use super::chrome;
use super::widgets::*;

const MARBLE_EDGE: Color = Color::new(0.38, 0.58, 0.46, 1.0);
pub const MANA: Color = Color::new(0.55, 0.72, 1.0, 1.0);
/// The captions of the event window's icon rows (the original's green font ae249c).
const CAPTION: Color = Color::new(0.45, 0.95, 0.45, 1.0);

/// A resource icon with its label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Gold,
    Mana,
    Income,
    Wages,
    Experience,
}

/// A picture beside the text: a unit's portrait, or an event's own image.
#[derive(Clone, Debug)]
pub enum Picture {
    Unit(UnitId),
    Image(Texture2D),
    /// The original's event picture (the record's +0xa7, 0x4a9d88 / 0x4d1679): at its own
    /// size left of the text box, framed by a 1 px line; the box moves right by its width
    /// plus 14 and is at least as tall as it, the text centred in it (the village offers).
    Side(Texture2D),
    /// The defeat or victory still (`Stnd-1`, `Stnd-2`): in the place of [`Picture::Side`],
    /// drawn with its own alpha and no frame line (0x4d1686 → 0x476a3c).
    Still(Texture2D),
}

/// How the player closed a dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Close {
    Ok,
    Yes,
    No,
}

#[derive(Clone, Debug)]
pub struct Dialog {
    pub title: String,
    pub text: Vec<String>,
    /// The event's own text, read as the original's markup (0x4a9d88 → 0x48e438): drawn
    /// instead of [`Dialog::text`], line by line with its fonts, centred or justified.
    pub marked: Option<String>,
    /// Resource icons with a caption such as "Gold + 30".
    pub resources: Vec<(Resource, String)>,
    pub items: Vec<ItemId>,
    /// Spells the event teaches (shown as their pictures under «Выучены новые заклинания:»).
    pub spells: Vec<u32>,
    /// The item or spell a press on its icon shows while the button is held (0x4c1fec,
    /// 0x4c2048): (a spell, its id, where the press was).
    pub preview: std::cell::Cell<Option<(bool, u32, Vec2)>>,
    /// An extra line in blue (system notices).
    pub notice: Option<String>,
    /// The scenario event this dialog shows, if any.
    pub event: Option<u16>,
    /// The event's yes/no question: the dialog has Yes and No buttons.
    pub question: bool,
    /// The question is a village's offer (0x4aca80): its answer goes to the village
    /// (`building_view::answer_offer`), not to the event engine.
    pub offer: bool,
    /// Both the village's offer and its result use the same scaled picture/text layout.
    pub village_event: bool,
    pub picture: Option<Picture>,
    /// Units that joined / left the army.
    pub joined: Vec<UnitId>,
    pub left: Vec<UnitId>,
    /// It is shown in the original's event window, which opens with a chord (a draw of the
    /// game's generator, 0x4d15d0): the events' dialogs, the victory box and the noon report.
    pub chord: bool,
    /// Its opening sound has played.
    pub cued: bool,
    /// Not shown before this clock time (seconds, `get_time`): the won battle's report comes
    /// 250 ms after the battle screen closes (the chained step 0x4af658).
    pub not_before: Option<f64>,
    /// Drawn in the original's defeat look (the event window's mode 2, 0x4a8ae8 / 0x4ed458):
    /// the red marble; its OK ends the game for the main menu (0x4c206c → 0x4bfb34).
    pub defeat: bool,
    /// The first text line shown when the text is taller than the screen allows (Razdor's
    /// scrolling: the wheel over the text, the arrow and page keys).
    pub scroll: std::cell::Cell<usize>,
    /// A key that closes it went down while it was shown: its release presses OK (0x4cd558).
    /// A key already held as it opened does nothing (0x47f358).
    pub key_down: std::cell::Cell<bool>,
}

impl Dialog {
    pub fn new(title: impl Into<String>) -> Dialog {
        Dialog {
            title: title.into(),
            text: Vec::new(),
            marked: None,
            resources: Vec::new(),
            items: Vec::new(),
            spells: Vec::new(),
            preview: std::cell::Cell::new(None),
            notice: None,
            event: None,
            question: false,
            offer: false,
            village_event: false,
            picture: None,
            joined: Vec::new(),
            left: Vec::new(),
            chord: false,
            cued: false,
            not_before: None,
            defeat: false,
            scroll: std::cell::Cell::new(0),
            key_down: std::cell::Cell::new(false),
        }
    }

    /// It still waits for its time ([`Dialog::not_before`]) at clock `now`.
    pub fn waiting(&self, now: f64) -> bool {
        self.not_before.is_some_and(|t| now < t)
    }

    /// Adds a line to the blue notice.
    pub fn add_notice(&mut self, line: &str) {
        self.notice = Some(match self.notice.take() {
            Some(n) => format!("{n}   {line}"),
            None => line.to_string(),
        });
    }

    /// The 12:00 report: gold, mana, income and wages (video notes §5).
    pub fn day_report(game: &Game, r: &DayReport) -> Dialog {
        let mut d = Dialog::new(tr("Report on resources, income and expenses"));
        // The original shows it in the event window, which opens with its chord.
        d.chord = true;
        d.text.push(tr("The report shows your gold, the daily income of your castles and the wages paid to your army.").into());
        d.resources = vec![
            (Resource::Gold, format!("{} = {}", tr("Gold"), r.gold)),
            (Resource::Mana, format!("{} = {}", tr("Mana"), r.mana_total)),
            (Resource::Income, trf!("Income + {n}", n = r.income)),
            (Resource::Wages, trf!("Wages - {n}", n = r.wages)),
        ];
        // The original warns when the gold and the income do not cover the wages (0x4a9b75).
        if r.gold + r.income < r.wages {
            d.add_notice(tr("Your gold and income do not cover the wages."));
        }
        if r.mana_wages > 0 {
            d.text.push(trf!("Mana today: - {paid} paid to elementals.", paid = r.mana_wages));
        }
        if r.unpaid > 0 {
            d.add_notice(&trf!("{n} unpaid units refuse to fight until they are paid.", n = r.unpaid));
        }
        if !r.deserted.is_empty() {
            let names: Vec<&str> = r.deserted.iter().map(|&u| game.content.unit(u).name.as_str()).collect();
            d.text.push(trf!("Left the army unpaid: {names}.", names = names.join(", ")));
        }
        d
    }

    /// The report after the army was lost (events.md §11, 0x4c527d / 0x4906eb set the defeat
    /// mark 0xae6d23, the chained step 0x4af658 opens the battle-report slot): the event window
    /// in its defeat look, titled `[Event] DefeatTitle`, the `Stnd-1` picture at its side
    /// (0x674958, loaded at 0x4dc871; 0x4aa1ac), and two centred texts: `[Event] Defeat` in
    /// the plain tint (font ae24a0), an empty row, `[Event] DefeatInfo` in the orange one
    /// (ae24a4; 0x4aa76c). The texts are drawn as typed, their marks taken out. OK goes to the
    /// main menu (0x4c206c with the defeat mark → 0x4bfb34).
    pub fn defeat() -> Dialog {
        let ini = |key: &str, ours: &'static str| {
            super::chrome::ui_text("Event", key).filter(|_| razdor::i18n::lang() == razdor::i18n::Lang::Ru).unwrap_or_else(|| tr(ours).to_string())
        };
        let plain = |s: String| s.chars().filter(|c| !matches!(c, '*' | '|' | '@' | '^')).collect::<String>();
        let mut d = Dialog::new(ini("DefeatTitle", razdor::i18n::n_("Defeat in battle!")));
        let lost = plain(ini("Defeat", razdor::i18n::n_("Your enemy proved stronger than your army, and you lost this battle.")));
        let info = plain(ini("DefeatInfo", razdor::i18n::n_("You let your hero perish, and the game on this scenario cannot go on.")));
        d.marked = Some(format!("^{lost}\r\n\r\n^@{info}"));
        d.picture = super::chrome::win_ugs("Stnd-1").map(Picture::Still);
        d.chord = true;
        d.defeat = true;
        d
    }

    /// The window after a won battle: gold and mana taken, a captured building, the loot.
    pub fn victory(game: &Game, result: &BattleResult) -> Option<Dialog> {
        let BattleResult::Victory { reward, mana, lost, loot, left_behind, level_ups, captured } = result else {
            return None;
        };
        let mut d = Dialog::new(tr("Victory over the enemy!"));
        d.chord = true;
        if let Some(l) = captured {
            let loc = &game.world.locations[*l];
            d.text.push(trf!("You have taken {place}. It pays you {gold} gold a day from now on.", place = loc.name, gold = loc.gold_income));
        }
        if *reward > 0 {
            d.text.push(tr("In this battle you won gold from the enemy.").into());
        }
        if *mana > 0 {
            d.text.push(tr("Grateful for your mercy, the surrendered troops pray for you.").into());
        }
        if *lost > 0 {
            d.text.push(trf!("{lost} of your units fell. Their bodies can be raised in a town or church.", lost));
        }
        for &(i, level) in level_ups {
            d.text.push(trf!("{name} reaches level {level}.", name = game.squad[i].name(&game.content), level));
        }
        if d.text.is_empty() {
            d.text.push(tr("The enemy is beaten.").into());
        }
        d.resources.push((Resource::Gold, trf!("Gold + {reward}", reward)));
        if *mana > 0 {
            d.resources.push((Resource::Mana, trf!("Mana + {mana}", mana)));
        }
        if captured.is_some() {
            d.resources.push((Resource::Income, trf!("Income + {n}", n = game.daily_income())));
        }
        d.items = loot.clone();
        if *left_behind > 0 {
            d.notice = Some(trf!("{left_behind} items were left behind: the pack is full.", left_behind));
        }
        Some(d)
    }
}

/// Small drawn icons for the resources (our own shapes).
pub fn resource_icon(r: Resource, cx: f32, cy: f32, s: f32) {
    // The original's pictures: coins, the magic book, the house, the paid knight.
    let art = match r {
        Resource::Gold => Some(0),
        Resource::Mana => Some(1),
        Resource::Income => Some(2),
        Resource::Wages => Some(3),
        Resource::Experience => None,
    };
    if let Some(n) = art {
        let name = if s <= 48.0 { format!("res{n}-44") } else { format!("res{n}") };
        if let Some(t) = super::chrome::win(&name) {
            super::chrome::tex(&t, Rect::new(cx - s / 2.0, cy - s / 2.0, s, s), WHITE);
            return;
        }
    }
    match r {
        Resource::Gold => {
            for k in 0..3 {
                let y = cy + s * 0.25 - k as f32 * s * 0.18;
                draw_ellipse(cx, y, s * 0.36, s * 0.14, 0.0, Color::new(0.85, 0.66, 0.2, 1.0));
                draw_ellipse_lines(cx, y, s * 0.36, s * 0.14, 0.0, 1.5, Color::new(0.5, 0.35, 0.1, 1.0));
            }
        }
        Resource::Mana => {
            let (t, b) = (vec2(cx, cy - s * 0.45), vec2(cx, cy + s * 0.45));
            let (l, r) = (vec2(cx - s * 0.28, cy), vec2(cx + s * 0.28, cy));
            draw_triangle(t, l, r, MANA);
            draw_triangle(b, l, r, Color::new(0.3, 0.5, 0.9, 1.0));
        }
        Resource::Income => {
            draw_rectangle(cx - s * 0.3, cy - s * 0.05, s * 0.6, s * 0.4, Color::new(0.8, 0.7, 0.55, 1.0));
            draw_triangle(vec2(cx, cy - s * 0.42), vec2(cx - s * 0.4, cy - s * 0.05), vec2(cx + s * 0.4, cy - s * 0.05), Color::new(0.75, 0.3, 0.2, 1.0));
        }
        Resource::Wages => {
            draw_circle(cx, cy - s * 0.2, s * 0.16, Color::new(0.75, 0.75, 0.8, 1.0));
            draw_rectangle(cx - s * 0.22, cy - s * 0.04, s * 0.44, s * 0.46, Color::new(0.6, 0.62, 0.7, 1.0));
        }
        Resource::Experience => {
            let star = Color::new(0.95, 0.85, 0.35, 1.0);
            let (r, k) = (s * 0.42, s * 0.18);
            for i in 0..5 {
                let a = std::f32::consts::TAU * i as f32 / 5.0 - std::f32::consts::FRAC_PI_2;
                let (b, c) = (a - 0.63, a + 0.63);
                draw_triangle(
                    vec2(cx + r * a.cos(), cy + r * a.sin()),
                    vec2(cx + k * b.cos(), cy + k * b.sin()),
                    vec2(cx + k * c.cos(), cy + k * c.sin()),
                    star,
                );
            }
            draw_circle(cx, cy, k, star);
        }
    }
}

/// A row of unit portraits with a caption. Returns its height.
fn unit_row(assets: &Assets, label: &str, units: &[UnitId], x: f32, y: f32) -> f32 {
    if units.is_empty() {
        return 0.0;
    }
    text(label, x + 30.0, y + 30.0, 18.0, INK);
    let lx = x + 40.0 + measure(label, 18.0).width;
    for (k, &u) in units.iter().enumerate().take(8) {
        assets.draw_unit(u, Team::Player, lx + 26.0 + k as f32 * 56.0, y + 26.0, 50.0);
    }
    60.0
}

/// Text lines that fit: all `total` when the window (`fixed` plus the text) fits in
/// `max_h`, else as many as the rest leaves, at least three.
fn visible_lines(total: usize, fixed: f32, max_h: f32) -> usize {
    let line_h = super::typography::Role::Name.line_height(23.0);
    let room = ((max_h - fixed - 24.0) / line_h).floor().max(3.0) as usize;
    total.min(room)
}

/// The first line shown of a text of `total` lines with `shown` in sight in box `tb`: the
/// wheel over the box, Up/Down, Page Up/Page Down, Home/End move it; the scrollbar on the
/// right too.
fn scrolled(d: &Dialog, total: usize, shown: usize, tb: Rect) -> usize {
    let last = total.saturating_sub(shown);
    let mut first = d.scroll.get().min(last);
    if last > 0 && !input_blocked() {
        let wh = if tb.contains(pointer().into()) { wheel() } else { 0.0 };
        let page = shown.saturating_sub(1).max(1);
        if wh < 0.0 || key(KeyCode::Down) {
            first = (first + 1).min(last);
        } else if wh > 0.0 || key(KeyCode::Up) {
            first = first.saturating_sub(1);
        } else if key(KeyCode::PageDown) {
            first = (first + page).min(last);
        } else if key(KeyCode::PageUp) {
            first = first.saturating_sub(page);
        } else if key(KeyCode::Home) {
            first = 0;
        } else if key(KeyCode::End) {
            first = last;
        }
    }
    // Razdor's scrollbar (a touch screen has neither wheel nor keys).
    scrollbar(Rect::new(tb.x + tb.w - 16.0, tb.y + 6.0, 12.0, tb.h - 12.0), &mut first, shown, total);
    d.scroll.set(first);
    first
}

/// Draws `d` centred on the screen; returns how it was closed: OK (or Enter, Escape), or for
/// a question Yes or No (Esc or N No, any other key Yes: `answer_key`).
pub fn draw(d: &Dialog, assets: &Assets, game: Option<&Game>) -> Option<Close> {
    super::dt_font::with_legacy_metrics(|| {
        super::dt_font::with_original_face(super::typography::Role::Name.face(), || draw_content(d, assets, game))
    })
}

fn draw_content(d: &Dialog, assets: &Assets, game: Option<&Game>) -> Option<Close> {
    // Dialog text uses the original fixed baseline spacing; shared font metrics made these
    // windows visibly cramped and also reduced their button labels.
    let line_h = 23.0;
    if d.village_event {
        return draw_village_event(d, assets, game);
    }
    let (sw, sh) = (screen_width(), screen_height());
    chrome::under_message();
    // Every dialog picture sits left of the text box, with a 14 px gap.
    let side = match &d.picture {
        Some(Picture::Unit(_)) => Some((128.0, 128.0)),
        Some(Picture::Image(t)) => {
            let scale = (128.0 / t.height()).min(220.0 / t.width());
            Some((t.width() * scale, t.height() * scale))
        }
        Some(Picture::Side(t)) => Some((t.width(), t.height())),
        Some(Picture::Still(t)) => Some((t.width(), t.height())),
        _ => None,
    };
    let side_w = side.map_or(0.0, |(pw, _)| pw + 14.0);
    // A long story text widens the window rather than running off the screen.
    let fit = |w: f32| -> (f32, Vec<String>, Vec<MarkupRow>) {
        match &d.marked {
            Some(t) => (w, Vec::new(), markup_rows(t, w - 80.0 - side_w, 19.0)),
            None => (w, d.text.iter().flat_map(|t| wrap(t, w - 80.0 - side_w, 19.0)).collect(), Vec::new()),
        }
    };
    // Event dialogs, including unit meetings without a portrait, are 634 px wide in the
    // original (interface.md §11); scale that reference width with the UI.
    let preferred_w: f32 = 634.0 * chrome::k();
    let (mut w, mut lines, mut rows) = fit(preferred_w.min(sw - 20.0));
    if (lines.len() + rows.len()) as f32 * line_h > sh * 0.45 {
        (w, lines, rows) = fit((980.0 * chrome::k()).min(sw - 20.0));
    }
    let total = lines.len() + rows.len();
    let mut text_h = total as f32 * line_h + 24.0;
    let res_h = if d.resources.is_empty() { 0.0 } else { 104.0 };
    // An icon row grows the window by its caption and icons (0x4a8e66: 87 for items, 82
    // for spells).
    let items_h = if d.items.is_empty() { 0.0 } else { 87.0 };
    let spells_h = if d.spells.is_empty() { 0.0 } else { 82.0 };
    // Reserve a little extra vertical space so the blue journal notice has breathing room
    // between the event text and the lower frame instead of touching its outline.
    let notice_h = if d.notice.is_some() { 36.0 } else { 0.0 };
    let units_h = [&d.joined, &d.left].iter().filter(|u| !u.is_empty()).count() as f32 * 60.0;
    let title_h = (26.0 * chrome::k()).round();
    let fixed = title_h + 16.0 + res_h + items_h + spells_h + units_h + notice_h + 64.0;
    // Too tall for the screen: the text scrolls inside a shorter box, so the buttons stay
    // in sight.
    let shown = visible_lines(total, fixed, sh - 20.0);
    text_h = text_h.min(shown as f32 * line_h + 24.0);
    // The box is at least as tall as the side picture and its frame, the text centred in it
    // (0x4aa88a).
    let text_top = side.map_or(0.0, |(_, ph)| ((ph + 2.0 - text_h) / 2.0).max(0.0));
    if let Some((_, ph)) = side {
        text_h = text_h.max(ph + 2.0);
    }
    let h = fixed + text_h;
    let (x, y) = ((sw - w) / 2.0, ((sh - h) / 2.0).max(10.0));
    // The defeat look is the red marble (the original's window after a lost battle).
    let skin = if d.defeat { chrome::Skin::Red } else { chrome::Skin::Marble };
    chrome::window(Rect::new(x, y, w, h), &d.title, skin, false);
    chrome::silver_frame(Rect::new(x + 3.0, y + title_h + 3.0, w - 6.0, h - title_h - 66.0), 1.0);
    chrome::silver_frame(Rect::new(x + 2.0, y + h - 60.0, w - 4.0, 58.0), 1.0);
    let mut cy = y + title_h + 16.0;
    match &d.picture {
        Some(Picture::Unit(u)) => {
            let r = Rect::new(x + 17.0, cy + 1.0, 128.0, 128.0);
            assets.draw_portrait(*u, Team::Player, r);
            chrome::silver_frame(r, 1.5);
        }
        Some(Picture::Image(tex)) => {
            let (tw, th) = side.expect("picture dimensions");
            let r = Rect::new(x + 17.0, cy + 1.0, tw, th);
            draw_texture_ex(tex, r.x, r.y, WHITE, DrawTextureParams { dest_size: Some(vec2(tw, th)), ..Default::default() });
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, MARBLE_EDGE);
        }
        Some(Picture::Still(tex)) => draw_texture(tex, x + 17.0, cy + 1.0, WHITE),
        Some(Picture::Side(tex)) => {
            let (px, py) = (x + 17.0, cy + 1.0);
            draw_texture(tex, px, py, WHITE);
            draw_rectangle_lines(px - 1.0, py - 1.0, tex.width() + 2.0, tex.height() + 2.0, 1.0, WHITE);
        }
        None => {}
    }
    let tb = Rect::new(x + 16.0 + side_w, cy, w - 32.0 - side_w, text_h);
    chrome::text_box(tb);
    let first = scrolled(d, total, shown, tb);
    let ty = cy + text_top;
    for (i, line) in lines.iter().enumerate().skip(first).take(shown) {
        chrome::shadow_centered(line, tb.x + tb.w / 2.0, ty + 30.0 + (i - first) as f32 * line_h, 19.0, Color::new(1.0, 0.9, 0.66, 1.0));
    }
    for (i, row) in rows.iter().enumerate().skip(first).take(shown) {
        draw_markup_row(row, tb.x + 24.0, ty + 30.0 + (i - first) as f32 * line_h, tb.w - 48.0, 19.0);
    }
    cy += text_h + 10.0;
    if !d.resources.is_empty() {
        let n = d.resources.len() as f32;
        let step = (w - 60.0) / n;
        for (k, (r, label)) in d.resources.iter().enumerate() {
            let cx = x + 30.0 + step * (k as f32 + 0.5);
            let c = if *r == Resource::Mana { MANA } else { Color::new(0.7, 0.92, 0.75, 1.0) };
            chrome::shadow_centered(label, cx, cy + 18.0, 18.0, c);
            resource_icon(*r, cx, cy + 62.0, 70.0);
        }
        cy += res_h;
    }
    // The original stacks its rows from the bottom up (0x4a8ae8): the items gained lowest,
    // the units above them, the spells above those; each row its green caption centred over
    // the window and its icons spread by their count, centre i at W·(i+1)/(n+1).
    let spread = |n: usize, i: usize| x + w * (i + 1) as f32 / (n + 1) as f32;
    let caption = |key: &str, ours: &'static str, cy: f32| {
        let t = chrome::ui_text("Event", key).filter(|_| razdor::i18n::lang() == razdor::i18n::Lang::Ru).unwrap_or_else(|| tr(ours).to_string());
        chrome::shadow_centered(&t, x + w / 2.0, cy + 18.0, 18.0, CAPTION);
    };
    let mut icons: Vec<(bool, u32, Rect)> = Vec::new();
    if !d.spells.is_empty() {
        caption("NewSpell", razdor::i18n::n_("New spells learnt:"), cy);
        let n = d.spells.len().min(4);
        for (i, &s) in d.spells.iter().enumerate().take(4) {
            let r = Rect::new(spread(n, i) - 25.0, cy + 26.0, 50.0, 50.0);
            if let Some(def) = game.and_then(|g| g.content.spells.iter().find(|x| x.id == s)) {
                chrome::spell_icon(def, r);
                chrome::silver_frame(r, 1.0);
            }
            icons.push((true, s, r));
        }
        cy += spells_h;
    }
    cy += unit_row(assets, tr("Left the army:"), &d.left, x, cy);
    cy += unit_row(assets, tr("Joined the army:"), &d.joined, x, cy);
    if !d.items.is_empty() {
        caption("AddItem", razdor::i18n::n_("Items received:"), cy);
        let n = d.items.len().min(8);
        for (i, &item) in d.items.iter().enumerate().take(8) {
            let r = Rect::new(spread(n, i) - 27.0, cy + 26.0, 55.0, 55.0);
            assets.draw_item(item, r.x, r.y, r.w);
            icons.push((false, item.0, r));
        }
        cy += items_h;
    }
    if let Some(n) = &d.notice {
        chrome::shadow_centered(n, x + w / 2.0, cy + 18.0, 18.0, Color::new(0.3, 0.72, 1.0, 1.0));
    }
    let by = y + h - 52.0;
    // A left or right press on an icon shows its item or spell centred on the press, kept
    // inside the window, until the button is let go (0x4c1fec / 0x4c2048, the frame handler
    // 0x4cd558); the window's buttons wait meanwhile.
    let held = is_mouse_button_down(MouseButton::Left) || is_mouse_button_down(MouseButton::Right);
    if d.preview.get().is_some() && !held {
        d.preview.set(None);
    } else if d.preview.get().is_none() && (is_mouse_button_pressed(MouseButton::Left) || is_mouse_button_pressed(MouseButton::Right)) {
        let p: Vec2 = pointer().into();
        if let Some(&(spell, id, _)) = icons.iter().find(|(_, _, r)| r.contains(p)) {
            d.preview.set(Some((spell, id, p)));
        }
    }
    if let (Some((spell, id, at)), Some(game)) = (d.preview.get(), game) {
        let (pw, ph) = if spell { (286.0, 112.0) } else { (272.0, 230.0) };
        let px = (at.x - pw / 2.0).clamp(x + 10.0, (x + w - 10.0 - pw).max(x + 10.0));
        let py = (at.y - ph / 2.0).clamp(y + 10.0, (y + h - 10.0 - ph).max(y + 10.0));
        draw_rectangle(px + 8.0, py + 8.0, pw, ph, Color::new(0.0, 0.0, 0.0, 0.45));
        if spell {
            if let Some(s) = game.content.spells.iter().find(|s| s.id == id) {
                super::spellbook::spell_card(game, s, Rect::new(px, py, pw, ph));
            }
        } else {
            super::building_view::item_description(game, assets, ItemId(id), px, py, pw, ph);
        }
        return None;
    }
    if d.question {
        let answer = answer_key();
        let yes = button(x + w / 2.0 - 140.0, by, 120.0, 38.0, tr("Yes"), true) || answer == Some(true);
        let no = button(x + w / 2.0 + 20.0, by, 120.0, 38.0, tr("No"), true) || answer == Some(false);
        return if yes {
            Some(Close::Yes)
        } else if no {
            Some(Close::No)
        } else {
            None
        };
    }
    // The label is the language file's `[Buttons] Ok` («Ок»); in the defeat look the button
    // is the red one (0x4aa263).
    let ok_label = chrome::ui_text("Buttons", "Ok").filter(|_| razdor::i18n::lang() == razdor::i18n::Lang::Ru).unwrap_or_else(|| "OK".to_string());
    let ok = if d.defeat {
        let r = Rect::new(x + w / 2.0 - 60.0, by, 120.0, 38.0);
        let hover = mouse_in(r.x, r.y, r.w, r.h);
        chrome::defeat_button(r, &ok_label, true, hover);
        let pressed = hover && clicked();
        if pressed {
            super::audio::cue(super::audio::Cue::Button);
        }
        pressed
    } else {
        button(x + w / 2.0 - 60.0, by, 120.0, 38.0, &ok_label, true)
    };
    // Any key but Tab, Alt and the Up and Down arrows, on its release (0x4cd558): Space,
    // Enter and Esc alike. Closed on the release, the key is no longer held when the map
    // takes over, so the hero walks on.
    if super::widgets::ok_key_pressed() {
        d.key_down.set(true);
    }
    (ok || (d.key_down.get() && super::widgets::ok_key_released())).then_some(Close::Ok)
}

/// Village offers and results share the original's 634 px wide event window.
/// The illustration stays beside the text, with rewards below and a framed footer.
fn draw_village_event(d: &Dialog, assets: &Assets, game: Option<&Game>) -> Option<Close> {
    chrome::under_message();
    let o = (chrome::k() * 0.9375).min((screen_width() - 12.0) / 634.0);
    let w = 634.0 * o;
    let size = 17.0 * o;
    let line_h = 18.0 * o;
    let picture = match &d.picture { Some(Picture::Side(t)) => Some(t), _ => None };
    let pw = picture.map_or(106.0, |t| t.width()) * o;
    let ph = picture.map_or(210.0, |t| t.height()) * o;
    let text_w = w - pw - 50.0 * o;
    let rows = markup_rows(d.marked.as_deref().unwrap_or(""), text_w - 40.0 * o, size);
    let rewards = !d.items.is_empty() || !d.resources.is_empty();
    let reward_h = if rewards { 110.0 * o } else { 0.0 };
    let fixed = 114.0 * o + reward_h;
    let available = (screen_height() - chrome::bar_height() - fixed - 8.0).max(60.0 * o);
    let panel_h = ph.max(rows.len() as f32 * line_h + 36.0 * o).min(available);
    let shown = ((panel_h - 36.0 * o) / line_h).floor().max(1.0) as usize;
    let h = fixed + panel_h;
    let x = ((screen_width() - w) / 2.0).round();
    let y = ((screen_height() - chrome::bar_height() - h) / 2.0).max(2.0).round();
    chrome::window(Rect::new(x, y, w, h), &d.title, chrome::Skin::Marble, false);
    let body = Rect::new(x + 4.0 * o, y + 31.0 * o, w - 8.0 * o, h - 89.0 * o);
    let footer = Rect::new(x + 2.0 * o, y + h - 55.0 * o, w - 4.0 * o, 53.0 * o);
    chrome::silver_frame(body, o.max(1.0));
    chrome::silver_frame(footer, o.max(1.0));
    let top = y + 45.0 * o;
    if let Some(t) = picture {
        let r = Rect::new(x + 18.0 * o, top, pw, ph.min(panel_h));
        chrome::tex(t, r, WHITE);
        chrome::silver_frame(r, o.max(1.0));
    }
    let tb = Rect::new(x + 32.0 * o + pw, top, text_w, panel_h);
    chrome::text_box(tb);
    let first = scrolled(d, rows.len(), shown, tb);
    let count = rows.len().saturating_sub(first).min(shown);
    let text_top = tb.y + (tb.h - count as f32 * line_h) / 2.0;
    let room = tb.w - 40.0 * o - if rows.len() > shown { 16.0 * o } else { 0.0 };
    for (i, row) in rows.iter().skip(first).take(shown).enumerate() {
        draw_markup_row(row, tb.x + 20.0 * o, text_top + i as f32 * line_h + size, room, size);
    }
    let reward_y = top + panel_h + 14.0 * o;
    if !d.items.is_empty() {
        let caption = chrome::ui_text("Event", "AddItem").filter(|_| razdor::i18n::lang() == razdor::i18n::Lang::Ru)
            .unwrap_or_else(|| tr("Items received:").to_string());
        chrome::shadow_centered(&caption, x + w / 2.0, reward_y + 15.0 * o, size, CAPTION);
        for (i, &item) in d.items.iter().enumerate() {
            let cx = x + w * (i + 1) as f32 / (d.items.len() + 1) as f32;
            let r = Rect::new(cx - 25.0 * o, reward_y + 28.0 * o, 50.0 * o, 50.0 * o);
            assets.draw_item(item, r.x, r.y, r.w);
            if r.contains(pointer().into()) && (is_mouse_button_down(MouseButton::Left) || is_mouse_button_down(MouseButton::Right)) {
                if let Some(game) = game {
                    super::building_view::item_description(game, assets, item, x + (w - 272.0 * o) / 2.0, y, 272.0 * o, 230.0 * o);
                    return None;
                }
            }
        }
    }
    for (i, (resource, label)) in d.resources.iter().enumerate() {
        let cx = x + w * (i + 1) as f32 / (d.resources.len() + 1) as f32;
        chrome::shadow_centered(label, cx, reward_y + 15.0 * o, size, MANA);
        resource_icon(*resource, cx, reward_y + 56.0 * o, 65.0 * o);
    }
    let by = footer.y + 9.0 * o;
    let (bw, bh) = (96.0 * o, 35.0 * o);
    if d.question {
        let answer = answer_key();
        let yes = button(x + w * 0.24 - bw / 2.0, by, bw, bh, tr("Yes"), true);
        let no = button(x + w * 0.76 - bw / 2.0, by, bw, bh, tr("No"), true);
        return if yes || answer == Some(true) { Some(Close::Yes) } else if no || answer == Some(false) { Some(Close::No) } else { None };
    }
    let ok = button(x + (w - bw) / 2.0, by, bw, bh, tr("OK"), true);
    if super::widgets::ok_key_pressed() { d.key_down.set(true); }
    (ok || (d.key_down.get() && super::widgets::ok_key_released())).then_some(Close::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lost battle's report (0x4aa76c): the defeat look, no event behind it (so its OK
    /// ends the game), the defeat text centred in the plain tint, an empty row, the info
    /// centred in the orange one.
    #[test]
    fn the_defeat_report_is_the_event_window_in_its_defeat_look() {
        use razdor::dt::markup::{parse, Align, Ink};
        let d = Dialog::defeat();
        assert!(d.defeat && d.event.is_none() && !d.question && d.chord);
        let lines = parse(d.marked.as_deref().unwrap());
        assert_eq!(lines.len(), 3);
        assert_eq!((lines[0].ink, lines[0].align), (Ink::Plain, Align::Centre));
        assert!(lines[1].text.trim_matches('_').is_empty());
        assert_eq!((lines[2].ink, lines[2].align), (Ink::At, Align::Centre));
    }

    /// The won battle's report waits its 250 ms after the screen closes (0x4af658), then shows.
    #[test]
    fn a_text_taller_than_the_screen_scrolls_in_a_shorter_box() {
        // 10 lines and 200 px of the rest fit a 700 px screen; 60 lines do not.
        assert_eq!(visible_lines(10, 200.0, 700.0), 10);
        assert_eq!(visible_lines(60, 200.0, 700.0), 20, "(700 − 200 − 24) / 23");
        assert_eq!(visible_lines(60, 200.0, 250.0), 3, "at least three lines");
    }

    #[test]
    fn a_dialog_with_a_time_waits_for_it() {
        let gap = razdor::av::BATTLE_REPORT_GAP_MS as f64 / 1000.0;
        assert_eq!(gap, 0.25);
        let d = Dialog { not_before: Some(10.0 + gap), ..Dialog::new("report") };
        assert!(d.waiting(10.0));
        assert!(d.waiting(10.2));
        assert!(!d.waiting(10.25));
        assert!(!Dialog::new("now").waiting(0.0));
    }
}
