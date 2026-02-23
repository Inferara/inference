# Inference VS Code Extension -- Manual QA Guide

**Version:** 0.0.3
**Branch:** `127-update-vscode-extension-after-removing-llvm`
**Date:** 2026-02-18

---

## Prerequisites

- VS Code 1.85+
- Node.js 20+
- Access to a Linux x64, macOS arm64, or Windows x64 machine
- Internet connection (for toolchain download tests)
- Optionally: a second machine or VM for cross-platform testing

---

## Automated Test Coverage

Many QA cases below are covered by automated tests (`npm test`). Cases marked with **[A]** are fully automated and only need re-checking if the automated test suite itself is changed or if visual/UX aspects need human judgment.

| Section | Automated Coverage |
|---------|--------------------|
| 0. Build & Tests | CI |
| 1. Activation | Manual (requires VS Code host) |
| 2. Toolchain Detection | Partial -- detection logic tested; UI notifications manual |
| 3. Status Bar | **[A]** Logic automated (`status-bar-state.test.ts`); click behavior manual |
| 3a. Configuration Sidebar | Manual (requires VS Code host and TreeView interaction) |
| 3b. Terminal PATH Integration | Manual (requires VS Code integrated terminal) |
| 4. Commands | Partial -- formatting, version picker, update check logic automated; UI interactions manual |
| 5. Syntax Highlighting | Manual (requires VS Code host) |
| 6. Language Configuration | Manual (requires VS Code host) |
| 7. Walkthrough | **[A]** Schema validated (`settings-schema.test.ts`); interactive steps manual |
| 8. Settings | **[A]** Schema validated (9 commands in `settings-schema.test.ts`) |
| 9. Error Handling | **[A]** Most paths automated (`install-failures.test.ts`, `version-parsing.test.ts`, `e2e-installation.test.ts`) |
| 10. Cross-Platform | Manual (requires physical platforms); detection and extraction logic tested |
| 11. Privacy & Security | **[A]** HTTPS redirect + SHA-256 automated (`https-redirect.test.ts`, `download.test.ts`) |

---

## 0. Build & Automated Tests

| # | Step | Expected |
|---|------|----------|
| 0.1 | `npm install` in `editors/vscode/` | Installs without errors |
| 0.2 | `npm run build` | Builds `dist/extension.js` without errors |
| 0.3 | `npm run build:prod` | Production build succeeds |
| 0.4 | `npm test` | All 216 tests pass, 0 failures |
| 0.5 | `npm run package` | Produces `inference-0.0.3.vsix` without errors |

---

## 1. Extension Activation

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 1.1 | Install the VSIX into VS Code via `Extensions: Install from VSIX...` | Extension appears in installed list as "Inference" by "inference-lang" | |
| 1.2 | Open a folder containing a `.inf` file | Extension activates (check Output > Inference channel for "Inference Activation" log line) | |
| 1.3 | Open a folder with **no** `.inf` files, then create a new file and save as `test.inf` | Extension activates upon file creation | |
| 1.4 | Check the Output channel ("Inference") | Shows: Platform, INFERENCE_HOME, INFS_DIST_SERVER, infs binary path and source, toolchain status | |

---

## 2. Toolchain Detection

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 2.1 | **No infs installed:** Remove/rename `~/.inference/bin/infs`, clear `inference.path` setting, ensure `infs` not in PATH. Reload window. | Notification: "Inference toolchain not found. Would you like to install it?" with buttons: Install / Download Manually / Configure Path | |
| 2.2 | Click "Configure Path" in the notification | Opens Settings editor filtered to `inference.path` | |
| 2.3 | Click "Download Manually" | Opens `https://github.com/Inferara/inference/releases` in browser | |
| 2.4 | **Custom path (settings):** Set `inference.path` to a valid `infs` binary path. Reload. | Extension detects and uses the custom path. Output shows: `infs binary: /your/path (settings)` | |
| 2.5 | **Custom path (invalid):** Set `inference.path` to `/nonexistent/infs`. Reload. | Extension treats it as not found. Shows "toolchain not found" notification | |
| 2.6 | **PATH detection:** Clear `inference.path`, put `infs` in system PATH but not in managed location. Reload. | Extension finds infs via PATH. Output shows: `infs binary: /path/to/infs (path)` | |
| 2.7 | **Managed location:** Clear `inference.path`, remove from PATH, place at `~/.inference/bin/infs`. Reload. | Extension finds infs at managed location. Output shows: `infs binary: ~/.inference/bin/infs (managed)` | |
| 2.8 | **INFERENCE_HOME override:** Set env var `INFERENCE_HOME=/custom/dir`, place `infs` at `/custom/dir/bin/infs`. Reload. | Extension uses custom home directory. Output shows the custom path. | |
| 2.9 | **PATH fallback warning:** Set `INFERENCE_HOME=/custom/dir` (no infs there), but have `infs` in PATH. Reload. | Warning: "infs binary not found in INFERENCE_HOME (/custom/dir). Found via PATH instead." with Install / Dismiss buttons | |
| 2.10 | Click "Dismiss" on PATH fallback warning, then reload | Warning is suppressed (remembered in globalState) | |
| 2.11 | **Reset PATH acceptance:** Ctrl+Shift+P > "Inference: Reset PATH Fallback Preference" | Info: "PATH fallback preference has been reset." Reloading will show the PATH fallback warning again. | |
| 2.12 | **Unsupported platform** (if testable) | Warning: "unsupported platform (platform-arch)" with "Download Page" button | |

