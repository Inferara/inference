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
//! Inference source → Typed AST → WASM → Rocq (.v)
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
//! let rocq_code = translate_bytes(
//!     "my_module",
//!     &wasm_bytes,
//!     &rustc_hash::FxHashMap::default(),
//!     &inference_hassert::HSpecMap::default(),
//! )?;
//! std::fs::write("output.v", rocq_code)?;
//! ```
//!
//! For integration with the Inference compiler, use the higher-level API:
//!
//! ```ignore
//! use inference::{wasm_to_v, FxHashMap};
//!
//! let rocq_code = wasm_to_v("module_name", &wasm_bytes, &FxHashMap::default())?;
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
//! | `f32` | rejected as [`errors::WasmToVError::UnsupportedFeature`] |
//! | `f64` | rejected as [`errors::WasmToVError::UnsupportedFeature`] |
//! | `v128` | rejected as [`errors::WasmToVError::UnsupportedFeature`] |
//! | `funcref` | `T_ref T_funcref` |
//! | `externref` | `T_ref T_externref` |
//!
//! The wasm-verifier proof contract admits only `T_i32 | T_i64` of `number_type`
//! and no vector type, so `f32`, `f64`, and `v128` have nothing verifiable to map
//! to. The rejection covers
//! function parameters and results, locals, globals, and block result types
//! through one chokepoint, so a float in a *signature* is refused even when no
//! float instruction appears in any body.
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
//! | `forall` | `0xfc 0x3a` | Begin universal quantification block |
//! | `exists` | `0xfc 0x3b` | Begin existential quantification block |
//! | `assume` | `0xfc 0x3c` | Filter execution paths by constraint |
//! | `unique` | `0xfc 0x3d` | Assert exactly one execution path exists — rejected in proof mode (no `hassert` encoding; fatal `P002` at codegen) |
//! | `i32.uzumaki` | `0xfc 0x31` | Generate non-deterministic i32 value |
//! | `i64.uzumaki` | `0xfc 0x32` | Generate non-deterministic i64 value |
//!
//! These instructions are parsed by the forked [`inf-wasmparser`] dependency, but
//! they never appear in the emitted Rocq: spec-function bodies are omitted from the
//! module record entirely (their logical content arrives separately as `hassert`
//! obligations via the `inference.hspecs` custom section), and a non-deterministic
//! instruction in any surviving (non-spec) body is a translation error — the
//! vanilla WasmCert proof model has no constructors for them.
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
//! ### Rejection policy
//!
//! The translator emits only what the vendored proof stub in `rocq-stub/`
//! declares. A construct outside that subset is refused with
//! [`errors::WasmToVError::UnsupportedFeature`] naming it — never a `.v` that
//! fails `coqc` downstream, and never a panic. Rejected: every floating-point,
//! SIMD/vector, and conversion instruction (integer width conversions included —
//! the model declares no conversion at all); `f32`/`f64`/`v128` in any type
//! position; and the proposal families the model does not describe (GC,
//! exception handling, stack switching, tail calls, wide arithmetic, typed
//! references, `memory.discard`, segment-indexed table operations).
//!
//! No Inference program can reach any of this — the language has no floats, no
//! vectors, and emits no conversion — so these arms are reachable only through
//! foreign bytes, via the external linking path or [`wasm_parser::translate_bytes`].
//! `core/wasm-linker` refuses the same content in external modules, making this
//! the second of two layers.
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
//! - [WASM Codegen](../wasm-codegen/README.md) - WebAssembly code generation
//! - [Inference Compiler](../inference/README.md) - Main compiler orchestration
//! - [Rocq Documentation](https://rocq-prover.org/) - Rocq proof assistant
//! - [WebAssembly Specification](https://webassembly.github.io/spec/) - WASM standard

pub mod errors;
mod gallina;
mod hassert_print;
pub mod rocq_names;
pub mod translator;
pub mod wasm_parser;

/// Name of the WASM custom section that carries spec-originated function
/// indices grouped by spec name. Authoritative for standalone-binary
/// translation when callers pass an empty explicit spec map.
///
/// Re-exported from `inference_wasm_codegen` so the encoder and decoder
/// share a single source of truth for the wire-format constant.
pub use inference_wasm_codegen::SPEC_FUNCS_SECTION_NAME;

/// Wire-format version of the `inference.spec_funcs` payload. Re-exported
/// from `inference_wasm_codegen` so the decoder and encoder agree on the
/// expected leading varuint32.
pub use inference_wasm_codegen::SPEC_FUNCS_SECTION_VERSION;

#[cfg(test)]
mod tests {
    use super::wasm_parser::translate_bytes;
    use rustc_hash::FxHashMap;
    use std::fs;
    use std::panic;
    use std::path::PathBuf;

    #[test]
    #[cfg_attr(miri, ignore)]
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
                let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
                translate_bytes(
                    module_name,
                    &bytes,
                    &empty,
                    &inference_hassert::HSpecMap::default(),
                )
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

