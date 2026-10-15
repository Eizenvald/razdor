//! English texts for the install's data, made by the map translator (`razdor-translate`):
//! `Eng_Units.ini`, `Eng_Artefacts.ini`, `Eng_Spells.ini` and `Eng_DiscordTimes.ini` next to
//! the Russian files, with the same sections and only the translated keys. Not part of the
//! original game, which reads only the Russian files; Razdor uses them when its interface is
//! English (asked for by the owner).
//!
//! The Russian data is loaded as it is (unit upgrades name their targets by the Russian name,
//! 0x4e0448); only the names and descriptions shown are swapped afterwards, by the Russian
//! text they translate ([`Dictionary::english`]).

use std::collections::HashMap;
use std::path::Path;

use super::data::{ArtefactDef, SpellDef, UnitDef};
use super::ini::Ini;
use super::install::{find_path, ARTEFACTS_FILE, SETTINGS_FILE, SPELLS_FILE, UNITS_FILE};

/// The English file of a Russian data file: `Rus_Units.ini` → `Eng_Units.ini`.
pub fn english_file(rus: &str) -> String {
    match rus.split_once('_') {
        Some((_, rest)) => format!("Eng_{rest}"),
        None => format!("Eng_{rus}"),
    }
}

fn read(dir: &Path, name: &str) -> Option<Ini> {
    let bytes = std::fs::read(find_path(dir, name).ok()?).ok()?;
    Some(Ini::from_cp1251(&bytes))
}

/// The English of the install's data: by Russian text for names and descriptions, by
/// section and key for the interface texts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dictionary {
    texts: HashMap<String, String>,
    interface: Option<Ini>,
}

impl Dictionary {
    /// The English files of the install at `dir`; `None` when it has none.
    pub fn load(dir: &Path) -> Option<Dictionary> {
        let mut d = Dictionary { interface: read(dir, &english_file(SETTINGS_FILE)), ..Dictionary::default() };
        for rus in [UNITS_FILE, ARTEFACTS_FILE, SPELLS_FILE] {
            let (Some(ru), Some(en)) = (read(dir, rus), read(dir, &english_file(rus))) else { continue };
            d.add_pairs(&ru, &en);
        }
        (!d.texts.is_empty() || d.interface.is_some()).then_some(d)
    }

    /// Pairs each translated value of `en` with the value of the same key in the section at
    /// the same place in `ru` (section names repeat: every spell is `[Заклинание]`); a file
    /// whose sections do not line up is left out.
    fn add_pairs(&mut self, ru: &Ini, en: &Ini) {
        if en.sections.len() != ru.sections.len() || en.sections.iter().zip(&ru.sections).any(|(e, r)| e.name != r.name) {
            crate::diag!("english: a file's sections do not match its Russian file; it is not used");
            return;
        }
        for (s, r) in en.sections.iter().zip(&ru.sections) {
            for (k, v) in &s.entries {
                if let Some(rv) = r.get(k).filter(|rv| !rv.trim().is_empty()) {
                    self.texts.insert(rv.trim().to_string(), v.trim().to_string());
                }
            }
        }
    }

    /// The English of a Russian name or description, if translated.
    pub fn english(&self, ru: &str) -> Option<&str> {
        self.texts.get(ru.trim()).map(String::as_str)
    }

    /// The English interface text of `[section] key`, if translated.
    pub fn interface(&self, section: &str, key: &str) -> Option<&str> {
        self.interface.as_ref()?.section(section)?.get_nonempty(key)
    }

    /// `units`, `items` and `spells` with their names and descriptions in English where
    /// translated (the rest as they are).
    pub fn apply(&self, units: &mut [UnitDef], items: &mut [ArtefactDef], spells: &mut [SpellDef]) {
        let swap = |s: &mut String| {
            if let Some(en) = self.english(s) {
                *s = en.to_string();
            }
        };
        for u in units {
            swap(&mut u.name);
            swap(&mut u.description);
        }
        for a in items {
            swap(&mut a.name);
            swap(&mut a.description);
        }
        for s in spells {
            swap(&mut s.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The install's English files, when it has them (razdor-translate made them): they line
    /// up with the Russian ones and name every unit, item and spell.
    #[test]
    fn real_english_files_cover_the_data() {
        let Some(dir) = std::env::var_os(super::super::install::ENV_VAR) else { return };
        let dir = std::path::PathBuf::from(dir);
        if find_path(&dir, &english_file(UNITS_FILE)).is_err() {
            return;
        }
        let dt = super::super::install::DtInstall::load(&dir).unwrap();
        let d = dt.english.as_ref().expect("the English files load");
        let missing: Vec<&str> = dt
            .units
            .iter()
            .map(|u| u.name.as_str())
            .chain(dt.artefacts.iter().map(|a| a.name.as_str()))
            .chain(dt.spells.iter().map(|s| s.name.as_str()))
            .filter(|n| d.english(n).is_none())
            .collect();
        assert!(missing.is_empty(), "untranslated: {missing:?}");
    }

    #[test]
    fn english_file_names() {
        assert_eq!(english_file("Rus_Units.ini"), "Eng_Units.ini");
        assert_eq!(english_file("Rus_DiscordTimes.ini"), "Eng_DiscordTimes.ini");
    }

    #[test]
    fn pairs_by_section_and_key() {
        let ru = Ini::parse("[Unit1]\nName=Мечник\nDescript=Воин\nCost=5\n[Unit2]\nName=Лучник\n[S]\nName=Лечение\n[S]\nName=Молния\n");
        let en = Ini::parse("; comment\n[Unit1]\nName=Swordsman\nDescript=Warrior\n[Unit2]\n[S]\nName=Healing\n[S]\nName=Lightning\n");
        let mut d = Dictionary::default();
        d.add_pairs(&ru, &en);
        assert_eq!(d.english("Мечник"), Some("Swordsman"));
        assert_eq!(d.english(" Воин "), Some("Warrior"));
        assert_eq!(d.english("Лучник"), None, "not translated: stays Russian");
        assert_eq!((d.english("Лечение"), d.english("Молния")), (Some("Healing"), Some("Lightning")), "repeated names pair by place");
        let mut other = Dictionary::default();
        other.add_pairs(&ru, &Ini::parse("[Unit1]\nName=Swordsman\n"));
        assert_eq!(other.english("Мечник"), None, "sections out of line: not used");
    }
}
