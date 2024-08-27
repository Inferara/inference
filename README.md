# Inference

## VS Code setup

### Extensions
- rust-analyzer
- Even Better TOML
- crates

### Configuration

Enable proc macros

```json
"rust-analyzer.procMacro.enable": true,
```

Default linter

```json
"rust-analyzer.check.command": "clippy",
```

Format on save

```json
"[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
}
```

## Requirements

### wasm-pack

wasm-pack is used to compile rust as IR to wasm. See installation instructions [here](https://rustwasm.github.io/wasm-pack/installer/).
