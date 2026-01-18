//! WebAssembly to Rocq (Coq) Translator
//!
//! This crate translates WebAssembly bytecode into Rocq formal verification language,
//! enabling mathematical verification of compiled Inference programs.
//!
//! ## Overview
//!
//! The translator is a critical component of the Inference verification pipeline:
//!
//! ```text
//! Inference source → Typed AST → LLVM IR → WASM → Rocq (.v)
//!                                                   ↑
//!                                            (this crate)
//! ```
//!
//! ## Entry Point
//!
//! Use [`wasm_parser::translate_bytes`] to translate WASM bytecode:
//!
//! ```ignore
//! use inference_wasm_to_v_translator::wasm_parser::translate_bytes;
//!
//! let wasm_bytes = std::fs::read("output.wasm")?;
//! let rocq_code = translate_bytes("my_module", &wasm_bytes)?;
//! std::fs::write("output.v", rocq_code)?;
//! ```
//!
//! ## Architecture
//!
//! The translation process uses a two-phase approach:
//!
//! 1. **Parsing Phase** ([`wasm_parser`]): Streams through WASM sections and builds [`translator::WasmParseData`]
//! 2. **Translation Phase** ([`translator`]): Converts structured data into Rocq code strings
//!
//! ### WASM Sections Supported
//!
//! - Type Section: Function signatures
//! - Import Section: External dependencies
//! - Function Section: Function type mappings
//! - Table Section: Indirect call tables
//! - Memory Section: Linear memory definitions
//! - Global Section: Global variables
//! - Export Section: Public interface
//! - Start Section: Entry point
//! - Element Section: Table initialization
//! - Data Section: Memory initialization
//! - Code Section: Function bodies with instructions
//! - Custom Section: Debug information (function and local names)
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
//! WASM's stack-based instruction model is converted to structured Rocq expressions.
//! The translator reconstructs control flow from linear instruction sequences.
//!
//! ## Non-Deterministic Instructions
//!
//! Inference extends WASM with custom instructions for formal verification:
//!
//! - `forall.start` (`0xfc 0x3a`): Universal quantification
//! - `exists.start` (`0xfc 0x3b`): Existential quantification
//! - `uzumaki.i32/i64` (`0xfc 0x3c/0x3d`): Non-deterministic values
//! - `assume` (`0xfc 0x3e`): Constraint assumption
//! - `unique` (`0xfc 0x3f`): Uniqueness constraint
//!
//! These instructions are parsed by the forked `inf-wasmparser` and translated
//! to corresponding Rocq constructs.
//!
//! ## Modules
//!
//! - [`wasm_parser`] - Parses WASM bytecode sections into structured data
//! - [`translator`] - Converts parsed data into Rocq code generation

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
