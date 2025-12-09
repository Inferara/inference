<div align="center">
  <h1><code>inf-wasmparser</code></h1>

<strong><a href="https://inferara.com/">Inferara</a> fork of Bytecode Alliance project</strong>

  <p>
    <strong>A Rust parser for the <a href="https://webassembly.github.io/spec/core/text/index.html">WebAssembly Binary Format</a>. Extended with <a href="https://www.inferara.com/en/papers/specifying-algorithms-using-non-deterministic-computations/">Inference non-deterministic instructions</a>.</strong>
  </p>

  <p>
    <a href="https://crates.io/crates/inf-wasmparser"><img src="https://img.shields.io/crates/v/inf-wasmparser.svg?style=flat-square" alt="Crates.io version" /></a>
    <a href="https://crates.io/crates/inf-wasmparser"><img src="https://img.shields.io/crates/d/inf-wasmparser.svg?style=flat-square" alt="Download" /></a>
  </p>

</div>

## Inference non-deterministic instructions

This repository extends the WebAssembly Binary Format with non-deterministic instructions. These instructions are used to specify algorithms in more general way to be able to compile the source code targeting proof assistants like [Rocq](https://rocq-prover.org/) or [Lean](https://lean-lang.org/).

More information about Inference can be found in the official [Inference language spec](https://github.com/Inferara/inference-language-spec) and on the [Inferara website](https://www.inferara.com).

### Block instructions

| Instruction | WAT syntax  | Binary representation | Description |
| ----------- | ----------- | --------------------- | ----------- |
| **Forall**  | `(forall )` | `0xfc 0x3a`           | In this block all possible program execution paths are considered |
| **Exists**  | `(exists )` | `0xfc 0x3b`           | In this block the existance of a certain program execution path is considered |
| **Assume**  | `(assume )` | `0xfc 0x3c`           | In this block the assumption about the program state in defined |
| **Unique**  | `(unique )` | `0xfc 0x3d`           | In this block it is assumed the existance of **only one** certain program execution path |

### Variable instructions

| Instruction     | WAT syntax      | Binary representation | Description |
| --------------- | --------------- | --------------------- | ----------- |
| **i32.uzumaki** | `(i32.uzumaki)` | `0xfc 0x31`           | `i32` value type attribute guiding [inference](https://github.com/Inferara/inference) compiler to generate a proof that reason about all possible values the variable can hold |
| **i64.uzumaki** | `(i64.uzumaki)` | `0xfc 0x32`           | `i64` value type attribute guiding [inference](https://github.com/Inferara/inference) compiler to generate a proof that reason about all possible values the variable can hold |

## Origin

This project is a fork of the [wasmparser](https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wasmparser).

## License

This project inherits the license from the original project, which is [Apache-2.0_WITH_LLVM-exception](./LICENSE-Apache-2.0_WITH_LLVM-exception).
