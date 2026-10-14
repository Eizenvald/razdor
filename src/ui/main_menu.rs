//! The original's main menu (the 18:08 frame of the gameplay video): the burning ruins
//! (`Windows/Castle.lit`) over the whole screen, the silver "Времена Раздора" logo
//! (`Title_RUS.ugs`) with flames rising off its letters (`TitleFlame_RUS.lit`), and five oval
//! buttons (`mb2.ugs`, the last one pointed) whose marble middles swirl in violet
//! (`MenuFlame.lit`) under the words of `SMText_RUS.lit`; the button under the mouse turns
//! gold. Razdor's own extras, the map editor, the custom battle and the interface language,
//! sit small in the bottom corners. Layout numbers are pixels of the 960×720 video, scaled by [`chrome::k`].

use crate::ui::input::{is_mouse_button_down};
use macroquad::prelude::*;

use razdor::i18n::{self, n_, tr, Lang};

use super::audio::{cue, Cue};
use super::chrome::{self, Fx};
use super::widgets::*;

/// What the player picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    NewGame,
    Load,
    Options,
    Authors,
    Exit,
    Editor,
    /// Razdor's custom battle (`ui::custom_battle`).
    Custom,
}

/// The five buttons, top to bottom, with the row of `SMText_RUS` holding their word.
const BUTTONS: [(Pick, &str); 5] = [
    (Pick::NewGame, n_("New game")),
    (Pick::Load, n_("Load")),
    (Pick::Options, n_("Options")),
    (Pick::Authors, n_("Authors")),
    (Pick::Exit, n_("Exit")),
];

/// Centre of button `i` (video pixels, x from the screen centre).
fn button_centre(i: usize) -> Vec2 {
    vec2(0.0, 232.0 + i as f32 * 92.0)
}

/// Size of an oval button (video pixels): `mb2.ugs` drawn 1:1 at 1024×768.
const OVAL: Vec2 = Vec2::new(214.0, 112.0);

// Final title-flame placement from the temporary development tuner.
const LOGO_FLAME_X: f32 = -16.0;
const LOGO_FLAME_Y: f32 = 12.0;
const LOGO_FLAME_WIDTH: f32 = 0.95;
const LOGO_FLAME_HEIGHT: f32 = 1.52;
const LOGO_FLAME_GLOW: f32 = 0.85;

fn video(p: Vec2) -> Vec2 {
    let k = chrome::k();
    vec2(screen_width() / 2.0 + p.x * k, p.y * k)
}

/// The burning ruins over the whole screen, a little warmer where the fire glows.
fn background() {
    match chrome::win("Castle") {
        Some(t) => chrome::tex(&t, Rect::new(0.0, 0.0, screen_width(), screen_height()), WHITE),
        None => clear_background(Color::from_rgba(60, 30, 12, 255)),
    }
}

/// The current and next frame, and the blend between them, for a looping animation.
fn flame_phase(t: f64, fps: f64, frames: usize) -> (usize, usize, f32) {
    let phase = (t * fps).rem_euclid(frames as f64);
    let current = phase.floor() as usize;
    (current, (current + 1) % frames, phase.fract() as f32)
}

/// One cell of the 4×4 sheet of 128×128 flame frames.
fn flame_frame(frame: usize) -> Rect {
    Rect::new((frame % 4) as f32 * 128.0, (frame / 4) as f32 * 128.0, 128.0, 128.0)
}

/// Cross-fades consecutive frames so the additive flame grows and dies away smoothly.
fn draw_flame_frames(
    texture: &Texture2D,
    t: f64,
    fps: f64,
    frames: usize,
    source: impl Fn(usize) -> Rect,
    dest: Rect,
    tint: Color,
) {
    let (current, next, blend) = flame_phase(t, fps, frames);
    chrome::additive(|| {
        let mut current_tint = tint;
        current_tint.a *= 1.0 - blend;
        chrome::tex_src(texture, source(current), dest, current_tint);
        if blend > 0.0 {
            let mut next_tint = tint;
            next_tint.a *= blend;
            chrome::tex_src(texture, source(next), dest, next_tint);
        }
    });
}

