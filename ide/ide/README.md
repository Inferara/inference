# inference-ide - IDE API

High-level IDE API for Inference language support.

## Status

Skeleton implementation. See the LSP plan for the full feature roadmap.

## Purpose

Public API boundary for IDE features. Returns plain-old-data (POD) types only - no compiler internals exposed.

## Planned Types

```rust
/// Mutable host for applying changes
pub struct AnalysisHost {
    db: RootDatabase,
}

/// Immutable snapshot for queries (thread-safe)
pub struct Analysis {
    db: Arc<RootDatabase>,
}

impl Analysis {
    /// Get diagnostics for a file
    pub fn diagnostics(&self, file_id: FileId) -> Vec<Diagnostic>;

    /// Go to definition at position
    pub fn goto_definition(&self, pos: FilePosition) -> Option<FileRange>;

    /// Get hover information at position
    pub fn hover(&self, pos: FilePosition) -> Option<HoverResult>;

    /// Get document symbols
    pub fn document_symbols(&self, file_id: FileId) -> Vec<DocumentSymbol>;

    /// Get completions at position
    pub fn completions(&self, pos: FilePosition) -> Vec<CompletionItem>;
}
```

## Design Principles

1. **POD Return Types** - No `Rc`, `Arc`, or internal compiler types in public API
2. **Snapshot Queries** - `Analysis` is immutable; changes go through `AnalysisHost`
3. **Editor Terminology** - Uses offsets, ranges, positions (not AST node IDs)

## Dependencies

- `inference-ide-db` - Semantic database
- `inference-base-db` - Position utilities
- `inference-vfs` - File identity

## Building

```bash
cargo build -p inference-ide
```