    #[test]
    #[cfg_attr(miri, ignore)]
    fn translate_bytes_emits_per_spec_definition_and_theorem() {
        let test_data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data");
        let bytes = fs::read(test_data_dir.join("fac.0.wasm")).expect("read fac.0.wasm");

        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        // A spec with no function indices exercises the per-spec emission path
        // without omitting any of `fac`'s real functions (arbitrary indices
        // would drop functions and shift every call). Its obligation list is
        // therefore the explicitly-typed empty list. Spec name `Spec1` avoids
        // shadowing the Peano successor `S`.
        map.insert("Spec1".to_string(), vec![]);
        let output = translate_bytes("Fac", &bytes, &map, &inference_hassert::HSpecMap::default())
            .expect("translate succeeds");

        assert!(
            output.contains("Definition Fac__Spec1_specs : list hassert := (@nil hassert)."),
            "output should contain the Fac__Spec1_specs obligation list; got:\n{output}",
        );
        assert!(
            output.contains("Theorem valid_Fac__Spec1 : ValidSpec Fac Fac__Spec1_specs."),
            "output should contain the per-spec ValidSpec theorem; got:\n{output}",
        );
        assert!(
            output.contains("Theorem valid_Fac : ValidModule Fac."),
            "output should always contain the 1-ary ValidModule theorem; got:\n{output}",
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn translate_bytes_emits_no_spec_lines_when_empty() {
        let test_data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data");
        let bytes = fs::read(test_data_dir.join("fac.0.wasm")).expect("read fac.0.wasm");

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let output = translate_bytes(
            "Fac",
            &bytes,
            &empty,
            &inference_hassert::HSpecMap::default(),
        )
        .expect("translate succeeds");

        assert!(
            !output.contains("_specs : list hassert"),
            "output should contain no per-spec definitions when the map is empty; got:\n{output}",
        );
        assert!(
            !output.contains("ValidSpec "),
            "output should contain no per-spec theorem when the spec map is empty; got:\n{output}",
        );
        // The 1-ary module theorem is emitted for every module, spec-bearing or
        // not.
        assert!(
            output.contains("Theorem valid_Fac : ValidModule Fac."),
            "output should always contain the module theorem; got:\n{output}",
        );
    }

    /// The flip's own remap guard: a spec function sitting BETWEEN two executable
    /// functions is omitted from the module record, and a surviving cross-call to
    /// a function ABOVE it must be renumbered down. Here `func 0` calls `func 2`
    /// while `func 1` is the omitted spec function, so the emitted body must read
    /// `BI_call 1%N` (not `2`), the omitted function contributes no `Definition`,
    /// and the two survivors remain. The `coqc` gate catches shape errors but not
    /// a wrong index, so this operand assertion carries that load.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn omitting_a_spec_function_renumbers_a_surviving_cross_call() {
        let bytes = wat::parse_str(
            r#"
            (module
              (func (;0;) (result i32) call 2)
              (func (;1;) (result i32) i32.const 0)
              (func (;2;) (result i32) i32.const 7))
            "#,
        )
        .expect("remap fixture assembles");

        // Mark `func 1` as the spec function (omitted). No obligations.
        let mut map: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        map.insert("Between".to_string(), vec![1]);
        let output = translate_bytes(
            "Prog",
            &bytes,
            &map,
            &inference_hassert::HSpecMap::default(),
        )
        .expect("translate succeeds");

        assert!(
            output.contains("BI_call 1%N"),
            "the cross-call to func 2 must be renumbered to 1 past the omitted spec \
             function at index 1; got:\n{output}",
        );
        assert!(
            !output.contains("BI_call 2%N"),
            "the original (unremapped) `BI_call 2` must not survive; got:\n{output}",
        );
        assert!(
            !output.contains("Definition func_1 :"),
            "the omitted spec function must contribute no `Definition`; got:\n{output}",
        );
        assert!(
            output.contains("Definition func_0 :") && output.contains("Definition func_2 :"),
            "both surviving executable functions must be emitted; got:\n{output}",
        );
    }
}

/// Robustness tests for the external `.wasm` static-linking path through
/// `wasm-to-v` (Issue #9 robustness audit, work unit 7).
///
/// These assemble the kind of module a static merge produces — a merged
/// external inner function sharing a name with a main-module function, and
/// bodies bearing typed-reference / exception-handling operators copied
/// verbatim from an adversarial external — and assert the CLEAN outcome:
/// globally-unique Rocq `Definition`s, and a recoverable
/// [`WasmToVError::UnsupportedFeature`] instead of a panic.
#[cfg(test)]
mod link_robustness {
    use super::errors::WasmToVError;
    use super::wasm_parser::translate_bytes;
    use rustc_hash::FxHashMap;

    fn translate(wat: &str) -> anyhow::Result<String> {
        let bytes = wat::parse_str(wat).expect("fixture WAT assembles");
        translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
    }

    /// H20: a merged module whose external inner function shares a name with a
    /// main-module function must yield distinct Rocq `Definition`s (Coq cannot
    /// overload), and the `mod_funcs` list must reference each unique name.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn duplicate_function_names_are_disambiguated() {
        // A module whose `name` section maps both function indices to the
        // identical string `add_three`, modelling a main-module `add_three`
        // (index 0) next to a merged external `add_three` (index 1).
        let bytes = duplicate_named_module();
        let output = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("translation succeeds");

        let definitions = output.matches("Definition add_three :").count();
        assert_eq!(
            definitions, 1,
            "exactly one `Definition add_three` may be emitted; got {definitions}:\n{output}",
        );
        // The colliding second function must be emitted under a disambiguated
        // name derived from its WASM function index.
        assert!(
            output.contains("Definition add_three_1 :"),
            "second `add_three` should be disambiguated to `add_three_1`:\n{output}",
        );
        // Both unique names must appear in the `mod_funcs` list so the proof
        // deliverable references both bodies.
        assert!(
            output.contains("add_three ::") && output.contains("add_three_1 ::"),
            "mod_funcs must list both disambiguated names:\n{output}",
        );
    }

    /// Hand-encodes a 2-function module whose `name` section maps both function
    /// indices to the identical string `add_three`. `wat` cannot express a
    /// name-section collision from symbolic identifiers, so we emit the bytes
    /// directly.
    fn duplicate_named_module() -> Vec<u8> {
        // Assemble a valid skeleton with `wat`, then append a `name` section
        // naming both functions `add_three`.
        let skeleton = wat::parse_str(
            r#"
            (module
              (func (param i32) (result i32) local.get 0 i32.const 100 i32.add)
              (func (param i32) (result i32) local.get 0 i32.const 3 i32.add))
            "#,
        )
        .expect("skeleton assembles");

        // name section: id=0 (custom), name "name"; subsection id=1 (function
        // names) with 2 entries, both "add_three".
        let func_name = b"add_three";
        let mut func_subsec = Vec::new();
        func_subsec.push(2u8); // count
        for idx in 0u8..2 {
            func_subsec.push(idx); // func index (LEB128, single byte for <128)
            func_subsec.push(func_name.len() as u8);
            func_subsec.extend_from_slice(func_name);
        }
        let mut name_payload = Vec::new();
        name_payload.push(0x04); // length of "name"
        name_payload.extend_from_slice(b"name");
        name_payload.push(0x01); // subsection id: function names
        name_payload.push(func_subsec.len() as u8);
        name_payload.extend_from_slice(&func_subsec);

        let mut bytes = skeleton;
        bytes.push(0x00); // custom section id
        bytes.push(name_payload.len() as u8);
        bytes.extend_from_slice(&name_payload);
        bytes
    }

    /// H13: a `ref.null` copied verbatim from an adversarial external must
    /// surface as a recoverable [`WasmToVError::UnsupportedFeature`], never a
    /// `todo!()` panic.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn ref_null_is_unsupported_feature_not_panic() {
        let err = translate(
            r#"
            (module
              (func (export "f") (result i32)
                ref.null func
                drop
                i32.const 0))
            "#,
        )
        .expect_err("ref.null must be rejected");