/// The logo, with the flames of `TitleFlame_RUS` (9 frames of the two words, one above the
/// other) licking up behind its letters.
fn logo() {
    let k = chrome::k();
    let w = 840.0 * k;
    let h = w * 128.0 / 896.0;
    // The letters sit left in the picture: the video shows them centred.
    let x = screen_width() / 2.0 - w / 2.0 + 25.0 * k;
    let y = 50.0 * k;
    if let Some(fl) = chrome::win_fx("TitleFlame_RUS", Fx::Glow) {
        let fh = fl.height() / 9.0;
        let fw = fl.width() / 2.0;
        // Keep the flame frame's aspect ratio and centre it over the visible lettering.
        // The source frame follows the word's silhouette; centring lets it fill the whole
        // text shape while the silver logo drawn below stays legible on top.
        let base_w = w / 2.0;
        let base_h = base_w * fh / fw;
        let base_y = y + (h - base_h) / 2.0;
        // Let the glow extend past the logo's sides and top while keeping the bottom
        // aligned with the letter flames in the source art.
        let flame_w = base_w * LOGO_FLAME_WIDTH;
        let flame_h = base_h * LOGO_FLAME_HEIGHT;
        let flame_y = base_y + base_h - flame_h + LOGO_FLAME_Y * k;
        let t = get_time();
        // The two words, each half of the sheet, over each half of the logo.
        for half in 0..2 {
            let dst_x = x + half as f32 * base_w - (flame_w - base_w) / 2.0 + LOGO_FLAME_X * k;
            let dst = Rect::new(dst_x, flame_y, flame_w, flame_h);
            draw_flame_frames(
                &fl,
                t,
                10.0,
                9,
                |frame| {
                    Rect::new(half as f32 * fw, frame as f32 * fh, fw, fh)
                },
                dst,
                Color::new(1.0, 0.55, 0.2, LOGO_FLAME_GLOW),
            );
        }
    }
    match chrome::win_ugs("Title_RUS") {
        Some(t) => chrome::tex(&t, Rect::new(x, y, w, h), WHITE),
        None => text_centered(tr("A time of discord"), screen_width() / 2.0, y + h * 0.7, 64.0 * k, chrome::SILVER),
    }
}

