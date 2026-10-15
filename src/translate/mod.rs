//! The map translator (`razdor-translate`): makes English copies of the install's Russian
//! maps with any OpenAI-compatible language model. Not part of the original game; the
//! window is in `src/bin/razdor-translate/`, the flow in `docs/flows/testing-and-tools.md`.
//!
//! - [`text`]: one string to the model and back (placeholders, checks).
//! - [`job`]: the strings of a map or campaign, batches, progress, the English maps.
//! - [`llm`]: the API and its settings.

pub mod job;
pub mod llm;
pub mod text;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::dt::dtm::Scenario;
use crate::dt::install::list_maps;
use job::Job;
use llm::Settings;

/// The maps of an install's maps folder: file name and scenario, the unreadable ones left out.
pub fn install_maps(install: &Path) -> Vec<(String, PathBuf, Scenario)> {
    let Ok(list) = list_maps(install) else { return Vec::new() };
    list.into_iter()
        .filter_map(|m| {
            let file = m.path.file_name()?.to_string_lossy().into_owned();
            let s = Scenario::load(&m.path).ok()?;
            Some((file, m.path, s))
        })
        .collect()
}

/// The job for `maps` (in play order): the saved one's English kept for strings it shares.
pub fn open_job(maps: &[(String, Scenario)]) -> (Job, Option<PathBuf>) {
    let mut job = Job::new(maps);
    let path = maps.first().and_then(|(f, _)| Job::path_for(f));
    if let Some(old) = path.as_deref().and_then(Job::load) {
        job.take_over(&old);
    }
    (job, path)
}

/// What [`run`] reports after each answer.
pub enum Event {
    /// A batch was sent: its size and the first text.
    Sending { items: usize, first: String },
    /// An answer was taken: how many of the batch passed, and how long it took.
    Answer { taken: usize, of: usize, seconds: f32 },
    /// The request or the answer failed as a whole.
    Failed(String),
}

/// Translates what is left of `job`, saving it to `path` after every answer, until it is
/// done, every open string was given up on, or `stop` is set. Stops after three requests in
/// a row fail as a whole (a server down, a bad key).
pub fn run(job: &mut Job, path: Option<&Path>, s: &Settings, stop: &AtomicBool, mut report: impl FnMut(&Job, Event)) -> Result<(), String> {
    let mut failures_in_a_row = 0;
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let batch = job.next_batch(s.batch_chars(), s.batch_items());
        if batch.is_empty() {
            return Ok(());
        }
        let glossary = job.glossary(&batch, 40);
        let texts: Vec<&str> = batch.iter().map(|&i| job.entries[i].src.as_str()).collect();
        let prompt = llm::user_prompt(&texts, &glossary);
        report(job, Event::Sending { items: batch.len(), first: texts[0].chars().take(60).collect() });
        let t0 = std::time::Instant::now();
        match llm::chat(s, llm::SYSTEM_PROMPT, &prompt).and_then(|a| llm::parse_answer(&a)) {
            Ok(answers) => {
                failures_in_a_row = 0;
                let taken = job.accept(&batch, &answers);
                report(job, Event::Answer { taken, of: batch.len(), seconds: t0.elapsed().as_secs_f32() });
            }
            Err(why) => {
                failures_in_a_row += 1;
                // A batch the model cannot answer as a whole is split by the retries.
                job.refuse(&batch, &why);
                report(job, Event::Failed(why.clone()));
                if failures_in_a_row >= 3 {
                    if let Some(p) = path {
                        let _ = job.save(p);
                    }
                    return Err(why);
                }
            }
        }
        if let Some(p) = path {
            job.save(p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
}

/// Writes the English copies of `job`'s maps into `maps_dir`; a file that exists is only
/// replaced with `overwrite`. Returns the files written.
pub fn write_maps(job: &Job, maps_dir: &Path, overwrite: bool) -> Result<Vec<PathBuf>, String> {
    let targets: Vec<PathBuf> = job.maps.iter().map(|m| maps_dir.join(&m.out)).collect();
    if !overwrite {
        if let Some(t) = targets.iter().find(|t| t.exists()) {
            return Err(format!("{} exists", t.display()));
        }
    }
    let mut written = Vec::new();
    for (m, target) in job.maps.iter().zip(targets) {
        let src = maps_dir.join(&m.file);
        let bytes = std::fs::read(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        let s = Scenario::from_file_bytes(&bytes).map_err(|e| format!("{}: {e}", src.display()))?;
        let out = job::file_bytes(&job.english(&s), &bytes).map_err(|e| format!("{}: {e}", src.display()))?;
        crate::editor::files::write_atomically(&target, &out).map_err(|e| format!("{}: {e}", target.display()))?;
        written.push(target);
    }
    Ok(written)
}

/// The install's maps folder.
pub fn maps_dir(install: &Path) -> Option<PathBuf> {
    crate::dt::install::find_path(install, crate::dt::install::MAPS_DIR).ok()
}
