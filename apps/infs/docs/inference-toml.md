# Inference.toml Manifest Format

This document describes the `Inference.toml` project manifest format used by Inference projects.

## Overview

Every Inference project contains an `Inference.toml` file in its root directory. This manifest describes the project metadata, dependencies, build configuration, and verification settings.

The manifest uses the [TOML](https://toml.io/) format for human-readable configuration.

## File Location

```
myproject/
├── Inference.toml    ← Project manifest
├── src/
│   └── main.inf
└── proofs/
```

## Basic Structure

```toml
[package]
name = "myproject"
version = "0.1.0"
infc_version = "0.1.0"

[dependencies]
# Future: package dependencies

[build]
target = "wasm32"
optimize = "debug"

[verification]
output-dir = "proofs/"
```

## Section Reference

### [package]

The `[package]` section defines project metadata.

#### Required Fields

- **`name`** (string): The project name
  - Must start with a letter or underscore
  - Can contain letters, numbers, underscores, and hyphens
  - Cannot be a reserved keyword (e.g., `fn`, `let`, `struct`)
  - Cannot be a reserved directory name (e.g., `src`, `target`, `out`)

- **`version`** (string): The project version in [semver](https://semver.org/) format
  - Example: `"0.1.0"`, `"1.2.3"`

- **`infc_version`** (string): The compiler version used to create this project
  - Automatically detected from `infc --version` when running `infs new` or `infs init`
  - Falls back to the `infs` version if `infc` is not available
  - Example: `"0.1.0"`

#### Optional Fields

- **`description`** (string): A brief project description
  - Example: `"A compiler for mission-critical applications"`

- **`authors`** (array of strings): List of project authors
  - Example: `["Alice <alice@example.com>", "Bob <bob@example.com>"]`

- **`license`** (string): The project license identifier
  - Example: `"MIT"`, `"Apache-2.0"`, `"GPL-3.0"`

#### Example

```toml
[package]
name = "my-inference-app"
version = "1.0.0"
infc_version = "0.1.0"
description = "A verified sorting algorithm implementation"
authors = ["Alice <alice@example.com>"]
license = "MIT"
```

### [dependencies]

The `[dependencies]` section lists project dependencies.

**Status**: Reserved for future package management support.

#### Example (Future)

```toml
[dependencies]
std = "0.1"
some-lib = { version = "1.0", features = ["feature1"] }
```

### [build]

The `[build]` section configures compilation settings.

#### Fields

- **`target`** (string, default: `"wasm32"`): The compilation target platform
  - Currently supported: `"wasm32"`

- **`optimize`** (string, default: `"debug"`): The optimization level
  - `"debug"`: No optimizations, faster compilation
  - `"release"`: Full optimizations, slower compilation

#### Example

```toml
[build]
target = "wasm32"
optimize = "release"
```

### [verification]

The `[verification]` section configures Rocq (Coq) proof generation.

#### Fields

- **`output-dir`** (string, default: `"proofs/"`): The directory for generated Rocq proofs
  - Path is relative to the project root

#### Example

```toml
[verification]
output-dir = "custom-proofs/"
```

## Complete Example

```toml
[package]
name = "verified-sort"
version = "2.1.0"
infc_version = "0.1.0"
description = "A formally verified sorting algorithm"
authors = [
    "Alice Johnson <alice@example.com>",
    "Bob Smith <bob@example.com>"
]
license = "MIT"

[dependencies]
# Future: package dependencies

[build]
target = "wasm32"
optimize = "release"

[verification]
output-dir = "proofs/"
```

## Field Evolution

### Version History

#### Current Version (0.1.0)

**Package section:**
- `infc_version` (String, semver): Records the compiler version used to create the project
  - Replaces the deprecated `manifest_version` field
  - Automatically detected from `infc --version` or falls back to `infs` version

**Removed fields:**
- `manifest_version` (u32): No longer used
- `edition` (String): Removed, no longer needed

## Validation Rules

### Project Name Validation

The `name` field is validated according to these rules:

1. Cannot be empty
2. Must start with a letter (`a-z`, `A-Z`) or underscore (`_`)
3. Can only contain:
   - Letters (`a-z`, `A-Z`)
   - Numbers (`0-9`)
   - Underscores (`_`)
   - Hyphens (`-`)
4. Cannot be a reserved keyword:
   - Language keywords: `fn`, `let`, `mut`, `if`, `else`, `match`, `return`, `type`, `struct`, `impl`, `trait`, `pub`, `use`, `mod`, `assume`, `assert`, `forall`, `exists`, `unique`, etc.
   - Directory names: `src`, `out`, `target`, `proofs`, `tests`, `self`, `super`, `crate`

### Version Validation

Both `version` and `infc_version` must be valid [semantic versions](https://semver.org/):
- Format: `MAJOR.MINOR.PATCH` (e.g., `1.0.0`)
- Optional pre-release suffix (e.g., `0.1.0-alpha`)
- Cannot be empty

## Creating a New Project

### Using `infs new`

```bash
infs new myproject
```

Creates a new project with:
- `Inference.toml` manifest
- `src/main.inf` entry point
- `tests/` and `proofs/` directories
- `.gitignore` and `.gitkeep` files
- Initialized git repository

To skip git initialization:

```bash
infs new myproject --no-git
```

This creates only the core project files without `.gitignore`, `.gitkeep`, or running `git init`.

### Using `infs init`

```bash
mkdir myproject
cd myproject
infs init
```

Initializes an `Inference.toml` in an existing directory, using the directory name as the project name.

If a `.git/` directory exists, `infs init` will also create `.gitignore` and `.gitkeep` files (without overwriting existing ones).

### Custom Project Name

```bash
infs init custom-name
```

Creates an `Inference.toml` with `name = "custom-name"` regardless of the directory name.

## Compiler Version Detection

When creating a new project, the `infc_version` field is automatically populated using the following logic:

1. Try to run `infc --version` and parse the output
2. If `infc` is not found or the command fails, use the `infs` version from `CARGO_PKG_VERSION`

This ensures that the manifest always records which compiler version was used to create the project, enabling reproducible builds and compatibility tracking.

All Inference ecosystem crates (`infs`, `infc`, and `core/*` libraries) share the same version number, so using the `infs` version as a fallback is safe and accurate.

## Related Documentation

- [Project Scaffolding Guide](./project-scaffolding.md) (if exists)
- [Build Configuration](./build-config.md) (if exists)
- [Verification Workflow](./verification.md) (if exists)

## References

- [TOML Specification](https://toml.io/)
- [Semantic Versioning](https://semver.org/)
- [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
