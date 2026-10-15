//! The install's own texts (unit, item and spell names and descriptions, the game's interface
//! texts) for the translator, and the English files it writes for Razdor: `Eng_Units.ini`
//! and the like next to the Russian ones, with the same sections and only the translated
//! keys. The Russian files stay as they are (the original reads only them, and unit upgrades
//! name their targets by the Russian name, 0x4e0448); Razdor reads the English ones in English
//! (`crate::dt::english`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::job::Job;
use super::text::has_cyrillic;
use crate::dt::install::{find_path, ARTEFACTS_FILE, SETTINGS_FILE, SPELLS_FILE, UNITS_FILE};

/// A data file: the Russian one, and the keys translated (`None`: every value with
/// Cyrillic in it) with whether each is a name. Its English copy is
/// [`crate::dt::english::english_file`].
pub struct DataFile {
    pub rus: &'static str,
    keys: Option<&'static [(&'static str, bool)]>,
}

const NAME_AND_TEXT: &[(&str, bool)] = &[("Name", true), ("Descript", false)];

pub const FILES: [DataFile; 4] = [
    DataFile { rus: UNITS_FILE, keys: Some(NAME_AND_TEXT) },
    DataFile { rus: ARTEFACTS_FILE, keys: Some(NAME_AND_TEXT) },
    DataFile { rus: SPELLS_FILE, keys: Some(&[("Name", true)]) },
    DataFile { rus: SETTINGS_FILE, keys: None },
];

/// One text of a data file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub file: usize,
    /// Its section's place among all sections of the file (a name may repeat: every spell
    /// of `Rus_Spells.ini` is `[Заклинание]`).
    pub section_no: usize,
    pub section: String,
    pub key: String,
    pub value: String,
    pub name: bool,
}

/// The install's translatable texts and the section names of each data file in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataTexts {
    pub sections: Vec<Vec<String>>,
    pub texts: Vec<Text>,
}

/// The section names of an ini text in order, and its `key=value` lines with their
/// section's place (lines before the first section are in none, as the original reads them).
pub fn ini_lines(text: &str) -> (Vec<String>, Vec<(usize, String, String)>) {
    let ini = crate::dt::ini::Ini::parse(text);
    let names = ini.sections.iter().map(|s| s.name.clone()).collect();
    let lines = ini.sections.iter().enumerate().flat_map(|(no, s)| s.entries.iter().map(move |(k, v)| (no, k.trim().to_string(), v.clone()))).collect();
    (names, lines)
}

/// The section names and translatable texts of file `file` (an index into [`FILES`]) from
/// its contents.
pub fn texts_of(file: usize, contents: &str) -> (Vec<String>, Vec<Text>) {
    let keys = FILES[file].keys;
    let (sections, lines) = ini_lines(contents);
    let texts = lines
        .into_iter()
        .filter_map(|(section_no, key, value)| {
            if !has_cyrillic(&value) {
                return None;
            }
            let name = match keys {
                Some(keys) => keys.iter().find(|(k, _)| k.eq_ignore_ascii_case(&key))?.1,
                // An interface text: short one-liners are names (labels, bonus names).
                None => value.trim().chars().count() <= 40 && !value.contains("#\\"),
            };
            Some(Text { file, section_no, section: sections[section_no].clone(), key, value, name })
        })
        .collect();
    (sections, texts)
}

/// The install's translatable texts, from the data files it has.
pub fn install_texts(install: &Path) -> DataTexts {
    let mut out = DataTexts::default();
    for (i, f) in FILES.iter().enumerate() {
        let bytes = find_path(install, f.rus).ok().and_then(|p| std::fs::read(p).ok()).unwrap_or_default();
        let (sections, texts) = texts_of(i, &crate::dt::text::decode(&bytes));
        out.sections.push(sections);
        out.texts.extend(texts);
    }
    out
}

/// The job for the install's texts: the saved one's English kept.
pub fn open_job(data: &DataTexts) -> (Job, Option<PathBuf>) {
    let mut job = Job::from_texts(data.texts.iter().map(|t| (t.value.trim(), t.name)));
    let path = data_job_path();
    if let Some(old) = path.as_deref().and_then(Job::load) {
        job.take_over(&old);
    }
    (job, path)
}

/// Where the game data's job is kept (its name cannot be a map's: maps end in `.DTm`).
pub fn data_job_path() -> Option<PathBuf> {
    Some(Job::dir()?.join("_game data.json"))
}

/// The contents of each English file: every section of the Russian file in its order (so
/// Razdor pairs them by place: names repeat), with the translated keys under it, CR LF lines,
/// cp1251. A text not translated yet is left out (Razdor then shows the Russian).
pub fn english_files(job: &Job, data: &DataTexts) -> Vec<(usize, Vec<u8>)> {
    let mut out = Vec::new();
    for (i, sections) in data.sections.iter().enumerate() {
        let mut any = false;
        let mut text = String::from("; English texts for Razdor, made by razdor-translate from the Russian file.\r\n");
        for (no, name) in sections.iter().enumerate() {
            text.push_str(&format!("\r\n[{name}]\r\n"));
            for t in data.texts.iter().filter(|t| t.file == i && t.section_no == no) {
                let core = t.value.trim();
                let Some(en) = job.english_of(core) else { continue };
                let lead = &t.value[..t.value.len() - t.value.trim_start().len()];
                let trail = &t.value[lead.len() + core.len()..];
                text.push_str(&format!("{}={lead}{en}{trail}\r\n", t.key));
                any = true;
            }
        }
        if any {
            out.push((i, crate::dt::text::encode(&text)));
        }
    }
    out
}

