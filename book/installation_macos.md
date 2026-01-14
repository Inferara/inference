# macOS Development Setup Guide

This guide walks you through setting up a complete development environment for the Inference project on macOS.

## Prerequisites

- macOS 13 (Ventura) or later
- Apple Silicon (M1/M2/M3) or Intel processor
- Administrator access for installing software
- At least 4GB of free disk space

## Step 1: Install Xcode Command Line Tools

```bash
xcode-select --install
```

Follow the on-screen prompts to complete installation. This provides essential build tools including `git`, `make`, and compilers.

## Step 2: Install Homebrew

Homebrew is the recommended package manager for macOS.

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

After installation, add Homebrew to your PATH.

**Apple Silicon (M1/M2/M3):**
```bash
echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile
eval "$(/opt/homebrew/bin/brew shellenv)"
```

**Intel Mac:**
```bash
echo 'eval "$(/usr/local/bin/brew shellenv)"' >> ~/.zprofile
eval "$(/usr/local/bin/brew shellenv)"
```

Verify installation:
```bash
brew --version
```

## Step 3: Install LLVM 21

Install LLVM 21 via Homebrew:

```bash
brew install llvm@21
```

If `llvm@21` is not available, install the latest LLVM:
```bash
brew install llvm
```

Verify installation:
```bash
$(brew --prefix llvm@21 2>/dev/null || brew --prefix llvm)/bin/llvm-config --version
```

**Note:** Homebrew's LLVM is "keg-only" and not symlinked to `/usr/local/bin` by default.

## Step 4: Install Rust

Install Rust using rustup:

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

The output should show a nightly version.

## Step 5: Clone the Repository

```bash
git clone https://github.com/Inferara/inference.git
cd inference
```

## Step 6: Download External Binaries

The Inference compiler requires custom LLVM tools with Inference intrinsics support.

**Important:** Unlike Linux, macOS does NOT require the `libLLVM.so` shared library.

```bash
mkdir -p external/bin/macos

curl -L "https://storage.googleapis.com/external_binaries/macos/bin/inf-llc.zip" -o /tmp/inf-llc.zip
unzip -o /tmp/inf-llc.zip -d external/bin/macos/

curl -L "https://storage.googleapis.com/external_binaries/macos/bin/rust-lld.zip" -o /tmp/rust-lld.zip
unzip -o /tmp/rust-lld.zip -d external/bin/macos/

chmod +x external/bin/macos/inf-llc external/bin/macos/rust-lld
```

Alternatively, run the dependency check script:
```bash
./book/check_deps.sh
```

## Step 7: Configure Environment Variables

Set the LLVM prefix environment variable. The path differs based on your Mac and LLVM installation:

```bash
LLVM_PREFIX=$(brew --prefix llvm@21 2>/dev/null || brew --prefix llvm)
echo "export LLVM_SYS_211_PREFIX=\"$LLVM_PREFIX\"" >> ~/.zshrc
source ~/.zshrc
```

Verify:
```bash
echo $LLVM_SYS_211_PREFIX
```

**Apple Silicon:** `/opt/homebrew/opt/llvm@21` or `/opt/homebrew/opt/llvm`
**Intel Mac:** `/usr/local/opt/llvm@21` or `/usr/local/opt/llvm`

## Step 8: Build the Project

```bash
cargo build
```

First build will take several minutes.

For optimized builds:
```bash
cargo build --release
```

## Step 9: Verify the Build

Run tests:
```bash
cargo test
```

Run the CLI:
```bash
./target/debug/infc --help
```

Compile a sample file:
```bash
echo 'fn main() -> i32 { return 42; }' > test.inf
./target/debug/infc test.inf --parse --codegen -o
ls -la out/
```

## Troubleshooting

### Build fails with "LLVM not found"

1. Verify LLVM is installed:
   ```bash
   brew list llvm@21 || brew list llvm
   ```

2. Check the environment variable:
   ```bash
   echo $LLVM_SYS_211_PREFIX
   ls -la $LLVM_SYS_211_PREFIX
   ```

3. Reconfigure if needed:
   ```bash
   export LLVM_SYS_211_PREFIX=$(brew --prefix llvm@21 2>/dev/null || brew --prefix llvm)
   ```

### Build fails with "inf-llc not found"

1. Verify binaries exist:
   ```bash
   ls -la external/bin/macos/
   ```

2. Check they are executable and not quarantined:
   ```bash
   file external/bin/macos/inf-llc
   xattr -l external/bin/macos/inf-llc
   ```

3. Remove quarantine attribute if present:
   ```bash
   xattr -d com.apple.quarantine external/bin/macos/inf-llc
   xattr -d com.apple.quarantine external/bin/macos/rust-lld
   ```

### "inf-llc cannot be opened because it is from an unidentified developer"

This is macOS Gatekeeper blocking the binary.

**Option 1 (Recommended):** Remove quarantine attribute:
```bash
xattr -d com.apple.quarantine external/bin/macos/inf-llc
xattr -d com.apple.quarantine external/bin/macos/rust-lld
```

**Option 2:** Allow in System Settings:
1. Go to System Settings > Privacy & Security
2. Scroll down to see the blocked application message
3. Click "Allow Anyway"
4. Try running the build again

**Security Note:** Gatekeeper is an important security mechanism. Only remove quarantine for binaries from trusted sources.

### Linker errors on Apple Silicon

Ensure binaries match your architecture:
```bash
file external/bin/macos/inf-llc
```

Should show `arm64` for Apple Silicon or `x86_64` for Intel.

### Homebrew LLVM not found in PATH

Homebrew LLVM is keg-only. Use the full prefix path:
```bash
$(brew --prefix llvm@21)/bin/llvm-config --version
```

## Apple Silicon vs Intel

The Inference project supports both Apple Silicon (M1/M2/M3) and Intel Macs.

| Aspect | Apple Silicon | Intel |
|--------|---------------|-------|
| Homebrew prefix | `/opt/homebrew` | `/usr/local` |
| LLVM path | `/opt/homebrew/opt/llvm@21` | `/usr/local/opt/llvm@21` |
| Binary architecture | `arm64` | `x86_64` |

### Verifying Architecture

```bash
uname -m
```
- `arm64` = Apple Silicon
- `x86_64` = Intel

## Environment Variables Reference

| Variable | Apple Silicon Value | Intel Value | Purpose |
|----------|---------------------|-------------|---------|
| `LLVM_SYS_211_PREFIX` | `/opt/homebrew/opt/llvm@21` | `/usr/local/opt/llvm@21` | LLVM installation path |

## Additional Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Homebrew Documentation](https://docs.brew.sh/)
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
   sw_vers
   brew --prefix llvm@21 || brew --prefix llvm
   rustc --version
   ```
