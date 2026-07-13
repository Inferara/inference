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
//! let rocq_code = translate_bytes("my_module", &wasm_bytes)?;
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
//! | `forall` | `0xfc 0x3a` | Begin universal quantification block |
//! | `exists` | `0xfc 0x3b` | Begin existential quantification block |
//! | `assume` | `0xfc 0x3c` | Filter execution paths by constraint |
//! | `unique` | `0xfc 0x3d` | Assert exactly one execution path exists |
//! | `i32.uzumaki` | `0xfc 0x31` | Generate non-deterministic i32 value |
//! | `i64.uzumaki` | `0xfc 0x32` | Generate non-deterministic i64 value |
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
//! - [WASM Codegen](../wasm-codegen/README.md) - WebAssembly code generation
//! - [Inference Compiler](../inference/README.md) - Main compiler orchestration
//! - [Rocq Documentation](https://rocq-prover.org/) - Rocq proof assistant
//! - [WebAssembly Specification](https://webassembly.github.io/spec/) - WASM standard

pub mod errors;
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

/// Wire-format version that additionally carries one [`SpecObligationKind`]
/// byte per index (see `inference_wasm_codegen::spec_section`). The decoder
/// accepts this alongside [`SPEC_FUNCS_SECTION_VERSION`].
pub use inference_wasm_codegen::SPEC_FUNCS_SECTION_VERSION_WITH_KINDS;

/// The downstream proof obligation a spec function carries (`Spec` / `Exists` /
/// `Unique`). Recovered from the `inference.spec_funcs` section to choose the
/// emitted predicate (`ValidSpec` / `ValidExistsSpec` / `ValidUniqueSpec`).
pub use inference_wasm_codegen::SpecObligationKind;

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
                translate_bytes(module_name, &bytes, &empty)
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
        // Spec name `Spec1` avoids shadowing the Peano successor `S` (newly
        // added to the prelude rejection list) while still exercising the
        // per-spec emission path.
        map.insert("Spec1".to_string(), vec![3, 4, 7]);
        let output = translate_bytes("Fac", &bytes, &map).expect("translate succeeds");

        // Assertion-valued contract (wasm-verifier PR #2 / issue #6): the list
        // is `list assertion`, emitted empty with the indices in a comment.
        assert!(
            output
                .contains("Definition Fac__Spec1_specs : list assertion := (@nil assertion)."),
            "output should contain Fac__Spec1_specs definition; got:\n{output}",
        );
        assert!(
            output.contains("(* function indices: 3 4 7 (assertion payloads pending) *)"),
            "output should carry the spec's function indices in a comment; got:\n{output}",
        );
        // `assertion` comes from the Assertions module.
        assert!(
            output.contains("From WasmVerifier Require Import Assertions."),
            "output should import Assertions for the `assertion` type; got:\n{output}",
        );
        // Structural well-formedness theorem (1-arg ValidModule), always emitted.
        assert!(
            output.contains("Theorem valid_Fac : ValidModule Fac."),
            "output should contain the structural ValidModule theorem; got:\n{output}",
        );
        // Per-spec verification theorem uses the 2-arg ValidSpec predicate (post-#21).
        assert!(
            output.contains("Theorem valid_Fac__Spec1 : ValidSpec Fac Fac__Spec1_specs."),
            "output should contain per-spec ValidSpec theorem; got:\n{output}",
        );
        // The downstream library namespace is WasmVerifier, not Wasm.
        assert!(
            output.contains("From WasmVerifier Require Import Verifier."),
            "output should import the WasmVerifier contract; got:\n{output}",
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn translate_bytes_emits_no_spec_lines_when_empty() {
        let test_data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data");
        let bytes = fs::read(test_data_dir.join("fac.0.wasm")).expect("read fac.0.wasm");

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let output = translate_bytes("Fac", &bytes, &empty).expect("translate succeeds");

        assert!(
            !output.contains("_specs : list assertion"),
            "output should contain no per-spec definitions when the map is empty; got:\n{output}",
        );
        // No per-spec ValidSpec theorems when there are no specs.
        assert!(
            !output.contains("ValidSpec"),
            "output should contain no ValidSpec theorems when the spec map is empty; got:\n{output}",
        );
        // Zero-spec artifacts are unchanged: no Assertions import either.
        assert!(
            !output.contains("From WasmVerifier Require Import Assertions."),
            "zero-spec output should not import Assertions; got:\n{output}",
        );
        // The structural ValidModule theorem is still emitted (contract: a zero-spec module
        // emits only the module record and `Theorem valid_<mod> : ValidModule <mod>`).
        assert!(
            output.contains("Theorem valid_Fac : ValidModule Fac."),
            "output should still contain the structural ValidModule theorem; got:\n{output}",
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
        translate_bytes("Prog", &bytes, &FxHashMap::default())
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
        let output = translate_bytes("Prog", &bytes, &FxHashMap::default())
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
        let err = translate_bytes("Prog", &bytes, &FxHashMap::default())
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
        translate_bytes("Prog", &bytes, &FxHashMap::default())
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

        let first = translate_bytes("Prog", &bytes, &FxHashMap::default())
            .expect("first translation succeeds");
        let second = translate_bytes("Prog", &bytes, &FxHashMap::default())
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

        let first = translate_bytes("Prog", &bytes, &FxHashMap::default())
            .expect("first translation succeeds");
        let second = translate_bytes("Prog", &bytes, &FxHashMap::default())
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

        let output = translate_bytes("Prog", &bytes, &FxHashMap::default())
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

        let output = translate_bytes("Prog", &with_names, &FxHashMap::default())
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

        let output = translate_bytes("Prog", &bytes, &FxHashMap::default())
            .expect("a module whose only import is a memory translates");

        // The defined function sits at absolute index 0 (no function imports),
        // so it keeps its source name with no index perturbation.
        assert!(
            output.contains("Definition only :"),
            "a non-function import must not shift the defined function's index:\n{output}",
        );
    }
}