---

## 3. Status Bar

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 3.1 | Observe status bar immediately after activation | Status bar shows `$(loading~spin) Inference`. Tooltip: "Checking toolchain..." **[A]** initial state tested | |
| 3.2 | Activate extension with **no** toolchain installed | Status bar shows `$(dash) Inference` (grey). Tooltip: "Inference: Toolchain not found. Click to run doctor." **[A]** | |
| 3.3 | Activate extension with a **healthy** toolchain | Status bar shows `$(check) Inference`. Tooltip: "Inference: Toolchain healthy" **[A]** | |
| 3.4 | Activate with toolchain that has **warnings** (e.g., infc found but not in managed location) | Status bar shows `$(warning) Inference` (warning background). Tooltip shows doctor summary. **[A]** | |
| 3.5 | Activate with toolchain that has **errors** | Status bar shows `$(error) Inference` (error background). Tooltip shows doctor summary. **[A]** | |
| 3.6 | Click the status bar item | Runs `inference.runDoctor` command | |

---

## 3a. Configuration Sidebar

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 3a.1 | Observe activity bar | Inference icon (file_icon.svg) appears in activity bar | |
| 3a.2 | Click the Inference icon | Configuration view opens with "Toolchain" and "Settings" groups | |
| 3a.3 | Toolchain group shows infs path, version, home, platform, status | Each property shows correct resolved value | |
| 3a.4 | Settings group shows Path, Auto Install, Check for Updates | Each shows current setting value (e.g., "(auto-detect)", "enabled") | |
| 3a.5 | Click a Settings item (e.g., "Auto Install: enabled") | VS Code settings editor opens filtered to that setting key | |
| 3a.6 | Click "Status: healthy" | Runs `inference.runDoctor` command | |
| 3a.7 | With no toolchain installed | Welcome content shows "Install Toolchain" and "Configure Path" buttons | |
| 3a.8 | Click "Install Toolchain" in welcome content | Triggers `inference.installToolchain` | |
| 3a.9 | Right-click a path item (infs path or Home) | Context menu shows "Copy Value" and "Reveal in File Explorer" | |
| 3a.10 | Click "Copy Value" on infs path item | Path string copied to clipboard; info notification shown | |
| 3a.11 | Click refresh button in Configuration view title bar | View refreshes, re-reads all state | |
| 3a.12 | Change an `inference.*` setting | Configuration view auto-refreshes to reflect new value | |
| 3a.13 | Run install or doctor command | Configuration view auto-refreshes afterward | |

---

## 3b. Terminal PATH Integration

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 3b.1 | After fresh install, open a new integrated terminal | `infs version` works immediately without manual PATH configuration | |
| 3b.2 | With an existing open terminal, run install/update | Terminal shows relaunch indicator (env changed icon) | |
| 3b.3 | Click the relaunch indicator on the terminal | Terminal relaunches; `infs version` now works | |
| 3b.4 | Hover the terminal env indicator | Tooltip shows "Adds the Inference toolchain to PATH" | |
| 3b.5 | Close and reopen VS Code | PATH modification persists; new terminals still have `infs` on PATH | |

---

## 4. Commands