        let downcast = err.downcast_ref::<WasmToVError>();
        assert!(
            matches!(downcast, Some(WasmToVError::UnsupportedFeature { .. })),
            "ref.null should surface as UnsupportedFeature; got: {err:?}",
        );
    }

    /// H13: `call_ref` likewise must be a recoverable error rather than a
    /// panic on the `-v` path.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn call_ref_is_unsupported_feature_not_panic() {
        let err = translate(
            r#"
            (module
              (type $sig (func (result i32)))
              (func (export "f") (result i32)
                ref.null $sig
                call_ref $sig))
            "#,
        )
        .expect_err("call_ref must be rejected");

        let downcast = err.downcast_ref::<WasmToVError>();
        assert!(
            matches!(downcast, Some(WasmToVError::UnsupportedFeature { .. })),
            "call_ref should surface as UnsupportedFeature; got: {err:?}",
        );
    }

    /// Assembles a one-function module whose body nests `depth` empty `block`s,
    /// mirroring the adversarially deep external the linker would otherwise
    /// merge before handing it to the translator.
    fn nested_blocks_module(depth: usize) -> Vec<u8> {
        let mut body = String::new();
        for _ in 0..depth {
            body.push_str("block ");
        }
        for _ in 0..depth {
            body.push_str("end ");
        }
        let wat = format!(r#"(module (func (export "f") {body}))"#);
        wat::parse_str(&wat).expect("nested-blocks WAT assembles")
    }

    /// H-3: a deeply-nested external body must surface as a recoverable
    /// [`WasmToVError::UnsupportedFeature`] rather than overflowing the
    /// translator's stack (an unrecoverable SIGABRT) on the `-v` proof path.
    ///
    /// The translator recurses once per nesting level both when building the
    /// expression tree (`translate_expression`) and when rendering it
    /// (`print_with_offset`); without a depth bound a body of a few thousand
    /// nested blocks aborts the process. A depth well past the cap must fail
    /// cleanly.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn deeply_nested_body_is_unsupported_feature_not_stack_overflow() {
        let bytes = nested_blocks_module(5_000);
        let err = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect_err("a deeply-nested body must be rejected, not abort");

        let downcast = err.downcast_ref::<WasmToVError>();
        assert!(
            matches!(downcast, Some(WasmToVError::UnsupportedFeature { .. })),
            "deep nesting should surface as UnsupportedFeature; got: {err:?}",
        );
    }

    /// H-3: a body nested *up to* the cap still translates cleanly, so the
    /// guard rejects only pathological depth, never a legitimately nested
    /// function.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn body_nested_within_the_cap_translates() {
        let bytes = nested_blocks_module(16);
        translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("a modestly-nested body translates");
    }

    /// Assembles a 2-function module with *no* name section: an exported `sum`
    /// (index 0) that calls an anonymous inner `func 1`. Models the supply path
    /// issue #9 serves — a third-party / `wasm-tools`-stripped external whose
    /// inner callees carry no debug name.
    fn nameless_two_function_module() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
              (func (export "sum") (param i32) (result i32)
                local.get 0 call 1)
              (func (param i32) (result i32)
                local.get 0 i32.const 1 i32.add))
            "#,
        )
        .expect("nameless module assembles")
    }

    /// H-4: a nameless function must receive a deterministic name derived from
    /// its output function index (`func_<idx>`), not a per-process random UUID,
    /// so the `.v` is byte-identical across runs for byte-identical input.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn nameless_functions_get_deterministic_names_and_reproducible_v() {
        let bytes = nameless_two_function_module();

        let first = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("first translation succeeds");
        let second = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("second translation succeeds");

        assert_eq!(
            first, second,
            "byte-identical input must produce a byte-identical `.v` across runs",
        );
        // Every nameless function is named from its output index; no random UUID
        // name leaks into the proof artifact.
        assert!(
            first.contains("Definition func_0 :") && first.contains("Definition func_1 :"),
            "nameless functions should be named `func_0`/`func_1` from their index:\n{first}",
        );
    }

    /// Assembles a 2-function module whose name section names only the exported
    /// root (`func 0` = `sum`), leaving the inner callee (`func 1`) nameless.
    /// Mirrors a static-merge output with a named closure root next to a
    /// nameless inner callee, exercising the translator's index-derived
    /// fallback in isolation.
    fn root_named_inner_nameless_module() -> Vec<u8> {
        let skeleton = wat::parse_str(
            r#"
            (module
              (func (param i32) (result i32) local.get 0 call 1)
              (func (param i32) (result i32) local.get 0 i32.const 1 i32.add))
            "#,
        )
        .expect("skeleton assembles");

        // name section: id=0 (custom), name "name"; subsection id=1 (function
        // names) with a single entry naming function 0 `sum`.
        let func_name = b"sum";
        let mut func_subsec = Vec::new();
        func_subsec.push(1u8); // count
        func_subsec.push(0u8); // func index 0
        func_subsec.push(func_name.len() as u8);
        func_subsec.extend_from_slice(func_name);

        let mut name_payload = Vec::new();
        name_payload.push(0x04); // length of "name"
        name_payload.extend_from_slice(b"name");
        name_payload.push(0x01); // subsection id: function names
        name_payload.push(func_subsec.len() as u8);
        name_payload.extend_from_slice(&func_subsec);

        let mut bytes = skeleton;
        bytes.push(0x00); // custom section id
        bytes.push(name_payload.len() as u8);
        bytes.extend_from_slice(&name_payload);
        bytes
    }

    /// H-4: when only the closure root carries a name, the nameless inner
    /// callee still gets a deterministic index-derived name and the artifact is
    /// reproducible — the named root keeps `sum`, the inner callee is `func_1`,
    /// and no UUID appears.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn nameless_inner_callee_with_named_root_is_deterministic() {
        let bytes = root_named_inner_nameless_module();

        let first = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("first translation succeeds");
        let second = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("second translation succeeds");

        assert_eq!(
            first, second,
            "byte-identical input must produce a byte-identical `.v` across runs",
        );
        // The root keeps its source name (sanitized for Rocq — `sum` collides
        // with a stdlib name and is suffixed to `sum_`), distinct from the
        // index-derived fallback the inner callee receives.
        assert!(
            first.contains("Definition sum_ :"),
            "the named root keeps its `sum`-derived name:\n{first}",
        );
        assert!(
            first.contains("Definition func_1 :"),
            "the nameless inner callee should be `func_1` from its index:\n{first}",
        );
    }

    /// D6: `function_bodies` is 0-based over the code section, but the name
    /// section keys on the *absolute* WASM function index, which numbers
    /// imported functions first. `translate_functions` offsets the body
    /// position by the function-import count to recover the absolute index.
    ///
    /// This module imports `host` (absolute index 0) and defines `local`
    /// (absolute index 1). The single code-section body is `local`; its
    /// name-section entry lives under absolute index 1. Without the offset the
    /// translator would look up index 0 and emit the body under the *import's*
    /// name (`host`) — a silently mis-named proof obligation. The offset must
    /// give it the correct name `local`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn function_import_offsets_the_name_lookup() {
        let bytes = wat::parse_str(
            r#"
            (module
              (import "env" "host" (func $host (param i32) (result i32)))
              (func $local (param i32) (result i32) local.get 0 i32.const 1 i32.add))
            "#,
        )
        .expect("import fixture WAT assembles");

        let output = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("an import-bearing module translates");

        assert!(
            output.contains("Definition local :"),
            "the sole defined function must be named from its absolute index (1 -> `local`), \
             not the import's index (0 -> `host`):\n{output}",
        );
        assert!(
            !output.contains("Definition host :"),
            "the import's name must never be emitted as a defined `module_func`:\n{output}",
        );
    }

    /// D6 companion: with no name section, the fallback name is derived from the
    /// *absolute* index too, so the offset is exercised even without debug
    /// names. The import occupies absolute index 0, so the single defined body
    /// is `func_1`, never `func_0`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn function_import_offsets_the_nameless_fallback() {
        // Assemble a named skeleton, then strip the name section so the
        // translator falls back to index-derived names.
        let with_names = wat::parse_str(
            r#"
            (module
              (import "env" "host" (func (param i32) (result i32)))
              (func (param i32) (result i32) local.get 0 i32.const 1 i32.add))
            "#,
        )
        .expect("import fixture WAT assembles");

        let output = translate_bytes(
            "Prog",
            &with_names,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("an import-bearing nameless module translates");

        assert!(
            output.contains("Definition func_1 :"),
            "the nameless defined body sits at absolute index 1, so it must be `func_1`:\n{output}",
        );
        assert!(
            !output.contains("Definition func_0 :"),
            "absolute index 0 belongs to the import, so `func_0` must not be a defined \
             function:\n{output}",
        );
    }

    /// D6 companion: a non-function import (a memory) does not occupy a function
    /// index, so the function-import offset stays 0 and the sole defined body
    /// keeps absolute index 0. Guards against over-counting non-function
    /// imports in the offset.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn non_function_import_does_not_offset_function_indices() {
        let bytes = wat::parse_str(
            r#"
            (module
              (import "env" "mem" (memory 1))
              (func $only (param i32) (result i32) local.get 0 i32.const 1 i32.add))
            "#,
        )
        .expect("memory-import fixture WAT assembles");

        let output = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("a module whose only import is a memory translates");

        // The defined function sits at absolute index 0 (no function imports),
        // so it keeps its source name with no index perturbation.
        assert!(
            output.contains("Definition only :"),
            "a non-function import must not shift the defined function's index:\n{output}",
        );
    }

    /// Slices the emitted `.v` from one `Definition <name> : module_func :=`
    /// header up to the next `Definition`, so an assertion about one function's
    /// body cannot be satisfied by a neighbour's text.
    fn definition_body<'a>(output: &'a str, name: &str) -> &'a str {
        let header = format!("Definition {name} : module_func :=");
        let start = output
            .find(&header)
            .unwrap_or_else(|| panic!("`{header}` must be emitted:\n{output}"));
        let body = &output[start + header.len()..];
        match body.find("Definition ") {
            Some(end) => &body[..end],
            None => body,
        }
    }

    /// The name section keys local names on the *function* index, so the
    /// `(*name*)` comments on `BI_local_get` / `BI_local_set` / `BI_local_tee`
    /// must be resolved with that index — not with the function's *type* index,
    /// which diverges the moment two functions share one type-section entry.
    ///
    /// `$a` and `$c` have the same signature, so the WAT assembler interns one
    /// type entry for both: `$c` is function index 2 but type index 0. Resolving
    /// by type index hands `$c` the local names of `$a`, labelling one
    /// function's body with another's parameter name. `$c` exercises all three
    /// name-bearing operators, since each carries the comment on its own arm.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn local_names_resolve_by_function_index_not_type_index() {
        let bytes = wat::parse_str(
            r#"
            (module
              (func $a (param $alpha i32) (result i32) local.get $alpha)
              (func $b (param $beta i64) (result i64) local.get $beta)
              (func $c (param $gamma i32) (result i32)
                (local $delta i32)
                local.get $gamma
                local.set $delta
                local.get $delta
                local.tee $delta))
            "#,
        )
        .expect("type-sharing fixture WAT assembles");

        let output = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("a module sharing one type across two functions translates");

        assert!(
            definition_body(&output, "a").contains("BI_local_get 0%N (*alpha*)"),
            "function index 0 must carry its own local name:\n{output}",
        );
        assert!(
            definition_body(&output, "b").contains("BI_local_get 0%N (*beta*)"),
            "function index 1 must carry its own local name:\n{output}",
        );

        let c = definition_body(&output, "c");
        for expected in [
            "BI_local_get 0%N (*gamma*)",
            "BI_local_set 1%N (*delta*)",
            "BI_local_get 1%N (*delta*)",
            "BI_local_tee 1%N (*delta*)",
        ] {
            assert!(
                c.contains(expected),
                "function index 2 must carry its own local names even though it shares \
                 function 0's type index; missing `{expected}`:\n{output}",
            );
        }
        assert!(
            !c.contains("(*alpha*)"),
            "function index 2 must not inherit function 0's local names:\n{output}",
        );
    }

    /// The name section numbers imported functions first, whereas the function
    /// section's type indices do not, so a single function import is enough to
    /// make the two numberings disagree for every defined body. Resolving by
    /// type index then hands `$second` the names of `$first` and leaves
    /// `$first` with the import's (absent) names.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn local_names_resolve_by_absolute_index_under_a_function_import() {
        let bytes = wat::parse_str(
            r#"
            (module
              (import "env" "host" (func (param i32) (result i32)))
              (func $first (param $x i32) (result i32) local.get $x)
              (func $second (param $y i64) (result i64) local.get $y))
            "#,
        )
        .expect("import fixture WAT assembles");

        let output = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("an import-bearing module with named locals translates");

        assert!(
            definition_body(&output, "first").contains("BI_local_get 0%N (*x*)"),
            "absolute index 1 must keep the local names the name section gives it:\n{output}",
        );
        let second = definition_body(&output, "second");
        assert!(
            second.contains("BI_local_get 0%N (*y*)"),
            "absolute index 2 must carry its own local name:\n{output}",
        );
        assert!(
            !second.contains("(*x*)"),
            "absolute index 2 must not inherit the preceding function's local names:\n{output}",
        );
    }
}

