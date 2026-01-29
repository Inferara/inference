# Inference VS Code Extension -- Manual QA Guide

**Version:** 0.0.3
**Branch:** `116-integrate-infs-to-vscode-extension`
**Date:** 2026-01-29

---

## Prerequisites

- VS Code 1.85+
- Node.js 20+
- Access to a Linux x64, macOS arm64, or Windows x64 machine
- Internet connection (for toolchain download tests)
- Optionally: a second machine or VM for cross-platform testing

---

## 0. Build & Automated Tests

| # | Step | Expected |
|---|------|----------|
| 0.1 | `npm install` in `editors/vscode/` | Installs without errors |
| 0.2 | `npm run build` | Builds `dist/extension.js` without errors |
| 0.3 | `npm run build:prod` | Production build succeeds |
| 0.4 | `npm test` | All 125 tests pass, 0 failures |
| 0.5 | `npm run package` | Produces `inference-0.0.3.vsix` without errors |

---

## 1. Extension Activation

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 1.1 | Install the VSIX into VS Code via `Extensions: Install from VSIX...` | Extension appears in installed list as "Inference" by "inference-lang" | |
| 1.2 | Open a folder containing a `.inf` file | Extension activates (check Output > Inference channel for "Platform:" log line) | |
| 1.3 | Open a folder with **no** `.inf` files, then create a new file and save as `test.inf` | Extension activates upon file creation | |
| 1.4 | Check the Output channel ("Inference") | Shows platform detection, infs search results, version info | |

---

## 2. Toolchain Detection

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 2.1 | **No infs installed:** Remove/rename `~/.inference/bin/infs`, clear `inference.path` setting, ensure `infs` not in PATH. Reload window. | Notification: "Inference toolchain not found. Would you like to install it?" with buttons: Install / Download Manually / Configure Path | |
| 2.2 | Click "Configure Path" in the notification | Opens Settings editor filtered to `inference.path` | |
| 2.3 | Click "Download Manually" | Opens `https://github.com/Inferara/inference/releases` in browser | |
| 2.4 | **Custom path:** Set `inference.path` to a valid `infs` binary path. Reload. | Extension detects and uses the custom path. Output shows: "infs found: /your/path" | |
| 2.5 | **Custom path (invalid):** Set `inference.path` to `/nonexistent/infs`. Reload. | Extension treats it as not found. Shows "toolchain not found" notification | |
| 2.6 | **PATH detection:** Clear `inference.path`, put `infs` in system PATH. Reload. | Extension finds infs via PATH. Output shows the PATH location | |
| 2.7 | **Managed location:** Clear `inference.path`, remove from PATH, place at `~/.inference/bin/infs`. Reload. | Extension finds infs at managed location | |
| 2.8 | **INFERENCE_HOME override:** Set env var `INFERENCE_HOME=/custom/dir`, place `infs` at `/custom/dir/bin/infs`. Reload. | Extension uses custom home directory | |

---

## 3. Status Bar

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 3.1 | Activate extension with **no** toolchain installed | Status bar shows `$(dash) Inference` (grey). Tooltip: "Toolchain not found. Click to run doctor." | |
| 3.2 | Activate extension with a **healthy** toolchain | Status bar shows `$(check) Inference` (green). Tooltip: "Inference: Toolchain healthy" | |
| 3.3 | Activate with toolchain that has **warnings** (e.g., missing `inf-llc`) | Status bar shows `$(warning) Inference` (yellow/warning background) | |
| 3.4 | Activate with toolchain that has **errors** | Status bar shows `$(error) Inference` (red/error background) | |
| 3.5 | Click the status bar item | Runs `inference.runDoctor` command | |

---

## 4. Commands

### 4.1 Install Toolchain

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.1.1 | Ctrl+Shift+P > "Inference: Install Toolchain" (no prior toolchain) | Progress notification: "Fetching release manifest..." -> "Downloading infs vX.Y.Z..." (with %) -> "Extracting..." -> "Running infs install..." -> "Verifying..." | |
| 4.1.2 | Wait for install to complete | Success notification with version. Status bar updates to healthy state. Output channel shows install log. | |
| 4.1.3 | Run install command **again** while one is already running | Shows: "Inference toolchain installation is already in progress." | |
| 4.1.4 | If install succeeds with doctor warnings | Warning notification: "installed, but doctor reported issues" with "Show Output" button | |
| 4.1.5 | Click "Show Output" on any install notification | Opens the Inference output channel | |
| 4.1.6 | **Offline test:** Disconnect network, run install | Error notification: "installation failed: ..." with Retry / Download Manually / Settings buttons | |
| 4.1.7 | Click "Retry" on error notification | Re-runs the install command | |
| 4.1.8 | Click "Download Manually" on error notification | Opens releases page in browser | |
| 4.1.9 | Click "Settings" on error notification | Opens Settings filtered to `inference.path` | |
| 4.1.10 | Run on unsupported platform (if testable) | Error: "unsupported platform (platform-arch)" | |

