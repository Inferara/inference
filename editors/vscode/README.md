# Inference VS Code Extension

Official VS Code extension for the [Inference](https://github.com/Inferara/inference) programming language.

## Features

### Syntax Highlighting

Full syntax highlighting support for Inference language constructs:

- **Keywords**: `fn`, `struct`, `enum`, `type`, `const`, `let`, `pub`, `mut`, `spec`, `external`
- **Control Flow**: `if`, `else`, `loop`, `break`, `return`, `assert`
- **Non-deterministic Constructs**: `forall`, `exists`, `assume`, `unique`, `@`
- **Primitive Types**: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, `bool`
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

### Toolchain Management

The extension integrates with the `infs` CLI to provide one-button toolchain installation and management. On activation, it automatically detects your toolchain by checking the `inference.path` setting, system `PATH`, and `~/.inference/bin/infs` (respecting `INFERENCE_HOME`).

Available commands (open Command Palette with `Ctrl+Shift+P`):

- **Inference: Install Toolchain** -- download and install `infs` for your platform
- **Inference: Update Toolchain** -- check for and install updates
- **Inference: Select Toolchain Version** -- switch between installed versions
- **Inference: Run Doctor** -- diagnose toolchain issues
- **Inference: Show Output** -- open the Inference log channel

A guided setup walkthrough is available via **Get Started: Open Walkthrough...** > **Get Started with Inference**.

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

## Privacy

This extension does not collect telemetry, usage data, or any personal information. All toolchain operations communicate only with `github.com/Inferara/inference/releases` and `inference-lang.org/releases.json`.

## Contributing

Contributions are welcome! Please see the [main repository](https://github.com/Inferara/inference) for contribution guidelines.

## License

GPL-3.0 - See [LICENSE](LICENSE) for details.