/// Fail-closed rejection of every construct outside the wasm-verifier proof
/// contract (mirrored in-repo by the vendored stub): floating-point, SIMD/vector,
/// the conversion (`cvtop`) family, and the proposal families that previously hit
/// `todo!()`.
///
/// The stub in `rocq-stub/` declares `number_type` with only `T_i32`/`T_i64`, no
/// `T_v128`, and no `cvtop`/`BI_cvtop` (see its README "Scope"). Every fixture here
/// therefore has no honest lowering: the translator must say so with a recoverable
/// [`WasmToVError::UnsupportedFeature`] naming the construct, rather than emit a
/// term the proof target cannot type, or abort the process.
///
/// Two failure modes are pinned, because both existed before this change:
///
/// * **silent ill-typed emission** — the float comparison arms emitted the *integer*
///   relop family inside the float wrapper (`BI_relop T_f32 (Relop_f ROI_eq)`), where
///   `Relop_f` wants `ROF_*` and `ROI_ge` is an unapplied function awaiting an `sx`.
///   Nothing caught it: the `coqc` gate's corpus is Inference source, and no Inference
///   program lowers to float WASM.
/// * **`todo!()` panic** — sign-extension, saturating truncation, most SIMD, and nine
///   proposal families aborted the process instead of returning. On the linking path
///   that is strictly worse than the bug being fixed.
///
/// # Two invariants these fixtures are built around
///
/// **A float or vector may only be materialized by a `const` or by a load's result —
/// never by a parameter, result, local, global, or block type.** The type section
/// renders before any body, so a float in a signature steals the error from the
/// operator under test; and since "floating-point" appears in *both* the operator
/// and the value-type message, a class-adjective assertion would keep passing while
/// silently exercising the wrong arm. Every fixture below drops its float/vector
/// result instead of returning it.
///
/// **Every assertion pins the operator's debug name as its primary needle**, class
/// adjective secondary. `translate_value_type` never prints an operator name, so the
/// operator name is the only thing that discriminates which arm fired.
///
/// # Why there are two tiers of fixture
///
/// Those invariants together make one group of operators unreachable from WAT. An
/// operator that *consumes* a float or vector needs an operand; the operand may only
/// come from a const; and the const's arm rejects first — so `F32Add`, `I8x16Eq` and
/// the rest could only ever pin the const's name, leaving their own arms untested.
///
/// * **WAT tier** — operators that consume only integers and merely *produce* a
///   float/vector (`f32.const`, `f32.load`, `f32.convert_i32_s`, `v128.const`,
///   `i8x16.splat`, …) plus the float-free integer conversions. These are reachable
///   from ordinary WAT and pin their own operator.
/// * **Hand-encoded tier** — the consuming operators, in a module whose single body
///   holds the bare opcode. `wat` cannot assemble this (the body is stack-invalid),
///   but the translator is a *parser*, not a validator: it walks the operator
///   sequence, so the arm is reached directly. This is the only way `F32Eq` — the
///   very operator the issue reports — is individually pinned, and it is also how
///   the ill-typed `Relop_f ROI_eq` emission is reproduced in isolation. The
///   precedent for hand-encoding what `wat` cannot express is `duplicate_named_module`
///   in `link_robustness`. [`raw_body_harness_translates_a_supported_operator`] keeps
///   the harness honest, so a rejection in this tier is always attributable to the
///   operator rather than to a malformed fixture.
#[cfg(test)]
mod unsupported_surface {
    use super::errors::WasmToVError;
    use super::wasm_parser::translate_bytes;
    use rustc_hash::FxHashMap;

