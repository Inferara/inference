# inference-ide-db - IDE Semantic Database

Semantic database and symbol index for Inference IDE support.

## Status

Skeleton implementation. See the LSP plan for the full feature roadmap.

## Purpose

Caches analysis results and provides symbol indexing:
- Parsed AST per file
- Type-checked results per file
- Symbol index (functions, structs, enums)
- Diagnostics collection

## Planned Types

```rust
/// Central cache of all analysis state
pub struct RootDatabase {
    files: FxHashMap<FileId, FileAnalysis>,
    symbol_index: SymbolIndex,
}

/// Analysis results for a single file
pub struct FileAnalysis {
    pub ast: Option<Arc<Ast>>,
    pub typed: Option<Arc<TypedAst>>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Index of all symbols for quick lookup
pub struct SymbolIndex {
    functions: FxHashMap<String, Vec<SymbolLocation>>,
    structs: FxHashMap<String, Vec<SymbolLocation>>,
    enums: FxHashMap<String, Vec<SymbolLocation>>,
}
```

## Dependencies

- `inference-vfs` - For `FileId`
- `inference-base-db` - For `SourceFile`, `LineIndex`
- `core/ast` - For AST types
- `core/type-checker` - For typed AST

## Building

```bash
cargo build -p inference-ide-db
```
