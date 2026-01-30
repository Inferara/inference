# Linux Development Setup Guide

This guide walks you through setting up a complete development environment for the Inference project on Linux.

## Prerequisites

- Ubuntu 22.04 LTS or later (recommended), Debian 12+, or Fedora 39+
- `curl`, `wget`, and `unzip` utilities
- `sudo` access for package installation
- At least 4GB of free disk space

**Install prerequisites (Ubuntu/Debian):**
```bash
sudo apt update && sudo apt install -y curl wget unzip lsb-release software-properties-common gnupg build-essential pkg-config
```

**Install prerequisites (Fedora):**
```bash
sudo dnf install -y curl wget unzip gcc gcc-c++ make pkg-config
```

## Step 1: Install LLVM 21

LLVM 21 is required for building the Inference compiler.

### Ubuntu/Debian

```bash
wget https://apt.llvm.org/llvm.sh
chmod +x llvm.sh
sudo ./llvm.sh 21

sudo apt-get install -y llvm-21-dev libpolly-21-dev
```

Verify installation:
```bash
llvm-config-21 --version
```

### Fedora

```bash
sudo dnf install -y llvm21-devel polly21-devel
```

Verify installation:
```bash
llvm-config-21 --version
```

**Important:** LLVM 21 is specifically required. Other versions are not compatible.

## Step 2: Install Rust

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

## Step 3: Clone the Repository

```bash
git clone https://github.com/Inferara/inference.git
cd inference
```

## Step 4: Download External Binaries

The Inference compiler requires custom LLVM tools with Inference intrinsics support.

```bash
mkdir -p external/bin/linux external/lib/linux

curl -L "https://storage.googleapis.com/external_binaries/linux/bin/inf-llc.zip" -o /tmp/inf-llc.zip
unzip -o /tmp/inf-llc.zip -d external/bin/linux/

curl -L "https://storage.googleapis.com/external_binaries/linux/bin/rust-lld.zip" -o /tmp/rust-lld.zip
unzip -o /tmp/rust-lld.zip -d external/bin/linux/

curl -L "https://storage.googleapis.com/external_binaries/linux/lib/libLLVM.so.21.1-rust-1.94.0-nightly.zip" -o /tmp/libLLVM.zip
unzip -o /tmp/libLLVM.zip -d external/lib/linux/

chmod +x external/bin/linux/inf-llc external/bin/linux/rust-lld
```

**Important:** The `libLLVM.so` shared library is required on Linux for runtime operation.

Alternatively, run the dependency check script:
```bash
./book/check_deps.sh
```

## Step 5: Configure Environment Variables

Set the LLVM prefix environment variable for building:

```bash
echo 'export LLVM_SYS_211_PREFIX="/usr/lib/llvm-21"' >> ~/.bashrc
source ~/.bashrc
```

For zsh users:
```bash
echo 'export LLVM_SYS_211_PREFIX="/usr/lib/llvm-21"' >> ~/.zshrc
source ~/.zshrc
```

Verify the configuration:
```bash
echo $LLVM_SYS_211_PREFIX
```

**Note:** The project's `.cargo/config.toml` automatically configures `LD_LIBRARY_PATH` for the vendored `libLLVM.so` during builds.

## Step 6: Build the Project

```bash
cargo build
```

First build will take several minutes as it compiles all dependencies including the LLVM bindings.

For optimized builds:
```bash
cargo build --release
```

## Step 7: Verify the Build

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

### Build fails with "LLVM not found" or "llvm-sys build failed"

1. Verify LLVM 21 is installed:
   ```bash
   llvm-config-21 --version
   ```

2. Check the environment variable:
   ```bash
   echo $LLVM_SYS_211_PREFIX
   ```

3. Verify the path exists:
   ```bash
   ls -la /usr/lib/llvm-21/
   ```

### Build fails with "inf-llc not found" or "rust-lld not found"

1. Verify binaries are downloaded:
   ```bash
   ls -la external/bin/linux/
   ```

2. Check they are executable:
   ```bash
   file external/bin/linux/inf-llc
   ```

3. Make them executable if needed:
   ```bash
   chmod +x external/bin/linux/inf-llc external/bin/linux/rust-lld
   ```

### Runtime error "libLLVM.so: cannot open shared object file"

The vendored libLLVM shared library is missing or not in the library path:

1. Verify the library exists:
   ```bash
   ls -la external/lib/linux/
   ```

2. If missing, download it:
   ```bash
   curl -L "https://storage.googleapis.com/external_binaries/linux/lib/libLLVM.so.21.1-rust-1.94.0-nightly.zip" -o /tmp/libLLVM.zip
   unzip -o /tmp/libLLVM.zip -d external/lib/linux/
   ```

3. Rebuild to copy to target directory:
   ```bash
   cargo clean && cargo build
   ```

### Linker errors with "undefined reference to polly_*"

Install the Polly development package:
```bash
sudo apt-get install -y libpolly-21-dev
```

### Tests fail with "Permission denied"

Ensure binaries have execute permissions:
```bash
chmod +x external/bin/linux/*
chmod +x target/debug/bin/* 2>/dev/null || true
```

### Slow compilation

- First build is expected to take 10-15 minutes (LLVM bindings)
- Subsequent builds use incremental compilation
- Use `cargo build --release` only when needed

## Environment Variables Reference

| Variable | Value | Purpose |
|----------|-------|---------|
| `LLVM_SYS_211_PREFIX` | `/usr/lib/llvm-21` | Points llvm-sys crate to LLVM installation |
| `LD_LIBRARY_PATH` | (auto-configured) | Set by `.cargo/config.toml` for vendored libLLVM |

## Additional Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [LLVM APT Repository](https://apt.llvm.org/)
- [Inkwell Documentation](https://thedan64.github.io/inkwell/)
- [LLVM Documentation](https://llvm.org/docs/)

## Getting Help

If you encounter issues not covered in this guide:
1. Check existing [GitHub issues](https://github.com/Inferara/inference/issues)
2. Run `cargo build --verbose` for detailed error messages
3. Run `./book/check_deps.sh` to verify your setup
4. Open a new issue with your error output and environment details:
   ```bash
   uname -a
   lsb_release -a 2>/dev/null || cat /etc/os-release
   llvm-config-21 --version
   rustc --version
   ```
