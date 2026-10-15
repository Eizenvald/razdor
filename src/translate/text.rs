//! One map string on its way to the model and back: what must survive translation is
//! hidden behind placeholders, and the answer is checked before it is taken.
//!
//! What a map string may hold that is not prose (see `docs/reference/dtm-format.md` §7, §9 and
//! `crate::dt::markup`):
//! - line breaks: CR LF, or `#\` (the original's markup break);
//! - `#` words: `#HERONAME`, `#ARMYNAME`, archetype suffixes such as `#рыцарь`;
//! - markup marks `*`, `|`, `@`, `^`, which pick a line's font and alignment.
//!
//! The model sees CR LF as `\n`, `#\` as `<br>` and each `#` word as `{N}`; the marks are left
//! in the text and their counts must come back unchanged.

/// A source string ready for the model: `text` with `{N}` for `tokens[N]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    pub text: String,
    pub tokens: Vec<String>,
}

/// The markup marks whose counts a translation must keep.
const MARKS: [char; 4] = ['*', '|', '@', '^'];

pub fn has_cyrillic(s: &str) -> bool {
    s.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
}

/// Hides line breaks and `#` words of `src` from the model.
pub fn prepare(src: &str) -> Prepared {
    let s = src.replace("\r\n", "\n").replace("#\\", "<br>");
    let mut text = String::with_capacity(s.len());
    let mut tokens = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '#' && it.peek().is_some_and(|n| n.is_alphanumeric() || *n == '_') {
            let mut tok = String::from('#');
            while let Some(&n) = it.peek().filter(|n| n.is_alphanumeric() || **n == '_') {
                tok.push(n);
                it.next();
            }
            text.push_str(&format!("{{{}}}", tokens.len()));
            tokens.push(tok);
        } else {
            text.push(c);
        }
    }
    Prepared { text, tokens }
}

/// The model's `answer` for `src`, checked and turned back into a map string: placeholders
/// replaced, `<br>` back to `#\`, line breaks in the source's style, punctuation the game's
/// fonts may lack made plain ASCII. Errors say what the answer broke.
pub fn restore(src: &str, prepared: &Prepared, answer: &str) -> Result<String, String> {
    let answer = plain_punctuation(answer.trim_matches(|c: char| c == ' ' || c == '\t'));
    if answer.trim().is_empty() {
        return Err("empty".into());
    }
    let mut out = answer.clone();
    for (i, tok) in prepared.tokens.iter().enumerate() {
        let ph = format!("{{{i}}}");
        match out.matches(&ph).count() {
            1 => out = out.replace(&ph, tok),
            n => return Err(format!("placeholder {ph} appears {n} times")),
        }
    }
    if has_cyrillic(&strip_tokens(&out, &prepared.tokens)) {
        return Err("still has Cyrillic".into());
    }
    let count = |s: &str, p: &str| s.matches(p).count();
    if count(&answer, "<br>") != count(&prepared.text, "<br>") {
        return Err("line breaks (<br>) changed".into());
    }
    let lines = |s: &str| s.trim_end_matches('\n').matches('\n').count();
    if lines(&answer) != lines(&prepared.text) {
        return Err("line count changed".into());
    }
    for m in MARKS {
        if answer.matches(m).count() != prepared.text.matches(m).count() {
            return Err(format!("markup mark {m} changed"));
        }
    }
    if let Some(bad) = out.chars().find(|c| !c.is_ascii() && !prepared.tokens.iter().any(|t| t.contains(*c))) {
        return Err(format!("character {bad:?} the game cannot show"));
    }
    // Trailing line breaks of the source come back as they were.
    let out = out.trim_end_matches('\n').to_string() + &"\n".repeat(prepared.text.len() - prepared.text.trim_end_matches('\n').len());
    let out = out.replace("<br>", "#\\");
    Ok(if src.contains("\r\n") { out.replace('\n', "\r\n") } else { out })
}

fn strip_tokens(s: &str, tokens: &[String]) -> String {
    tokens.iter().fold(s.to_string(), |acc, t| acc.replace(t.as_str(), ""))
}

/// Typographic punctuation as plain ASCII (the original's fonts are cp1251 bitmaps; plain
/// ASCII is safe in all of them).
pub fn plain_punctuation(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{00AB}' | '\u{00BB}' => out.push('"'),
            '\u{2018}' | '\u{2019}' | '\u{201A}' => out.push('\''),
            '\u{2013}' | '\u{2014}' | '\u{2212}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            '\u{00A0}' | '\u{2009}' | '\u{202F}' => out.push(' '),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_breaks_and_hash_words() {
        let p = prepare("Привет, #HERONAME!\r\n*Конец#\\^Да");
        assert_eq!(p.text, "Привет, {0}!\n*Конец<br>^Да");
        assert_eq!(p.tokens, ["#HERONAME"]);
        let p = prepare("Страж#рыцарь и # один");
        assert_eq!(p.text, "Страж{0} и # один");
        assert_eq!(p.tokens, ["#рыцарь"]);
    }

    #[test]
    fn restores_a_good_answer_in_the_source_style() {
        let src = "Привет, #HERONAME!\r\n*Конец#\\^Да";
        let p = prepare(src);
        let got = restore(src, &p, "Hello, {0}!\n*The end<br>^Yes").unwrap();
        assert_eq!(got, "Hello, #HERONAME!\r\n*The end#\\^Yes");
        // A source without CR keeps bare line breaks; a trailing break survives.
        let src = "Строка\nдругая\n";
        assert_eq!(restore(src, &prepare(src), "Line\nanother").unwrap(), "Line\nanother\n");
    }

    #[test]
    fn rejects_broken_answers() {
        let src = "Привет, #HERONAME!\r\n*Конец";
        let p = prepare(src);
        assert!(restore(src, &p, "Hello!\n*The end").is_err(), "lost placeholder");
        assert!(restore(src, &p, "Hello, {0}! *The end").is_err(), "lost line");
        assert!(restore(src, &p, "Hello, {0}!\nThe end").is_err(), "lost mark");
        assert!(restore(src, &p, "Привет, {0}!\n*The end").is_err(), "Cyrillic left");
        assert!(restore(src, &p, "  ").is_err(), "empty");
        // A Cyrillic `#` word is kept, not counted as untranslated.
        let src = "Страж#рыцарь";
        assert_eq!(restore(src, &prepare(src), "Guard{0}").unwrap(), "Guard#рыцарь");
    }

    #[test]
    fn typographic_punctuation_becomes_ascii() {
        let src = "Он сказал";
        assert_eq!(restore(src, &prepare(src), "He said: \u{201C}Go\u{201D} \u{2014} now\u{2026}").unwrap(), "He said: \"Go\" - now...");
        assert!(restore(src, &prepare(src), "He said \u{00E9}").is_err());
    }
}
