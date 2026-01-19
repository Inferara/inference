# inference-vfs - Virtual File System

Virtual file system for Inference IDE support. Provides file identity and change tracking without performing disk I/O.

## Status

Skeleton implementation. See the LSP plan for the full feature roadmap.

## Design Principles

1. **No Disk I/O** - VFS only tracks state; actual file reading is done by clients
2. **Integer File IDs** - Files identified by `FileId(u32)`, not paths
3. **Change-Based Updates** - Tracks modifications via `ChangeSet`
4. **Path Abstraction** - `VfsPath` normalizes real and virtual paths

## Planned Types

```rust
/// Opaque file identifier
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct FileId(u32);

/// Abstraction over file paths (real or virtual)
pub struct VfsPath { /* ... */ }

/// Stable path-to-FileId mapping
pub struct PathInterner { /* ... */ }

/// Batched file modifications
pub struct ChangeSet { /* ... */ }

/// VFS coordinator
pub struct Vfs { /* ... */ }
```

## Architecture

This crate is the foundation of the IDE infrastructure layer:

```
apps/lsp
    |
ide/ide -> ide/ide-db -> ide/base-db -> ide/vfs
```

## Building

```bash
cargo build -p inference-vfs
```
