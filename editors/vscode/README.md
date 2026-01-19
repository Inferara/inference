# Inference VS Code Extension

Official VS Code extension for the Inference programming language.

## Status

Placeholder. See the VS Code Extension Plan for the full feature roadmap.

## Planned Features

### Phase 1: Minimal Viable Extension
- TextMate syntax highlighting
- Language configuration (brackets, comments)
- LSP client connecting to `inference-lsp`
- Basic diagnostics display

### Phase 2: Live Codegen
- View WAT output (live, updated on save)
- Non-deterministic block visualization
- Virtual document provider

### Phase 3: Enhanced Features
- View Rocq output
- Code snippets
- Server restart command
- Status bar integration

### Phase 4: Distribution
- Platform-specific VSIX packages
- Bundled LSP binary
- Marketplace publishing

## Directory Structure (Planned)

```
editors/vscode/
├── src/
│   ├── extension.ts        # Entry point
│   ├── client.ts           # LanguageClient wrapper
│   ├── config.ts           # Settings management
│   ├── commands/           # Command implementations
│   └── providers/          # Document providers
├── syntaxes/
│   └── inference.tmLanguage.json
├── snippets/
│   └── inference.json
├── package.json
├── tsconfig.json
└── language-configuration.json
```

## Development

See the main repository README for development instructions.
