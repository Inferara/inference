# Linux Development Setup Guide

This guide walks you through setting up a complete development environment for the Inference project on Linux.

## Prerequisites

- Ubuntu 22.04 LTS or later (recommended), Debian 12+, or Fedora 39+
- `curl` and `git` utilities
- `sudo` access for package installation
- At least 2GB of free disk space

**Install prerequisites (Ubuntu/Debian):**
```bash
sudo apt update && sudo apt install -y curl git build-essential pkg-config
```

**Install prerequisites (Fedora):**
```bash
sudo dnf install -y curl git gcc gcc-c++ make pkg-config
```

## Step 1: Install Rust

Inference requires the Rust nightly toolchain.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the on-screen prompts (default installation is fine). Then:
```bash
source "$HOME/.cargo/env"

rustup default nightly

rustc --version
cargo --version
```

The output should show a nightly version, e.g., `rustc 1.xx.0-nightly`.

## Step 2: Clone the Repository

```bash
git clone https://github.com/Inferara/inference.git
cd inference
```

## Step 3: Build the Project

```bash
cargo build
```

For optimized builds:
```bash
cargo build --release
```

## Step 4: Verify the Build

Run tests:
```bash
cargo test
```

Run the CLI (either `infs` or `infc`):
```bash
./target/debug/infs --help
./target/debug/infc --help
```

Compile a sample file:
```bash
echo 'fn main() -> i32 { return 42; }' > test.inf
./target/debug/infs build test.inf --parse --codegen -o
ls -la out/
```

## Troubleshooting

### Slow compilation

- First build is expected to take several minutes
- Subsequent builds use incremental compilation
- Use `cargo build --release` only when needed

## LLVM Legacy Setup

If you are working with [v0.0.1-beta.3](https://github.com/Inferara/inference/releases/tag/v0.0.1-beta.3) or earlier releases that require LLVM, inf-llc, and rust-lld, see the [LLVM Legacy Setup Guide](archive/llvm-legacy-setup.md). The GCP-hosted binaries remain available for these older versions.

## Additional Resources

- [Rust Book](https://doc.rust-lang.org/book/)

## Getting Help

If you encounter issues not covered in this guide:
1. Check existing [GitHub issues](https://github.com/Inferara/inference/issues)
2. Run `cargo build --verbose` for detailed error messages
3. Open a new issue with your error output and environment details:
   ```bash
   uname -a
   lsb_release -a 2>/dev/null || cat /etc/os-release
   rustc --version
   ```
