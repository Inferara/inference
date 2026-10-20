# Threat model

## What this project does and where untrusted input enters

Inference is a verification-first programming language. One source program compiles to:

- a WebAssembly module, and
- Rocq (Coq) proof obligations stating what that module does.

The targets are `wasm32`, Stellar Soroban smart contracts (`--target stellar`) and NASA JPL's SpaceWasm flight interpreter (`--target spacewasm`). Users deploy the Wasm and trust the proofs, so **the compiler's output is a security boundary**. A wrong binary or a wrong proof obligation is a vulnerability in every program built with it, even when nothing crashes.

The binaries:

- `infc` is the compiler (`core/cli`).
- `infs` is the project and toolchain manager (`apps/infs`). It builds projects by running `infc` and `wasm-opt`, runs modules with `wasmtime`, and downloads and installs toolchains.
- `inference-lsp` is the language server (`apps/lsp`, `ide/*`).
- `editors/vscode` is the VS Code extension.

Treat as untrusted (attacker-controlled):

- **Source files** (`.inf`): a project someone cloned, or a file an editor opened.
- **`Inference.toml` manifests**: path fields (`[wasm-dependencies]`, `[verification] output-dir`), the target, features, memory limits and host imports.
- **External Wasm modules** linked with `use … from` or `--wasm-dep`. These are third-party binaries. They are parsed with `tools/inf-wasmparser`, our fork of `wasmparser`, and merged into the output.
- **Everything the language server reads**: a workspace opened in an editor is analysed automatically, without the user running anything.
- **Downloads by `infs` and the VS Code extension**: release manifests, archives, `wasm-opt` from Binaryen. Hashes come from the release manifest. Archives are extracted on the user's machine.

Trusted: the person running the tools, their command-line flags and environment variables (`INFS_DIST_SERVER`, `INFC_PATH`, `WASM_OPT_PATH`, `INFERENCE_HOME` and so on), the editor talking to the language server, and TLS to `inference-lang.org`.

## Components that matter most / least

In rough order of importance:

1. **Wasm code generation** (`core/wasm-codegen`), and the type checker and analysis passes that decide what reaches it (`core/type-checker`, `core/analysis`).
   - A miscompile is the worst outcome: Wasm that computes something other than what the source says. So is Wasm that drops a check the language promises, such as array bounds, integer overflow or division traps.
   - See `book/src/arithmetic-overflow-in-wasm-codegen.md` and `book/src/memory-allocation-in-wasm-codegen.md`.
2. **Proof obligations** (`core/wasm-to-v`, `core/hassert`, the specification paths in `core/wasm-codegen`). Two failures matter here:
   - the emitted Rocq does not describe the emitted Wasm;
   - an obligation is provable although the Wasm violates the property it states.

   The translation is meant to fail closed: a construct it cannot translate faithfully must be refused (`core/wasm-to-v/README.md`).
3. **Linking and target checks**:
   - `core/wasm-linker` and `core/inference/src/wasm_link`, including the host-import allowlist;
   - `core/target-conformance`;
   - `core/stellar-abi`, the Soroban `Val` encoding (measured against the real host in `tests/tests/stellar/MEASURED_ABI.md`);
   - `tools/inf-wasmparser`.

   Each check must fail closed. A module accepted for a target that the target would reject or misread is a bug, and so is a host import that slips past the allowlist.
4. **`infs` toolchain management** (`apps/infs/src/toolchain`, `commands/install.rs`, `commands/self_cmd.rs`, `commands/uninstall.rs`): download, hash check, archive extraction, self-replacement of the binary, and edits to shell profiles.
5. **The language server and IDE database** (`apps/lsp`, `ide/*`), and the extension's download and extraction code (`editors/vscode/src/utils`).
6. **`unsafe` code**: the allocator and raw slices in `core/spacewasm-runner`, the range-checked transmute in `core/parser/src/syntax_kind.rs`, and `ide/ide-db/src/database.rs`.

Less important: `tools/inf-wast`, which is a test-only dependency of `inf-wasmparser`, and `tools/wat-fmt` and `tools/wasm-fmt`, which are formatters nothing ships with. Out of scope: `tests/`, `book/`, `ci/`, benches, the `infs` TUI's presentation, and the extension's UI.

## How to exercise it

The image has every test binary prebuilt, plus `wasmtime` and `coqc` (Coq 8.20.1). Everything runs offline.