/// Writes the English files next to the Russian ones; returns them.
pub fn write(job: &Job, data: &DataTexts, install: &Path) -> Result<Vec<PathBuf>, String> {
    let mut written = Vec::new();
    for (i, bytes) in english_files(job, data) {
        let rus = find_path(install, FILES[i].rus).map_err(|e| e.to_string())?;
        let target = rus.with_file_name(crate::dt::english::english_file(FILES[i].rus));
        crate::editor::files::write_atomically(&target, &bytes).map_err(|e| format!("{}: {e}", target.display()))?;
        written.push(target);
    }
    Ok(written)
}

/// The names of the game data already translated (Russian, English), for the glossary of
/// map jobs: a map's texts then call a unit what its card calls it.
pub fn glossary() -> Vec<(String, String)> {
    let Some(job) = data_job_path().as_deref().and_then(Job::load) else { return Vec::new() };
    let mut seen = BTreeMap::new();
    for e in job.entries.iter().filter(|e| e.name) {
        if let Some(en) = &e.en {
            seen.entry(e.src.clone()).or_insert_with(|| en.clone());
        }
    }
    seen.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const UNITS: &str = "[Unit1]\r\nGlobalIndex=1\r\nName=Мечник\r\nDescript=Воин с мечом.#\\Крепкий.\r\nNextUnit1=Рыцарь\r\nCost=10\r\n\r\n[Empty]\r\nCost=1\r\n[Unit2]\r\nName=Рыцарь\r\nDescript= Конный воин \r\n";

    fn data_of(file: usize, contents: &str) -> DataTexts {
        let (s, texts) = texts_of(file, contents);
        let mut sections = vec![Vec::new(); FILES.len()];
        sections[file] = s;
        DataTexts { sections, texts }
    }

    #[test]
    fn takes_only_the_text_keys() {
        let (sections, t) = texts_of(0, UNITS);
        assert_eq!(sections, ["Unit1", "Empty", "Unit2"]);
        let got: Vec<(usize, &str, &str, bool)> = t.iter().map(|t| (t.section_no, t.key.as_str(), t.value.as_str(), t.name)).collect();
        assert_eq!(
            got,
            [(0, "Name", "Мечник", true), (0, "Descript", "Воин с мечом.#\\Крепкий.", false), (2, "Name", "Рыцарь", true), (2, "Descript", " Конный воин", false)],
            "NextUnit (a reference by name) and numbers are left alone; trailing blanks go, as the original reads lines"
        );
        let (_, ui) = texts_of(3, "[Buttons]\nOk=Да\nLong=Очень длинный текст, который никак не может быть просто названием кнопки\nN=5\n");
        assert_eq!(ui.iter().map(|t| t.name).collect::<Vec<_>>(), [true, false]);
    }

    #[test]
    fn writes_every_section_in_place_so_razdor_pairs_them() {
        let data = data_of(2, "[Заклинание]\nName=Лечение\n[Заклинание]\nName=Молния\n");
        let mut job = Job::from_texts(data.texts.iter().map(|t| (t.value.trim(), t.name)));
        let batch: Vec<usize> = (0..job.entries.len()).collect();
        let answers: HashMap<String, String> = [("1", "Healing"), ("2", "Lightning")].map(|(k, v)| (k.to_string(), v.to_string())).into();
        assert_eq!(job.accept(&batch, &answers), 2);
        let files = english_files(&job, &data);
        let text = crate::dt::text::decode(&files[0].1);
        assert!(text.ends_with("\r\n[Заклинание]\r\nName=Healing\r\n\r\n[Заклинание]\r\nName=Lightning\r\n"), "{text:?}");
    }

    #[test]
    fn writes_the_english_file_by_section() {
        let data = data_of(0, UNITS);
        let mut job = Job::from_texts(data.texts.iter().map(|t| (t.value.trim(), t.name)));
        let batch: Vec<usize> = (0..job.entries.len()).collect();
        let en = |s: &str| match s {
            "Мечник" => "Swordsman",
            "Рыцарь" => "Knight",
            "Воин с мечом.#\\Крепкий." => "A warrior with a sword.<br>Sturdy.",
            _ => "A mounted warrior",
        };
        let answers: HashMap<String, String> = batch.iter().map(|&i| ((i + 1).to_string(), en(&job.entries[i].src).to_string())).collect();
        assert_eq!(job.accept(&batch, &answers), 4);
        let files = english_files(&job, &data);
        assert_eq!(files.len(), 1);
        let text = crate::dt::text::decode(&files[0].1);
        assert!(
            text.ends_with("\r\n[Unit1]\r\nName=Swordsman\r\nDescript=A warrior with a sword.#\\Sturdy.\r\n\r\n[Empty]\r\n\r\n[Unit2]\r\nName=Knight\r\nDescript= A mounted warrior\r\n"),
            "{text:?}"
        );
    }
}