    /// Translates `bytes` with empty spec/hspec maps — the standalone-binary path a
    /// `wasm_to_v` over foreign WASM takes.
    fn translate(bytes: &[u8]) -> anyhow::Result<String> {
        translate_bytes(
            "Prog",
            bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
    }

    /// A module with one `() -> ()` function whose body is `opcode` then `end`,
    /// hand-encoded because the body is stack-invalid and `wat` would reject it.
    /// `opcode` carries any immediates the instruction needs (a `memarg`, a SIMD
    /// prefix byte). Section and body lengths are single-byte LEB128, which holds
    /// for every opcode sequence here (all well under 128 bytes).
    fn module_with_raw_body(opcode: &[u8]) -> Vec<u8> {
        let mut body = vec![0x00]; // zero local declarations
        body.extend_from_slice(opcode);
        body.push(0x0b); // end

        let mut code = vec![0x01]; // one function body
        code.push(body.len() as u8);
        code.extend_from_slice(&body);

        let mut module = vec![
            0x00, 0x61, 0x73, 0x6d, // magic
            0x01, 0x00, 0x00, 0x00, // version
            0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // type section: () -> ()
            0x03, 0x02, 0x01, 0x00, // function section: one func, type 0
        ];
        module.push(0x0a); // code section id
        module.push(code.len() as u8);
        module.extend_from_slice(&code);
        module
    }

    /// The contract every row shares: translation returns a recoverable
    /// [`WasmToVError::UnsupportedFeature`] whose description contains each of
    /// `needles` (lowercased comparison, so an operator debug name is written
    /// `f32load` rather than `F32Load`).
    ///
    /// Deliberately no `catch_unwind`: a `todo!()` still reachable for one of these
    /// constructs fails the test as a panic, which is exactly the outcome this module
    /// exists to rule out.
    ///
    /// This is the single place phrasing is pinned. If review moves a message, retune
    /// the needle sets at their call sites — no row inspects the error any other way.
    fn assert_rejected(label: &str, bytes: &[u8], needles: &[&str]) {
        let err = translate(bytes)
            .map(|v| panic!("{label}: must be rejected, but a `.v` was emitted:\n{v}"))
            .unwrap_err();

        let Some(WasmToVError::UnsupportedFeature { description }) =
            err.downcast_ref::<WasmToVError>()
        else {
            panic!("{label}: expected UnsupportedFeature, got {err:?}");
        };

        let lowered = description.to_lowercase();
        for needle in needles {
            assert!(
                lowered.contains(needle),
                "{label}: the description must name `{needle}`; got {description:?}"
            );
        }
    }

    /// [`assert_rejected`] for the WAT tier.
    fn assert_wat_rejected(label: &str, wat: &str, needles: &[&str]) {
        let bytes = wat::parse_str(wat).expect("fixture WAT assembles");
        assert_rejected(label, &bytes, needles);
    }

    /// [`assert_rejected`] for the hand-encoded tier.
    fn assert_raw_rejected(label: &str, opcode: &[u8], needles: &[&str]) {
        assert_rejected(label, &module_with_raw_body(opcode), needles);
    }

    /// Guards the hand-encoded tier itself: the same harness carrying a *supported*
    /// integer operator must translate cleanly. Without this, a malformed skeleton
    /// would make every raw-tier row pass for the wrong reason.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn raw_body_harness_translates_a_supported_operator() {
        let v = translate(&module_with_raw_body(&[0x6a])) // i32.add
            .expect("the raw-body harness must produce a translatable module");
        assert!(
            v.contains("BI_binop T_i32 (Binop_i BOI_add)"),
            "the harness must lower its opcode as the operator it encodes:\n{v}"
        );
    }

