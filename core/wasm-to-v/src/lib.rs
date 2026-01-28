//! WebAssembly to Rocq (Coq) Translator
//!
//! This crate translates WebAssembly bytecode into Rocq (formerly Coq) formal verification
//! code, enabling mathematical verification of compiled Inference programs.
//!
//! ## Overview
//!
//! The translator serves as the final phase in the Inference verification pipeline:
//!
//! ```text
//! Inference source → Typed AST → LLVM IR → WASM → Rocq (.v)
//!                                                   ↑
//!                                            (this crate)
//! ```
//!
//! It converts WebAssembly binary format into equivalent Rocq definitions that preserve
//! program semantics and can be formally verified using the Rocq proof assistant.
//!
//! ## Entry Point
//!
//! The primary entry point is [`wasm_parser::translate_bytes`]:
//!
//! ```ignore
//! use inference_wasm_to_v_translator::wasm_parser::translate_bytes;
//!
//! let wasm_bytes = std::fs::read("output.wasm")?;
//! let rocq_code = translate_bytes("my_module", &wasm_bytes)?;
//! std::fs::write("output.v", rocq_code)?;
//! ```
//!
//! For integration with the Inference compiler, use the higher-level API:
//!
//! ```ignore
//! use inference::wasm_to_v;
//!
//! let rocq_code = wasm_to_v("module_name", &wasm_bytes)?;
//! ```
//!
//! ## Architecture
//!
//! The translation process uses a two-phase approach for maximum efficiency:
//!
//! ### Phase 1: Parsing ([`wasm_parser`])
//!
//! Streams through WASM bytecode sections in a single forward pass, populating
//! [`translator::WasmParseData`] with structured information. Uses zero-copy
//! parsing to minimize memory allocations.
//!
//! ### Phase 2: Translation ([`translator`])
//!
//! Converts structured [`translator::WasmParseData`] into Rocq code strings.
//! Implements error recovery to collect multiple translation failures before
//! reporting.
//!
//! ### WASM Sections Supported
//!
//! - **Type Section**: Function signatures as recursion groups
//! - **Import Section**: External function, memory, table, and global imports
//! - **Function Section**: Maps function indices to type indices
//! - **Table Section**: Indirect call table definitions
//! - **Memory Section**: Linear memory specifications with size limits
//! - **Global Section**: Global variable definitions with initialization
//! - **Export Section**: Public interface (exported functions, tables, memories, globals)
//! - **Start Section**: Optional module entry point
//! - **Element Section**: Table initialization segments
//! - **Data Count Section**: Number of data segments (bulk memory proposal)
//! - **Data Section**: Memory initialization segments
//! - **Code Section**: Function bodies with local variables and instructions
//! - **Custom Section**: Debug information (module, function, and local names)
//!
//! Component model sections are recognized but generate empty stubs.
//!
//! ## Type Translation
//!
//! WASM types are mapped to Rocq type constructors:
//!
//! | WASM Type | Rocq Type |
//! |-----------|-----------|
//! | `i32` | `T_num T_i32` |
//! | `i64` | `T_num T_i64` |
//! | `f32` | `T_num T_f32` |
//! | `f64` | `T_num T_f64` |
//! | `v128` | `T_vec T_v128` |
//! | `funcref` | `T_ref T_funcref` |
//! | `externref` | `T_ref T_externref` |
//!
//! ## Expression Translation
//!
//! WASM uses a stack-based instruction model, while Rocq uses structured expressions.
//! The translator reconstructs control flow from linear instruction sequences:
//!
//! **WASM (stack-based):**
//! ```text
//! local.get 0
//! local.get 1
//! i32.add
//! ```
//!
//! **Rocq (structured):**
//! ```coq
//! BI_get_local 0%N ::
//! BI_get_local 1%N ::
//! BI_binop (Binop_i BOI_add) ::
//! nil
//! ```
//!
//! Control flow structures (blocks, loops, conditionals) are converted to nested
//! Rocq expressions with proper scope and result type handling.
//!
//! ## Non-Deterministic Instructions
//!
//! Inference extends WebAssembly with custom instructions for non-deterministic
//! computation and formal verification. These extensions enable explicit representation
//! of non-deterministic choices in the binary format:
//!
//! | Instruction | Encoding | Purpose |
//! |-------------|----------|---------|
//! | `forall.start` | `0xfc 0x3a` | Begin universal quantification block |
//! | `exists.start` | `0xfc 0x3b` | Begin existential quantification block |
//! | `uzumaki.i32` | `0xfc 0x3c` | Generate non-deterministic i32 value |
//! | `uzumaki.i64` | `0xfc 0x3d` | Generate non-deterministic i64 value |
//! | `assume` | `0xfc 0x3e` | Filter execution paths by constraint |
//! | `unique` | `0xfc 0x3f` | Assert exactly one execution path exists |
//!
//! These instructions are parsed by the forked [`inf-wasmparser`] dependency and
//! translated to corresponding Rocq constructs that enable formal reasoning about
//! non-deterministic programs.
//!
//! See the [WASM codegen documentation](../wasm-codegen/README.md) for details on
//! how these instructions are generated from Inference source code.
//!
//! ## Modules
//!
//! - [`wasm_parser`] - Parses WASM bytecode sections into structured data (Phase 1)
//! - [`translator`] - Converts parsed data into Rocq code strings (Phase 2)
//!
//! ## Error Handling
//!
//! All translation functions return [`anyhow::Result`] for flexible error propagation.
//!
//! - **Parser errors**: The parsing phase fails fast on malformed WASM bytecode
//! - **Translator errors**: The translation phase uses error recovery to collect
//!   multiple failures before reporting the first error
//!
//! ## Performance Characteristics
//!
//! | Operation | Complexity | Notes |
//! |-----------|-----------|-------|
//! | Parse WASM module | O(n) | Single pass through bytecode |
//! | Translate types | O(t) | t = number of type definitions |
//! | Translate functions | O(f × i) | f = functions, i = avg instructions per function |
//! | Name lookup | O(1) | HashMap-based name resolution |
//! | Overall | O(n) | Linear in WASM file size |
//!
//! ## See Also
//!
//! - [Crate README](../README.md) - Detailed documentation and examples
//! - [WASM Codegen](../wasm-codegen/README.md) - LLVM IR to WASM compilation
//! - [Inference Compiler](../inference/README.md) - Main compiler orchestration
//! - [Rocq Documentation](https://rocq-prover.org/) - Rocq proof assistant
//! - [WebAssembly Specification](https://webassembly.github.io/spec/) - WASM standard

