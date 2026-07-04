//! Byte offset ⇄ line/UTF-16-column conversion for a single file's text.

use crate::LineCol;

/// A precomputed index over a file's text that converts between byte offsets
/// and LSP-style [`LineCol`] positions (0-based line, 0-based UTF-16 column).
///
/// # Line splitting
///
/// Lines are split on the LSP 3.17 end-of-line set — `'\n'`, `'\r\n'`, and a lone
/// `'\r'` — so this index's line count always matches a conformant client's. The
/// terminator is never part of a line's content: `"a\r\nb"` has two lines
/// starting at byte offsets `[0, 3]` (line 0 is `"a"`, line 1 is `"b"`), and
/// `"a\rb"` likewise splits at the bare `'\r'` into `"a"` and `"b"`. A trailing
/// terminator produces a final empty line.
///
/// # Why it holds the text
///
/// UTF-16 column arithmetic needs to inspect the actual characters on a line
/// (a char above U+FFFF counts as two UTF-16 units), so the index keeps an owned
/// copy of the text and scans the relevant line on demand. This duplicates the
/// source bytes already held elsewhere as an `Arc<str>`, a deliberate v1
/// trade-off of memory for a self-contained, obviously-correct conversion.
///
/// # Relationship to the compiler's `Location`
///
/// The compiler's `Location` (in `core/ast`) reports 1-based lines and 1-based
/// *byte* columns. Those fields are never consumed here. The IDE stack feeds
/// this index a byte **offset** (`Location::offset_start` / `offset_end`) and
/// receives a 0-based, UTF-16-column [`LineCol`] suitable for LSP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineIndex {
    /// Owned copy of the file text, scanned during conversion.
    text: Box<str>,
    /// Byte offset at which each line starts. Always begins with `0`; a new entry
    /// is pushed for the byte immediately following every line terminator in the
    /// LSP 3.17 set (`'\n'`, `'\r\n'`, and a lone `'\r'`).
    line_starts: Vec<u32>,
}

