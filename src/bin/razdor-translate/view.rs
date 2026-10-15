//! The translator's window: the install's maps and campaigns on the left; the model's
//! settings, the chosen map's progress, the buttons and the log on the right. The work runs
//! on a thread ([`razdor::translate::run`]); the window shows its progress.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use macroquad::prelude::*;
use razdor::dt::dtm::Scenario;
use razdor::i18n::tr;
use razdor::translate::job::{self, Job};
use razdor::translate::llm::Settings;
use razdor::translate::{self, Event};
use razdor::trf;

use crate::ui::widgets::{self, button, red_button, text, text_fit, text_field, ACCENT, DIM, FIELD_BG, INK, PANEL};

const RED: Color = Color::new(0.95, 0.45, 0.40, 1.0);
const GREEN: Color = Color::new(0.55, 0.85, 0.50, 1.0);
const ROW_H: f32 = 46.0;
const LIST_W: f32 = 430.0;

/// The server's answer to "Test": its models, or why not.
type ModelList = Result<Vec<String>, String>;

/// A line of the list: one map, or a whole campaign.
struct Item {
    title: String,
    /// The install's own texts rather than maps (`chapters` is empty).
    data: bool,
    /// Indexes into [`View::maps`], in play order.
    chapters: Vec<usize>,
}

/// What the work thread shares with the window.
#[derive(Default)]
struct Shared {
    job: Option<Job>,
    log: Vec<(String, Color)>,
    running: bool,
}

pub struct View {
    install: Option<PathBuf>,
    maps_dir: Option<PathBuf>,
    /// Where the chosen item's English goes.
    target: Option<translate::Target>,
    maps: Vec<(String, Scenario)>,
    items: Vec<Item>,
    selected: Option<usize>,
    scroll: f32,
    settings: Settings,
    saved: Settings,
    /// The chosen item's job and its file.
    job: Option<(Job, Option<PathBuf>)>,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    check: Arc<Mutex<Option<ModelList>>>,
    confirm_replace: bool,
    /// "Translate again" was pressed once: the next press forgets the translations.
    confirm_fresh: bool,
}

impl View {
    pub fn new() -> View {
        let install = razdor::dt::install::locate();
        let maps_dir = install.as_deref().and_then(translate::maps_dir);
        let maps: Vec<(String, Scenario)> =
            install.as_deref().map(translate::install_maps).unwrap_or_default().into_iter().map(|(f, _, s)| (f, s)).collect();
        let items = items(&maps);
        let settings = Settings::load();
        let v = View {
            install: install.clone(),
            maps_dir,
            target: None,
            maps,
            items,
            selected: None,
            scroll: 0.0,
            saved: settings.clone(),
            settings,
            job: None,
            shared: Arc::new(Mutex::new(Shared::default())),
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            check: Arc::new(Mutex::new(None)),
            confirm_replace: false,
            confirm_fresh: false,
        };
        if install.is_none() {
            v.log(tr("No Discord Times install found: start Razdor once to choose it."), RED);
        }
        v
    }

    fn log(&self, line: impl Into<String>, color: Color) {
        let mut sh = self.shared.lock().unwrap();
        sh.log.push((line.into(), color));
        let n = sh.log.len();
        if n > 200 {
            sh.log.drain(..n - 200);
        }
    }

    fn running(&self) -> bool {
        self.shared.lock().unwrap().running
    }

