# LLVM Legacy Setup (v0.0.1-beta.3 and earlier)

> **Note:** Starting from version after v0.0.1-beta.3, the Inference compiler no longer requires LLVM, inf-llc, rust-lld, or libLLVM. The compiler now generates WebAssembly directly via `wasm-encoder`. This document is preserved for users working with [v0.0.1-beta.3](https://github.com/Inferara/inference/releases/tag/v0.0.1-beta.3) or earlier releases.
>
> The GCP-hosted binaries remain available, so these older builds continue to work.

## LLVM 21 Installation

### Linux (Ubuntu/Debian)

```bash
wget https://apt.llvm.org/llvm.sh
chmod +x llvm.sh
sudo ./llvm.sh 21
sudo apt-get install -y llvm-21-dev libpolly-21-dev
```

Verify:
```bash
llvm-config-21 --version
```

### Linux (Fedora)

```bash
sudo dnf install -y llvm21-devel polly21-devel
```

### macOS

```bash
brew install llvm@21
```

If `llvm@21` is not available:
```bash
brew install llvm
```

Verify:
```bash
$(brew --prefix llvm@21 2>/dev/null || brew --prefix llvm)/bin/llvm-config --version
```

**Note:** Homebrew's LLVM is "keg-only" and not symlinked to `/usr/local/bin` by default.

### Windows (MSYS2)

In the MSYS2 UCRT64 terminal:

```bash
cd /tmp
curl -LO 'https://repo.msys2.org/mingw/ucrt64/mingw-w64-ucrt-x86_64-llvm-21.1.1-2-any.pkg.tar.zst'
curl -LO 'https://repo.msys2.org/mingw/ucrt64/mingw-w64-ucrt-x86_64-llvm-libs-21.1.1-2-any.pkg.tar.zst'
curl -LO 'https://repo.msys2.org/mingw/ucrt64/mingw-w64-ucrt-x86_64-llvm-tools-21.1.1-2-any.pkg.tar.zst'
curl -LO 'https://repo.msys2.org/mingw/ucrt64/mingw-w64-ucrt-x86_64-clang-21.1.1-2-any.pkg.tar.zst'
curl -LO 'https://repo.msys2.org/mingw/ucrt64/mingw-w64-ucrt-x86_64-clang-libs-21.1.1-2-any.pkg.tar.zst'

pacman -U --noconfirm \
  /tmp/mingw-w64-ucrt-x86_64-llvm-21.1.1-2-any.pkg.tar.zst \
  /tmp/mingw-w64-ucrt-x86_64-llvm-libs-21.1.1-2-any.pkg.tar.zst \
  /tmp/mingw-w64-ucrt-x86_64-llvm-tools-21.1.1-2-any.pkg.tar.zst \
  /tmp/mingw-w64-ucrt-x86_64-clang-21.1.1-2-any.pkg.tar.zst \
  /tmp/mingw-w64-ucrt-x86_64-clang-libs-21.1.1-2-any.pkg.tar.zst
```

**Important:** LLVM 21.1.1 is required. Version 21.1.7 has compatibility issues.

To prevent accidental upgrades:
```bash
echo "IgnorePkg = mingw-w64-ucrt-x86_64-llvm mingw-w64-ucrt-x86_64-llvm-libs mingw-w64-ucrt-x86_64-llvm-tools mingw-w64-ucrt-x86_64-clang mingw-w64-ucrt-x86_64-clang-libs" | sudo tee -a /etc/pacman.conf
```

## External Binaries

### Download Links (GCP)

#### Linux
- **inf-llc**: [Download](https://storage.googleapis.com/external_binaries/linux/bin/inf-llc.zip) → Extract to `external/bin/linux/`
- **rust-lld**: [Download](https://storage.googleapis.com/external_binaries/linux/bin/rust-lld.zip) → Extract to `external/bin/linux/`
- **libLLVM**: [Download](https://storage.googleapis.com/external_binaries/linux/lib/libLLVM.so.21.1-rust-1.94.0-nightly.zip) → Extract to `external/lib/linux/`

#### macOS
- **inf-llc**: [Download](https://storage.googleapis.com/external_binaries/macos/bin/inf-llc.zip) → Extract to `external/bin/macos/`
- **rust-lld**: [Download](https://storage.googleapis.com/external_binaries/macos/bin/rust-lld.zip) → Extract to `external/bin/macos/`

macOS does NOT require the `libLLVM.so` shared library.

#### Windows
- **inf-llc.exe**: [Download](https://storage.googleapis.com/external_binaries/windows/bin/inf-llc.zip) → Extract to `external/bin/windows/`
- **rust-lld.exe**: [Download](https://storage.googleapis.com/external_binaries/windows/bin/rust-lld.zip) → Extract to `external/bin/windows/`

### Setup

```bash
# Linux
mkdir -p external/bin/linux external/lib/linux
curl -L "https://storage.googleapis.com/external_binaries/linux/bin/inf-llc.zip" -o /tmp/inf-llc.zip
unzip -o /tmp/inf-llc.zip -d external/bin/linux/
curl -L "https://storage.googleapis.com/external_binaries/linux/bin/rust-lld.zip" -o /tmp/rust-lld.zip
unzip -o /tmp/rust-lld.zip -d external/bin/linux/
curl -L "https://storage.googleapis.com/external_binaries/linux/lib/libLLVM.so.21.1-rust-1.94.0-nightly.zip" -o /tmp/libLLVM.zip
unzip -o /tmp/libLLVM.zip -d external/lib/linux/
chmod +x external/bin/linux/inf-llc external/bin/linux/rust-lld
```

```bash
# macOS
mkdir -p external/bin/macos
curl -L "https://storage.googleapis.com/external_binaries/macos/bin/inf-llc.zip" -o /tmp/inf-llc.zip
unzip -o /tmp/inf-llc.zip -d external/bin/macos/
curl -L "https://storage.googleapis.com/external_binaries/macos/bin/rust-lld.zip" -o /tmp/rust-lld.zip
unzip -o /tmp/rust-lld.zip -d external/bin/macos/
chmod +x external/bin/macos/inf-llc external/bin/macos/rust-lld
```

## Environment Variables

| Variable | Platform | Value | Purpose |
|----------|----------|-------|---------|
| `LLVM_SYS_211_PREFIX` | Linux | `/usr/lib/llvm-21` | Points llvm-sys to LLVM installation |
| `LLVM_SYS_211_PREFIX` | macOS (Apple Silicon) | `/opt/homebrew/opt/llvm@21` | Points llvm-sys to LLVM installation |
| `LLVM_SYS_211_PREFIX` | macOS (Intel) | `/usr/local/opt/llvm@21` | Points llvm-sys to LLVM installation |
| `LLVM_SYS_211_PREFIX` | Windows | `C:\msys64\ucrt64` | Points llvm-sys to LLVM installation |
| `LD_LIBRARY_PATH` | Linux | (auto-configured by .cargo/config.toml) | Runtime libLLVM loading |

## Troubleshooting

### "LLVM not found" or "llvm-sys build failed"
1. Verify LLVM 21 is installed: `llvm-config-21 --version`
2. Check environment variable: `echo $LLVM_SYS_211_PREFIX`
3. Verify path exists: `ls -la $LLVM_SYS_211_PREFIX`

### "inf-llc not found" or "rust-lld not found"
1. Verify binaries exist: `ls -la external/bin/<platform>/`
2. Check they are executable: `file external/bin/<platform>/inf-llc`
3. Make executable: `chmod +x external/bin/<platform>/inf-llc external/bin/<platform>/rust-lld`

### "libLLVM.so: cannot open shared object file" (Linux)
1. Verify library exists: `ls -la external/lib/linux/`
2. Re-download if missing
3. Rebuild: `cargo clean && cargo build`

### macOS Gatekeeper quarantine
```bash
xattr -d com.apple.quarantine external/bin/macos/inf-llc
xattr -d com.apple.quarantine external/bin/macos/rust-lld
```

### "LLVMConst*Mul undefined reference" (Windows)
You likely have LLVM 21.1.7 instead of 21.1.1. Downgrade to 21.1.1.