### 4.1 Install Toolchain (`inference.installToolchain`)

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.1.1 | Ctrl+Shift+P > "Inference: Install Toolchain" (no prior toolchain) | Progress notification: "Fetching release manifest..." -> "Downloading infs vX.Y.Z..." (with %) -> "Extracting archive..." -> "Running infs install..." -> "Verifying installation..." | |
| 4.1.2 | Wait for install to complete | Success notification: "Inference toolchain vX.Y.Z installed successfully." with "Show Output" button. Status bar updates. | |
| 4.1.3 | Run install command **again** while one is already running | Shows: "Inference toolchain installation is already in progress." | |
| 4.1.4 | If install succeeds with doctor warnings | Warning: "Inference toolchain vX.Y.Z installed, but doctor reported issues. See output for details." with "Show Output" button | |
| 4.1.5 | Click "Show Output" on any install notification | Opens the Inference output channel | |
| 4.1.6 | **Offline test:** Disconnect network, run install | Error: "Inference toolchain installation failed: ..." with Retry / Download Manually / Settings buttons **[A]** network error tested | |
| 4.1.7 | Click "Retry" on error notification | Re-runs the install command | |
| 4.1.8 | Click "Download Manually" on error notification | Opens `https://github.com/Inferara/inference/releases` in browser | |
| 4.1.9 | Click "Settings" on error notification | Opens Settings filtered to `inference.path` | |
| 4.1.10 | Run on unsupported platform (if testable) | Error: "Inference: unsupported platform (platform-arch)." **[A]** "No compatible infs release" tested | |

### 4.2 Run Doctor (`inference.runDoctor`)

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.2.1 | Ctrl+Shift+P > "Inference: Run Doctor" (toolchain installed) | Doctor output appears in Output channel with formatted checks: `[OK]  `, `[WARN]`, `[FAIL]` wrapped in separator lines **[A]** formatting tested | |
| 4.2.2 | Doctor with all checks passing | Info notification: "Inference: Toolchain is healthy." | |
| 4.2.3 | Doctor with warnings | Warning notification: "Inference doctor: {summary}" with "Show Output" button | |
| 4.2.4 | Doctor with errors | Error notification: "Inference doctor: {summary}" with "Show Output" button | |
| 4.2.5 | Run doctor with no toolchain | Warning: "Inference toolchain not found. Install it first." with "Install" button | |
| 4.2.6 | Click "Install" on that warning | Triggers `inference.installToolchain` | |
| 4.2.7 | Run doctor **while** doctor is already running | Silently no-ops (no duplicate runs) | |
| 4.2.8 | Verify status bar updates after doctor completes | Status bar icon and tooltip reflect doctor result | |

### 4.3 Update Toolchain (`inference.updateToolchain`)

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.3.1 | Ctrl+Shift+P > "Inference: Update Toolchain" (already on latest) | Info: "Inference toolchain is up to date (vX.Y.Z)." **[A]** | |
| 4.3.2 | With an older toolchain installed | Info: "Inference toolchain update available: vX.Y.Z (current: vA.B.C)" with Update / Release Notes buttons **[A]** | |
| 4.3.3 | Click "Update" | Progress notification "Updating to vX.Y.Z...". On success: "Inference toolchain updating to to vX.Y.Z." and doctor re-runs. | |
| 4.3.4 | Click "Release Notes" | Opens `https://github.com/Inferara/inference/releases/tag/vX.Y.Z` in browser | |
| 4.3.5 | Run update with no toolchain | Warning: "Inference toolchain not found. Install it first." with Install button **[A]** | |
| 4.3.6 | Run update while update is already in progress | Shows: "Update check is already in progress." | |
| 4.3.7 | **Auto-update on activation:** Set `inference.checkForUpdates: true`. Reload with outdated toolchain. | Automatic update notification appears (non-blocking) | |
| 4.3.8 | **Auto-update disabled:** Set `inference.checkForUpdates: false`. Reload with outdated toolchain. | No update notification on activation | |

### 4.4 Select Toolchain Version (`inference.selectVersion`)

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.4.1 | Ctrl+Shift+P > "Inference: Select Toolchain Version" | QuickPick appears with available versions, sorted descending by semver. Only versions available for current platform shown. **[A]** | |
| 4.4.2 | Current version is marked | Shows "(current)" tag next to the active version. Current version appears first. **[A]** | |
| 4.4.3 | Stable versions are marked | Shows "(stable)" tag. Current stable shows "(current, stable)". **[A]** | |
| 4.4.4 | Select a different version | Progress notification "Switching to vX.Y.Z...". On success: info notification and doctor re-runs. | |
| 4.4.5 | Select the current version | Info: "Already using toolchain vX.Y.Z." | |
| 4.4.6 | Press Escape on QuickPick | No action taken | |
| 4.4.7 | Run with no toolchain | Warning: "Inference toolchain not found. Install it first." with Install button | |
| 4.4.8 | If install succeeds but setting default fails | Warning: "Inference: vX.Y.Z was installed but could not be set as default. Run `infs default X.Y.Z` manually." with "Show Output" button | |
| 4.4.9 | If install itself fails | Error: "Inference: Failed to install vX.Y.Z: {error}" | |

### 4.5 Show Output (`inference.showOutput`)

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.5.1 | Ctrl+Shift+P > "Inference: Show Output" | Opens the "Inference" output channel panel | |

