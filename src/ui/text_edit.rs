//! Editing a text as a normal edit box does (Razdor's own; the original's edit box keeps
//! only a caret at the end, see `docs/reference/original-mechanics/interface.md` §5): a
//! caret and a selection over a `String`, counted in characters. Pure logic, no drawing and
//! no input: `widgets::edit_keys` feeds it the keys, the fields draw it.

use std::ops::Range;

/// The caret and the selection's other end (`anchor`), in characters. Nothing is selected
/// when they are equal. Positions past the text's end stand for its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub caret: usize,
    pub anchor: usize,
}

impl Default for TextEdit {
    fn default() -> TextEdit {
        TextEdit::at_end()
    }
}

/// The byte offset of character `i` of `s` (its length past the end).
fn byte(s: &str, i: usize) -> usize {
    s.char_indices().nth(i).map_or(s.len(), |(b, _)| b)
}

/// `text` as it may go into a field: line breaks kept in a multi-line one (Windows' CR LF
/// as one), else made spaces; tabs made spaces; other control characters dropped.
pub fn clean(text: &str, multiline: bool) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = if multiline { text } else { text.trim_end_matches('\n').to_string() };
    text.chars()
        .filter_map(|c| match c {
            '\n' if multiline => Some('\n'),
            '\n' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

impl TextEdit {
    /// The caret at the end, nothing selected.
    pub const fn at_end() -> TextEdit {
        TextEdit { caret: usize::MAX, anchor: usize::MAX }
    }

    /// The whole of `s` selected, the caret at its end (the first key replaces it).
    pub fn all(s: &str) -> TextEdit {
        TextEdit { caret: s.chars().count(), anchor: 0 }
    }

    /// Keeps the caret and the anchor inside `s`.
    pub fn fit(&mut self, s: &str) {
        let n = s.chars().count();
        self.caret = self.caret.min(n);
        self.anchor = self.anchor.min(n);
    }

    /// The selected characters (empty at the caret when nothing is).
    pub fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    pub fn has_selection(&self) -> bool {
        self.caret != self.anchor
    }

    /// The selection's bytes in `s`.
    pub fn selected_bytes(&self, s: &str) -> Range<usize> {
        let r = self.selection();
        byte(s, r.start)..byte(s, r.end)
    }

    /// The selected text of `s`.
    pub fn selected<'a>(&self, s: &'a str) -> &'a str {
        &s[self.selected_bytes(s)]
    }

    pub fn select_all(&mut self, s: &str) {
        *self = TextEdit::all(s);
    }

    /// Puts `text` in place of the selection (at the caret when nothing is selected), cut to
    /// what `max_chars` leaves room for; the caret after it. True if `s` changed.
    pub fn replace(&mut self, s: &mut String, text: &str, max_chars: usize) -> bool {
        self.fit(s);
        let r = self.selection();
        let kept = s.chars().count() - r.len();
        let text: String = text.chars().take(max_chars.saturating_sub(kept)).collect();
        if r.is_empty() && text.is_empty() {
            return false;
        }
        let bytes = byte(s, r.start)..byte(s, r.end);
        let changed = s[bytes.clone()] != text;
        s.replace_range(bytes, &text);
        self.caret = r.start + text.chars().count();
        self.anchor = self.caret;
        changed
    }

    /// Typed characters (control characters are not text): they replace the selection.
    pub fn type_chars(&mut self, s: &mut String, chars: &[char], max_chars: usize) -> bool {
        let text: String = chars.iter().filter(|c| !c.is_control()).collect();
        !text.is_empty() && self.replace(s, &text, max_chars)
    }

    /// Pasted text, [`clean`]ed for the field: it replaces the selection.
    pub fn paste(&mut self, s: &mut String, text: &str, max_chars: usize, multiline: bool) -> bool {
        let text = clean(text, multiline);
        !text.is_empty() && self.replace(s, &text, max_chars)
    }

    /// Backspace: deletes the selection, else the character before the caret.
    pub fn backspace(&mut self, s: &mut String) -> bool {
        self.fit(s);
        if !self.has_selection() {
            if self.caret == 0 {
                return false;
            }
            self.anchor = self.caret - 1;
        }
        self.replace(s, "", usize::MAX)
    }

    /// Delete: deletes the selection, else the character after the caret.
    pub fn delete(&mut self, s: &mut String) -> bool {
        self.fit(s);
        if !self.has_selection() {
            if self.caret >= s.chars().count() {
                return false;
            }
            self.anchor = self.caret + 1;
        }
        self.replace(s, "", usize::MAX)
    }

    /// Cuts the selection out of `s` and gives it (None when nothing is selected).
    pub fn cut(&mut self, s: &mut String) -> Option<String> {
        self.fit(s);
        if !self.has_selection() {
            return None;
        }
        let taken = self.selected(s).to_string();
        self.replace(s, "", usize::MAX);
        Some(taken)
    }

    /// Moves the caret to `to`; with `extend` (Shift) the selection grows, else it is
    /// dropped.
    pub fn move_to(&mut self, s: &str, to: usize, extend: bool) {
        self.caret = to.min(s.chars().count());
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Left: one character back, or to the selection's start when one is dropped.
    pub fn left(&mut self, s: &str, extend: bool) {
        self.fit(s);
        let to = if self.has_selection() && !extend { self.selection().start } else { self.caret.saturating_sub(1) };
        self.move_to(s, to, extend);
    }

    /// Right: one character on, or to the selection's end when one is dropped.
    pub fn right(&mut self, s: &str, extend: bool) {
        self.fit(s);
        let to = if self.has_selection() && !extend { self.selection().end } else { self.caret + 1 };
        self.move_to(s, to, extend);
    }

    pub fn home(&mut self, s: &str, extend: bool) {
        self.move_to(s, 0, extend);
    }

    pub fn end(&mut self, s: &str, extend: bool) {
        self.move_to(s, usize::MAX, extend);
    }
}

/// The caret positions of `s` (one more than its characters) from its left edge, with
/// `advance` each character's width.
pub fn caret_xs(s: &str, advance: impl Fn(char) -> f32) -> Vec<f32> {
    let mut xs = vec![0.0];
    let mut x = 0.0;
    for c in s.chars() {
        x += advance(c);
        xs.push(x);
    }
    xs
}

/// The caret position nearest to `x` (from the text's left edge) over `xs` ([`caret_xs`]).
pub fn index_at(xs: &[f32], x: f32) -> usize {
    let mut best = 0;
    for (i, &at) in xs.iter().enumerate() {
        if (at - x).abs() < (xs[best] - x).abs() {
            best = i;
        }
    }
    best
}

/// The first character drawn of a line too long for `room` so that the caret stays in
/// sight: the whole line from its start when it fits, else as much before the caret as
/// fits (with the caret at the end, the line's end is shown).
pub fn first_shown(xs: &[f32], caret: usize, room: f32) -> usize {
    let caret = caret.min(xs.len().saturating_sub(1));
    if xs.last().copied().unwrap_or(0.0) <= room {
        return 0;
    }
    (0..=caret).find(|&i| xs[caret] - xs[i] <= room).unwrap_or(caret)
}

/// The rows of a multi-line text `width` wide, as character ranges of `s`: a line break
/// ends a row (it belongs to no row), a long line breaks after its last space that fits
/// (the space stays at the row's end), a word longer than a row is broken anywhere.
pub fn wrap_rows(s: &str, width: f32, advance: impl Fn(char) -> f32) -> Vec<Range<usize>> {
    let chars: Vec<char> = s.chars().collect();
    let widths: Vec<f32> = chars.iter().map(|&c| advance(c)).collect();
    let mut rows = Vec::new();
    let mut start = 0;
    while start <= chars.len() {
        let line_end = chars[start..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| start + p);
        let mut row = start;
        loop {
            // The most characters from `row` that fit.
            let (mut end, mut used) = (row, 0.0);
            while end < line_end && used + widths[end] <= width {
                used += widths[end];
                end += 1;
            }
            if end >= line_end {
                rows.push(row..line_end);
                break;
            }
            let end = if end == row {
                row + 1
            } else {
                // Break after the last space in the row, when there is one.
                chars[row..end].iter().rposition(|&c| c == ' ').map_or(end, |p| row + p + 1)
            };
            rows.push(row..end);
            row = end;
        }
        start = line_end + 1;
    }
    rows
}

/// The row of `rows` ([`wrap_rows`]) the caret `at` is drawn on: the last row starting at
/// or before it.
pub fn row_of(rows: &[Range<usize>], at: usize) -> usize {
    rows.iter().rposition(|r| r.start <= at).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character 10 wide.
    fn w(_: char) -> f32 {
        10.0
    }

    #[test]
    fn typing_goes_in_at_the_caret_and_replaces_the_selection() {
        let mut s = String::from("Камень");
        let mut e = TextEdit::at_end();
        assert!(e.type_chars(&mut s, &['!'], 100));
        assert_eq!((s.as_str(), e.caret), ("Камень!", 7));
        e.move_to(&s, 2, false);
        assert!(e.type_chars(&mut s, &['р', '\u{8}'], 100), "control characters are not text");
        assert_eq!(s, "Кармень!");
        e.select_all(&s);
        assert!(e.type_chars(&mut s, &['И', 'в', 'о'], 100));
        assert_eq!((s.as_str(), e), ("Иво", TextEdit { caret: 3, anchor: 3 }));
        assert!(!e.type_chars(&mut s, &['\r'], 100), "a lone control character changes nothing");
    }

    #[test]
    fn a_field_takes_no_more_than_its_length() {
        let mut s = String::from("abcd");
        let mut e = TextEdit::at_end();
        assert!(!e.type_chars(&mut s, &['e'], 4));
        e.move_to(&s, 1, false);
        e.move_to(&s, 3, true);
        assert!(e.paste(&mut s, "XYZW", 4, false), "the selection's room is taken");
        assert_eq!(s, "aXYd", "two selected, two pasted: the rest of the paste is cut");
    }

    #[test]
    fn backspace_and_delete_take_a_character_or_the_selection() {
        let mut s = String::from("Иво");
        let mut e = TextEdit::at_end();
        assert!(e.backspace(&mut s));
        assert_eq!(s, "Ив");
        e.home(&s, false);
        assert!(!e.backspace(&mut s), "nothing before the caret");
        assert!(e.delete(&mut s));
        assert_eq!((s.as_str(), e.caret), ("в", 0));
        assert!(e.delete(&mut s));
        assert!(!e.delete(&mut s), "nothing after the caret");
        let mut s = String::from("(нет имени)");
        let mut e = TextEdit::all(&s);
        assert!(e.backspace(&mut s), "a selected default name goes at once");
        assert_eq!(s, "");
    }

    #[test]
    fn paste_and_cut_go_through_the_selection() {
        let mut s = String::from("http://localhost");
        let mut e = TextEdit::all(&s);
        assert_eq!(e.selected(&s), "http://localhost");
        assert!(e.paste(&mut s, "https://api.example.com/v1\r\n", 200, false), "a copied line's break is dropped");
        assert_eq!(s, "https://api.example.com/v1");
        e.move_to(&s, 8, false);
        e.move_to(&s, 12, true);
        assert_eq!(e.cut(&mut s).as_deref(), Some("api."));
        assert_eq!((s.as_str(), e.caret), ("https://example.com/v1", 8));
        assert_eq!(e.cut(&mut s), None, "nothing selected, nothing cut");
        let mut m = String::new();
        let mut e = TextEdit::at_end();
        assert!(e.paste(&mut m, "один\r\nдва\tтри", 100, true));
        assert_eq!(m, "один\nдва три", "a multi-line field keeps the line breaks");
        assert_eq!(clean("a\nb\n", false), "a b");
    }

    #[test]
    fn arrows_move_the_caret_and_shift_selects() {
        let s = "abcdef";
        let mut e = TextEdit::at_end();
        e.left(s, false);
        assert_eq!(e, TextEdit { caret: 5, anchor: 5 });
        e.left(s, true);
        e.left(s, true);
        assert_eq!((e.selection(), e.selected(s)), (3..5, "de"));
        e.right(s, false);
        assert_eq!(e, TextEdit { caret: 5, anchor: 5 }, "Right drops the selection at its end");
        e.home(s, true);
        assert_eq!(e.selected(s), "abcde");
        e.left(s, false);
        assert_eq!(e.caret, 0, "Left drops it at its start");
        e.end(s, false);
        e.right(s, false);
        assert_eq!(e.caret, 6, "no further than the end");
        let mut stale = TextEdit { caret: 40, anchor: 2 };
        stale.fit(s);
        assert_eq!(stale, TextEdit { caret: 6, anchor: 2 }, "a shorter text keeps the caret in it");
    }

    #[test]
    fn a_click_finds_the_nearest_caret_position() {
        let xs = caret_xs("Иво", w);
        assert_eq!(xs, vec![0.0, 10.0, 20.0, 30.0]);
        assert_eq!(index_at(&xs, -5.0), 0);
        assert_eq!(index_at(&xs, 14.0), 1);
        assert_eq!(index_at(&xs, 16.0), 2);
        assert_eq!(index_at(&xs, 99.0), 3);
    }

    #[test]
    fn a_long_line_scrolls_to_keep_the_caret_in_sight() {
        let xs = caret_xs("abcdefghij", w);
        assert_eq!(first_shown(&caret_xs("abc", w), 3, 50.0), 0, "a line that fits is shown whole");
        assert_eq!(first_shown(&xs, 10, 50.0), 5, "the caret at the end shows the end");
        assert_eq!(first_shown(&xs, 2, 50.0), 0, "near the start, the start");
        assert_eq!(first_shown(&xs, 7, 50.0), 2);
    }

    #[test]
    fn multi_line_text_wraps_by_character_ranges() {
        let s = "один два три\n\nконец";
        let rows = wrap_rows(s, 90.0, w);
        let text: Vec<String> = rows.iter().map(|r| s.chars().skip(r.start).take(r.len()).collect()).collect();
        assert_eq!(text, ["один два ", "три", "", "конец"]);
        assert_eq!(rows[1], 9..12);
        assert_eq!(row_of(&rows, 9), 1, "the caret after a wrapped space starts the next row");
        assert_eq!(row_of(&rows, 12), 1, "before the line break, the line's own row");
        assert_eq!(row_of(&rows, 13), 2);
        let long = wrap_rows("abcdefghijkl", 50.0, w);
        assert_eq!(long, vec![0..5, 5..10, 10..12], "a word longer than a row breaks anywhere");
        assert_eq!(wrap_rows("", 50.0, w), vec![0..0], "an empty text has its one row");
    }
}
