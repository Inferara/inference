# Introduction

Welcome to the Inference Compiler Book — a collection of design and implementation
notes for the [Inference](https://github.com/Inferara/inference) compiler
toolchain. Inference is a programming language for mission-critical software with
first-class support for formal verification via translation to Rocq (Coq), and it
targets WebAssembly as its primary runtime.

This book is **not** a language tutorial. It documents *how the compiler works
internally* and *why specific implementation decisions were made* — the kind of
context that is hard to recover from the source code alone. Each chapter takes a
single subsystem or problem, explains the constraints, walks through the chosen
approach, and compares it against how other compilers solve the same problem.

## The compilation pipeline

The compiler is a multi-phase pipeline. Each `.inf` source file flows through:

```text
.inf source → parse → type-check → analyze → codegen (WASM) → wasm-to-v (Rocq)
```

The chapters follow that order:

- **[The Inference Parser](parser.md)** — the resilient
  recursive-descent parser that turns source text into the typed AST.
- **[Static Analysis in Inference](static-analysis.md)** — the rule-based pass
  that enforces the control-flow invariants formal verification depends on.
- **[Memory Allocation in WASM Codegen](memory-allocation-in-wasm-codegen.md)** —
  linear-memory layout, stack frames, and array lowering.
- **[Arithmetic Overflow in WASM Codegen](arithmetic-overflow-in-wasm-codegen.md)**
  — WebAssembly's wrapping semantics and what they mean for proofs.
- **[Unreachable Emission in Codegen](unreachable-emission-in-codegen.md)** — why
  the compiler emits a trailing `unreachable` in non-void functions.
- **[Compilation Targets](compilation_targets.md)** — compile vs. proof modes and
  the supported backends.

A second group of chapters covers how programs are organised and built beyond a
single source file:

- **[Module Hierarchy & Multi-File Compilation](module-hierarchy-and-multi-file-compilation.md)**
  — the file-as-module system, the `use` directive, and how a multi-file program
  flattens into one WASM module and one proof.
- **[Projects & the infs Toolchain](projects-and-the-infs-toolchain.md)** — the
  `Inference.toml` manifest, project discovery, and the `infs build`/`run` workflow
  that drives the compiler.
- **[External Functions and WASM Linking](external-functions-and-wasm-linking.md)**
  — declaring `external fn`s and binding them to pre-compiled `.wasm` modules.
- **[The WASM Linker](the-wasm-linker.md)** — the static merge that folds external
  function bodies into a single self-contained module.

The appendix preserves historical notes from the project's earlier LLVM-based
backend, retained for reference.

## Building this book

This book is built with [mdBook](https://rust-lang.github.io/mdBook/). From the
`book/` directory:

```bash
mdbook build      # render static HTML into book/book/
mdbook serve      # serve locally with live reload at http://localhost:3000
```

Source for every chapter lives under `book/src/`.