### 4.6 Reset PATH Fallback Preference (`inference.resetPathAcceptance`)

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 4.6.1 | Ctrl+Shift+P > "Inference: Reset PATH Fallback Preference" | Info: "Inference: PATH fallback preference has been reset." | |
| 4.6.2 | Reload window after reset (with INFERENCE_HOME set, infs only in PATH) | PATH fallback warning reappears | |

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
| 7.1 | Ctrl+Shift+P > "Get Started: Open Walkthrough..." > "Get Started with Inference" | Walkthrough opens with 4 steps **[A]** schema validated | |
| 7.2 | Step 1: "Install the Toolchain" | Shows install button, manual download link, configure path link. Completion event: `onCommand:inference.installToolchain` **[A]** step IDs validated | |
| 7.3 | Click "Install Toolchain" in walkthrough | Triggers install command, step completes | |
| 7.4 | Step 2: "Verify Your Installation" | Shows "Run Doctor" button. Completion event: `onCommand:inference.runDoctor` | |
| 7.5 | Click "Run Doctor" in walkthrough | Triggers doctor command, step completes | |
| 7.6 | Step 3: "Create a Project" | Shows "Create New File" link. Completion event: `onLanguage:inference` | |
| 7.7 | Step 4: "Build Your Program" | Shows terminal command example: `infs build main.inf`. Completion event: `stepSelected` | |

---

## 8. Settings

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 8.1 | Open Settings, search "inference" | Shows exactly 3 settings: path, autoInstall, checkForUpdates **[A]** | |
| 8.2 | `inference.path` | Type: string, default: empty, scope: machine. Accepts file path to infs binary. **[A]** | |
| 8.3 | `inference.autoInstall` | Type: boolean, default: true. Toggleable. **[A]** | |
| 8.4 | `inference.checkForUpdates` | Type: boolean, default: true. Toggleable. **[A]** | |

---

## 9. Error Handling & Edge Cases

| # | Step | Expected | Pass? |
|---|------|----------|-------|
| 9.1 | Corrupt `infs` binary (wrong architecture or truncated) | Graceful error: "infs version failed (exit N): ..." in Output. Status bar shows dash (not found state). | |
| 9.2 | `infs version` returns unexpected format | Output: "Could not parse infs version from: ..." **[A]** `parseCurrentVersion` edge cases tested | |
| 9.3 | `infs` version below minimum (0.0.1-beta.1) | Warning: "Inference: infs version X is outdated (minimum: 0.0.1-beta.1). Please update." with "Update" button **[A]** `compareSemver` ordering tested | |
| 9.4 | Click "Update" on outdated warning | Triggers `inference.updateToolchain` | |
| 9.5 | Network error during manifest fetch | Error: "Inference toolchain installation failed: Network error fetching ..." with Retry / Download Manually / Settings buttons **[A]** | |
| 9.6 | SHA-256 mismatch after download | Error: "SHA-256 verification failed for infs vX.Y.Z. Expected ..., got ..." **[A]** | |
| 9.7 | Archive missing infs binary after extraction | Error: "infs binary not found at ... after extraction." **[A]** | |
| 9.8 | `infs install` command fails after extraction | Error: "infs install failed (exit N): {stderr}" **[A]** | |
| 9.9 | Version switch: install succeeds but `infs default` fails | Warning: "vX.Y.Z was installed but could not be set as default. Run `infs default X.Y.Z` manually." with "Show Output" button | |
| 9.10 | Rapidly invoke same command multiple times | Concurrency guard prevents parallel execution; shows "already in progress" | |
| 9.11 | No compatible release for current platform in manifest | Error: "No compatible infs release found for {platform}." **[A]** | |

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
| 11.1 | Monitor network during activation (e.g., with DevTools or proxy) | Only contacts `inference-lang.org` (manifest) and `github.com/Inferara/inference` (releases). Configurable via `INFS_DIST_SERVER` env var. | |
| 11.2 | Verify no telemetry endpoints are contacted | No analytics or tracking requests | |
| 11.3 | Downloaded archive SHA-256 is verified before extraction | If hash tampered, install fails: "SHA-256 verification failed for infs vX.Y.Z" **[A]** | |
| 11.4 | HTTPS-to-HTTP redirect is blocked | If manifest/download redirects to HTTP, fails with "Refusing HTTPS-to-HTTP redirect: {url} -> {target}" **[A]** | |
| 11.5 | JSON response size limit | Responses larger than 10 MB are rejected | |
| 11.6 | Redirect chain limit | More than 5 redirects are rejected: "Too many redirects fetching {url}" | |