    // == WAT tier: operators that only PRODUCE a float/vector ==============

    /// A float constant alone — the narrowest float fixture there is.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn float_constants_are_rejected() {
        assert_wat_rejected(
            "f32.const",
            r#"(module (func (export "f") f32.const 1 drop))"#,
            &["f32const", "floating-point"],
        );
        assert_wat_rejected(
            "f64.const",
            r#"(module (func (export "f") f64.const 1 drop))"#,
            &["f64const", "floating-point"],
        );
    }

    /// A float load consumes only an `i32` address, so the float appears
    /// solely as the load's result and the load's own arm is what rejects.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn float_loads_are_rejected() {
        assert_wat_rejected(
            "f32.load",
            r#"(module (memory 1) (func (export "f") i32.const 0 f32.load drop))"#,
            &["f32load", "floating-point"],
        );
        assert_wat_rejected(
            "f64.load",
            r#"(module (memory 1) (func (export "f") i32.const 0 f64.load drop))"#,
            &["f64load", "floating-point"],
        );
    }

    /// Conversions *into* a float take an integer operand, so unlike the
    /// float-source conversions they need no float const ahead of them and pin their
    /// own operator. `f32.reinterpret_i32` covers the reinterpret direction that is
    /// likewise integer-sourced.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn integer_sourced_float_conversions_are_rejected() {
        assert_wat_rejected(
            "f32.convert_i32_s",
            r#"(module (func (export "f") i32.const 1 f32.convert_i32_s drop))"#,
            &["f32converti32s", "conversion"],
        );
        assert_wat_rejected(
            "f64.convert_i32_u",
            r#"(module (func (export "f") i32.const 1 f64.convert_i32_u drop))"#,
            &["f64converti32u", "conversion"],
        );
        assert_wat_rejected(
            "f32.reinterpret_i32",
            r#"(module (func (export "f") i32.const 1 f32.reinterpret_i32 drop))"#,
            &["f32reinterpreti32", "conversion"],
        );
        assert_wat_rejected(
            "f64.reinterpret_i64",
            r#"(module (func (export "f") i64.const 1 f64.reinterpret_i64 drop))"#,
            &["f64reinterpreti64", "conversion"],
        );
    }

    /// The integer-only width conversions: no float anywhere, so nothing can
    /// steal the error. They make the scope explicit — the stub declares no
    /// `BI_cvtop` at all, so even an integer-to-integer conversion has no lowering.
    ///
    /// The wasm-linker's allow-list mirrors this: it rejects these three at link
    /// time for the same reason (an allow-listed operator must have a translator
    /// lowering, and no `cvtop` lowering exists under the proof contract).
    #[test]
    #[cfg_attr(miri, ignore)]
    fn integer_width_conversions_are_rejected() {
        assert_wat_rejected(
            "i32.wrap_i64",
            r#"(module (func (export "f") i64.const 1 i32.wrap_i64 drop))"#,
            &["i32wrapi64", "conversion"],
        );
        assert_wat_rejected(
            "i64.extend_i32_s",
            r#"(module (func (export "f") i32.const 1 i64.extend_i32_s drop))"#,
            &["i64extendi32s", "conversion"],
        );
        assert_wat_rejected(
            "i64.extend_i32_u",
            r#"(module (func (export "f") i32.const 1 i64.extend_i32_u drop))"#,
            &["i64extendi32u", "conversion"],
        );
    }

    /// Sign extension, a `todo!()` panic before this change, folded into the
    /// conversion class. Integer-only, so each pins its own operator.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn sign_extension_operators_are_rejected_not_panic() {
        assert_wat_rejected(
            "i32.extend8_s",
            r#"(module (func (export "f") i32.const 1 i32.extend8_s drop))"#,
            &["i32extend8s", "conversion"],
        );
        assert_wat_rejected(
            "i64.extend32_s",
            r#"(module (func (export "f") i64.const 1 i64.extend32_s drop))"#,
            &["i64extend32s", "conversion"],
        );
    }

    /// The vector operators reachable from WAT: a constant, a load (`i32`
    /// address only), and a splat (`i32` operand). `v128.const` and `v128.load`
    /// emitted `BI_const_vec`/`BI_load_vec`, neither declared by the stub.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn vector_producing_operators_are_rejected_not_panic() {
        assert_wat_rejected(
            "v128.const",
            r#"(module (func (export "f") v128.const i32x4 1 2 3 4 drop))"#,
            &["v128const", "vector"],
        );
        assert_wat_rejected(
            "v128.load",
            r#"(module (memory 1) (func (export "f") i32.const 0 v128.load drop))"#,
            &["v128load", "vector"],
        );
        assert_wat_rejected(
            "i8x16.splat",
            r#"(module (func (export "f") i32.const 1 i8x16.splat drop))"#,
            &["i8x16splat", "vector"],
        );
    }

    // == Hand-encoded tier: operators that CONSUME a float/vector ==========

    /// The operators the issue actually reports. Each emitted
    /// `BI_relop T_f32 (Relop_f ROI_*)` — the float wrapper around the integer
    /// family, with `ROI_ge` left unapplied. Only the raw tier can reach them: a WAT
    /// fixture would need two float operands, and the const feeding them rejects
    /// first.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn float_comparisons_are_rejected() {
        assert_raw_rejected("f32.eq", &[0x5b], &["f32eq", "floating-point"]);
        assert_raw_rejected("f32.lt", &[0x5d], &["f32lt", "floating-point"]);
        assert_raw_rejected("f64.ge", &[0x66], &["f64ge", "floating-point"]);
    }

    /// Float binops and unops, whose `Binop_f`/`Unop_f` families the stub
    /// omits.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn float_arithmetic_is_rejected() {
        assert_raw_rejected("f32.add", &[0x92], &["f32add", "floating-point"]);
        assert_raw_rejected("f64.sqrt", &[0x9f], &["f64sqrt", "floating-point"]);
        assert_raw_rejected("f32.copysign", &[0x98], &["f32copysign", "floating-point"]);
    }

    /// Float stores consume the value they write, so they too are raw-tier.
    /// The opcode carries its `memarg` (alignment, offset).
    #[test]
    #[cfg_attr(miri, ignore)]
    fn float_stores_are_rejected() {
        assert_raw_rejected(
            "f32.store",
            &[0x38, 0x02, 0x00],
            &["f32store", "floating-point"],
        );
        assert_raw_rejected(
            "f64.store",
            &[0x39, 0x03, 0x00],
            &["f64store", "floating-point"],
        );
    }

    /// The float-*source* conversions, unreachable from WAT because each needs a
    /// float operand that only a rejecting const could supply. In the raw tier each pins its own
    /// operator instead of degrading to a class-only assertion. `i32.trunc_sat_f32_s`
    /// was a `todo!()` panic.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn float_sourced_conversions_are_rejected_not_panic() {
        assert_raw_rejected("i32.trunc_f32_s", &[0xa8], &["i32truncf32s", "conversion"]);
        assert_raw_rejected("f32.demote_f64", &[0xb6], &["f32demotef64", "conversion"]);
        assert_raw_rejected(
            "i64.reinterpret_f64",
            &[0xbd],
            &["i64reinterpretf64", "conversion"],
        );
        assert_raw_rejected(
            "i32.trunc_sat_f32_s",
            &[0xfc, 0x00],
            &["i32truncsatf32s", "conversion"],
        );
    }

    /// The lane-wise vector operators, all `todo!()` panics before this
    /// change. Raw-tier for the same reason as the float relops: their operands can
    /// only come from a `v128.const`, which would reject first. The SIMD prefix
    /// `0xfd` plus a LEB128 sub-opcode is exactly what the parser must still decode
    /// before dispatching, so these also cover the prefixed-opcode path.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn lane_wise_vector_operators_are_rejected_not_panic() {
        assert_raw_rejected("i8x16.eq", &[0xfd, 0x23], &["i8x16eq", "vector"]);
        assert_raw_rejected("f32x4.add", &[0xfd, 0xe4, 0x01], &["f32x4add", "vector"]);
    }

    // == Value types, with no unsupported operator anywhere ================

    /// A float or vector that never reaches an operator must still be rejected: a
    /// float parameter is emitted through the *type* section as
    /// `Tf (T_num T_f32 :: nil) (nil)`, naming a `T_f32` the stub does not declare —
    /// the same unverifiable `.v` the operator arms produced, reachable with no float
    /// instruction at all.
    ///
    /// `translate_value_type` is the single chokepoint for all six positions, so each
    /// row enters through a different one, and each also asserts the role clause the
    /// message carries so a mis-threaded role surfaces here. The result and block
    /// rows use `unreachable` to satisfy the type without a float constant. The
    /// global row is safe despite its `f32.const` initializer: `translate_global`
    /// resolves the value type before the init expression.
    ///
    /// These messages spell the type in wat form with no debug spelling, so the type
    /// token plus `"value type"` is what identifies the arm.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn float_and_vector_value_types_are_rejected() {
        assert_wat_rejected(
            "param f32",
            r#"(module (func (export "f") (param f32)))"#,
            &["value type", "f32", "floating-point", "parameter"],
        );
        assert_wat_rejected(
            "local f64",
            r#"(module (func (export "f") (local f64)))"#,
            &["value type", "f64", "floating-point", "local"],
        );
        assert_wat_rejected(
            "result f32",
            r#"(module (func (export "f") (result f32) unreachable))"#,
            &["value type", "f32", "floating-point", "result"],
        );
        assert_wat_rejected(
            "global f32",
            r#"(module (global (export "g") f32 (f32.const 1)))"#,
            &["value type", "f32", "floating-point", "global"],
        );
        assert_wat_rejected(
            "param v128",
            r#"(module (func (export "f") (param v128)))"#,
            &["value type", "v128", "vector", "parameter"],
        );
        assert_wat_rejected(
            "block result type f32",
            r#"(module (func (export "f") block (result f32) unreachable end drop))"#,
            &["value type", "f32", "floating-point", "block result"],
        );
    }

    // == Unmodeled proposal families, one row each =========================

    /// Every proposal family that previously hit `todo!()` now returns a grouped
    /// family error. One row each, so no family can silently fall through to the
    /// residual catch-all: the family label is the assertion, and `"no lowering"`
    /// pins the suffix the family arms share.
    ///
    /// A panic here is worse than the bug this issue fixes — on the linking path it
    /// aborts the process instead of producing a diagnostic — so every row doubles as
    /// a crash-surface regression guard. All nine fixtures were verified to assemble
    /// under `wat` 1.225.0 and to reach the operator match rather than dying at the
    /// parse boundary.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn unsupported_proposal_families_are_rejected_not_panic() {
        assert_wat_rejected(
            "struct.new (GC)",
            r#"(module (type $s (struct (field i32)))
                 (func (export "f") i32.const 1 struct.new $s drop))"#,
            &["garbage collection", "no lowering"],
        );
        assert_wat_rejected(
            "ref.i31",
            r#"(module (func (export "f") i32.const 1 ref.i31 drop))"#,
            &["i31 references", "no lowering"],
        );
        // `ref.func` rather than `ref.null` deliberately: `ref.null` is rejected by
        // its own pre-existing arm, so a null operand would never reach this family.
        assert_wat_rejected(
            "ref.as_non_null (typed refs)",
            r#"(module (func $g) (func (export "f") ref.func $g ref.as_non_null drop))"#,
            &["typed function references", "no lowering"],
        );
        assert_wat_rejected(
            "i64.add128 (wide arithmetic)",
            r#"(module (func (export "f") (result i64 i64)
                 i64.const 1 i64.const 0 i64.const 1 i64.const 0 i64.add128))"#,
            &["128-bit wide arithmetic", "no lowering"],
        );
        assert_wat_rejected(
            "try/catch_all (legacy EH)",
            r#"(module (func (export "f") try nop catch_all nop end))"#,
            &["legacy exception handling", "no lowering"],
        );
        assert_wat_rejected(
            "cont.new (stack switching)",
            r#"(module (type $ft (func)) (type $ct (cont $ft))
                 (func $g) (func (export "f") ref.func $g cont.new $ct drop))"#,
            &["stack switching", "no lowering"],
        );
        assert_wat_rejected(
            "table.init (segment table ops)",
            r#"(module (table 1 funcref) (elem $e func)
                 (func (export "f") i32.const 0 i32.const 0 i32.const 0 table.init 0 $e))"#,
            &["segment-indexed table initialization", "no lowering"],
        );
        assert_wat_rejected(
            "memory.discard",
            r#"(module (memory 1) (func (export "f") i32.const 0 i32.const 0 memory.discard))"#,
            &["memory.discard", "no lowering"],
        );
        assert_wat_rejected(
            "return_call (tail calls)",
            r#"(module (func $g (result i32) i32.const 1)
                 (func (export "f") (result i32) return_call $g))"#,
            &["tail calls", "no lowering"],
        );
    }

    /// The two singletons, which deliberately do *not* share the family
    /// wording. `typed select` is attributed to the translator, not the model — the
    /// stub does declare `BI_select`, so a model-attributed reason would be false —
    /// and `throw_ref` is modern exception handling rather than the legacy family.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn singleton_unsupported_instructions_are_rejected_not_panic() {
        assert_wat_rejected(
            "typed select",
            r#"(module (func (export "f") (result i32)
                 i32.const 1 i32.const 2 i32.const 0 select (result i32)))"#,
            &["typed select"],
        );
        assert_wat_rejected(
            "throw_ref",
            r#"(module (func (export "f") unreachable throw_ref))"#,
            &["throw_ref"],
        );
    }

    // == Positive control =================================================

    /// The acceptance criterion's other half: no behavior change for the integer
    /// surface. A module spanning integer arithmetic and comparison, structured
    /// control flow, locals, and memory access must still translate — and the emitted
    /// `.v` must mention none of the constructors the stub omits, which is a stronger
    /// statement than "it translated" and holds independently of how any rejection is
    /// worded.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn the_integer_surface_still_translates() {
        let bytes = wat::parse_str(
            r#"
            (module
              (memory 1)
              (func (export "compute") (param i32 i32) (result i32)
                (local i32)
                block
                  loop
                    local.get 0
                    local.get 1
                    i32.lt_s
                    br_if 1
                    local.get 0
                    local.get 1
                    i32.add
                    local.set 2
                    br 0
                  end
                end
                i32.const 0
                local.get 2
                i32.store
                i32.const 0
                i32.load)
              (func (export "wide") (param i64 i64) (result i64)
                local.get 0
                local.get 1
                i64.add
                local.get 0
                local.get 1
                i64.mul
                i64.xor))
            "#,
        )
        .expect("control fixture WAT assembles");
        let v = translate(&bytes).expect("the integer-only surface must still translate");

        for absent in [
            "BI_cvtop",
            "T_f32",
            "T_f64",
            "Relop_f",
            "Binop_f",
            "Unop_f",
            "T_v128",
            "BI_const_vec",
            "BI_load_vec",
        ] {
            assert!(
                !v.contains(absent),
                "the integer surface must emit no `{absent}` (the Rocq stub declares none):\n{v}"
            );
        }

        // The integer constructors the stub *does* declare must still be present, so
        // this control cannot pass by emitting nothing.
        for present in [
            "BI_binop",
            "Relop_i",
            "BI_load",
            "BI_store",
            "BI_loop",
            "BI_block",
            "BI_local_get",
        ] {
            assert!(
                v.contains(present),
                "the integer surface must still emit `{present}`:\n{v}"
            );
        }
    }

    /// The mirror of the family rows above, and the guard on the sweep's
    /// highest-risk swallow site.
    ///
    /// The segment-indexed table family is exactly `table.init` / `elem.drop` /
    /// `table.copy`, and it sits amid operators the translator DOES lower —
    /// `memory.init`, `data.drop`, `memory.copy`, `memory.fill`, and the five
    /// `table.*` accessors — several of which read as "segment-related".
    /// `data.drop` is the closest call of all. Nothing else guards them: the
    /// `coqc` gate's corpus exercises neither `memory.init` nor `data.drop`, and
    /// Inference codegen emits neither, so a one-variant mis-grouping would ship
    /// as silent over-rejection of supported surface with every other test green.
    ///
    /// Plain `select` is here for the same reason against a different arm: it
    /// must keep lowering to `BI_select None` while `TypedSelect`, its immediate
    /// neighbour, rejects.
    ///
    /// Each row asserts the constructor, not merely that translation succeeded,
    /// so an arm that survives but emits something else still fails.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn the_neighbours_of_the_rejected_families_still_translate() {
        for (label, wat, constructor) in [
            (
                "memory.init / data.drop",
                r#"(module (memory 1) (data $d "ab")
                     (func (export "f")
                       i32.const 0 i32.const 0 i32.const 2 memory.init $d
                       data.drop $d))"#,
                "BI_memory_init",
            ),
            (
                "memory.copy / memory.fill",
                r#"(module (memory 1)
                     (func (export "f")
                       i32.const 0 i32.const 1 i32.const 2 memory.copy
                       i32.const 0 i32.const 0 i32.const 2 memory.fill))"#,
                "BI_memory_copy",
            ),
            (
                "table.get / set / grow / size / fill",
                r#"(module (table 1 funcref) (func $g)
                     (func (export "f") (result i32)
                       i32.const 0 ref.func $g table.set
                       i32.const 0 table.get drop
                       ref.func $g i32.const 1 table.grow drop
                       i32.const 0 ref.func $g i32.const 0 table.fill
                       table.size))"#,
                "BI_table_size",
            ),
            (
                "plain select",
                r#"(module (func (export "f") (result i32)
                     i32.const 1 i32.const 2 i32.const 0 select))"#,
                "BI_select None",
            ),
        ] {
            let bytes = wat::parse_str(wat).expect("neighbour fixture WAT assembles");
            let v = translate(&bytes)
                .unwrap_or_else(|e| panic!("{label}: must still translate, got {e:?}"));
            assert!(
                v.contains(constructor),
                "{label}: must still lower to `{constructor}`:\n{v}"
            );
        }

        // The rest of the `data.drop` claim: it lowers as itself, not as the
        // segment-indexed table family it reads like.
        let bytes = wat::parse_str(
            r#"(module (memory 1) (data $d "ab") (func (export "f") data.drop $d))"#,
        )
        .expect("data.drop fixture WAT assembles");
        let v = translate(&bytes).expect("data.drop must still translate");
        assert!(
            v.contains("BI_data_drop"),
            "data.drop must lower to `BI_data_drop`:\n{v}"
        );
    }
}
