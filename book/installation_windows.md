# Windows Development Setup Guide

This guide walks you through setting up a complete development environment for the Inference project on Windows.

## Prerequisites

- Windows 10/11 (64-bit)
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

In the MSYS2 UCRT64 terminal:

```bash
pacman -S --noconfirm mingw-w64-ucrt-x86_64-gcc
pacman -S --noconfirm mingw-w64-ucrt-x86_64-binutils
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

## Step 5: Clone and Build the Project

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

## Step 6: Verify the Build

Run tests to ensure everything is working:
```powershell
cargo test
```

Run the CLI (either `infs` or `infc`):
```powershell
.\target\debug\infs.exe --help
.\target\debug\infc.exe --help
```

## Troubleshooting

### Build fails with "dlltool.exe not found"
- Ensure `mingw-w64-ucrt-x86_64-binutils` is installed in MSYS2
- Verify `C:\msys64\ucrt64\bin` is in your PATH

### "multiple definition of pthread_*" errors
- Ensure `.cargo/config.toml` contains the `--allow-multiple-definition` flag
- Clean and rebuild: `cargo clean && cargo build`

### Slow compilation
- First build is always slow (several minutes)
- Subsequent builds are much faster (incremental compilation)
- Use `cargo build --release` only when needed for final binaries

## LLVM Legacy Setup

If you are working with [v0.0.1-beta.3](https://github.com/Inferara/inference/releases/tag/v0.0.1-beta.3) or earlier releases that require LLVM, inf-llc, and rust-lld, see the [LLVM Legacy Setup Guide](archive/llvm-legacy-setup.md). The GCP-hosted binaries remain available for these older versions.

## Additional Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [MSYS2 Documentation](https://www.msys2.org/docs/what-is-msys2/)

## Getting Help

If you encounter issues not covered in this guide:
1. Check existing GitHub issues
2. Run `cargo build --verbose` for detailed error messages
3. Open a new issue with your error output and environment details