### 4.2 Run Doctor

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.2.1 | Ctrl+Shift+P > "Inference: Run Doctor" (toolchain installed) | Doctor output appears in Output channel with formatted checks: `[OK]`, `[WARN]`, `[FAIL]` | |
| 4.2.2 | Doctor with all checks passing | Info notification: "Toolchain is healthy" | |
| 4.2.3 | Doctor with warnings | Warning notification with summary, "Show Output" button | |
| 4.2.4 | Doctor with errors | Error notification with summary, "Show Output" button | |
| 4.2.5 | Run doctor with no toolchain | Warning: "Toolchain not found. Install it first." with Install button | |
| 4.2.6 | Click "Install" on that warning | Triggers `inference.installToolchain` | |
| 4.2.7 | Run doctor **while** doctor is already running | Silently no-ops (no duplicate runs) | |
| 4.2.8 | Verify status bar updates after doctor completes | Status bar icon and tooltip reflect doctor result | |

### 4.3 Update Toolchain

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.3.1 | Ctrl+Shift+P > "Inference: Update Toolchain" (already on latest) | Info notification: "Inference toolchain is up to date (vX.Y.Z)" | |
| 4.3.2 | With an older toolchain installed | Info notification: "Update available: vX.Y.Z (current: vA.B.C)" with Update / Release Notes buttons | |
| 4.3.3 | Click "Update" | Progress notification "Updating to vX.Y.Z...". On success: info notification and status bar refresh. | |
| 4.3.4 | Click "Release Notes" | Opens `https://github.com/Inferara/inference/releases/tag/vX.Y.Z` in browser | |
| 4.3.5 | Run update with no toolchain | Warning: "Toolchain not found. Install it first." with Install button | |
| 4.3.6 | Run update while update is already in progress | Shows: "Update check is already in progress." | |
| 4.3.7 | **Auto-update on activation:** Set `inference.checkForUpdates: true`, `inference.channel: "stable"`. Reload with outdated toolchain. | Automatic update notification appears (non-blocking) | |
| 4.3.8 | **Auto-update disabled:** Set `inference.checkForUpdates: false`. Reload with outdated toolchain. | No update notification on activation | |
| 4.3.9 | **Channel "none":** Set `inference.channel: "none"`. Reload with outdated toolchain. | No update check performed | |

### 4.4 Select Toolchain Version

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.4.1 | Ctrl+Shift+P > "Inference: Select Toolchain Version" | QuickPick appears with available versions, sorted descending by semver | |
| 4.4.2 | Current version is marked | Shows "(current)" tag next to the active version. Current version appears first. | |
| 4.4.3 | Stable versions are marked | Shows "(stable)" tag | |
| 4.4.4 | Select a different version | Progress notification "Switching to vX.Y.Z...". On success: info notification and status bar/doctor refresh. | |
| 4.4.5 | Select the current version | Info: "Already using toolchain vX.Y.Z." | |
| 4.4.6 | Press Escape on QuickPick | No action taken | |
| 4.4.7 | Run with no toolchain | Warning: "Toolchain not found." with Install button | |
| 4.4.8 | If install succeeds but setting default fails | Warning: "installed but could not be set as default. Run `infs default X.Y.Z` manually." | |

### 4.5 Show Output

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.5.1 | Ctrl+Shift+P > "Inference: Show Output" | Opens the "Inference" output channel panel | |

---

## 5. Syntax Highlighting

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 5.1 | Open a `.inf` file | Language mode shows "Inference" in status bar | |
| 5.2 | Keywords `fn`, `struct`, `enum`, `type`, `const`, `let`, `pub`, `mut`, `spec`, `external` | Highlighted as keywords | |
| 5.3 | Control flow `if`, `else`, `loop`, `break`, `return`, `assert` | Highlighted as control keywords | |
| 5.4 | Non-det constructs `forall`, `exists`, `assume`, `unique` | Highlighted distinctly | |
| 5.5 | Uzumaki symbol `@` | Highlighted as a special symbol | |
| 5.6 | Primitive types `i32`, `u64`, `bool`, etc. | Highlighted as type keywords | |
| 5.7 | String literals `"hello"` | Highlighted as strings | |
| 5.8 | Numeric literals: `42`, `0xFF`, `0b1010`, `0o77` | All highlighted as numbers | |
| 5.9 | Line comment `//` | Grayed out/highlighted as comment | |
| 5.10 | Doc comment `///` | Highlighted as doc comment | |
| 5.11 | Block comment `/* ... */` | Highlighted as comment | |
| 5.12 | Function names in declarations | Highlighted as function definitions | |

---

