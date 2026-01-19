# infs - Inference Unified CLI

Unified command-line interface for the Inference compiler toolchain.

## Status

Skeleton implementation. See the master plan for the full feature roadmap.

## Planned Features

- `infs build [file.inf]` - Compile Inference source files
- `infs install [version]` - Download and install toolchain versions
- `infs list` - List installed toolchain versions
- `infs doctor` - Verify installation health
- `infs new <project>` - Scaffold new projects
- `infs` (no args) - Launch TUI interface

## Architecture

This crate is a thin binary wrapper that orchestrates:
- `core/inference` - Compilation pipeline
- `ide/` crates - IDE support (future)

The CLI implements `FsFileProvider` to bridge filesystem operations with the compiler core's `FileProvider` trait.

## Building

```bash
cargo build -p infs
```
