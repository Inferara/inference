# Security Policy

## Supported versions

Inference is pre-1.0. Security fixes land on `main` and ship in the next release; older releases are not patched.

| Version                   | Supported |
| ------------------------- | --------- |
| `main` and latest release | Yes       |
| Older releases            | No        |

## Reporting a vulnerability

Please do not report a vulnerability in a public issue, pull request or discussion.

Email **accembler@gmail.com** with:

- the affected component (`infc`, `infs`, `inference-lsp`, the VS Code extension, or a crate under `core/`, `ide/` or `tools/`) and the version or commit;
- a minimal reproducer: the `.inf` source, `Inference.toml`, Wasm module or command line that triggers it;
- what happens and what you expected, including the impact you see.

We will acknowledge the report, keep you informed while we work on a fix, and credit you in the release notes unless you prefer otherwise.

## What counts

Besides memory-safety bugs, crashes on untrusted input and unsafe file or network handling, Inference treats these as security bugs:

- a **miscompile**: Wasm that does not do what the source says, or that skips a check the language requires (bounds, overflow, a trap);
- an **unsound proof obligation**: emitted Rocq that a prover can discharge although the compiled Wasm violates the property it states;
- a **linker or target check that fails open**: a module accepted for a target (Soroban, SpaceWasm) that the target would reject or misread.

[`.oss-scanner/threat_model.md`](../.oss-scanner/threat_model.md) describes where untrusted input enters, which components matter most, and how we rate severity.
