# inference-base-db - Base Database

Source file handling and position utilities for Inference IDE support.

## Status

Skeleton implementation. See the LSP plan for the full feature roadmap.

## Purpose

Provides the foundation for source-level operations:
- Source file content management
- Line/column to byte offset conversion
- Position and range utilities for diagnostics

## Planned Types

```rust
/// A source file with content and line index
pub struct SourceFile {
    pub id: FileId,
    pub content: Arc<str>,
    pub line_index: LineIndex,
}

/// Efficient line/column <-> offset conversion
pub struct LineIndex { /* ... */ }

/// A position in a file (line, column)
pub struct FilePosition {
    pub file_id: FileId,
    pub offset: TextSize,
}

/// A range in a file
pub struct FileRange {
    pub file_id: FileId,
    pub range: TextRange,
}
```

## Dependencies

- `inference-vfs` - For `FileId` and `ChangeSet`

## Building

```bash
cargo build -p inference-base-db
```
