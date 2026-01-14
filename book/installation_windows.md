# Windows Development Setup Guide

This guide walks you through setting up a complete development environment for the Inference project on Windows.

## Prerequisites

- Windows 10/11 (64-bit)
- Visual Studio Code installed
- Administrator access for installing software

## Step 1: Install MSYS2

MSYS2 provides Unix-like tools and libraries for Windows.

1. Download MSYS2 installer from https://www.msys2.org/
2. Run the installer and install to `C:\msys64` (default location)
3. After installation, open "MSYS2 UCRT64" terminal from Start Menu
4. Update the package database:
   ```bash
   pacman -Syu
   ```
5. Close the terminal when prompted and reopen it
6. Update remaining packages:
   ```bash
   pacman -Su
   ```

## Step 2: Install Required MSYS2 Packages

In the MSYS2 UCRT64 terminal, install the required development tools:

```bash
# Install essential build tools
pacman -S --noconfirm mingw-w64-ucrt-x86_64-gcc
pacman -S --noconfirm mingw-w64-ucrt-x86_64-binutils
pacman -S --noconfirm mingw-w64-ucrt-x86_64-libffi

# Install LLVM 21.1.1 (specific version required)
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

**Important:** LLVM 21.1.1 is required. Do not upgrade to 21.1.7 as it has compatibility issues.

To prevent accidental upgrades, add to `/etc/pacman.conf`:
```bash
echo "IgnorePkg = mingw-w64-ucrt-x86_64-llvm mingw-w64-ucrt-x86_64-llvm-libs mingw-w64-ucrt-x86_64-llvm-tools mingw-w64-ucrt-x86_64-clang mingw-w64-ucrt-x86_64-clang-libs" | sudo tee -a /etc/pacman.conf
```

## Step 3: Install Rust

1. Download and run rustup-init.exe from https://rustup.rs/
2. Choose the default installation (option 1)
3. Select the `x86_64-pc-windows-gnu` toolchain when prompted
4. After installation completes, close and reopen your terminal

Verify the installation:
```powershell
rustc --version
cargo --version
```

## Step 4: Add MSYS2 to System PATH

Add the MSYS2 UCRT64 bin directory to your Windows PATH:

1. Press `Win + X` and select "System"
2. Click "Advanced system settings"
3. Click "Environment Variables"
4. Under "System variables", find "Path" and click "Edit"
5. Click "New" and add: `C:\msys64\ucrt64\bin`
6. Click "OK" on all dialogs
7. Restart any open terminals/VS Code for changes to take effect

## Step 5: Configure Cargo Build Settings

The project already includes the necessary Cargo configuration at `.cargo/config.toml`.

If you need to verify or recreate it, it should contain:

```toml
[target.x86_64-pc-windows-gnu]
rustflags = [
    "-C", "link-arg=-Wl,--allow-multiple-definition",
    "-C", "link-arg=-lffi"
]

[env]
LLVM_SYS_211_PREFIX = "C:\\msys64\\ucrt64"
```

This configuration:
- Resolves pthread library conflicts
- Links libffi required by LLVM
- Points LLVM-sys to the correct LLVM installation

## Step 6: Install VS Code Extensions (Optional)

Recommended extensions for development:

1. **rust-analyzer** (rust-lang.rust-analyzer)
   - Provides code completion, navigation, and more
   
2. **CodeLLDB** (vadimcn.vscode-lldb)
   - Debugger for Rust

3. **Error Lens** (usernamehw.errorlens)
   - Shows errors inline in the editor

4. **Better TOML** (bungcip.better-toml)
   - TOML syntax highlighting for Cargo.toml files

Install via VS Code:
- Press `Ctrl+Shift+X`
- Search for each extension and click "Install"

## Step 7: Clone and Build the Project

1. Open PowerShell or Windows Terminal
2. Clone the repository:
   ```powershell
   git clone https://github.com/Inferara/inference.git
   cd inference
   ```

3. Build the project:
   ```powershell
   cargo build
   ```

   First build will take several minutes as it compiles all dependencies.

4. For optimized builds:
   ```powershell
   cargo build --release
   ```

## Step 8: Verify the Build

Run tests to ensure everything is working:
```powershell
cargo test
```

Run the CLI:
```powershell
.\target\debug\infc.exe --help
```

## Troubleshooting

### Build fails with "dlltool.exe not found"
- Ensure `mingw-w64-ucrt-x86_64-binutils` is installed in MSYS2
- Verify `C:\msys64\ucrt64\bin` is in your PATH

### Build fails with "undefined reference to ffi_*"
- Ensure `mingw-w64-ucrt-x86_64-libffi` is installed
- Check `.cargo/config.toml` has the correct rustflags

### Build fails with "LLVMConst*Mul undefined reference"
- You likely have LLVM 21.1.7 instead of 21.1.1
- Follow Step 2 to downgrade to LLVM 21.1.1
- Run `cargo clean` and rebuild

### "multiple definition of pthread_*" errors
- Ensure `.cargo/config.toml` contains the `--allow-multiple-definition` flag
- Clean and rebuild: `cargo clean && cargo build`

### Slow compilation
- First build is always slow (10-15 minutes)
- Subsequent builds are much faster (incremental compilation)
- Use `cargo build --release` only when needed for final binaries

## Environment Variables Reference

You may need these environment variables set in your shell/terminal:

```powershell
$env:PATH = "C:\msys64\ucrt64\bin;$env:PATH"
```

Add to your PowerShell profile (`$PROFILE`) to make permanent:
```powershell
# Open profile
notepad $PROFILE

# Add this line:
$env:PATH = "C:\msys64\ucrt64\bin;$env:PATH"
```

## Additional Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [MSYS2 Documentation](https://www.msys2.org/docs/what-is-msys2/)
- [Inkwell Documentation](https://thedan64.github.io/inkwell/)
- [LLVM Documentation](https://llvm.org/docs/)

## Getting Help

If you encounter issues not covered in this guide:
1. Check existing GitHub issues
2. Run `cargo build --verbose` for detailed error messages
3. Open a new issue with your error output and environment details
