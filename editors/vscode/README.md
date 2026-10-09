# Inference VS Code Extension

Official VS Code extension for the [Inference](https://github.com/Inferara/inference) programming language.

## Features

### Syntax Highlighting

Full syntax highlighting support for Inference language constructs:

- **Keywords**: `fn`, `struct`, `enum`, `const`, `let`, `pub`, `mut`, `spec`, `external`, `use`, `from`
- **Arithmetic modes**: `checked`, `wrapping`
- **Control Flow**: `if`, `else`, `loop`, `break`, `return`, `assert`
- **Non-deterministic Constructs**: `forall`, `exists`, `assume`, `unique`, `@` (uzumaki)
- **Primitive Types**: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, `bool`, and the unit type `()`; `unit` is highlighted as a reserved word
- **Literals**: strings, numbers (decimal, hex, binary, octal), booleans
- **Comments**: line (`//`), documentation (`///`), and block (`/* */`)

### Language Configuration

- Auto-closing brackets: `{}`, `[]`, `()`, `""`, `''`
- Comment toggling with `Ctrl+/` (line) and `Shift+Alt+A` (block)
- Bracket matching and highlighting
- Code folding with `// #region` and `// #endregion` markers
- Smart indentation for blocks

### File Association

- Automatically activates for `.inf` files
- Custom file icon for Inference source files

### Rocq (`.v`) Highlighting

The Rocq files you work with while proving — the generated `out/*.v`, uploaded
files and returned proofs — are highlighted even without a Rocq extension:
commands, declarations, tactics, `Proof`/`Qed`/`Admitted`, comments, strings
and numbers. The language mode shows **Rocq (Inference)**; `(* *)` toggles
comments.

This applies only to `.v` files that would otherwise open as plain text. When
a Rocq extension (VsCoq, coq-lsp) or a Verilog extension handles `.v`, it keeps
doing so. To keep `.v` files as plain text, add
`"files.associations": { "*.v": "plaintext" }`; choosing another language for a
file in the status bar keeps that choice until the file is reopened. The
highlighting is available while the Inference extension is active, for example
in a workspace with `.inf` files.

### Language Server

The extension automatically starts the Inference language server (`inference-lsp`) for `.inf` files, providing rich language intelligence:

- **Diagnostics** - Compiler errors and warnings as you type
- **Hover** - Type information and documentation, including explanations of the non-deterministic operators (`forall`, `exists`, `assume`, `unique`, `@`)
- **Go to Definition** - Jump to symbol definitions (F12)
- **Completions** - Context-aware code completion
- **Document Symbols** - Outline view and breadcrumb navigation
- **Inlay Hints** - Inline type annotations

The server binary is resolved using the following priority (mirroring `infs` detection):

1. Custom path from `inference.lsp.path` setting (if set but not executable, the server is not started - no fallback)
2. Managed installation in `INFERENCE_HOME/bin/inference-lsp` (respects `INFERENCE_HOME` environment variable)
3. System `PATH`

If the binary is not found, the extension stays quiet: a line is logged to the "Inference" output channel and the server simply stays off until the toolchain is installed or updated. Server traces are written to the dedicated "Inference Language Server" output channel. Use **Inference: Restart Language Server** to pick up a new binary after an update or a settings change made outside VS Code.

### Toolchain Management

The extension provides comprehensive toolchain management through integration with the `infs` CLI. All operations are fully automated and require no manual configuration.

#### Automatic Detection

On activation, the extension automatically detects your toolchain using the following priority:

1. Custom path from `inference.path` setting
2. Managed installation in `INFERENCE_HOME/bin/infs` (respects `INFERENCE_HOME` environment variable)
3. System `PATH`

The detection result is displayed in the Configuration sidebar and logged to the Output channel.

#### Configuration Sidebar

A dedicated Inference icon appears in the VS Code activity bar. Click it to open the Configuration view with real-time toolchain information:

**Toolchain Group:**
- Binary path and detection source (settings/managed/path)
- Installed version number
- `INFERENCE_HOME` directory location (default or custom)
- Detected platform (e.g., `linux-x64`, `macos-arm64`, `windows-x64`)
- Health status with diagnostic results

**Proof Server Group:**
- Server URL (click to change `inference.prover.serverUrl`)
- API key status (click to set a key)
- Whether your `infc` matches the compiler the proof server accepts, after the last Prove

**Settings Group:**
- `inference.path` - Custom binary path (click to configure)
- `inference.autoInstall` - Auto-install prompt behavior
- `inference.checkForUpdates` - Automatic update checking

**Interactive Actions:**
- Click any path item to copy its value to clipboard
- Right-click path items to reveal in file explorer
- Click status to run doctor diagnostics
- Use the refresh button in the title bar to reload

The view automatically refreshes when settings change or after install/update operations.

#### Terminal Integration

The extension automatically prepends `INFERENCE_HOME/bin` to `PATH` for all VS Code integrated terminals using the `EnvironmentVariableCollection` API:

- New terminals immediately have `infs` and `infc` available
- Existing terminals show a relaunch indicator when the toolchain changes
- No VS Code restart required after installation or updates
- Works across all supported platforms

#### Status Bar

The bottom-left status bar shows real-time toolchain health. Click the status bar item to run full diagnostics via `infs doctor`. When a doctor check fails but `infs` still resolves a working `infc` (for example the Platform check with a custom `inference.path`), the item shows a warning instead of an error: compiling and proving work.

While proof jobs run, a second item shows the job and its progress (`$(sync~spin) controller 3/15`, or `Proving 2` for several), then the last result; click it to open the job.

#### Available Commands

Open Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`):

- **Inference: Install Toolchain** - Download and install the latest `infs` release for your platform
- **Inference: Update Toolchain** - Check for updates and install the latest version
- **Inference: Select Toolchain Version** - Browse and switch between available versions
- **Inference: Run Doctor** - Execute comprehensive health diagnostics
- **Inference: Restart Language Server** - Stop and restart `inference-lsp`, re-resolving the binary location
- **Inference: Refresh Configuration** - Reload the Configuration sidebar view
- **Inference: Show Output** - Open the Inference output log channel
- **Inference: Reset PATH Fallback Preference** - Clear saved PATH fallback acceptance
- **Inference: Prove This File** - Compile the active `.inf` in proof mode and submit it to the proof server
- **Inference: Submit Rocq File for Proof** - Submit an existing `.v` file
- **Inference: Set Proof Server API Key** / **Clear Proof Server API Key** - Store or remove the key for the configured server
- **Inference: Refresh Proof Jobs**, **Filter Proof Jobs by Status**, **Show All Proof Jobs** - Manage the Proof Jobs view

A guided setup walkthrough is available via **Get Started: Open Walkthrough...** > **Get Started with Inference**.

### Proving

Prove the `spec` properties of an Inference program on the Inference proof server, then review and manage the runs without leaving the editor.

1. Get an API key from your proof server operator and run **Inference: Set Proof Server API Key**. The key is checked against the server and stored in VS Code's secret storage.
2. Open an `.inf` file and click **Prove This File** in the editor title, the Proof Jobs view title, or the Explorer context menu. The extension saves the file, then:
   - finds the `infc` that `infs build` will use and compares its `--commit-hash` and `--abi-version` with the compiler the proof server accepts. A different compiler is refused before anything is uploaded; when the accepted compiler is a release, **Install … and Prove** switches to it and proves again. A server that does not publish its compiler is confirmed once per server;
   - runs `infs build <file>.inf -v` in the file's folder, which writes `out/<file>.v`. Compile errors go to the **Problems** panel;
   - checks the generated file before uploading: it must have proof holes, fit the server's upload limit, and not define a name twice. (The compiler names the module after the file, so `clamp.inf` with `fn clamp` cannot compile; you are pointed at the function to rename.)
   - asks once per server before the first upload, then submits.
3. The job opens in a panel that follows it live:
   - a verdict in plain words at the top, with the actions that fit it (open the proof, compare it with what you submitted, open the certificate in the portal, go to a failing line, run it again);
   - the run's steps, obligations with their goals, how each was closed (template or agent), and source lines;
   - elapsed and remaining time against the job's budget;
   - an Activity list grouped by obligation, filterable to steps, the agent transcript, or everything.

   Positive results count only after the server's independent verifier accepts them; **How this was verified** lists its checks and the kernel-reported assumptions. When the server cannot compile the file, the panel shows the first compiler error and links to the line.
4. When a job you are watching finishes, a notification says how it ended, with **Open** (and **Run Again** for failures).

An existing Rocq `.v` file can be submitted directly with **Inference: Submit Rocq File for Proof**. Submitting a file the server already has (from this window, another one, or the portal) shows that job and tells you when it ran, with **Run Again** for a fresh run.

The **Proof Jobs** view in the Inference sidebar lists your jobs as "In progress" and "Finished", each named after the `.inf` it came from, with its result, progress and age. Filter by status, load older jobs, and use each job's menu to open it in the portal, cancel a running job, run a finished one again, copy its ID, or delete it. Deleted jobs disappear immediately and are purged within 7 days; finished jobs are kept for 30 days.

A structural result (`ValidModule` only) is labelled as such: it is not a functional-correctness claim.

## Installation

### From VS Code Marketplace

1. Open VS Code
2. Press `Ctrl+P` to open Quick Open
3. Type `ext install inference-lang.inference`
4. Press Enter

### From VSIX

1. Download the `.vsix` file from [Releases](https://github.com/Inferara/inference/releases)
2. In VS Code, press `Ctrl+Shift+P`
3. Type "Install from VSIX" and select the command
4. Choose the downloaded `.vsix` file

## Configuration

### Settings

- **`inference.path`** (string, default: `""`) - Custom path to the `infs` binary. Leave empty for automatic detection. Scope: machine (not synced across devices).
- **`inference.autoInstall`** (boolean, default: `true`) - Prompt to install toolchain if not found on activation.
- **`inference.checkForUpdates`** (boolean, default: `true`) - Automatically check for toolchain updates on activation.
- **`inference.lsp.enabled`** (boolean, default: `true`) - Start the Inference language server automatically. Disable to turn off all language intelligence features.
- **`inference.lsp.path`** (string, default: `""`) - Custom path to the `inference-lsp` binary. Leave empty for automatic detection. Scope: machine (not synced across devices).
- **`inference.prover.serverUrl`** (string, default: `""`) - Proof server for proving and the Proof Jobs view. Leave empty for the hosted Inference proof service. Must use `https`; plain `http` is accepted only for `localhost`. Scope: machine.

### Environment Variables

- **`INFERENCE_HOME`** - Override default toolchain directory (default: `~/.inference` on Linux/macOS, `%APPDATA%\inference` on Windows — the same locations `infs` uses)
- **`INFS_DIST_SERVER`** - Override distribution server URL (for development/testing)

## Supported Platforms

Automatic toolchain installation is supported on:

- **Linux**: x86_64 (glibc)
- **macOS**: ARM64 (Apple Silicon)
- **Windows**: x86_64

Other platforms can use the extension for syntax highlighting but must install the toolchain manually.

## Example

```inference
/// Computes factorial using non-deterministic verification
pub fn factorial(n: i32) -> i32 {
    let mut result: i32 = 1;
    let mut i: i32 = 1;

    loop {
        if i > n {
            break;
        }
        result = result * i;
        i = i + 1;
    }

    // Verify the result using forall block
    forall {
        const witness: i32 = @;
        assume {
            const valid: bool = witness >= 0;
        }
    }

    return result;
}
```

## What is Inference?

Inference is a programming language designed for mission-critical applications development. It includes first-class support for formal verification via translation to Rocq (Coq) and targets WebAssembly as its primary runtime platform.

Key features:
- **Formal Verification**: Built-in support for proofs and specifications
- **Non-deterministic Programming**: `forall`, `exists`, `assume`, `unique` constructs
- **WebAssembly Target**: Compiles to efficient WASM
- **Rocq Translation**: Generate Coq proofs from your code

Learn more:
- [Inference Repository](https://github.com/Inferara/inference)
- [Language Specification](https://github.com/Inferara/inference-language-spec)
- [Inference Book](https://github.com/Inferara/book)

## Troubleshooting

### Toolchain not detected

1. Check the Output panel (View > Output > Select "Inference")
2. Run **Inference: Run Doctor** to see detailed diagnostics
3. Verify `inference.path` setting if using a custom location
4. Try **Inference: Install Toolchain** to install automatically

### Language server not running

1. Check the Output panel (View > Output > Select "Inference") for a "Language server" log line explaining why it did not start
2. Ensure `inference.lsp.enabled` is `true`
3. Install or update the toolchain (**Inference: Install Toolchain** / **Inference: Update Toolchain**) so `inference-lsp` is present in `INFERENCE_HOME/bin`
4. Alternatively, set `inference.lsp.path` to the binary location (note: if this setting points to a non-executable path, the server is not started and no fallback occurs)
5. Run **Inference: Restart Language Server** after fixing the binary location

### Terminal commands not found

1. Close all open terminals and open a new one (Terminal > New Terminal)
2. The extension automatically adds `INFERENCE_HOME/bin` to `PATH`
3. For external terminals, add the path to your shell profile manually

## Privacy

This extension does not collect telemetry, usage data, or any personal information. Toolchain operations communicate only with `github.com/Inferara/inference/releases` and `inference-lang.org/releases.json`.

The proving features contact only the configured proof server (`inference.prover.serverUrl`), and only after you store an API key. **Prove This File** and **Submit Rocq File for Proof** upload the generated Rocq file, which contains your program logic and specifications. Submitted files and results are visible to your account and the proof-server operator, and an AI prover may send file content to its model provider. The extension asks once per server before the first upload, after the file has compiled locally. Your `.inf` source is compiled locally and is not uploaded.

## Contributing

Contributions are welcome! Please see the [main repository](https://github.com/Inferara/inference) for contribution guidelines.

## License

GPL-3.0 - See [LICENSE](LICENSE) for details.