impl LineIndex {
    /// Builds the index for `text`, recording the byte offset of every line
    /// start.
    #[must_use = "constructing a LineIndex is pointless if it is discarded"]
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0u32];
        let bytes = text.as_bytes();
        for (byte_index, &byte) in bytes.iter().enumerate() {
            // Split on the LSP 3.17 EOL set: after every `'\n'`, and after a
            // `'\r'` that is not immediately followed by `'\n'` (so a `'\r\n'`
            // pair counts once, at its `'\n'`).
            let is_terminator = match byte {
                b'\n' => true,
                b'\r' => bytes.get(byte_index + 1) != Some(&b'\n'),
                _ => false,
            };
            if is_terminator {
                line_starts.push((byte_index + 1) as u32);
            }
        }
        Self {
            text: text.into(),
            line_starts,
        }
    }

    /// Converts a byte `offset` into its [`LineCol`].
    ///
    /// An `offset` past the end of the text clamps to the end position. An
    /// `offset` that falls inside a multi-byte character rounds down to that
    /// character's start.
    #[must_use = "the converted position is the reason to call this"]
    pub fn line_col(&self, offset: u32) -> LineCol {
        let mut offset = (offset as usize).min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }

        let line = self
            .line_starts
            .partition_point(|&start| start as usize <= offset)
            - 1;
        let line_start = self.line_starts[line] as usize;
        let character = self.text[line_start..offset]
            .chars()
            .map(|ch| ch.len_utf16() as u32)
            .sum();

        LineCol {
            line: line as u32,
            character,
        }
    }

    /// Converts a [`LineCol`] into a byte offset.
    ///
    /// Returns `None` only when `line_col.line` is out of range. A `character`
    /// past the end of the line clamps to the line's end offset (the LSP rule:
    /// "if the character value is greater than the line length it defaults back
    /// to the line length"). A `character` that lands inside a character rounds
    /// down to that character's start.
    #[must_use = "the converted offset is the reason to call this"]
    pub fn offset(&self, line_col: LineCol) -> Option<u32> {
        let line = line_col.line as usize;
        let line_start = *self.line_starts.get(line)? as usize;
        let line_end = self.line_end(line);
        let line_text = &self.text[line_start..line_end];

        let mut utf16_col = 0u32;
        let mut byte_offset = line_start as u32;
        for ch in line_text.chars() {
            let ch_units = ch.len_utf16() as u32;
            if line_col.character < utf16_col + ch_units {
                return Some(byte_offset);
            }
            utf16_col += ch_units;
            byte_offset += ch.len_utf8() as u32;
        }
        Some(line_end as u32)
    }

    /// Exclusive byte offset of the end of a line's content.
    ///
    /// For every line but the last this excludes the whole line terminator: two
    /// bytes for `'\r\n'`, one for a lone `'\n'` or `'\r'`. The last line ends at
    /// the text length.
    fn line_end(&self, line: usize) -> usize {
        let Some(&next_start) = self.line_starts.get(line + 1) else {
            return self.text.len();
        };
        let next_start = next_start as usize;
        let bytes = self.text.as_bytes();
        if next_start >= 2 && bytes[next_start - 2] == b'\r' && bytes[next_start - 1] == b'\n' {
            next_start - 2
        } else {
            next_start - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lc(line: u32, character: u32) -> LineCol {
        LineCol { line, character }
    }

    #[test]
    fn empty_text_is_one_empty_line() {
        let index = LineIndex::new("");
        assert_eq!(index.line_col(0), lc(0, 0));
        // Past EOF clamps to the sole position.
        assert_eq!(index.line_col(5), lc(0, 0));
        assert_eq!(index.offset(lc(0, 0)), Some(0));
        // Character past the (empty) line clamps to line end.
        assert_eq!(index.offset(lc(0, 9)), Some(0));
        // Line out of range.
        assert_eq!(index.offset(lc(1, 0)), None);
    }

    #[test]
    fn single_line_no_newline() {
        let index = LineIndex::new("hello");
        // First and last positions of the line.
        assert_eq!(index.line_col(0), lc(0, 0));
        assert_eq!(index.line_col(5), lc(0, 5));
        assert_eq!(index.line_col(3), lc(0, 3));
        // offset == len maps to the end.
        assert_eq!(index.line_col(5), lc(0, 5));
        // offset > len clamps to end.
        assert_eq!(index.line_col(100), lc(0, 5));

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        assert_eq!(index.offset(lc(0, 5)), Some(5));
        // Character past line end clamps to line end.
        assert_eq!(index.offset(lc(0, 99)), Some(5));
        // Line out of range.
        assert_eq!(index.offset(lc(1, 0)), None);
    }

    #[test]
    fn trailing_newline_makes_empty_last_line() {
        let index = LineIndex::new("abc\n");
        // Line starts: [0, 4].
        assert_eq!(index.line_col(0), lc(0, 0));
        assert_eq!(index.line_col(3), lc(0, 3)); // the '\n' byte -> end of line 0
        assert_eq!(index.line_col(4), lc(1, 0)); // EOF -> start of empty line 1

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        assert_eq!(index.offset(lc(0, 3)), Some(3)); // end of line 0 content
        assert_eq!(index.offset(lc(0, 50)), Some(3)); // clamp to line 0 end
        assert_eq!(index.offset(lc(1, 0)), Some(4)); // empty last line
        assert_eq!(index.offset(lc(2, 0)), None); // out of range
    }

    #[test]
    fn multiple_lines_first_and_last_positions() {
        let index = LineIndex::new("ab\ncd\nef");
        // Line starts: [0, 3, 6].
        // Line 0 = "ab", line 1 = "cd", line 2 = "ef".
        assert_eq!(index.line_col(0), lc(0, 0));
        assert_eq!(index.line_col(2), lc(0, 2));
        assert_eq!(index.line_col(3), lc(1, 0));
        assert_eq!(index.line_col(5), lc(1, 2));
        assert_eq!(index.line_col(6), lc(2, 0));
        assert_eq!(index.line_col(8), lc(2, 2));

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        assert_eq!(index.offset(lc(0, 2)), Some(2));
        assert_eq!(index.offset(lc(1, 0)), Some(3));
        assert_eq!(index.offset(lc(1, 2)), Some(5));
        assert_eq!(index.offset(lc(2, 0)), Some(6));
        assert_eq!(index.offset(lc(2, 2)), Some(8));
    }

    #[test]
    fn consecutive_newlines_are_empty_lines() {
        let index = LineIndex::new("\n\n");
        // Line starts: [0, 1, 2]; three lines, all empty.
        assert_eq!(index.line_col(0), lc(0, 0));
        assert_eq!(index.line_col(1), lc(1, 0));
        assert_eq!(index.line_col(2), lc(2, 0));

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        assert_eq!(index.offset(lc(1, 0)), Some(1));
        assert_eq!(index.offset(lc(2, 0)), Some(2));
        assert_eq!(index.offset(lc(3, 0)), None);
        // Any character on an empty line clamps to its start.
        assert_eq!(index.offset(lc(0, 4)), Some(0));
        assert_eq!(index.offset(lc(1, 4)), Some(1));
    }

    #[test]
    fn crlf_is_a_single_line_terminator() {
        let index = LineIndex::new("a\r\nb");
        // Line starts: [0, 3]. The '\r\n' is one terminator, excluded from
        // content: line 0 = "a", line 1 = "b".
        assert_eq!(index.line_col(0), lc(0, 0)); // 'a'
        assert_eq!(index.line_col(3), lc(1, 0)); // 'b'
        assert_eq!(index.line_col(4), lc(1, 1)); // EOF

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        // A character past 'a' clamps to byte 1, where the '\r\n' terminator
        // begins (the terminator itself is not addressable content).
        assert_eq!(index.offset(lc(0, 1)), Some(1));
        assert_eq!(index.offset(lc(0, 9)), Some(1));
        assert_eq!(index.offset(lc(1, 0)), Some(3)); // 'b'
        assert_eq!(index.offset(lc(1, 1)), Some(4)); // end of "b"
    }

    #[test]
    fn lone_carriage_return_starts_a_new_line() {
        // Classic-Mac line ending: a bare '\r' is a line break in the LSP 3.17
        // EOL set, so a conformant client sees two lines and addresses 'b' at
        // line 1 — the index must agree, not report every later line off by one.
        let index = LineIndex::new("a\rb");
        // Line starts: [0, 2]. Line 0 = "a", line 1 = "b".
        assert_eq!(index.line_col(0), lc(0, 0)); // 'a'
        assert_eq!(index.line_col(2), lc(1, 0)); // 'b' — the byte after the '\r'
        assert_eq!(index.line_col(3), lc(1, 1)); // EOF

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        assert_eq!(index.offset(lc(0, 1)), Some(1)); // clamp to line 0 content end
        assert_eq!(index.offset(lc(1, 0)), Some(2)); // 'b' resolves, not dead
        assert_eq!(index.offset(lc(1, 1)), Some(3)); // end of "b"
    }

    #[test]
    fn consecutive_lone_carriage_returns_are_empty_lines() {
        let index = LineIndex::new("\r\r");
        // Line starts: [0, 1, 2]; three lines, all empty.
        assert_eq!(index.line_col(0), lc(0, 0));
        assert_eq!(index.line_col(1), lc(1, 0));
        assert_eq!(index.line_col(2), lc(2, 0));

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        assert_eq!(index.offset(lc(1, 0)), Some(1));
        assert_eq!(index.offset(lc(2, 0)), Some(2));
        assert_eq!(index.offset(lc(3, 0)), None);
    }

    #[test]
    fn two_byte_char_utf16_is_one_unit() {
        // "aéb": a=0, é=bytes 1..3 (0xC3 0xA9), b=3. len = 4.
        let index = LineIndex::new("aéb");
        assert_eq!(index.line_col(0), lc(0, 0)); // 'a'
        assert_eq!(index.line_col(1), lc(0, 1)); // 'é' start
        assert_eq!(index.line_col(2), lc(0, 1)); // inside 'é' -> rounds down to its start
        assert_eq!(index.line_col(3), lc(0, 2)); // 'b'
        assert_eq!(index.line_col(4), lc(0, 3)); // EOF

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        assert_eq!(index.offset(lc(0, 1)), Some(1)); // 'é'
        assert_eq!(index.offset(lc(0, 2)), Some(3)); // 'b'
        assert_eq!(index.offset(lc(0, 3)), Some(4)); // end
        assert_eq!(index.offset(lc(0, 99)), Some(4)); // clamp to line end
    }

    #[test]
    fn three_byte_char_utf16_is_one_unit() {
        // "∀x": ∀ = bytes 0..3 (U+2200), x = 3. len = 4.
        let index = LineIndex::new("∀x");
        assert_eq!(index.line_col(0), lc(0, 0)); // '∀' start
        assert_eq!(index.line_col(1), lc(0, 0)); // inside '∀'
        assert_eq!(index.line_col(2), lc(0, 0)); // inside '∀'
        assert_eq!(index.line_col(3), lc(0, 1)); // 'x'
        assert_eq!(index.line_col(4), lc(0, 2)); // EOF

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        assert_eq!(index.offset(lc(0, 1)), Some(3)); // 'x'
        assert_eq!(index.offset(lc(0, 2)), Some(4)); // end
    }

    #[test]
    fn four_byte_char_is_a_surrogate_pair() {
        // "😀!": 😀 = bytes 0..4 (U+1F600, two UTF-16 units), ! = 4. len = 5.
        let index = LineIndex::new("😀!");
        assert_eq!(index.line_col(0), lc(0, 0)); // '😀' start
        assert_eq!(index.line_col(1), lc(0, 0)); // inside '😀'
        assert_eq!(index.line_col(2), lc(0, 0)); // inside '😀'
        assert_eq!(index.line_col(3), lc(0, 0)); // inside '😀'
        assert_eq!(index.line_col(4), lc(0, 2)); // '!' -> after the surrogate pair
        assert_eq!(index.line_col(5), lc(0, 3)); // EOF

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        // Character 1 lands in the middle of the surrogate pair -> char start.
        assert_eq!(index.offset(lc(0, 1)), Some(0));
        assert_eq!(index.offset(lc(0, 2)), Some(4)); // '!'
        assert_eq!(index.offset(lc(0, 3)), Some(5)); // end
        assert_eq!(index.offset(lc(0, 50)), Some(5)); // clamp to line end
    }

    #[test]
    fn astral_math_letter_surrogate_pair() {
        // "𝔘y": 𝔘 = U+1D518, bytes 0..4, two UTF-16 units; y = 4. len = 5.
        let index = LineIndex::new("𝔘y");
        assert_eq!(index.line_col(0), lc(0, 0));
        assert_eq!(index.line_col(4), lc(0, 2)); // 'y' after the pair
        assert_eq!(index.line_col(5), lc(0, 3)); // EOF

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        assert_eq!(index.offset(lc(0, 1)), Some(0)); // mid surrogate -> char start
        assert_eq!(index.offset(lc(0, 2)), Some(4)); // 'y'
    }

    #[test]
    fn multibyte_across_multiple_lines() {
        // "é∀\n😀b": line 0 = "é∀", line 1 = "😀b".
        // Bytes: é(0..2), ∀(2..5), \n(5), 😀(6..10), b(10). len = 11.
        // Line starts: [0, 6].
        let index = LineIndex::new("é∀\n😀b");

        // Line 0.
        assert_eq!(index.line_col(0), lc(0, 0)); // 'é'
        assert_eq!(index.line_col(2), lc(0, 1)); // '∀'
        assert_eq!(index.line_col(5), lc(0, 2)); // '\n' -> end of line 0 content
        // Line 1.
        assert_eq!(index.line_col(6), lc(1, 0)); // '😀'
        assert_eq!(index.line_col(10), lc(1, 2)); // 'b' after the surrogate pair
        assert_eq!(index.line_col(11), lc(1, 3)); // EOF

        assert_eq!(index.offset(lc(0, 0)), Some(0));
        assert_eq!(index.offset(lc(0, 1)), Some(2)); // '∀'
        assert_eq!(index.offset(lc(0, 2)), Some(5)); // clamp to line 0 end
        assert_eq!(index.offset(lc(1, 0)), Some(6)); // '😀'
        assert_eq!(index.offset(lc(1, 2)), Some(10)); // 'b'
        assert_eq!(index.offset(lc(1, 3)), Some(11)); // end of file
    }

    #[test]
    fn round_trip_at_every_char_boundary() {
        // Every char-boundary offset must survive line_col -> offset unchanged.
        // (Interior bytes of a multi-byte '\r\n' terminator are not addressable
        // positions, so CRLF sources are exercised by their own tests instead.)
        for source in ["hello", "a\rb", "é∀\n😀b", "\n\n", "\r\r", "abc\n", "😀!"] {
            let index = LineIndex::new(source);
            for offset in 0..=source.len() {
                if !source.is_char_boundary(offset) {
                    continue;
                }
                let position = index.line_col(offset as u32);
                assert_eq!(
                    index.offset(position),
                    Some(offset as u32),
                    "round trip failed for {source:?} at offset {offset}"
                );
            }
        }
    }
}
