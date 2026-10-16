//! The map translator (`razdor-translate`): makes English copies of the install's Russian
//! maps with any OpenAI-compatible language model. Not part of the original game; the
//! window is in `src/bin/razdor-translate/`, the flow in `docs/flows/testing-and-tools.md`.
//!
//! - [`text`]: one string to the model and back (placeholders, checks).
//! - [`job`]: the strings of a map or campaign, batches, progress, the English maps.
//! - [`data`]: the install's own texts (units, items, spells, interface) and their translated
//!   files for Razdor.
//! - [`llm`]: the API and its settings.

pub mod data;
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

/// The job for `maps` (in play order) into `language`: the saved one's translations kept for
/// strings it shares.
pub fn open_job(maps: &[(String, Scenario)], language: &str, any_script: bool) -> (Job, Option<PathBuf>) {
    let mut job = Job::new(maps, language, any_script);
    let path = maps.first().and_then(|(f, _)| Job::path_for(f, language, any_script));
    if let Some(old) = path.as_deref().and_then(Job::load) {
        job.take_over(&old);
    }
    // The game data's names in the same language (and letters), when it has been translated:
    // a map's texts call a unit what its card calls it.
    job.outside_glossary = data::glossary(language, any_script);
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
    /// Texts of the batch whose answers were refused: the start of each, and why.
    Refused(Vec<(String, String)>),
}

/// The request log of a job: every request and the model's raw answer, next to its progress
/// file (`<job>.log`), for finding out why a text is refused or a style not followed.
pub fn request_log(job_path: &Path) -> PathBuf {
    job_path.with_extension("log")
}

/// Appends `text` to the request log, starting it again past 4 MB.
fn log_request(path: Option<&Path>, text: &str) {
    use std::io::Write;
    let Some(p) = path.map(request_log) else { return };
    let fresh = std::fs::metadata(&p).is_ok_and(|m| m.len() > 4 << 20);
    let file = std::fs::OpenOptions::new().create(true).append(!fresh).write(true).truncate(fresh).open(&p);
    if let Ok(mut f) = file {
        let _ = writeln!(f, "{text}");
    }
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
        let prompt = llm::user_prompt(&texts, &glossary, &job.language, &s.style);
        report(job, Event::Sending { items: batch.len(), first: texts[0].chars().take(60).collect() });
        let t0 = std::time::Instant::now();
        // The job's own language and letters.
        let rules = llm::system_prompt(&Settings { language: job.language.clone(), any_script: job.any_script, ..s.clone() });
        if job.made_with.is_none() || job.done() == 0 {
            job.made_with = Some(job::MadeWith::of(s));
        }
        let asked = llm::chat(s, &rules, &prompt);
        log_request(
            path,
            &format!(
                "===== {} (unix s) | model {} | {} texts\n--- system\n{rules}\n--- user\n{prompt}\n--- answer\n{}\n",
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
                s.model,
                batch.len(),
                match &asked {
                    Ok(a) => a.as_str(),
                    Err(e) => e.as_str(),
                }
            ),
        );
        match asked.and_then(|a| llm::parse_answer(&a)) {
            Ok(answers) => {
                failures_in_a_row = 0;
                let taken = job.accept(&batch, &answers);
                report(job, Event::Answer { taken, of: batch.len(), seconds: t0.elapsed().as_secs_f32() });
                let refused: Vec<(String, String)> = batch
                    .iter()
                    .map(|&i| &job.entries[i])
                    .filter(|e| e.en.is_none())
                    .map(|e| (e.src.chars().take(50).collect(), e.last_error.clone()))
                    .collect();
                if !refused.is_empty() {
                    log_request(path, &format!("--- refused\n{}\n", refused.iter().map(|(t, w)| format!("{t}: {w}")).collect::<Vec<_>>().join("\n")));
                    report(job, Event::Refused(refused));
                }
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

/// Where a job's translation goes.
#[derive(Clone, Debug)]
pub enum Target {
    /// English copies of maps, in this maps folder.
    Maps(PathBuf),
    /// The data files of the job's language, next to the install's Russian ones.
    Data { install: PathBuf, data: data::DataTexts },
}

impl Target {
    /// Writes `job`'s English; returns the files written. Maps that exist are replaced only
    /// with `overwrite`; the data's files are always replaced.
    pub fn write(&self, job: &Job, overwrite: bool) -> Result<Vec<PathBuf>, String> {
        match self {
            Target::Maps(dir) => write_maps(job, dir, overwrite),
            Target::Data { install, data } => data::write(job, data, install),
        }
    }
}

/// The install's maps folder.
pub fn maps_dir(install: &Path) -> Option<PathBuf> {
    crate::dt::install::find_path(install, crate::dt::install::MAPS_DIR).ok()
}