## 6. Language Configuration

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 6.1 | Type `{` | Auto-closes with `}` | |
| 6.2 | Type `[` | Auto-closes with `]` | |
| 6.3 | Type `(` | Auto-closes with `)` | |
| 6.4 | Type `"` | Auto-closes with `"` (not inside strings) | |
| 6.5 | Type `'` | Auto-closes with `'` (not inside strings/comments) | |
| 6.6 | Select text, type `{` | Wraps selection with `{}` | |
| 6.7 | Ctrl+/ on a line | Toggles `//` line comment | |
| 6.8 | Shift+Alt+A on selection | Toggles `/* */` block comment | |
| 6.9 | Click bracket `{` | Matching `}` is highlighted | |
| 6.10 | Add `// #region` and `// #endregion` markers | Code between markers is foldable | |
| 6.11 | Type `fn foo() {` then Enter | Next line is auto-indented | |
| 6.12 | Type `}` on indented line | Line is auto-dedented | |

---

## 7. Walkthrough

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 7.1 | Ctrl+Shift+P > "Get Started: Open Walkthrough..." > "Get Started with Inference" | Walkthrough opens with 4 steps | |
| 7.2 | Step 1: "Install the Toolchain" | Shows install button, manual download link, configure path link | |
| 7.3 | Click "Install Toolchain" in walkthrough | Triggers install command, step completes | |
| 7.4 | Step 2: "Verify Your Installation" | Shows "Run Doctor" button | |
| 7.5 | Click "Run Doctor" in walkthrough | Triggers doctor command, step completes | |
| 7.6 | Step 3: "Create a Project" | Shows "Create New File" link | |
| 7.7 | Step 4: "Build Your Program" | Shows terminal command example | |

---

## 8. Settings

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 8.1 | Open Settings, search "inference" | Shows 4 settings: path, autoInstall, channel, checkForUpdates | |
| 8.2 | `inference.path` | Default empty. Accepts string path. | |
| 8.3 | `inference.autoInstall` | Default true. Boolean toggle. | |
| 8.4 | `inference.channel` | Default "stable". Dropdown: stable / latest / none. | |
| 8.5 | `inference.checkForUpdates` | Default true. Boolean toggle. | |
| 8.6 | Change `inference.channel` to "latest" and check for updates | Unstable/pre-release versions are included in update check | |
| 8.7 | Change `inference.channel` to "none" and reload | No update check performed on activation | |

---

## 9. Error Handling & Edge Cases

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 9.1 | Corrupt `infs` binary (wrong architecture or truncated) | Graceful error: "infs version failed" or similar. Status bar shows grey/not found. | |
| 9.2 | `infs version` returns unexpected format | Output: "Could not parse infs version from: ..." | |
| 9.3 | `infs` version below minimum (0.0.1-beta.1) | Warning: "infs version X is outdated (minimum: 0.0.1-beta.1). Please update." with Update button | |
| 9.4 | Click "Update" on outdated warning | Triggers `inference.updateToolchain` | |
| 9.5 | Network timeout during manifest fetch | Error notification with Retry option | |
| 9.6 | SHA-256 mismatch after download | Error: "SHA-256 verification failed for infs vX.Y.Z" | |
| 9.7 | Archive extraction failure | Error with details in output channel | |
| 9.8 | `infs install` command fails after extraction | Error: "infs install failed (exit N): ..." | |
| 9.9 | Version switch: install succeeds but `infs default` fails | Warning about partial success with manual command suggestion | |
| 9.10 | Rapidly invoke same command multiple times | Concurrency guard prevents parallel execution; shows "already in progress" | |

---

## 10. Cross-Platform (if applicable)

| # | Platform | Step | Expected | Pass? |
|---|----------|------|----------|-------|
| 10.1 | Linux x64 | Full install flow | Downloads `.tar.gz`, extracts with `tar`, sets +x permissions | |
| 10.2 | macOS arm64 | Full install flow | Downloads `.tar.gz`, extracts with `tar`, sets +x permissions | |
| 10.3 | Windows x64 | Full install flow | Downloads `.zip`, extracts with PowerShell `Expand-Archive` | |
| 10.4 | Windows x64 | Doctor output with CRLF | Parses correctly, no extra blank lines | |
| 10.5 | Windows x64 | File detection uses `F_OK` (not `X_OK`) | `infs.exe` detected without needing executable permission bit | |

---

## 11. Privacy & Security

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 11.1 | Monitor network during activation (e.g., with DevTools or proxy) | Only contacts `inference-lang.org` (manifest) and `github.com/Inferara/inference` (releases) | |
| 11.2 | Verify no telemetry endpoints are contacted | No analytics or tracking requests | |
| 11.3 | Downloaded archive SHA-256 is verified before extraction | If hash tampered, install fails with clear error | |
| 11.4 | HTTPS-to-HTTP redirect is blocked | If manifest/download redirects to HTTP, install fails with "Refusing HTTPS-to-HTTP redirect" | |

---

## Sign-Off

| Role | Name | Date | Verdict |
|------|------|------|---------|
| QA Tester | | | |
| Developer | | | |

**Notes:**
