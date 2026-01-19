# inference-lsp - Inference Language Server

Language Server Protocol implementation for the Inference programming language.

## Status

Skeleton implementation. See the LSP plan for the full feature roadmap.

## Planned Features

### Phase 1: Diagnostics
- Parse error reporting
- Type error reporting
- File synchronization (open/change/close)

### Phase 2: Core IDE Features
- Go to definition
- Hover information
- Document symbols

### Phase 3: Advanced Features
- Completions
- Find references
- Rename symbol
- Semantic tokens

### Custom LSP Extensions
- `inference/viewOutput` - Live WAT/Rocq output
- `inference/viewNondet` - Non-deterministic block visualization

## Architecture

This crate is a thin binary wrapper using `tower-lsp` that orchestrates:
- `ide/ide` - High-level IDE API
- `ide/ide-db` - Semantic database
- `ide/base-db` - Source file handling
- `ide/vfs` - Virtual file system

## Building

```bash
cargo build -p inference-lsp
```
