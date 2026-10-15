//! Windows-1251 (cp1251) text, the encoding of every original data file and map string.

use encoding_rs::WINDOWS_1251;

/// Decode a string of a data file or map: cp1251, as the original writes them, unless it is
/// valid UTF-8 holding a character cp1251 has not (Razdor's own: [`encode_any`] writes such a
/// string in UTF-8, the file's layout unchanged). A cp1251 string from the original is all
/// but never valid UTF-8, and when it is, every character it decodes to would have had a
/// cp1251 form, so it stays cp1251. Every byte value maps to a character, so this cannot fail.
pub fn decode(bytes: &[u8]) -> String {
    if bytes.iter().any(|&b| b >= 0x80) {
        if let Ok(s) = std::str::from_utf8(bytes) {
            if s.chars().any(|c| !in_cp1251(c)) {
                return s.to_string();
            }
        }
    }
    WINDOWS_1251.decode_without_bom_handling(bytes).0.into_owned()
}

/// A character cp1251 holds.
pub fn in_cp1251(c: char) -> bool {
    // The encoder writes what it cannot map as `&#NNN;` and says so.
    !WINDOWS_1251.encode(c.encode_utf8(&mut [0; 4])).2
}

/// A map or data string as Razdor writes it: cp1251 when it can be (what the original reads),
/// else UTF-8 (Razdor's own: text in other scripts from razdor-translate; the original shows
/// such a string garbled). [`decode`] reads both.
pub fn encode_any(text: &str) -> Vec<u8> {
    if text.chars().all(in_cp1251) { encode(text) } else { text.as_bytes().to_vec() }
}

/// Encode to cp1251. A character with no cp1251 form is written as an HTML reference (`&#233;`,
/// encoding_rs's way): check text with `encoding_rs::WINDOWS_1251.encode(..).2` first.
pub fn encode(text: &str) -> Vec<u8> {
    WINDOWS_1251.encode(text).0.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_cyrillic() {
        // "Рус" in cp1251.
        assert_eq!(decode(&[0xD0, 0xF3, 0xF1]), "Рус");
        assert_eq!(decode(b"abc"), "abc");
    }

    #[test]
    fn any_script_goes_as_utf8_and_comes_back() {
        for t in ["हिन्दी", "中文", "Ελληνικά", "Größe", "Привет, мир", "plain"] {
            assert_eq!(decode(&encode_any(t)), t, "{t}");
        }
        // What cp1251 holds is written as before.
        assert_eq!(encode_any("Рус"), [0xD0, 0xF3, 0xF1]);
        // A cp1251 string that happens to be valid UTF-8 ("ВЁ" is C2 A8, U+00A8 "¨" in UTF-8)
        // would be read as UTF-8: the one case the rule gets wrong; none in the shipped maps.
    }

    /// Every string of every shipped map reads as plain cp1251 (no original string is taken
    /// for UTF-8).
    #[test]
    fn real_map_strings_stay_cp1251() {
        let Some(dir) = std::env::var_os(crate::dt::install::ENV_VAR) else { return };
        let maps = crate::dt::install::list_maps(std::path::Path::new(&dir)).unwrap();
        let mut checked = 0;
        for m in maps.iter().filter(|m| !crate::translate::job::is_translated_file(&m.name)) {
            let bytes = std::fs::read(&m.path).unwrap();
            let payload = crate::dt::container::decode(&bytes).unwrap().payload;
            let at = u32::from_le_bytes(payload[0x18..0x1C].try_into().unwrap()) as usize;
            for piece in payload[at.min(payload.len())..].split(|&b| b == 0) {
                assert_eq!(decode(piece), WINDOWS_1251.decode_without_bom_handling(piece).0, "{}", m.name);
                checked += 1;
            }
        }
        assert!(checked > 1000);
    }

    #[test]
    fn roundtrips_every_byte() {
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(encode(&decode(&all)), all);
    }
}