thread_local! {
    /// The main menu item under the pointer at the last frame.
    static HOVERED: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// An oval button: the violet swirl, the frame (gold under the mouse) and its word. Returns
/// true when clicked.
fn oval(i: usize, label: &str, hot: Option<usize>) -> bool {
    let k = chrome::k();
    let c = video(button_centre(i));
    let size = OVAL * k;
    let r = oval_rect(i);
    let (hover, _down, pressed) = super::widgets::button_state(r, hot == Some(i));
    // The bell as the pointer comes onto an item (interface.md §4, 0x4b9b3c); leaving clears
    // the light.
    HOVERED.with(|h| {
        if hover && h.get() != Some(i) {
            h.set(Some(i));
            cue(Cue::MenuSelect);
        } else if !hover && h.get() == Some(i) {
            h.set(None);
        }
    });
    // Fire behind the button (the glow around the column of buttons in the video).
    if let Some(fl) = chrome::win_fx("MenuFlame", Fx::Glow) {
        let f = Rect::new(c.x - 190.0 * k, c.y - 130.0 * k, 380.0 * k, 260.0 * k);
        draw_flame_frames(
            &fl,
            // The previous per-button offset jumped almost four frames at a time;
            // keep a small ordered phase step down the menu instead.
            get_time() * 0.8 + i as f64 * 0.10,
            6.0,
            16,
            flame_frame,
            f,
            Color::new(1.0, 0.55, 0.2, 0.5),
        );
    }
    // `mb2.ugs`: the round oval on top, the pointed one (the last button) below.
    if let Some(t) = chrome::win_ugs("mb2") {
        let pointed = i + 1 == BUTTONS.len();
        let half = t.height() / 2.0;
        let src = Rect::new(0.0, if pointed { half } else { 0.0 }, t.width(), half);
        let frame = if hover { Color::new(1.0, 0.80, 0.42, 1.0) } else { WHITE };
        let s = k * 0.9375;
        // Each oval's middle is 46 px below the top of its half; the pointed one is drawn
        // in the left 196 px of the picture.
        let dx = if pointed { 16.0 } else { 0.0 };
        let dst = Rect::new(c.x - (t.width() / 2.0 - dx) * s, c.y - 46.0 * s, t.width() * s, half * s);
        chrome::tex_src(&t, src, dst, frame);
    } else {
        draw_ellipse_lines(c.x, c.y, size.x / 2.0, size.y / 2.0, 0.0, 3.0, if hover { chrome::GOLD } else { chrome::SILVER });
    }
    // The marble middle glows violet (warm under the mouse), a flame swirling in it.
    let pointed = i + 1 == BUTTONS.len();
    let (iw, ih) = if pointed { (0.54, 0.42) } else { (0.62, 0.46) };
    let inner = Rect::new(c.x - r.w * iw / 2.0, c.y - r.h * ih / 2.0, r.w * iw, r.h * ih);
    let wash = if hover { Color::new(0.75, 0.40, 0.45, 0.40) } else { Color::new(0.30, 0.26, 0.90, 0.42) };
    draw_ellipse(inner.center().x, inner.center().y, inner.w / 2.0, inner.h / 2.0, 0.0, wash);
    if let Some(fl) = chrome::win_fx("MenuFlame", Fx::Glow) {
        let tint = if hover { Color::new(1.0, 0.8, 0.7, 0.5) } else { Color::new(0.8, 0.65, 1.0, 0.5) };
        draw_flame_frames(
            &fl,
            get_time() + i as f64 * 0.37,
            12.0,
            16,
            flame_frame,
            inner,
            tint,
        );
    }
    word(i, label, c, hover);
    if pressed {
        cue(Cue::MenuPress);
        return true;
    }
    false
}

/// The button's word: its row of `SMText_RUS` in Russian, else the text.
fn word(i: usize, label: &str, c: Vec2, hover: bool) {
    let k = chrome::k();
    let color = if hover { Color::new(1.0, 0.93, 0.80, 1.0) } else { Color::new(0.93, 0.90, 1.0, 1.0) };
    let sheet = (i18n::lang() == Lang::Ru).then(|| chrome::win_fx("SMText_RUS", Fx::Glow)).flatten();
    match sheet {
        Some(t) => {
            let rh = t.height() / BUTTONS.len() as f32;
            let src = Rect::new(0.0, i as f32 * rh, t.width(), rh);
            let (w, h) = (t.width() * 1.05 * k, rh * 1.05 * k);
            let dst = Rect::new(c.x - w / 2.0, c.y - h / 2.0, w, h);
            // A dark rim, then the word.
            for (dx, dy) in [(-1.5, 0.0), (1.5, 0.0), (0.0, -1.5), (0.0, 1.5)] {
                chrome::tex_src(&t, src, Rect::new(dst.x + dx * k, dst.y + dy * k, dst.w, dst.h), Color::new(0.12, 0.05, 0.25, 0.8));
            }
            chrome::tex_src(&t, src, dst, color);
        }
        None => chrome::strong_centered(label, c.x, c.y + 8.0 * k, 24.0 * k, color),
    }
}

/// A small text link (the Razdor extras). Returns true when clicked.
fn link(label: &str, x: f32, y: f32, right: bool) -> bool {
    let size = 18.0 * chrome::k();
    let w = measure(label, size).width;
    let x = if right { x - w } else { x };
    let (_, down, pressed) = super::widgets::button_state(Rect::new(x, y - size, w, size * 1.3), true);
    let color = if down { chrome::GOLD } else { Color::new(0.95, 0.88, 0.72, 0.85) };
    chrome::shadow_text(label, x, y, size, color);
    if pressed {
        cue(Cue::Button);
        return true;
    }
    false
}

/// The menu's ruins and logo without the buttons (behind the load and new-game windows).
pub fn backdrop() {
    background();
    logo();
}

/// The screen rectangle of oval button `i`.
fn oval_rect(i: usize) -> Rect {
    let k = chrome::k();
    let c = video(button_centre(i));
    let size = OVAL * k;
    Rect::new(c.x - size.x / 2.0, c.y - size.y / 2.0, size.x, size.y)
}

/// The one button under the pointer: neighbouring ovals overlap by 20 px, and the original's
/// hit test (0x4743c8) reports a single widget, the first in order. Without this the pointer in
/// an overlap counted as on two buttons, which took the light from each other every frame and
/// rang the bell each time.
fn hot_button(pointer: Vec2, n: usize) -> Option<usize> {
    (0..n).find(|&i| oval_rect(i).contains(pointer))
}

/// Draws the menu; returns what was picked.
pub fn frame() -> Option<Pick> {
    backdrop();
    let mut pick = None;
    let hot = if input_blocked() { None } else { hot_button(crate::ui::widgets::pointer().into(), BUTTONS.len()) };
    for (i, (p, label)) in BUTTONS.iter().enumerate() {
        if oval(i, tr(label), hot) {
            pick = Some(*p);
        }
    }
    let k = chrome::k();
    let y = screen_height() - 16.0 * k;
    let editor = tr("Map editor");
    if link(editor, 16.0 * k, y, false) {
        pick = Some(Pick::Editor);
    }
    let x = 16.0 * k + measure(editor, 18.0 * k).width + 28.0 * k;
    if link(tr("Custom battle"), x, y, false) {
        pick = Some(Pick::Custom);
    }
    let lang = format!("{} / {}", Lang::En.label(), Lang::Ru.label());
    if link(&lang, screen_width() - 16.0 * k, y, true) {
        super::language::toggle();
    }
    // This program's version, small in the corner above the language switch.
    let version = concat!("v", env!("CARGO_PKG_VERSION"));
    let vw = measure(version, 12.0 * k).width;
    chrome::shadow_text(version, screen_width() - 16.0 * k - vw, y - 26.0 * k, 12.0 * k, Color::new(0.95, 0.88, 0.72, 0.6));
    // An update put in place says so (an "Always" update has no window of its own).
    super::update_view::title_line();
    // Esc quits the game at once, without a question, as in the original (0x4c8059); a
    // gamepad's Start, which is Esc elsewhere, does not (Razdor's: pressed to open a menu,
    // it would end the game).
    if key(KeyCode::Escape) && !crate::ui::input::pad_key_pressed(KeyCode::Escape) {
        pick = Some(Pick::Exit);
    }
    pick
}

/// The credits picture of the install: its dark outline (`Credits_Alpha` as the alpha of
/// black), its letters (`Credits_Color`, drawn additively), and the row of its first letters
/// (it starts with a blank stretch).
#[derive(Clone)]
struct Credits {
    shadow: Texture2D,
    letters: Texture2D,
    first_row: f32,
}

thread_local! {
    /// The credits, made once.
    static CREDITS: std::cell::OnceCell<Option<Credits>> = const { std::cell::OnceCell::new() };
    /// The credits window the player scrolled (when it opened, how far from the timed scroll
    /// in the picture's pixels).
    static CREDITS_NUDGE: std::cell::Cell<(f64, f32)> = const { std::cell::Cell::new((f64::NAN, 0.0)) };
}

/// Razdor's own lines over the original's credits: their height in the picture's pixels.
const OWN_CREDITS_H: f32 = 96.0;

fn credits() -> Option<Credits> {
    CREDITS.with(|c| {
        c.get_or_init(|| {
            let mut color = chrome::image("Windows/Credits_Color.lit")?;
            let alpha = chrome::image("Windows/Credits_Alpha.lit")?;
            if (color.width, color.height) != (alpha.width, alpha.height) {
                return None;
            }
            let (w, h) = (u16::try_from(color.width).ok()?, u16::try_from(color.height).ok()?);
            let shadow: Vec<u8> = alpha.rgba.chunks_exact(4).flat_map(|a| [0, 0, 0, a[0].max(a[1]).max(a[2])]).collect();
            color.rgba.chunks_exact_mut(4).for_each(|p| p[3] = 255);
            let make = |rgba: &[u8]| {
                let t = Texture2D::from_rgba8(w, h, rgba);
                t.set_filter(FilterMode::Linear);
                t
            };
            let width = color.width as usize;
            let first_row = color.rgba.chunks_exact(width * 4).position(|row| row.chunks_exact(4).any(|p| p[0].max(p[1]).max(p[2]) > 60)).unwrap_or(0);
            // A margin above, for the faint edges of the first letters.
            let first_row = first_row.saturating_sub(16);
            Some(Credits { shadow: make(&shadow), letters: make(&color.rgba), first_row: first_row as f32 })
        })
        .clone()
    })
}

/// A text of the install (`[<section>] <key>`) in Russian, else ours.
fn own(section: &str, key: &str, ours: &'static str) -> String {
    let t = (i18n::lang() == Lang::Ru).then(|| chrome::ui_text(section, key)).flatten();
    t.unwrap_or_else(|| tr(ours).to_string())
}

/// A window over the ruins, `w`×`h` video pixels, under the logo. Returns its inside and
/// whether its close box was clicked.
fn window(title: &str, w: f32, h: f32) -> (Rect, bool) {
    let k = chrome::k();
    let (w, h) = (w * k, h * k);
    let r = Rect::new((screen_width() - w) / 2.0, (190.0 * k).min(screen_height() - h), w, h);
    chrome::window(r, title, chrome::Skin::Marble, true)
}

/// A window over the map, centred over the view above the bottom bar, as the original's map
/// windows are (over its 1024×682 view), its foot never on the bar.
fn map_window(title: &str, w: f32, h: f32) -> (Rect, bool) {
    let k = chrome::k();
    let (w, h) = (w * k, h * k);
    let view = screen_height() - chrome::bar_height();
    let r = Rect::new((screen_width() - w) / 2.0, ((view - h) / 2.0).max(0.0), w, h);
    chrome::window(r, title, chrome::Skin::Marble, true)
}

/// "Информация об авторах": the splash of scattered drawings with the credits rolling up
/// over it (`started` is when the window opened). A click or Esc closes it.
pub fn authors(started: f64) -> bool {
    backdrop();
    let title = own("Authors", "Title", n_("About the authors"));
    let (inner, closed) = window(&title, 632.0, 460.0);
    let k = chrome::k();
    match chrome::win("Authors-Splash") {
        Some(t) => chrome::tex(&t, inner, WHITE),
        None => chrome::text_box(inner),
    }
    if let Some(c) = credits() {
        // As the original (0x4c82c4): the picture's top at the page's top to begin with (its
        // own blank start lets the first lines rise from below), 35 of its pixels a second,
        // and it stays on its last view once the end is in sight. Razdor's own lines come
        // first, before the original's; the wheel and the arrow keys scroll by hand, and the
        // scroll goes on from there.
        let px = k * 0.9375;
        let w = c.letters.width() * px;
        let total = c.letters.height() + OWN_CREDITS_H;
        let most = (total - inner.h / px).max(0.0);
        let (opened, mut nudge) = CREDITS_NUDGE.with(|n| n.get());
        if opened != started {
            nudge = 0.0;
        }
        let over = inner.contains(crate::ui::widgets::pointer().into());
        let step = 48.0;
        nudge -= if over { wheel() * step } else { 0.0 };
        if key(KeyCode::Down) {
            nudge += step;
        }
        if key(KeyCode::Up) {
            nudge -= step;
        }
        if key(KeyCode::PageDown) {
            nudge += inner.h / px * 0.8;
        }
        if key(KeyCode::PageUp) {
            nudge -= inner.h / px * 0.8;
        }
        let timed = (get_time() - started) as f32 * 35.0;
        // The clock while the credits roll on their own (Credits_OnOpen 0x4cfc6c calls
        // 0x48ecf0); the arrow once their time is up (0x4c84e7), whatever the hand scroll.
        if timed < most {
            super::cursor::set(super::cursor::Shape::Clock);
        }
        let scrolled = (timed + nudge).clamp(0.0, most);
        // Held at an end, the scroll owes nothing: turning back moves at once.
        CREDITS_NUDGE.with(|n| n.set((started, scrolled - timed)));
        let _clip = Clip::new(inner);
        // The picture's part from row `from` (its pixels), drawn from page row `at`.
        let picture = |from: f32, to: f32, at: f32| {
            let top = (inner.y + (at - scrolled) * px).max(inner.y);
            let bottom = (inner.y + (at + to - from - scrolled) * px).min(inner.y + inner.h);
            if bottom > top {
                let skip = (top - (inner.y + (at - scrolled) * px)) / px;
                let src = Rect::new(0.0, from + skip, c.letters.width(), (bottom - top) / px);
                let dst = Rect::new(inner.center().x - w / 2.0, top, w, bottom - top);
                chrome::tex_src(&c.shadow, src, dst, WHITE);
                chrome::additive(|| chrome::tex_src(&c.letters, src, dst, WHITE));
            }
        };
        picture(0.0, c.first_row, 0.0);
        // Razdor's modder, in the credits' colours: the role in gold, the name in white.
        let y = inner.y + (c.first_row + 24.0 - scrolled) * px;
        for (line, dy, size, color) in [(tr("RAZDOR modder"), 0.0, 20.0, chrome::GOLD), ("indicozy", 26.0, 22.0, WHITE)] {
            let lw = measure(line, size * px).width;
            chrome::shadow_text(line, inner.center().x - lw / 2.0, y + dy * px, size * px, color);
        }
        picture(c.first_row, c.letters.height(), c.first_row + OWN_CREDITS_H);
    }
    closed || key(KeyCode::Escape) || (clicked() && inner.contains(crate::ui::widgets::pointer().into()))
}

/// A horizontal slider of the original (`HSTexture` track, `HSlider` knob): returns the
/// new value (0..1) while dragged or clicked.
fn slider(r: Rect, value: f32) -> Option<f32> {
    let k = chrome::k();
    match chrome::win("HSTexture") {
        Some(t) => chrome::three_slice(&t, r, 12.0, WHITE),
        None => draw_rectangle(r.x, r.y + r.h / 3.0, r.w, r.h / 3.0, chrome::SILVER_DARK),
    }
    let knob_w = 32.0 * k * r.h / (18.0 * k);
    let x = r.x + (r.w - knob_w) * value.clamp(0.0, 1.0);
    match chrome::win("HSlider") {
        Some(t) => chrome::tex(&t, Rect::new(x, r.y, knob_w, r.h), WHITE),
        None => draw_rectangle(x, r.y, knob_w, r.h, chrome::SILVER),
    }
    let (mx, my) = crate::ui::widgets::pointer();
    let over = !input_blocked() && Rect::new(r.x, r.y - 4.0 * k, r.w, r.h + 8.0 * k).contains(vec2(mx, my));
    (over && is_mouse_button_down(MouseButton::Left)).then(|| ((mx - r.x - knob_w / 2.0) / (r.w - knob_w)).clamp(0.0, 1.0))
}

/// The battle AI's level: the player's choice, else the install's "improved enemy AI in
/// battle" (`[Options] OptValue9`), else easy.
pub fn expert_ai(audio: &super::audio::Settings) -> bool {
    audio.expert_ai.unwrap_or_else(|| chrome::ui_text("Options", "OptValue9").is_some_and(|v| razdor::dt::ini::loose_int(&v) == 1))
}

/// The battle animation speed (percent): the player's choice, else the install's
/// `[Options] AnimationSpeed` (the Community's); `None` without either (the vanilla game).
pub fn anim_speed(audio: &super::audio::Settings) -> Option<f32> {
    audio.anim_speed.or_else(|| chrome::options_value("AnimationSpeed").and_then(|v| v.trim().parse::<f32>().ok()).map(|s| s.clamp(0.0, 99.0)))
}

/// The front row's width for new games: the player's choice, else the install's "wide
/// front row in battle" (`[Options] OptValue11`).
pub fn wide_row(audio: &super::audio::Settings, install: bool) -> bool {
    audio.wide_row.unwrap_or(install)
}

/// "Настройки звука, графики и геймплея", with what Razdor lets the player change: the
/// music and sound volumes (saved at once) and the interface language. Returns true when
/// closed.
pub fn options(audio: &mut super::audio::Settings, install_wide: bool) -> bool {
    backdrop();
    options_window(audio, install_wide, false)
}

thread_local! {
    /// The advanced settings window is open over the settings.
    static ADVANCED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Opens the advanced settings over the settings window (a snapshot scene).
pub fn open_advanced() {
    ADVANCED.with(|a| a.set(true));
}

/// Razdor's advanced settings (the settings window's "Advanced…"): rules where Razdor can
/// differ from the original. Returns true when closed.
fn advanced_window(audio: &mut super::audio::Settings, over_map: bool) -> bool {
    // Razdor's own window, taller than the original's: centred (over the map, above the bar),
    // not hung from the menu's 190, where its foot reached the screen's bottom edge.
    let k = chrome::k();
    let (inner, closed) = if over_map {
        map_window(tr("Advanced settings"), 594.0, 570.0)
    } else {
        let (w, h) = (594.0 * k, 570.0 * k);
        let r = Rect::new((screen_width() - w) / 2.0, ((screen_height() - h) / 2.0).max(0.0), w, h);
        chrome::window(r, tr("Advanced settings"), chrome::Skin::Marble, true)
    };
    let y = inner.y + 30.0 * k;
    chrome::shadow_text(tr("Stepping onto a friendly army"), inner.x + 24.0 * k, y + 18.0 * k, 14.0 * k, chrome::CREAM);
    let br = Rect::new(inner.x + inner.w - 24.0 * k - 170.0 * k, y, 170.0 * k, 28.0 * k);
    let label = if audio.friends_let_pass { tr("Let pass") } else { tr("Battle (original)") };
    if settings_button(br, &label) {
        audio.friends_let_pass = !audio.friends_let_pass;
    }
    let hint = tr("With no event of the map for the meeting, the original opens a battle against a friendly army the hero steps onto; Razdor can let him pass instead.");
    for (i, line) in wrap(hint, inner.w - 48.0 * k, 12.0 * k).iter().enumerate() {
        chrome::shadow_text(line, inner.x + 24.0 * k, y + 56.0 * k + i as f32 * 17.0 * k, 12.0 * k, WHITE);
    }
    // Armies only passing by on a bridge (Razdor's): the original meets each one.
    let y = y + 100.0 * k;
    chrome::shadow_text(tr("Armies on bridges"), inner.x + 24.0 * k, y + 18.0 * k, 14.0 * k, chrome::CREAM);
    let pr = Rect::new(br.x, y, br.w, br.h);
    let label = if audio.pass_on_bridges { tr("Let pass") } else { tr("Stop (original)") };
    if settings_button(pr, &label) {
        audio.pass_on_bridges = !audio.pass_on_bridges;
    }
    let hint = tr("The original stops the hero at any army on a bridge. Razdor can let him through those only passing by; guards, enemies and the army clicked still meet him.");
    for (i, line) in wrap(hint, inner.w - 48.0 * k, 12.0 * k).iter().enumerate() {
        chrome::shadow_text(line, inner.x + 24.0 * k, y + 56.0 * k + i as f32 * 17.0 * k, 12.0 * k, WHITE);
    }
    // Updates from the GitHub releases (`razdor::update`): offered, put in place quietly, or
    // not looked for; "Check now" asks at once and shows the answer.
    let y = y + 100.0 * k;
    chrome::shadow_text(tr("Updates"), inner.x + 24.0 * k, y + 18.0 * k, 14.0 * k, chrome::CREAM);
    let ur = Rect::new(br.x, y, br.w, br.h);
    let mode = match audio.updates {
        razdor::update::Mode::Ask => tr("Ask"),
        razdor::update::Mode::Always => tr("Always"),
        razdor::update::Mode::Off => tr("Off"),
    };
    if settings_button(ur, mode) {
        audio.updates = audio.updates.next();
    }
    let cr = Rect::new(ur.x, y + 40.0 * k, ur.w, ur.h);
    if settings_button(cr, tr("Check now")) {
        super::update_view::check_by_hand();
    }
    let state = super::update_view::status_label();
    let sw = measure(&state, 13.0 * k).width;
    chrome::shadow_text(&state, cr.x - 16.0 * k - sw, cr.y + 19.0 * k, 13.0 * k, chrome::GOLD);
    let hint = tr("Ask: a newer release is offered in a window. Always: it is downloaded and starts the next time. The check never holds up the game.");
    for (i, line) in wrap(hint, inner.w - 48.0 * k, 12.0 * k).iter().enumerate() {
        chrome::shadow_text(line, inner.x + 24.0 * k, y + 92.0 * k + i as f32 * 17.0 * k, 12.0 * k, WHITE);
    }
    // The map's own zoom (Razdor's): the wheel steps from it, 0 comes back to it; locked, the
    // wheel and +/- leave it alone.
    let y = y + 140.0 * k;
    chrome::shadow_text(tr("Map zoom"), inner.x + 24.0 * k, y + 18.0 * k, 14.0 * k, chrome::CREAM);
    let zr = Rect::new(br.x, y, br.w, br.h);
    let lock_label = if audio.zoom_locked { tr("Locked") } else { tr("Wheel and +/-") };
    if settings_button(zr, lock_label) {
        audio.zoom_locked = !audio.zoom_locked;
    }
    let (lo, hi) = super::world_view::ZOOM_RANGE;
    let value = format!("×{:.2}", audio.map_zoom);
    let track = Rect::new(inner.x + 24.0 * k, y + 44.0 * k, inner.w - 48.0 * k - 70.0 * k, 18.0 * k);
    chrome::shadow_text(&value, track.x + track.w + 16.0 * k, track.y + 14.0 * k, 14.0 * k, chrome::GOLD);
    if let Some(v) = slider(track, (audio.map_zoom - lo) / (hi - lo)) {
        audio.map_zoom = ((lo + v * (hi - lo)) / 0.05).round() * 0.05;
    }
    let hint = tr("The zoom the map plays at. The wheel steps away from it and the 0 key comes back to it; locked, the map always shows it.");
    for (i, line) in wrap(hint, inner.w - 48.0 * k, 12.0 * k).iter().enumerate() {
        chrome::shadow_text(line, inner.x + 24.0 * k, y + 80.0 * k + i as f32 * 17.0 * k, 12.0 * k, WHITE);
    }
    let ok = Rect::new(inner.x + inner.w - 120.0 * k, inner.y + inner.h - 44.0 * k, 96.0 * k, 28.0 * k);
    let ok_label = own("Buttons", "Ok", n_("OK"));
    closed || key(KeyCode::Escape) || key(KeyCode::Enter) || settings_button(ok, &ok_label)
}

fn settings_button(r: Rect, label: &str) -> bool {
    let (_, down, pressed) = super::widgets::button_state(r, true);
    chrome::marble_button(r, label, true, down);
    if pressed { cue(Cue::Button); }
    pressed
}

/// The settings window alone (over the main menu, or `over_map` from the bar's gears: then
/// above the bar, not over it).
pub fn options_window(audio: &mut super::audio::Settings, install_wide: bool, over_map: bool) -> bool {
    if ADVANCED.with(|a| a.get()) {
        if advanced_window(audio, over_map) {
            ADVANCED.with(|a| a.set(false));
        }
        return false;
    }
    let title = own("Options", "Title", n_("Sound, graphics and gameplay settings"));
    let (inner, closed) = if over_map { map_window(&title, 594.0, 542.0) } else { window(&title, 594.0, 542.0) };
    let k = chrome::k();
    let rows = [
        (own("Options", "OptionSld0", n_("Background music volume")), audio.music_volume, true),
        (own("Options", "OptionSld1", n_("Sound effects volume")), audio.sfx_volume, false),
    ];
    for (i, (label, value, music)) in rows.into_iter().enumerate() {
        let y = inner.y + 36.0 * k + i as f32 * 52.0 * k;
        chrome::shadow_text(&label, inner.x + 24.0 * k, y, 14.0 * k, chrome::CREAM);
        let pct = format!("{:.0}%", value * 100.0);
        chrome::shadow_text(&pct, inner.x + inner.w - 24.0 * k - measure(&pct, 14.0 * k).width, y, 14.0 * k, chrome::GOLD);
        let track = Rect::new(inner.x + 24.0 * k, y + 10.0 * k, inner.w - 48.0 * k, 18.0 * k);
        if let Some(v) = slider(track, value) {
            let steps = ((v - value) / super::audio::VOLUME_STEP).round() as i32;
            if music {
                audio.music_muted = false;
                audio.step_music(steps);
            } else {
                audio.sfx_muted = false;
                audio.step_sfx(steps);
            }
        }
    }
    // The Community's "Скорость анимаций в битве" (`AnimationSpeed`): it caps a strike's
    // slide at (100 − S)·5 ms and its effect at (100 − S)·3 ms (0x4afd45, 0x4afe7c).
    let y = inner.y + 36.0 * k + 2.0 * 52.0 * k;
    let label = own("Options", "OptionSld4", n_("Battle animation speed"));
    chrome::shadow_text(&label, inner.x + 24.0 * k, y, 14.0 * k, chrome::CREAM);
    let speed = anim_speed(audio).unwrap_or(0.0);
    let pct = format!("{speed:.0}%");
    chrome::shadow_text(&pct, inner.x + inner.w - 24.0 * k - measure(&pct, 14.0 * k).width, y, 14.0 * k, chrome::GOLD);
    let track = Rect::new(inner.x + 24.0 * k, y + 10.0 * k, inner.w - 48.0 * k, 18.0 * k);
    if let Some(v) = slider(track, speed / 99.0) {
        audio.anim_speed = Some((v * 99.0).round());
    }
    let lang = format!("{} / {}", Lang::En.label(), Lang::Ru.label());
    let lr = Rect::new(inner.x + 24.0 * k, inner.y + inner.h - 44.0 * k, 140.0 * k, 28.0 * k);
    if settings_button(lr, &lang) {
        super::language::toggle();
    }
    let fr = Rect::new(lr.x + lr.w + 16.0 * k, lr.y, 150.0 * k, lr.h);
    let fps_label = if audio.show_fps { tr("FPS shown") } else { tr("FPS hidden") };
    if settings_button(fr, fps_label) {
        audio.show_fps = !audio.show_fps;
    }
    // Razdor's advanced settings, in a window of their own.
    let xr = Rect::new(fr.x + fr.w + 16.0 * k, lr.y, inner.x + inner.w - 136.0 * k - (fr.x + fr.w + 16.0 * k), lr.h);
    if settings_button(xr, tr("Advanced…")) {
        ADVANCED.with(|a| a.set(true));
        return false;
    }
    // The battle AI, as the original's "Улучшенный интеллект противника в битве": easy or
    // expert, from the install's setting until chosen here.
    let y = inner.y + 36.0 * k + 3.0 * 52.0 * k;
    let label = own("Options", "Option9", n_("Improved enemy AI in battle"));
    chrome::shadow_text(&label, inner.x + 24.0 * k, y + 18.0 * k, 14.0 * k, chrome::CREAM);
    let expert = expert_ai(audio);
    let ar = Rect::new(inner.x + inner.w - 24.0 * k - 150.0 * k, y, 150.0 * k, 28.0 * k);
    let ai_label = if expert { tr("Expert") } else { tr("Easy") };
    if settings_button(ar, ai_label) {
        audio.expert_ai = Some(!expert);
    }
    // The front row's width (Razdor's switch for the original's "wide front row in battle",
    // which only the install's ini set): for the games started from now on.
    let y = y + 52.0 * k;
    chrome::shadow_text(tr("Front row in battle (new games)"), inner.x + 24.0 * k, y + 18.0 * k, 14.0 * k, chrome::CREAM);
    let wide = wide_row(audio, install_wide);
    let rr = Rect::new(ar.x, y, ar.w, ar.h);
    let row_label = if wide { tr("6 cells") } else { tr("4 cells") };
    if settings_button(rr, row_label) {
        audio.wide_row = Some(!wide);
    }
    // Windowed, borderless or full screen, set at once (`display::follow_settings`).
    let y = y + 52.0 * k;
    chrome::shadow_text(tr("Screen (Alt+Enter)"), inner.x + 24.0 * k, y + 18.0 * k, 14.0 * k, chrome::CREAM);
    let dr = Rect::new(ar.x, y, ar.w, ar.h);
    if settings_button(dr, audio.display.label()) {
        audio.display = audio.display.next();
    }
    // The interface scale, as Minecraft's: Auto (the largest that fits) or a fixed one below it.
    let y = y + 52.0 * k;
    chrome::shadow_text(tr("Interface scale"), inner.x + 24.0 * k, y + 18.0 * k, 14.0 * k, chrome::CREAM);
    let fit = chrome::fit_k();
    let sr = Rect::new(ar.x, y, ar.w, ar.h);
    if settings_button(sr, &super::display::scale_label(audio.ui_scale, fit)) {
        audio.ui_scale = super::display::next_scale(audio.ui_scale, fit);
    }
    // The interface's fonts (Razdor's choice): the original's sheets from the install, PT
    // Serif, or the pixel faces; at once.
    let y = y + 52.0 * k;
    chrome::shadow_text(tr("Font"), inner.x + 24.0 * k, y + 18.0 * k, 14.0 * k, chrome::CREAM);
    let fr = Rect::new(ar.x, y, ar.w, ar.h);
    let font_label = match audio.font {
        super::dt_font::FontMode::Original => tr("As in the original"),
        super::dt_font::FontMode::Modern => tr("Modern"),
    };
    let over_font = fr.contains(crate::ui::widgets::pointer().into()) && !input_blocked();
    chrome::marble_button(fr, font_label, true, over_font);
    if over_font && clicked() {
        cue(Cue::Button);
        audio.font = audio.font.next();
    }
    let ok = Rect::new(inner.x + inner.w - 120.0 * k, inner.y + inner.h - 44.0 * k, 96.0 * k, 28.0 * k);
    let ok_label = own("Buttons", "Ok", n_("OK"));
    closed || key(KeyCode::Escape) || key(KeyCode::Enter) || settings_button(ok, &ok_label)
}