- **Tests**:
  - `cargo test`: the default members, as CI runs them.
  - `cargo test-full`: also covers `tools/`.
  - `cargo test -p inference-tests rocq_typecheck::`: compiles the emitted Rocq with `coqc`.
  - `cargo test -p inference-tests --test stellar`: deploys contracts on an in-process Soroban host (`soroban-env-host`).
- **Compile one file**:
  - `target/debug/infc path/to/file.inf --out-dir /tmp/out` writes `file.wasm`.
  - Add `-v` (or `--mode proof`) to build in proof mode and also write the Rocq `file.v`. `--mode compile -v` writes the `.v` of the executable module instead.
  - `--target stellar` or `--target spacewasm` switches target.
  - `target/debug/infs run path/to/file.inf` compiles the file and runs it in `wasmtime`.
- **Corpus** (`tests/test_data/`):
  - `inf/`: language programs.
  - `codegen/wasm/<case>/`: `.inf` sources with their expected `.wasm`.
  - `stellar/`: Soroban contracts.
  - `wasmlib/`: an external Rust-built library for linking.
  - `rocq/`: expected `.v` output.
  - `panic_free/`.
- **Fuzzing**: `core/wasm-linker/fuzz` has a linker fuzz target and seeds. It needs nightly and `cargo-fuzz`, which this image does not include.
- **The language**: `book/src/` documents the compiler's guarantees. `book/src/compilation_targets.md` covers targets, and `book/src/external-functions-and-wasm-linking.md` covers linking and host imports.

To show a miscompile, give the `.inf` source, the `infc` command line, and the result `wasmtime` (or the Soroban host) gives next to the result the source requires. To show an unsound obligation, give the source, the emitted obligation, and a concrete execution of the Wasm that violates it.

## How you rate severity

- **Critical**:
  - A miscompile from ordinary, type-correct source that silently changes what a `stellar` or `spacewasm` module does. Examples: a dropped bounds, overflow or trap check, a wrong `Val` encoding or decoding, or a wrong result.
  - A proof obligation that can be discharged although the Wasm violates it.
  - Code execution, or writing or deleting files outside the intended directories, when someone opens a project in the editor or builds it with `infs`/`infc`.
  - A network attacker able to change what `infs` or the extension installs.
- **High**:
  - A miscompile that needs unusual but valid source, or that affects only `wasm32`.
  - A linker, host-import or target check that fails open.
  - Rocq output that no longer describes the Wasm without making a false obligation provable.
  - Memory unsafety in our `unsafe` code or in `inf-wasmparser`, even without a demonstrated exploit.
  - Archive extraction that writes outside its destination given a malicious archive. This includes path traversal, and symlink or hardlink entries.
- **Medium**:
  - A panic, hang or unbounded memory use in the language server or `infs` triggered by opening or building an untrusted project.
  - Reading files outside the project through manifest paths.
  - Accepting a downloaded artifact without checking its hash.
- **Low**:
  - A panic or hang in `infc` on crafted input given on its own command line. We treat any panic on user input as a bug and want to hear about it.
  - Misleading diagnostics.

## Anything to leave alone

- **Proof-only constructs**: `forall`, `exists`, `assume`, `unique` and the uzumaki `@` emit custom `0xfc`-prefixed instructions in `--mode proof`. That Wasm is meant for the prover, not for execution. Its failure to run, or the `stellar`/`spacewasm` targets refusing it, is expected.
- **`Admitted.` skeletons**: emitted obligations end in `Admitted.` by design, because a separate prover discharges them. Report an obligation only if it is false or does not match the Wasm, not because it is admitted.
- **User-disabled checks**: missing runtime checks are expected when the user turned them off, for example `--bounds-checks omit-proven` dropping a guard that analysis proved unnecessary. A guard dropped without that proof is in scope.
- **Trusted inputs**: behaviour controlled only by the person running the tool (command-line flags, environment variables, `infs uninstall <version>` arguments) is not an attack.
- **Upstream code**: bugs in `wasmtime`, Binaryen, Coq, `soroban-env-host` or other dependencies belong upstream, unless our use of them creates the vulnerability. Report a known advisory only if our code reaches the affected path.
- **Floats**: the language excludes floating point on purpose. Rejecting floats is not a bug.

For patches, follow `CONTRIBUTING.md`:

- add a test for each fix, with no `#[should_panic]` and no `#[ignore]`;
- keep the diff minimal;
- **do not run `rustfmt` or `cargo fmt`**: the tree is not formatter-clean, and reformatting buries the fix.