    /// Stops the work and waits for it (the job is saved after every answer).
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }

    /// Chooses the first item with a map whose file starts with `name`.
    pub fn pick(&mut self, name: &str) {
        if name == "data" {
            if let Some(i) = self.items.iter().position(|it| it.data) {
                self.select(i);
            }
            return;
        }
        let want = name.to_lowercase();
        if let Some(i) = self.items.iter().position(|it| it.chapters.iter().any(|&k| self.maps[k].0.to_lowercase().starts_with(&want))) {
            self.select(i);
        }
    }

    fn select(&mut self, i: usize) {
        if self.running() {
            return;
        }
        self.selected = Some(i);
        self.confirm_replace = false;
        self.confirm_fresh = false;
        if self.items[i].data {
            let Some(install) = self.install.clone() else { return };
            let data = translate::data::install_texts(&install);
            self.job = Some(translate::data::open_job(&data));
            self.target = Some(translate::Target::Data { install, data });
            return;
        }
        let chapters: Vec<(String, Scenario)> = self.items[i].chapters.iter().map(|&k| self.maps[k].clone()).collect();
        self.job = Some(translate::open_job(&chapters));
        self.target = self.maps_dir.clone().map(translate::Target::Maps);
    }

    pub fn frame(&mut self) {
        crate::ui::input::begin_frame();
        widgets::track_held_key();
        widgets::fields_begin_frame();
        clear_background(Color::from_rgba(12, 12, 11, 255));
        // The thread's latest job.
        if let (Some(j), Some((job, _))) = (self.shared.lock().unwrap().job.take(), self.job.as_mut()) {
            *job = j;
        }
        if self.worker.as_ref().is_some_and(|w| w.is_finished()) {
            let _ = self.worker.take().map(|w| w.join());
        }
        let (sw, sh) = (screen_width(), screen_height());
        text(tr("Map translator"), 16.0, 30.0, 24.0, ACCENT);
        text_fit(tr("English copies of the install's maps, made by a language model"), 230.0, 29.0, sw - 360.0, 15.0, DIM);
        crate::ui::language::switch_button(sw - 110.0, 8.0, 94.0, 28.0);
        self.list(Rect::new(12.0, 48.0, LIST_W, sh - 60.0));
        let right = Rect::new(LIST_W + 24.0, 48.0, sw - LIST_W - 36.0, sh - 60.0);
        let y = self.settings_panel(right);
        let y = self.job_panel(Rect::new(right.x, y + 10.0, right.w, 150.0));
        self.log_panel(Rect::new(right.x, y + 10.0, right.w, right.y + right.h - y - 10.0));
        widgets::fields_end_frame();
        // Settings are kept once typing ends.
        if !widgets::typing() && self.settings != self.saved {
            if let Err(e) = self.settings.save() {
                self.log(trf!("Settings not saved: {e}", e), RED);
            }
            self.saved = self.settings.clone();
        }
    }

    fn list(&mut self, r: Rect) {
        draw_rectangle(r.x, r.y, r.w, r.h, PANEL);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, DIM);
        if self.items.is_empty() {
            text(tr("No maps"), r.x + 12.0, r.y + 28.0, 17.0, DIM);
            return;
        }
        let content = self.items.len() as f32 * ROW_H;
        if widgets::mouse_in(r.x, r.y, r.w, r.h) {
            self.scroll = (self.scroll - widgets::wheel() * ROW_H).clamp(0.0, (content - r.h + 8.0).max(0.0));
        }
        let mut clicked = None;
        for (i, item) in self.items.iter().enumerate() {
            let y = r.y + 4.0 + i as f32 * ROW_H - self.scroll;
            if y + ROW_H < r.y || y > r.y + r.h - ROW_H + 4.0 {
                continue;
            }
            let row = Rect::new(r.x + 4.0, y, r.w - 8.0, ROW_H - 4.0);
            let hover = widgets::mouse_in(row.x, row.y, row.w, row.h);
            if self.selected == Some(i) {
                draw_rectangle(row.x, row.y, row.w, row.h, Color::new(0.30, 0.24, 0.12, 0.9));
            } else if hover {
                draw_rectangle(row.x, row.y, row.w, row.h, Color::new(1.0, 1.0, 1.0, 0.06));
            }
            if hover && widgets::clicked() {
                clicked = Some(i);
            }
            let english = if item.data {
                self.install.as_ref().is_some_and(|d| d.join(razdor::dt::english::english_file(razdor::dt::install::UNITS_FILE)).exists())
            } else {
                self.maps_dir.as_ref().is_some_and(|d| item.chapters.iter().all(|&k| d.join(job::english_file(&self.maps[k].0)).exists()))
            };
            let title = if english { format!("{}  [EN]", item.title) } else { item.title.clone() };
            text_fit(&title, row.x + 8.0, row.y + 19.0, row.w - 16.0, 17.0, if english { GREEN } else { INK });
            let sub = if item.data {
                tr("Units, items, spells and the game's texts: for Razdor in English").to_string()
            } else if item.chapters.len() > 1 {
                trf!("Campaign: {n} chapters", n = item.chapters.len())
            } else {
                self.maps[item.chapters[0]].0.clone()
            };
            text_fit(&sub, row.x + 8.0, row.y + 37.0, row.w - 16.0, 14.0, DIM);
        }
        if let Some(i) = clicked {
            self.select(i);
        }
    }

    /// The model's settings; returns the panel's bottom.
    fn settings_panel(&mut self, r: Rect) -> f32 {
        let h = 312.0;
        draw_rectangle(r.x, r.y, r.w, h, PANEL);
        draw_rectangle_lines(r.x, r.y, r.w, h, 1.0, DIM);
        text(tr("Language model (OpenAI-compatible API)"), r.x + 12.0, r.y + 24.0, 18.0, ACCENT);
        let lx = r.x + 12.0;
        let fx = r.x + 190.0;
        let fw = r.w - 190.0 - 132.0;
        let bx = fx + fw + 10.0;
        let busy = self.running();
        let mut y = r.y + 38.0;
        text(tr("Address"), lx, y + 18.0, 16.0, INK);
        text_field("tr:endpoint", fx, y, fw, 26.0, &mut self.settings.endpoint, false);
        if button(bx, y, 110.0, 26.0, tr("Test"), !busy) {
            self.check_connection();
        }
        y += 34.0;
        text(tr("Model"), lx, y + 18.0, 16.0, INK);
        text_field("tr:model", fx, y, fw, 26.0, &mut self.settings.model, false);
        y += 34.0;
        text(tr("API key"), lx, y + 18.0, 16.0, INK);
        text_field("tr:key", fx, y, fw, 26.0, &mut self.settings.api_key, false);
        // The key is hidden unless it is being typed.
        if !widgets::has_focus("tr:key") {
            draw_rectangle(fx + 2.0, y + 2.0, fw - 4.0, 22.0, FIELD_BG);
            let shown = if self.settings.api_key.is_empty() { tr("none (a local server)").to_string() } else { "\u{2022}".repeat(self.settings.api_key.chars().count().min(24)) };
            text(&shown, fx + 5.0, y + 19.0, 17.0, DIM);
        }
        if button(bx, y, 110.0, 26.0, tr("Paste"), true) {
            if let Some(k) = macroquad::miniquad::window::clipboard_get() {
                self.settings.api_key = k.trim().to_string();
            }
        }
        y += 34.0;
        text(tr("Context (tokens)"), lx, y + 18.0, 16.0, INK);
        if let Some(v) = widgets::number_field("tr:context", fx, y, 200.0, self.settings.context as i64, 512, 1_048_576) {
            self.settings.context = v as u32;
        }
        y += 34.0;
        text(tr("Reasoning"), lx, y + 18.0, 16.0, INK);
        text_field("tr:effort", fx, y, 200.0, 26.0, &mut self.settings.reasoning_effort, false);
        text_fit(tr("none: thinking off (Ollama); empty: the server's default"), fx + 212.0, y + 18.0, r.x + r.w - fx - 224.0, 14.0, DIM);
        y += 34.0;
        // The player's own wishes for the style, added to the fixed rules.
        text(tr("Translation style"), lx, y + 18.0, 16.0, INK);
        text_field("tr:style", fx, y, r.x + r.w - fx - 12.0, 56.0, &mut self.settings.style, true);
        if self.settings.style.is_empty() && !widgets::has_focus("tr:style") {
            text_fit(tr("e.g. \"archaic, like an old chronicle; keep names Slavic\""), fx + 5.0, y + 19.0, r.x + r.w - fx - 22.0, 15.0, DIM);
        }
        y += 64.0;
        let check = self.check.lock().unwrap().clone();
        match check {
            Some(Ok(models)) => {
                let has = models.iter().any(|m| m == self.settings.model.trim() || m.trim_end_matches(":latest") == self.settings.model.trim());
                let line = if has {
                    tr("Connected; the model is there.").to_string()
                } else {
                    trf!("Connected, but no model {m}. There are: {list}", m = self.settings.model.trim(), list = models.join(", "))
                };
                text_fit(&line, lx, y + 16.0, r.w - 24.0, 15.0, if has { GREEN } else { RED });
            }
            Some(Err(why)) => text_fit(&trf!("No connection: {why}", why), lx, y + 16.0, r.w - 24.0, 15.0, RED),
            None => text_fit(tr("Ollama: http://localhost:11434/v1, no key. OpenAI: https://api.openai.com/v1 with a key."), lx, y + 16.0, r.w - 24.0, 15.0, DIM),
        }
        r.y + h
    }

    fn check_connection(&mut self) {
        let s = self.settings.clone();
        let out = self.check.clone();
        *out.lock().unwrap() = None;
        std::thread::spawn(move || {
            let got = translate::llm::models(&s);
            *out.lock().unwrap() = Some(got);
        });
    }

    /// The chosen map's progress and the buttons; returns the panel's bottom.
    fn job_panel(&mut self, r: Rect) -> f32 {
        draw_rectangle(r.x, r.y, r.w, r.h, PANEL);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, DIM);
        let (Some(i), Some((job, _))) = (self.selected, self.job.as_ref()) else {
            text(tr("Choose a map or a campaign on the left."), r.x + 12.0, r.y + 28.0, 17.0, DIM);
            return r.y + r.h;
        };
        let (total, done, failed, names, chars) = (job.entries.len(), job.done(), job.failed(), job.entries.iter().filter(|e| e.name).count(), job.pending_chars());
        let (complete, open) = (job.complete(), job.entries.iter().any(|e| e.en.is_none() && e.tries < job::MAX_TRIES));
        let busy = self.running();
        text_fit(&self.items[i].title, r.x + 12.0, r.y + 26.0, r.w - 24.0, 20.0, ACCENT);
        let line = trf!("{total} texts ({names} names), {done} translated, {failed} given up; {chars} characters left", total, names, done, failed, chars);
        text_fit(&line, r.x + 12.0, r.y + 50.0, r.w - 24.0, 16.0, INK);
        // Progress bar.
        let (bx, by, bw) = (r.x + 12.0, r.y + 62.0, r.w - 24.0);
        draw_rectangle(bx, by, bw, 14.0, FIELD_BG);
        if total > 0 {
            draw_rectangle(bx, by, bw * done as f32 / total as f32, 14.0, if failed > 0 { RED } else { GREEN });
        }
        draw_rectangle_lines(bx, by, bw, 14.0, 1.0, DIM);
        let y = r.y + 90.0;
        let w = ((r.w - 24.0 - 30.0) / 4.0).min(200.0);
        if busy {
            if button(r.x + 12.0, y, w, 32.0, tr("Stop"), true) {
                self.stop.store(true, Ordering::Relaxed);
                self.log(tr("Stopping after this answer..."), DIM);
            }
        } else if button(r.x + 12.0, y, w, 32.0, if done == 0 { tr("Translate") } else { tr("Go on") }, open) {
            self.start(false);
        }
        if button(r.x + 22.0 + w, y, w, 32.0, tr("Retry given up"), !busy && failed > 0) {
            self.start(true);
        }
        let can_write = !busy && done > 0 && self.target.is_some();
        let write_x = r.x + 32.0 + 2.0 * w;
        if self.confirm_replace {
            if red_button(write_x, y, w, 32.0, tr("Replace English maps"), can_write) {
                self.write(true);
            }
        } else if button(write_x, y, w, 32.0, tr("Write English maps"), can_write) {
            self.write(false);
        }
        // Everything again (another model or style): a second press confirms.
        let fresh_x = r.x + 42.0 + 3.0 * w;
        if self.confirm_fresh {
            if red_button(fresh_x, y, w, 32.0, tr("Forget and translate"), !busy) {
                self.confirm_fresh = false;
                if let Some((job, _)) = self.job.as_mut() {
                    job.forget_all();
                }
                self.start(false);
            }
        } else if button(fresh_x, y, w, 32.0, tr("Translate again"), !busy && done > 0) {
            self.confirm_fresh = true;
            self.log(tr("Press again to translate everything anew (with the style and model set now)."), ACCENT);
        }
        if !complete && done > 0 && !busy {
            text_fit(tr("Untranslated texts stay in Russian in the written maps."), r.x + 12.0, y + 50.0, r.w - 24.0, 14.0, DIM);
        }
        r.y + r.h
    }

    fn start(&mut self, retry: bool) {
        let Some((job, path)) = self.job.as_mut() else { return };
        if retry {
            job.retry_failed();
        }
        let (mut job, path) = (job.clone(), path.clone());
        let s = self.settings.clone();
        let target = self.target.clone();
        let shared = self.shared.clone();
        self.stop.store(false, Ordering::Relaxed);
        let stop = self.stop.clone();
        shared.lock().unwrap().running = true;
        self.log(trf!("Started: model {m}, {n} characters per batch", m = s.model, n = s.batch_chars()), INK);
        self.worker = Some(std::thread::spawn(move || {
            let push = |line: String, color: Color| {
                let mut sh = shared.lock().unwrap();
                sh.log.push((line, color));
            };
            let result = translate::run(&mut job, path.as_deref(), &s, &stop, |job, e| {
                let line = match e {
                    Event::Sending { items, first } => (trf!("Sending {items}: {first}...", items, first), DIM),
                    Event::Answer { taken, of, seconds } => {
                        (trf!("Taken {taken} of {of} in {s} s", taken, of, s = format!("{seconds:.1}")), if taken == of { GREEN } else { RED })
                    }
                    Event::Failed(why) => (trf!("Failed: {why}", why), RED),
                };
                let mut sh = shared.lock().unwrap();
                sh.log.push(line);
                sh.job = Some(job.clone());
            });
            match result {
                // Done: the English copies go straight into the game's maps folder, replacing
                // older copies made by this tool.
                Ok(()) if job.complete() => match target.as_ref().map(|t| t.write(&job, true)) {
                    Some(Ok(files)) => {
                        push(tr("All texts translated.").to_string(), GREEN);
                        for f in files {
                            push(trf!("Written: {f}", f = f.display()), GREEN);
                        }
                    }
                    Some(Err(why)) => push(trf!("Not written: {why}", why), RED),
                    None => push(tr("All texts translated, but the game has no maps folder.").to_string(), RED),
                },
                Ok(()) if stop.load(Ordering::Relaxed) => push(tr("Stopped.").to_string(), DIM),
                Ok(()) => push(trf!("Done; {n} texts given up (Retry given up).", n = job.failed()), RED),
                Err(why) => push(trf!("Stopped: {why}", why), RED),
            }
            let mut sh = shared.lock().unwrap();
            sh.job = Some(job);
            sh.running = false;
        }));
    }

    fn write(&mut self, overwrite: bool) {
        let (Some((job, _)), Some(target)) = (self.job.as_ref(), self.target.as_ref()) else { return };
        match target.write(job, overwrite) {
            Ok(files) => {
                self.confirm_replace = false;
                for f in files {
                    self.log(trf!("Written: {f}", f = f.display()), GREEN);
                }
            }
            Err(_) if !overwrite => {
                self.confirm_replace = true;
                self.log(tr("English copies exist already: press again to replace them."), ACCENT);
            }
            Err(why) => self.log(trf!("Not written: {why}", why), RED),
        }
    }

    fn log_panel(&mut self, r: Rect) {
        if r.h < 30.0 {
            return;
        }
        draw_rectangle(r.x, r.y, r.w, r.h, FIELD_BG);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, DIM);
        let sh = self.shared.lock().unwrap();
        let fit = ((r.h - 10.0) / 19.0).floor().max(0.0) as usize;
        let start = sh.log.len().saturating_sub(fit);
        for (k, (line, color)) in sh.log[start..].iter().enumerate() {
            text_fit(line, r.x + 8.0, r.y + 20.0 + k as f32 * 19.0, r.w - 16.0, 15.0, *color);
        }
    }
}

/// The list: each campaign once (by its first chapter), then the maps alone; English copies
/// made by this tool left out.
fn items(maps: &[(String, Scenario)]) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    for k in 0..maps.len() {
        if job::is_english_file(&maps[k].0) {
            continue;
        }
        let chapters = job::campaign(maps, k);
        if out.iter().any(|it| it.chapters.contains(&k)) {
            continue;
        }
        let first = &maps[chapters[0]].1;
        let title = if chapters.len() > 1 && !first.campaign_name.trim().is_empty() { first.campaign_name.trim() } else { first.title.trim() };
        let title = if title.is_empty() { maps[chapters[0]].0.clone() } else { title.to_string() };
        out.push(Item { title, data: false, chapters });
    }
    out.sort_by_key(|it| (it.chapters.len() == 1, it.title.clone()));
    // The install's own texts first: maps translated after them use their names.
    if !maps.is_empty() {
        out.insert(0, Item { title: tr("Game data").to_string(), data: true, chapters: Vec::new() });
    }
    out
}
