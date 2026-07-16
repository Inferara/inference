//! Text-positioning primitives shared across the Inference IDE stack.
//!
//! This crate holds the small, dependency-light plain-old-data types that let
//! the higher IDE layers talk about *where* something is in a file, plus the
//! [`LineIndex`] that converts between the compiler's byte offsets and the
//! line/character positions the Language Server Protocol speaks.
//!
//! # Two coordinate systems
//!
//! The compiler and the editor disagree about how to name a position:
//!
//! * The compiler's `Location` (in `core/ast`) carries byte `offset_start` /
//!   `offset_end` **and** 1-based line / 1-based *byte* column fields. The IDE
//!   stack uses only the byte **offsets**; it never reads the compiler's
//!   line/column fields, because they are 1-based and column-in-bytes, which is
//!   neither what LSP wants nor cheap to translate.
//! * LSP positions are 0-based line and 0-based **UTF-16 code unit** character.
//!
//! [`LineIndex`] is the single bridge: byte offset ⇄ [`LineCol`]. Everything
//! else here is a POD carrying a [`FileId`] alongside an offset or range.
//!
//! Keeping this crate free of compiler dependencies is deliberate: it converts
//! offsets and nothing more, so it can be reused and tested in isolation.

mod line_index;

pub use inference_vfs::FileId;
pub use line_index::LineIndex;

/// A half-open range of byte offsets within a single file, `start..end`.
///
/// `end` is exclusive. An empty range has `start == end`. Offsets are byte
/// positions into UTF-8 source text, matching the compiler's `Location`
/// offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextRange {
    /// Inclusive start byte offset.
    pub start: u32,
    /// Exclusive end byte offset.
    pub end: u32,
}

/// A 0-based line and 0-based UTF-16 code-unit character, in LSP coordinates.
///
/// `character` counts UTF-16 code units from the start of the line, so a
/// character above U+FFFF (which is a surrogate pair in UTF-16) advances it by
/// two. This is the position encoding every LSP client must support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LineCol {
    /// 0-based line number.
    pub line: u32,
    /// 0-based offset within the line, in UTF-16 code units.
    pub character: u32,
}

/// A byte offset within a specific file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FilePosition {
    /// The file the offset refers to.
    pub file_id: FileId,
    /// Byte offset into that file's text.
    pub offset: u32,
}

/// A byte range within a specific file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileRange {
    /// The file the range refers to.
    pub file_id: FileId,
    /// Byte range within that file's text.
    pub range: TextRange,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_range_is_pod_and_comparable() {
        let a = TextRange { start: 3, end: 7 };
        let b = a; // Copy
        assert_eq!(a, b);
        assert_eq!(a.start, 3);
        assert_eq!(a.end, 7);
        assert_ne!(a, TextRange { start: 3, end: 8 });
    }

    #[test]
    fn line_col_is_pod_and_comparable() {
        let a = LineCol {
            line: 2,
            character: 5,
        };
        let b = a; // Copy
        assert_eq!(a, b);
        assert_eq!(a.line, 2);
        assert_eq!(a.character, 5);
        assert_ne!(
            a,
            LineCol {
                line: 2,
                character: 6
            }
        );
    }

    #[test]
    fn file_position_and_range_carry_their_file() {
        use std::path::PathBuf;

        let mut vfs = inference_vfs::Vfs::default();
        let file = vfs.intern(&PathBuf::from("/a.inf"));

        let pos = FilePosition {
            file_id: file,
            offset: 12,
        };
        assert_eq!(
            pos,
            FilePosition {
                file_id: file,
                offset: 12
            }
        );
        assert_eq!(pos.file_id, file);
        assert_eq!(pos.offset, 12);

        let range = FileRange {
            file_id: file,
            range: TextRange { start: 1, end: 4 },
        };
        assert_eq!(range.range, TextRange { start: 1, end: 4 });
        assert_eq!(range.file_id, file);
    }
}