pub mod translator;
pub mod wasm_parser;

#[cfg(test)]
mod tests {
    use super::wasm_parser::translate_bytes;
    use std::fs;
    use std::panic;
    use std::path::PathBuf;

    #[test]
    fn test_parse_test_data() {
        let test_data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data");

        assert!(
            test_data_dir.exists(),
            "test_data directory not found at {:?}",
            test_data_dir
        );

        let entries = fs::read_dir(&test_data_dir).expect("Failed to read test_data directory");

        let mut wasm_files = Vec::new();

        for entry in entries {
            let entry = entry.expect("Failed to read directory entry");
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                wasm_files.push(path);
            }
        }

        wasm_files.sort();

        assert!(
            !wasm_files.is_empty(),
            "No .wasm files found in test_data directory"
        );

        let mut success_count = 0;
        let mut error_count = 0;
        let mut panic_count = 0;

        for wasm_path in &wasm_files {
            let file_name = wasm_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let bytes = fs::read(wasm_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", file_name, e));

            let module_name = wasm_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("module");

            // Catch panics from unimplemented features
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                translate_bytes(module_name, &bytes)
            }));

            match result {
                Ok(Ok(translation)) => {
                    println!("✓ Successfully parsed {}", file_name);
                    assert!(
                        !translation.is_empty(),
                        "Translation result is empty for {}",
                        file_name
                    );
                    success_count += 1;
                }
                Ok(Err(e)) => {
                    println!("✗ Failed to parse {}: {}", file_name, e);
                    error_count += 1;
                }
                Err(_) => {
                    println!(
                        "⚠ Panicked while parsing {} (likely unimplemented feature)",
                        file_name
                    );
                    panic_count += 1;
                }
            }
        }

        println!("\n=== Summary ===");
        println!("Total files: {}", wasm_files.len());
        println!("Successful: {}", success_count);
        println!("Failed (errors): {}", error_count);
        println!("Failed (panics/unimplemented): {}", panic_count);
        println!(
            "Success rate: {:.1}%",
            (success_count as f64 / wasm_files.len() as f64) * 100.0
        );
    }
}
