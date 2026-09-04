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
//! | `unique` | `0xfc 0x3d` | Assert exactly one execution path exists — as a nested *block*, rejected in proof mode (no `hassert` encoding; fatal `P002` at codegen); a `unique`-quantified spec-function *body* is reachability-lowered instead and never emits this opcode |
//! | `i32.uzumaki` | `0xfc 0x31` | Generate non-deterministic i32 value |
//! | `i64.uzumaki` | `0xfc 0x32` | Generate non-deterministic i64 value |
//!
//! These instructions are parsed by the forked [`inf-wasmparser`] dependency, but
//! they never appear in the emitted Rocq: `forall`/plain spec-function bodies are
//! omitted from the module record entirely (their logical content arrives
//! separately as `hassert` obligations via the `inference.hspecs` custom section),
//! `exists`/`unique` spec-function bodies are retained but reachability-lowered to
//! vanilla WASM (each `@` a hidden trailing choice parameter, filters trap), and a
//! non-deterministic instruction in any body the emitted module retains is a
//! translation error — the vanilla WasmCert proof model has no constructors for
//! them.
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
//! fails `coqc` downstream, and never a panic. Rejected: every floating-point
//! and SIMD/vector instruction; every conversion naming a float on either side
//! (`trunc`, `trunc_sat`, `convert`, `demote`, `promote`, `reinterpret`), since
//! the model declares no float number type for such a term to mention;
//! `f32`/`f64`/`v128` in any type position; and the proposal families the model
//! does not describe (GC, exception handling, stack switching, tail calls, wide
//! arithmetic, typed references, `memory.discard`, segment-indexed table
//! operations).
//!
//! Translated, not rejected: the three integer-to-integer width conversions
//! (`BI_cvtop` with `CVO_wrap`/`CVO_extend`) and the five sign-extension
//! operators, which the model spells as unops — `BI_unop t (Unop_extend n)`,
//! with `n` the source width in **bits**.
//!
//! No Inference program can reach any of this — the language has no floats, no
//! vectors, and emits no conversion or sign-extension — so these arms are
//! reachable only through foreign bytes, via the external linking path or
//! [`wasm_parser::translate_bytes`]. `core/wasm-linker` refuses the same float
//! content in external modules, making this the second of two layers.
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

    /// The same renumbering across both element item forms. A segment carries
    /// either bare function indexes or initializer expressions, and once the
    /// shorthand is desugared both reach the `.v` as `BI_ref_func` — so both
    /// operands index the same instantiated function space and both have to be
    /// renumbered. Here `func 1` is the omitted spec function, so each segment's
    /// reference to `func 2` must read `BI_ref_func 1%N`; a form that skipped
    /// the remap would leave a dangling index that no `coqc` gate can catch.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn omitting_a_spec_function_renumbers_both_element_item_forms() {
        let bytes = wat::parse_str(
            r#"
            (module
              (table (;0;) 2 2 funcref)
              (elem (;0;) (i32.const 0) func 2)
              (elem (;1;) funcref (item ref.func 2))
              (func (;0;) (result i32) i32.const 0)
              (func (;1;) (result i32) i32.const 0)
              (func (;2;) (result i32) i32.const 7))
            "#,
        )
        .expect("element remap fixture assembles");

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

        assert_eq!(
            output.matches("BI_ref_func 1%N").count(),
            2,
            "both element item forms must renumber their reference to func 2 past \
             the omitted spec function at index 1; got:\n{output}",
        );
        assert!(
            !output.contains("BI_ref_func 2%N"),
            "no element item may keep its pre-omission function index; got:\n{output}",
        );
    }

    /// Every index immediate reaches the `.v` with an explicit `%N` scope, and
    /// every term reaching it is one the proof contract can elaborate.
    ///
    /// The contract types all of these operands as `N`, and Rocq's numeral
    /// notation is type-directed, so a bare numeral elaborates correctly *as
    /// long as* the expected type is inferable at that position. That makes a
    /// bare operand silently fine today and silently wrong the moment a
    /// contract or notation change loses the inference — a failure that would
    /// land on the paid prover worker, not here. Pinning the spelling holds
    /// every arm to the same explicit form.
    ///
    /// The same fixture pins the constructor and notation spellings around
    /// those operands, because a scope is only worth pinning on a term the
    /// contract has a constructor for at all.
    ///
    /// None of the constructs below are emitted by Inference codegen —
    /// `br_table`, `call_indirect`, `memory.init`, `data.drop` and element
    /// segments reach this translator only from foreign or statically-linked
    /// `.wasm`. No fixture in the `coqc` corpus covers them, so this
    /// handcrafted module has to.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn index_immediates_and_segment_spellings_match_the_contract() {
        // `br_table 0 1 0` yields two explicit targets plus a default, pinning
        // the `%N` scope on both operands of the label list and on the default;
        // `br_table 0` carries a default and nothing else. The passive data
        // segment reaches `memory.init`/`data.drop`, and the active element
        // segment's function index reaches its `ref.func` initializer. Its two
        // bytes straddle the range the contract's own hex notations cover:
        // `0x78` is one the notation block declares, `0x12` is one of the
        // twelve it skips. Neither spelling is emitted — the point of the pair
        // is that the uniform `encode` form does not vary with that split.
        let bytes = wat::parse_str(
            r#"
            (module
              (type (;0;) (func (param i32) (result i32)))
              (table (;0;) 1 1 funcref)
              (memory (;0;) 1)
              (data (;0;) "x\12")
              (elem (;0;) (i32.const 0) func 0)
              (func (;0;) (type 0) (param i32) (result i32)
                i32.const 0
                i32.const 0
                i32.const 1
                memory.init 0
                data.drop 0
                block
                  block
                    local.get 0
                    br_table 0 1 0
                  end
                  block
                    local.get 0
                    br_table 0
                  end
                  local.get 0
                  br_if 0
                  br 0
                end
                local.get 0
                i32.const 0
                call_indirect (type 0)))
            "#,
        )
        .expect("index-immediate fixture assembles");

        let output = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("translate succeeds");

        for needle in [
            "BI_br 0%N",
            "BI_br_if 0%N",
            "BI_br_table (0%N :: 1%N :: nil) 0%N",
            "BI_br_table nil 0%N",
            "BI_call_indirect 0%N 0%N",
            "BI_memory_init 0%N",
            "BI_data_drop 0%N",
            "(BI_ref_func 0%N :: nil)",
        ] {
            assert!(
                output.contains(needle),
                "index immediate must be emitted as `{needle}`; got:\n{output}",
            );
        }

        for bare in [
            "BI_br 0 ",
            "BI_br_table (0 ::",
            "BI_call_indirect 0 0",
            "BI_memory_init 0 ",
            "BI_data_drop 0 ",
            "BI_ref_func 0 ",
        ] {
            assert!(
                !output.contains(bare),
                "`{bare}` leaves the numeral's scope to inference; got:\n{output}",
            );
        }

        // Spellings the proof contract has no constructor for, both of them
        // emitted here before and type-checked nowhere: `ME_functions` is an
        // element mode written into the field that holds initializer
        // expressions, and a `BI_br_table` carrying no label list at all is a
        // partial application rather than an instruction. The retired
        // `ME_declared` needs a `declare` segment to be observable at all,
        // which this fixture has none of; its retirement is pinned in the tests
        // crate's `foreign_segments_type_check_against_vendored_stub`.
        for retired in ["ME_functions", "BI_br_table ::"] {
            assert!(
                !output.contains(retired),
                "`{retired}` is not a term the proof contract accepts; got:\n{output}",
            );
        }

        // Every data byte is spelled as an `encode` application, whatever the
        // contract's own hex notations cover: those notations expand to
        // arithmetic over bare numerals that read as `nat` wherever `Z_scope`
        // is not open, so the backend rejects the ones carrying a hex digit
        // `A` .. `F`. The application's argument carries the private key the
        // preamble claims, since `Z` itself may be pointing at mathcomp's
        // `int_scope` by the time the module is read.
        assert!(
            output.contains("(encode 120%Zst) :: (encode 18%Zst) :: nil"),
            "a data byte must reach the `.v` as an `encode` application, \
             whether or not the contract declares a notation for it; \
             got:\n{output}",
        );
        for value in 0..=u8::MAX {
            let notation = format!("#{value:02X}");
            assert!(
                !output.contains(&notation),
                "`{notation}` is a hex byte notation, and none of them \
                 elaborate against the contract's `encode` from a module that \
                 leaves `Z_scope` closed; got:\n{output}",
            );
        }
        assert!(
            !output.contains("Open Scope byte_scope.\n"),
            "no emitted term is written in `byte_scope`, so the preamble must \
             not open it; got:\n{output}",
        );
        assert!(
            output.contains("Local Delimit Scope Z_scope with Zst.\n"),
            "a module carrying a data segment must claim the private key its \
             `encode` arguments are spelled with; got:\n{output}",
        );
    }

    /// An imported table reaches the `.v` as a complete `table_type` record.
    ///
    /// The contract declares `MID_table : table_type -> module_import_desc`,
    /// and `table_type` is `{tt_limits : limits; tt_elem_type : reference_type}`
    /// — two fields. Its neighbour `MID_mem` takes a bare `limits`, because the
    /// contract's `memory_type` *is* `limits`, and that symmetry is what made
    /// applying `MID_table` to a bare `limits` look right: the emitted term
    /// dropped the element type and did not type-check. No fixture in the
    /// `coqc` corpus imports a table — Inference codegen never emits one, and
    /// the static-merge linker removes every import before `-v` — so the gate
    /// elaborated this arm exactly never (issue #401).
    ///
    /// Both element types are pinned, since the funcref spelling alone would
    /// pass on a translator that hardcoded it and never consulted the table's
    /// own type.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_imported_table_carries_its_element_type() {
        let bytes = wat::parse_str(
            r#"
            (module
              (import "env" "imported_table" (table 1 funcref))
              (import "env" "imported_extern_table" (table 2 4 externref)))
            "#,
        )
        .expect("table-import fixture assembles");

        let output = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .expect("translate succeeds");

        for needle in [
            "Mi \"env\" \"imported_table\" (MID_table {|tt_limits := \
             {|lim_min := 1%N; lim_max := None|}; tt_elem_type := T_funcref|})",
            "Mi \"env\" \"imported_extern_table\" (MID_table {|tt_limits := \
             {|lim_min := 2%N; lim_max := Some(4%N)|}; tt_elem_type := T_externref|})",
        ] {
            assert!(
                output.contains(needle),
                "an imported table must be emitted as `{needle}`; got:\n{output}",
            );
        }

        assert!(
            !output.contains("MID_table {|lim_min"),
            "`MID_table` applied to a bare `limits` is a type error the contract \
             rejects — the element type must not be dropped again; got:\n{output}",
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
/// the float-naming half of the conversion (`cvtop`) family, and the proposal
/// families that previously hit `todo!()`.
///
/// The stub in `rocq-stub/` declares `number_type` with only `T_i32`/`T_i64`, no
/// `T_v128`, and a `cvtop` carrying only the two integer-to-integer constructors
/// (see its README "Scope"). Every fixture here therefore has no honest lowering:
/// the translator must say so with a recoverable
/// [`WasmToVError::UnsupportedFeature`] naming the construct, rather than emit a
/// term the proof target cannot type, or abort the process.
///
/// The integer-to-integer conversions and the five sign-extension operators are
/// the counterweight: they are *not* rejected, and
/// [`integer_width_conversions_translate`] /
/// [`sign_extension_operators_translate_with_bit_widths`] pin their emitted
/// terms. Keeping both directions in one module is deliberate — the rejections
/// above are about the float types the contract omits, not about conversion or
/// width-change as a category.
///
/// Two failure modes are pinned, because both existed before this change:
///
/// * **silent ill-typed emission** — the float comparison arms emitted the *integer*
///   relop family inside the float wrapper (`BI_relop T_f32 (Relop_f ROI_eq)`), where
///   `Relop_f` wants `ROF_*` and `ROI_ge` is an unapplied function awaiting an `sx`.
///   Nothing caught it: the `coqc` gate's corpus is Inference source, and no Inference
///   program lowers to float WASM.
/// * **`todo!()` panic** — saturating truncation, most SIMD, and nine
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

    /// Translates `wat` and asserts the emitted `.v` contains `needle`
    /// **verbatim**. The counterpart to [`assert_wat_rejected`] for the operators
    /// the contract does cover: a substring match on the exact printed term, so
    /// an arity or spelling drift in the emitted constructor fails here rather
    /// than surviving to `coqc`.
    fn assert_wat_emits(label: &str, wat: &str, needle: &str) {
        let bytes = wat::parse_str(wat).expect("fixture WAT assembles");
        let v = translate(&bytes).unwrap_or_else(|e| panic!("{label}: must translate, got {e:?}"));
        assert!(
            v.contains(needle),
            "{label}: the emitted `.v` must contain `{needle}`; got:\n{v}"
        );
    }

    /// The three integer-to-integer width conversions: no float anywhere, so the
    /// proof contract's `cvtop` covers them. Each operand triple is pinned
    /// because `cvtop_valid` admits exactly one per constructor — `CVO_wrap` at
    /// `(i32, i64, None)` and `CVO_extend` at `(i64, i32, Some sx)` — so a
    /// transposed number type or a dropped `sx` is a well-formed term the
    /// backend still refuses.
    ///
    /// The wasm-linker's allow-list mirrors this: it admits these three because
    /// a lowering exists, which is the standard it holds every allow-listed
    /// operator to.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn integer_width_conversions_translate() {
        assert_wat_emits(
            "i32.wrap_i64",
            r#"(module (func (export "f") i64.const 1 i32.wrap_i64 drop))"#,
            "BI_cvtop T_i32 CVO_wrap T_i64 None",
        );
        assert_wat_emits(
            "i64.extend_i32_s",
            r#"(module (func (export "f") i32.const 1 i64.extend_i32_s drop))"#,
            "BI_cvtop T_i64 CVO_extend T_i32 (Some SX_S)",
        );
        assert_wat_emits(
            "i64.extend_i32_u",
            r#"(module (func (export "f") i32.const 1 i64.extend_i32_u drop))"#,
            "BI_cvtop T_i64 CVO_extend T_i32 (Some SX_U)",
        );
    }

    /// Sign extension, which the proof contract spells as a **unop** — `BI_unop t
    /// (Unop_extend n)`, beside `clz`/`ctz`/`popcnt` — and not as a conversion.
    ///
    /// This test exists for `n`. The argument is the source width in **bits**;
    /// the model's `app_unop` divides it by eight before extending. A byte count
    /// (`Unop_extend 1`) is an equally well-typed term: the model's
    /// `unop_type_agree` ignores the argument entirely, so the `.v` compiles,
    /// every obligation over it is provable, and each of the five instructions
    /// silently denotes a constant-zero extension of its input.
    ///
    /// **Do not delete this as redundant with the `coqc` gate.** That was
    /// measured, not assumed: emitting `Unop_extend 1%N` produced a `.v` that
    /// `coqc` **compiled clean**, and only a byte comparison caught it. The gate
    /// proves a term elaborates, never that it means what the instruction means,
    /// so it is structurally incapable of catching an argument-*value* error
    /// here. This test and
    /// `sign_extension_widths_are_bit_counts_not_byte_counts` in
    /// `tests/src/rocq_typecheck.rs` are the entire guard, which is why the
    /// convention is pinned by byte comparison against the one spelling that
    /// denotes the instruction WebAssembly actually specifies.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn sign_extension_operators_translate_with_bit_widths() {
        for (label, wat, needle) in [
            (
                "i32.extend8_s",
                r#"(module (func (export "f") i32.const 1 i32.extend8_s drop))"#,
                "BI_unop T_i32 (Unop_extend 8%N)",
            ),
            (
                "i32.extend16_s",
                r#"(module (func (export "f") i32.const 1 i32.extend16_s drop))"#,
                "BI_unop T_i32 (Unop_extend 16%N)",
            ),
            (
                "i64.extend8_s",
                r#"(module (func (export "f") i64.const 1 i64.extend8_s drop))"#,
                "BI_unop T_i64 (Unop_extend 8%N)",
            ),
            (
                "i64.extend16_s",
                r#"(module (func (export "f") i64.const 1 i64.extend16_s drop))"#,
                "BI_unop T_i64 (Unop_extend 16%N)",
            ),
            (
                "i64.extend32_s",
                r#"(module (func (export "f") i64.const 1 i64.extend32_s drop))"#,
                "BI_unop T_i64 (Unop_extend 32%N)",
            ),
        ] {
            assert_wat_emits(label, wat, needle);
        }

        // The byte counts the bit widths would collapse to. Each is a term the
        // proof model accepts and gives the wrong meaning, so absence is the
        // only thing that distinguishes a correct emission from a provable lie.
        let v = translate(
            &wat::parse_str(
                r#"(module (func (export "f")
                     i32.const 1 i32.extend8_s drop
                     i32.const 1 i32.extend16_s drop
                     i64.const 1 i64.extend8_s drop
                     i64.const 1 i64.extend16_s drop
                     i64.const 1 i64.extend32_s drop))"#,
            )
            .expect("fixture WAT assembles"),
        )
        .expect("the sign-extension surface must translate");
        for byte_count in ["Unop_extend 1%N", "Unop_extend 2%N", "Unop_extend 4%N"] {
            assert!(
                !v.contains(byte_count),
                "`{byte_count}` is a byte count where the proof model wants a bit \
                 width; it type-checks and denotes a constant-zero extension:\n{v}"
            );
        }
    }

    /// The conversions that remain rejected: every one names a float on one side
    /// or the other, and the proof contract declares no float number type. The
    /// integer-only rows above are what makes this list meaningful — the
    /// rejection is about floats, not about conversion as a category.
    ///
    /// Hand-encoded rather than assembled from WAT: a saturating truncation
    /// consumes a float, its only source is an `f32.const`/`f64.const`, and that
    /// const's own arm rejects first — so a WAT fixture would pin the const's
    /// name and leave these arms untested. See "Why there are two tiers of
    /// fixture" above.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn saturating_truncations_are_rejected() {
        assert_raw_rejected(
            "i32.trunc_sat_f32_s",
            &[0xfc, 0x00],
            &["i32truncsatf32s", "conversion"],
        );
        assert_raw_rejected(
            "i64.trunc_sat_f64_u",
            &[0xfc, 0x07],
            &["i64truncsatf64u", "conversion"],
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
        // Hand-encoded rather than WAT: a `(struct …)` fixture must declare the
        // composite in its type section, and the type section is rendered before
        // any body, so its own rejection would be the one observed and this row
        // would stop exercising the operator arm while still passing. The raw
        // body carries `struct.new 0` against the harness's ordinary function
        // type, which reaches the operator arm and nothing else.
        assert_raw_rejected(
            "struct.new (GC)",
            &[0xfb, 0x00, 0x00],
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
        // Hand-encoded for the same reason as `struct.new` above: a `(cont $ft)`
        // fixture declares a continuation type, and the type-section rejection
        // would steal this row's needles.
        assert_raw_rejected(
            "cont.new (stack switching)",
            &[0xe0, 0x00],
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

/// Reachability (`exists`/`unique`) obligation emission: kind-aware retention
/// in the module record, the kind-selected `reachability_spec` /
/// `ValidExistsSpec` / `ValidUniqueSpec` grammar, the conditional ` Exists`
/// preamble import, and every fail-closed consistency arm the retention adds.
///
/// These tests pin the emitted text against hand-built obligation maps, so a
/// grammar change shows up here as a diff rather than as a downstream `coqc`
/// error; the `inference-tests` gate elaborates the same grammar for real
/// against the vendored stub's `Exists.v`.
#[cfg(test)]
mod reachability_emission {
    use super::errors::WasmToVError;
    use super::wasm_parser::translate_bytes;
    use inference_hassert::{HAssert, HFnRef, HSpecEntry, HSpecMap, HTerm, ReachMeta, SpecKind};
    use rustc_hash::FxHashMap;

    fn exists_entry(symbol: &str, entry_arity: u32, visible_locs: Vec<u32>) -> HSpecEntry {
        HSpecEntry::new(
            HFnRef(symbol.to_string()),
            HAssert::Defined(HTerm::Local(0)),
            SpecKind::Exists(ReachMeta {
                entry_arity,
                visible_locs,
            }),
        )
    }

    fn unique_entry(symbol: &str, entry_arity: u32, visible_locs: Vec<u32>) -> HSpecEntry {
        HSpecEntry::new(
            HFnRef(symbol.to_string()),
            HAssert::Defined(HTerm::Local(0)),
            SpecKind::Unique(ReachMeta {
                entry_arity,
                visible_locs,
            }),
        )
    }

    fn spec_maps(
        spec: &str,
        indices: Vec<u32>,
        entries: Vec<HSpecEntry>,
    ) -> (FxHashMap<String, Vec<u32>>, HSpecMap) {
        let mut spec_funcs: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        spec_funcs.insert(spec.to_string(), indices);
        let mut hspecs = HSpecMap::default();
        hspecs.insert(spec.to_string(), entries);
        (spec_funcs, hspecs)
    }

    fn translate(
        wat: &str,
        spec_funcs: &FxHashMap<String, Vec<u32>>,
        hspecs: &HSpecMap,
    ) -> anyhow::Result<String> {
        let bytes = wat::parse_str(wat).expect("reachability fixture WAT assembles");
        translate_bytes("Prog", &bytes, spec_funcs, hspecs)
    }

    /// The omitted-reference rejection stays alongside the retained one: a
    /// surviving call to an omitted (forall/plain) spec function — including
    /// one from a retained `exists` body, the newly reachable context — is a
    /// fail-closed error naming the omission.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn references_to_an_omitted_spec_function_stay_rejected() {
        // `omitted_fn` is a plain spec function (no obligation); the retained
        // `ex_fn` body calls it.
        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![1, 2], vec![exists_entry("ex_fn", 0, vec![0])]);
        let err = translate(
            r#"
            (module
              (func $exec)
              (func $omitted_fn (param i32) (result i32) local.get 0)
              (func $ex_fn (param i32) local.get 0 call $omitted_fn drop))
            "#,
            &spec_funcs,
            &hspecs,
        )
        .expect_err("a retained body calling an omitted spec function must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("which is an omitted specification function"),
            "the rejection must name the omitted-function rule; got: {msg}",
        );
    }

    /// One spec carrying an `exists` and a `unique` obligation emits the full
    /// kind-selected grammar: a `reachability_spec` record and gathering list
    /// per non-empty partition, the partition-selected theorems, the retained
    /// bodies as ordinary `Definition`s, the ` Exists` preamble import — and
    /// the universal grammar stays present with its explicitly-typed empty
    /// list, since no entry is universal.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn mixed_kind_entries_emit_the_kind_selected_grammar() {
        let (spec_funcs, hspecs) = spec_maps(
            "Reach",
            vec![1, 2],
            vec![
                HSpecEntry::new(
                    HFnRef("ex_probe".to_string()),
                    HAssert::Defined(HTerm::Local(1)),
                    SpecKind::Exists(ReachMeta {
                        entry_arity: 1,
                        visible_locs: vec![0, 1],
                    }),
                ),
                unique_entry("uq_probe", 0, vec![0]),
            ],
        );
        let output = translate(
            r#"
            (module
              (func $exec (result i32) i32.const 7)
              (func $ex_probe (param i32 i32))
              (func $uq_probe (param i32) (local i32)))
            "#,
            &spec_funcs,
            &hspecs,
        )
        .expect("a module with reachability obligations translates");

        for needle in [
            "From WasmVerifier Require Import Assertions Verifier Exists.\n",
            // The exists partition: record, gathering list, theorem.
            "Definition Prog__Reach_exspec1 : reachability_spec :=",
            "reach_func := 1%N; reach_entry_arity := 1%nat",
            "reach_visible_locs := (0%N :: 1%N :: nil); reach_payload := HA_defined (T_local 1%N)",
            "Definition Prog__Reach__ex_specs : list reachability_spec := (Prog__Reach_exspec1 :: nil).",
            "Theorem valid_exists_Prog__Reach : ValidExistsSpec Prog Prog__Reach__ex_specs.",
            // The unique partition, analogous.
            "Definition Prog__Reach_uqspec1 : reachability_spec :=",
            "reach_func := 2%N; reach_entry_arity := 0%nat",
            "reach_visible_locs := (0%N :: nil); reach_payload := HA_defined (T_local 0%N)",
            "Definition Prog__Reach__uq_specs : list reachability_spec := (Prog__Reach_uqspec1 :: nil).",
            "Theorem valid_unique_Prog__Reach : ValidUniqueSpec Prog Prog__Reach__uq_specs.",
            // The universal grammar is unconditional; with no universal entry
            // it is the explicitly-typed empty list.
            "Definition Prog__Reach_specs : list hassert := (@nil hassert).",
            "Theorem valid_Prog__Reach : ValidSpec Prog Prog__Reach_specs.",
            // Retained bodies are ordinary `module_func` definitions listed in
            // `mod_funcs`.
            "Definition ex_probe : module_func :=",
            "Definition uq_probe : module_func :=",
            "ex_probe ::",
            "uq_probe ::",
        ] {
            assert!(
                output.contains(needle),
                "reachability emission must contain `{needle}`; got:\n{output}",
            );
        }

        // No reachability payload may leak into the universal list.
        assert!(
            !output.contains("_hspec1"),
            "no universal obligation exists, so no `_hspec` definition may be emitted:\n{output}",
        );

        const OPEN_PROOF_SKELETON: &str = "Proof.\n  (* TODO: fill the proof *)\nAdmitted.\n";
        assert_eq!(
            output.matches(OPEN_PROOF_SKELETON).count(),
            4,
            "the ValidModule, ValidSpec, ValidExistsSpec, and ValidUniqueSpec \
             theorems must each carry the exact direct-admission skeleton:\n{output}",
        );
        assert!(
            !output.lines().any(|line| line.trim() == "Qed."),
            "generated unfinished proofs must use `Admitted.`, never `Qed.`:\n{output}",
        );
    }

    /// A forall-only module keeps its pre-reachability output: the preamble
    /// import line without ` Exists`, and none of the reachability grammar.
    /// Empty partitions emit nothing.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn forall_only_module_emits_no_reachability_grammar() {
        let (spec_funcs, hspecs) = spec_maps(
            "Only",
            vec![0],
            vec![HSpecEntry::new(
                HFnRef("forall_probe".to_string()),
                HAssert::Defined(HTerm::Local(0)),
                SpecKind::Forall,
            )],
        );
        let output = translate(
            "(module (func $forall_probe (param i32)))",
            &spec_funcs,
            &hspecs,
        )
        .expect("a forall-only module translates");

        assert!(
            output.contains("From WasmVerifier Require Import Assertions Verifier.\n"),
            "a forall-only preamble must not import the reachability module:\n{output}",
        );
        for absent in [
            "Exists",
            "reachability_spec",
            "_ex_specs",
            "_uq_specs",
            "ValidUniqueSpec",
            "reach_func",
        ] {
            assert!(
                !output.contains(absent),
                "`{absent}` must not appear in a forall-only module's output:\n{output}",
            );
        }
        assert!(
            output.contains("Definition Prog__Only_hspec1 : hassert :=")
                && output.contains("Theorem valid_Prog__Only : ValidSpec Prog Prog__Only_specs."),
            "the universal grammar must be unchanged:\n{output}",
        );
    }

    /// Retention does not renumber: a retained `exists` spec function keeps
    /// its place in `mod_funcs`, so a cross-call and both element item forms
    /// referencing a function ABOVE it keep their original operands — the
    /// kind-aware sibling of `omitting_a_spec_function_renumbers_a_surviving_
    /// cross_call`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn retaining_a_spec_function_preserves_surviving_operands() {
        let (spec_funcs, hspecs) = spec_maps(
            "Between",
            vec![1],
            vec![exists_entry("spec_fn", 0, vec![0])],
        );
        let output = translate(
            r#"
            (module
              (table (;0;) 2 2 funcref)
              (elem (;0;) (i32.const 0) func $f2)
              (elem (;1;) funcref (item ref.func $f2))
              (func $f0 (result i32) call $f2)
              (func $spec_fn (param i32))
              (func $f2 (result i32) i32.const 7))
            "#,
            &spec_funcs,
            &hspecs,
        )
        .expect("a module retaining its only spec function translates");

        assert!(
            output.contains("BI_call 2%N") && !output.contains("BI_call 1%N"),
            "a retained spec function shifts nothing, so the cross-call keeps \
             its operand; got:\n{output}",
        );
        assert_eq!(
            output.matches("BI_ref_func 2%N").count(),
            2,
            "both element item forms keep their original operand past a \
             retained spec function; got:\n{output}",
        );
        assert!(
            output.contains("Definition spec_fn : module_func :="),
            "the retained spec function must contribute its `Definition`; got:\n{output}",
        );
        assert!(
            output.contains("reach_func := 1%N"),
            "the retained function's own record indexes it at its unshifted \
             `mod_funcs` position; got:\n{output}",
        );
    }

    /// Renumbering in both directions at once: an omitted forall spec function
    /// below a retained exists spec function shifts both the retained
    /// function's `reach_func` and a surviving cross-call down by one, while
    /// the retained function itself shifts nothing.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn omission_and_retention_renumber_independently() {
        let (spec_funcs, hspecs) = spec_maps(
            "Mix",
            vec![1, 2],
            vec![
                HSpecEntry::new(
                    HFnRef("forall_fn".to_string()),
                    HAssert::Defined(HTerm::Local(0)),
                    SpecKind::Forall,
                ),
                exists_entry("ex_fn", 0, vec![0]),
            ],
        );
        let output = translate(
            r#"
            (module
              (func $exec0 (result i32) call $exec3)
              (func $forall_fn (param i32))
              (func $ex_fn (param i32))
              (func $exec3 (result i32) i32.const 7))
            "#,
            &spec_funcs,
            &hspecs,
        )
        .expect("a mixed-kind module translates");

        assert!(
            output.contains("BI_call 2%N") && !output.contains("BI_call 3%N"),
            "the cross-call renumbers past the omitted forall spec function \
             only; got:\n{output}",
        );
        assert!(
            output.contains("reach_func := 1%N"),
            "the retained function's `reach_func` shifts down past the omitted \
             function below it; got:\n{output}",
        );
        assert!(
            !output.contains("Definition forall_fn"),
            "the forall spec function stays omitted; got:\n{output}",
        );
        assert!(
            output.contains("Definition ex_fn : module_func :="),
            "the exists spec function must be retained; got:\n{output}",
        );
        assert!(
            output.contains("Definition Prog__Mix_hspec1 : hassert :=")
                && output.contains("Definition Prog__Mix__ex_specs : list reachability_spec :=")
                && output.contains("Theorem valid_Prog__Mix : ValidSpec Prog Prog__Mix_specs.")
                && output.contains(
                    "Theorem valid_exists_Prog__Mix : ValidExistsSpec Prog Prog__Mix__ex_specs."
                ),
            "both partitions must emit side by side; got:\n{output}",
        );
    }

    /// The gathering list joins its suffix with the reserved `__` run, and
    /// that separator is the whole defence against a spec name forging it.
    /// A spec `Reach` carrying an `exists` obligation and a sibling spec
    /// literally named `Reach_ex` both emit a gathering list into one file.
    /// Under a single-`_` join the two are spelled `Prog__Reach_ex_specs`
    /// alike — one `list reachability_spec`, one `list hassert` — and the
    /// translator returns that file at exit 0, leaving `coqc` to reject the
    /// second definition as already existing. Nothing upstream rejects the
    /// pair: both names are legal Rocq identifiers and neither spec is
    /// individually at fault.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_sibling_spec_cannot_forge_a_reachability_list_name() {
        let mut spec_funcs: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        spec_funcs.insert("Reach".to_string(), vec![1]);
        spec_funcs.insert("Reach_ex".to_string(), vec![2]);
        let mut hspecs = HSpecMap::default();
        hspecs.insert("Reach".to_string(), vec![exists_entry("ex_fn", 0, vec![0])]);
        hspecs.insert(
            "Reach_ex".to_string(),
            vec![HSpecEntry::new(
                HFnRef("forall_fn".to_string()),
                HAssert::Defined(HTerm::Local(0)),
                SpecKind::Forall,
            )],
        );
        let output = translate(
            r#"
            (module
              (func $exec (result i32) i32.const 7)
              (func $ex_fn (param i32))
              (func $forall_fn (param i32)))
            "#,
            &spec_funcs,
            &hspecs,
        )
        .expect("a spec and its `_ex`-suffixed sibling translate");

        // Both lists reach the output, at their two different types.
        for needle in [
            "Definition Prog__Reach__ex_specs : list reachability_spec :=",
            "Definition Prog__Reach_ex_specs : list hassert :=",
        ] {
            assert!(
                output.contains(needle),
                "the pair must emit both lists; missing `{needle}`:\n{output}",
            );
        }

        let mut names: Vec<&str> = output
            .lines()
            .filter_map(|line| line.strip_prefix("Definition "))
            .filter_map(|rest| rest.split([' ', ':']).next())
            .filter(|name| !name.is_empty())
            .collect();
        let emitted = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            emitted,
            "every emitted `Definition` name must be unique; got:\n{output}",
        );
    }

    /// `reach_func` indexes `mod_funcs` — the defined-function (`T_app`)
    /// space, which excludes imports — not the instantiated space that counts
    /// them. With one function import, the retained function's absolute index
    /// 2 must emit as `reach_func := 1`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn reach_func_indexes_the_defined_space_under_a_function_import() {
        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![2], vec![exists_entry("ex_fn", 0, vec![0])]);
        let output = translate(
            r#"
            (module
              (import "env" "host" (func (param i32) (result i32)))
              (func $exec (result i32) i32.const 7)
              (func $ex_fn (param i32)))
            "#,
            &spec_funcs,
            &hspecs,
        )
        .expect("an import-bearing module with a reachability obligation translates");

        assert!(
            output.contains("reach_func := 1%N"),
            "`reach_func` must count defined functions only (abs 2 minus one \
             import), never the instantiated index 2; got:\n{output}",
        );
        assert!(
            !output.contains("reach_func := 2%N"),
            "the instantiated index must not leak into `reach_func`; got:\n{output}",
        );
    }

    /// An empty `visible_locs` renders as the bare `nil`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn empty_visible_locs_render_as_nil() {
        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![0], vec![unique_entry("uq_fn", 0, vec![])]);
        let output = translate("(module (func $uq_fn (param i32)))", &spec_funcs, &hspecs)
            .expect("translates");
        assert!(
            output.contains("reach_visible_locs := nil; reach_payload :="),
            "an empty projection list must render as `nil`; got:\n{output}",
        );
    }

    /// Every executable reference to a retained spec function is rejected: its
    /// body stays in the module record only as the subject of its reachability
    /// obligation. One arm per reference site — `BI_call`, export, element
    /// item, `mod_start`, `BI_ref_func` — plus the `T_app` symbol arm below.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn references_to_a_retained_spec_function_are_rejected() {
        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![1], vec![exists_entry("ex_fn", 0, vec![0])]);
        let (start_spec_funcs, start_hspecs) =
            spec_maps("Reach", vec![1], vec![exists_entry("ex_fn", 0, vec![])]);

        for (site, wat, sf, hs) in [
            (
                "BI_call",
                r#"
                (module
                  (func $exec i32.const 0 call $ex_fn)
                  (func $ex_fn (param i32)))
                "#,
                &spec_funcs,
                &hspecs,
            ),
            (
                "export",
                r#"
                (module
                  (func $exec)
                  (func $ex_fn (param i32))
                  (export "e" (func $ex_fn)))
                "#,
                &spec_funcs,
                &hspecs,
            ),
            (
                "element item",
                r#"
                (module
                  (table 1 1 funcref)
                  (elem (i32.const 0) func $ex_fn)
                  (func $exec)
                  (func $ex_fn (param i32)))
                "#,
                &spec_funcs,
                &hspecs,
            ),
            (
                "mod_start",
                r#"
                (module
                  (func $exec)
                  (func $ex_fn)
                  (start $ex_fn))
                "#,
                &start_spec_funcs,
                &start_hspecs,
            ),
            (
                "BI_ref_func",
                r#"
                (module
                  (func $exec ref.func $ex_fn drop)
                  (func $ex_fn (param i32)))
                "#,
                &spec_funcs,
                &hspecs,
            ),
        ] {
            let err = translate(wat, sf, hs).expect_err(&format!(
                "{site}: a reference to a retained spec function must be rejected"
            ));
            let msg = err.to_string();
            assert!(
                msg.contains("retained `exists`/`unique` specification function"),
                "{site}: the rejection must name the retained-function rule; got: {msg}",
            );
        }
    }

    /// A `T_app`/`HA_app_ok` symbol resolving to a retained spec function is
    /// rejected: the retained function is the subject of its own obligation,
    /// not an interpretable symbol.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn applying_a_retained_spec_function_symbol_is_rejected() {
        let (spec_funcs, mut hspecs) =
            spec_maps("Reach", vec![1], vec![exists_entry("ex_fn", 0, vec![0])]);
        hspecs
            .get_mut("Reach")
            .expect("inserted above")
            .push(HSpecEntry::new(
                HFnRef("applier".to_string()),
                HAssert::Defined(HTerm::App(HFnRef("ex_fn".to_string()), vec![])),
                SpecKind::Forall,
            ));
        let err = translate(
            r#"
            (module
              (func $exec)
              (func $ex_fn (param i32)))
            "#,
            &spec_funcs,
            &hspecs,
        )
        .expect_err("applying a retained spec function symbol must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("not an interpretable symbol") && msg.contains("ex_fn"),
            "the rejection must name the applied symbol and the rule; got: {msg}",
        );
    }

    /// Appends a `name` section mapping each `(function index, name)` pair.
    ///
    /// Symbolic WAT identifiers are unique by construction, so a fixture that
    /// needs one string on two functions — the whole subject of the tests below
    /// — has to write the section itself. Indices and lengths are emitted as
    /// single LEB128 bytes, which is exact for the small fixtures here.
    fn with_func_names(mut bytes: Vec<u8>, names: &[(u8, &str)]) -> Vec<u8> {
        let mut subsection = vec![u8::try_from(names.len()).expect("fixture is small")];
        for (idx, name) in names {
            subsection.push(*idx);
            subsection.push(u8::try_from(name.len()).expect("fixture names are short"));
            subsection.extend_from_slice(name.as_bytes());
        }
        let mut payload = vec![0x04];
        payload.extend_from_slice(b"name");
        payload.push(0x01);
        payload.push(u8::try_from(subsection.len()).expect("fixture is small"));
        payload.extend_from_slice(&subsection);

        bytes.push(0x00);
        bytes.push(u8::try_from(payload.len()).expect("fixture is small"));
        bytes.extend_from_slice(&payload);
        bytes
    }

    /// A specification function is not a candidate for an applied symbol, so it
    /// does not make the program's own function of that name ambiguous.
    ///
    /// A spec-inner function's `name`-section symbol is deliberately left
    /// unqualified by its defining file, while a non-entry program function's is
    /// qualified — so a spec-inner `fn helper` and the program's own
    /// `lib.util.helper` do not collide, but a spec-inner `fn helper` and an
    /// *entry-file* `fn helper` share one string. That is a coincidence of two
    /// naming rules, not an ambiguity: no obligation may apply a specification
    /// function at all. The application resolves to the one function it can
    /// mean.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_specification_function_does_not_make_an_applied_symbol_ambiguous() {
        let (spec_funcs, hspecs) = spec_maps(
            "Claims",
            vec![1],
            vec![HSpecEntry::new(
                HFnRef("Claims.claim".to_string()),
                HAssert::Defined(HTerm::App(
                    HFnRef("helper".to_string()),
                    vec![HTerm::Local(0)],
                )),
                SpecKind::Forall,
            )],
        );
        // Function 0 is the program's own `helper`; function 1 is spec `S`'s
        // inner `helper`. Both carry the string `helper`.
        let skeleton = wat::parse_str(
            r#"
            (module
              (func (param i32) (result i32) local.get 0)
              (func (param i32) (result i32) local.get 0))
            "#,
        )
        .expect("fixture WAT assembles");
        let bytes = with_func_names(skeleton, &[(0, "helper"), (1, "helper")]);

        let v = translate_bytes("Prog", &bytes, &spec_funcs, &hspecs)
            .expect("the application resolves to the program's own function");
        assert!(
            v.contains("T_app 0"),
            "the application must resolve to the program's own function at \
             `mod_funcs` index 0; got:\n{v}"
        );
    }

    /// Two of the program's *own* functions sharing one symbol stay a hard
    /// error: neither is a specification function, so the filter above removes
    /// nothing and there is no principled way to pick one.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn two_program_functions_sharing_an_applied_symbol_are_ambiguous() {
        let (spec_funcs, hspecs) = spec_maps(
            "Claims",
            vec![],
            vec![HSpecEntry::new(
                HFnRef("Claims.claim".to_string()),
                HAssert::Defined(HTerm::App(
                    HFnRef("lib.mid.make".to_string()),
                    vec![HTerm::Local(0)],
                )),
                SpecKind::Forall,
            )],
        );
        let skeleton = wat::parse_str(
            r#"
            (module
              (func (param i32) (result i32) local.get 0)
              (func (param i32) (result i32) local.get 0))
            "#,
        )
        .expect("fixture WAT assembles");
        let bytes = with_func_names(skeleton, &[(0, "lib.mid.make"), (1, "lib.mid.make")]);

        let err = translate_bytes("Prog", &bytes, &spec_funcs, &hspecs)
            .expect_err("two program functions of one symbol must not resolve");
        let msg = err.to_string();
        assert!(
            msg.contains("2 defined") && msg.contains("ambiguous") && msg.contains("lib.mid.make"),
            "the rejection must name the symbol and count its carriers; got: {msg}",
        );
    }

    /// A symbol whose only carrier is a specification function keeps the precise
    /// rejection rather than degrading to "nothing carries it".
    ///
    /// Dropping specification functions from the candidate set would empty it
    /// here, and an empty set reads as "no defined function carries the symbol"
    /// — which is false and points nowhere. The full set stands instead, so the
    /// omitted-function rejection gets to say what is actually wrong. Its
    /// retained counterpart is pinned by
    /// [`applying_a_retained_spec_function_symbol_is_rejected`].
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_applied_symbol_carried_only_by_a_spec_function_keeps_its_reason() {
        let (spec_funcs, hspecs) = spec_maps(
            "Claims",
            vec![1],
            vec![HSpecEntry::new(
                HFnRef("Claims.claim".to_string()),
                HAssert::Defined(HTerm::App(HFnRef("spec_helper".to_string()), vec![])),
                SpecKind::Forall,
            )],
        );
        let skeleton = wat::parse_str(
            r#"
            (module
              (func)
              (func))
            "#,
        )
        .expect("fixture WAT assembles");
        let bytes = with_func_names(skeleton, &[(0, "exec"), (1, "spec_helper")]);

        let err = translate_bytes("Prog", &bytes, &spec_funcs, &hspecs)
            .expect_err("applying an omitted specification function must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("omitted specification function"),
            "the rejection must name the omission rule; got: {msg}",
        );
    }

    /// The two ways an obligation can name an *imported* function, and the two
    /// different rejections they get.
    ///
    /// Both are live, and which one fires turns on the name section. Inference
    /// code generation names only the functions it compiles, so a symbol
    /// naming one of its imports matches nothing and takes the unresolved arm —
    /// which recognizes the symbol as one of this module's function imports and
    /// says the merge has not run. A module whose name section *does* name the
    /// import (hand-assembled, or third-party) resolves the symbol to an
    /// import index, and the arithmetic in `mod_funcs_index` rejects it there
    /// instead. Deleting either arm leaves one of these applications printing a
    /// `T_app` into an index space it does not belong to.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_obligation_naming_an_import_is_rejected_on_both_paths() {
        let applier = |symbol: &str| {
            spec_maps(
                "Applied",
                vec![],
                vec![HSpecEntry::new(
                    HFnRef("applier".to_string()),
                    HAssert::AppOk(HFnRef(symbol.to_string()), vec![]),
                    SpecKind::Forall,
                )],
            )
        };

        // Unnamed import: no name-section entry to match, and the symbol is
        // recognizably this module's own import.
        let (spec_funcs, hspecs) = applier("mathlib::sum");
        let err = translate(
            r#"(module (import "mathlib" "sum" (func)) (func $applier))"#,
            &spec_funcs,
            &hspecs,
        )
        .expect_err("an obligation about an unmerged import must be rejected");
        assert!(
            err.to_string()
                .contains("Link the module before translating it"),
            "the rejection must name the missing merge; got: {err}",
        );

        // Named import: the symbol resolves, to an index below the defined
        // functions, and the index arithmetic rejects it.
        let (spec_funcs, hspecs) = applier("imported");
        let err = translate(
            r#"(module (import "mathlib" "sum" (func $imported)) (func $applier))"#,
            &spec_funcs,
            &hspecs,
        )
        .expect_err("an obligation applying a named import must be rejected");
        assert!(
            err.to_string().contains("imports rather than defines"),
            "the rejection must say the target is imported; got: {err}",
        );
    }

    /// After the link there is no import section left, so a merged-body symbol
    /// that resolves to nothing can no longer be recognized as an unmerged
    /// import — the branch above is unreachable on exactly the modules it was
    /// written for. The symbol's shape still says what it is (no compiled
    /// Inference function's name can carry `::`), and the linker clears every
    /// applied symbol against the module it emits, so a linked module arriving
    /// here carries a `name` section something rewrote after the link.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_missing_merged_symbol_in_an_import_free_module_names_the_post_link_cause() {
        let (spec_funcs, hspecs) = spec_maps(
            "Applied",
            vec![],
            vec![HSpecEntry::new(
                HFnRef("applier".to_string()),
                HAssert::AppOk(HFnRef("mathlib::sum".to_string()), vec![]),
                SpecKind::Forall,
            )],
        );
        let err = translate(r#"(module (func $applier))"#, &spec_funcs, &hspecs)
            .expect_err("a merged-body symbol no function carries must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("this module imports nothing")
                && msg.contains("Translate the linker's own output"),
            "the rejection must say the merge has already run; got: {msg}",
        );
        assert!(
            !msg.contains("Link the module before translating it"),
            "the pre-link repair cannot apply to a module with no imports; got: {msg}",
        );
    }

    /// An application whose argument count is not its target's parameter count
    /// is rejected before emission, in both the `T_app` and `HA_app_ok`
    /// positions and in both directions (too few, too many).
    ///
    /// Nothing downstream can catch this. `T_app`'s arguments are a `seq term`,
    /// so a wrong-width application is well-formed Gallina: it elaborates, the
    /// `coqc` gate passes, and the obligation goes on to say something about a
    /// function other than the one it names.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_wrong_arity_application_is_rejected() {
        let module = r#"
            (module
              (func $pair (param i32) (param i32) (result i32)
                local.get 0)
              (func $applier))
            "#;
        for (label, payload, arity) in [
            (
                "T_app, one argument short",
                HAssert::Defined(HTerm::App(
                    HFnRef("pair".to_string()),
                    vec![HTerm::Local(0)],
                )),
                1,
            ),
            (
                "T_app, one argument too many",
                HAssert::Defined(HTerm::App(
                    HFnRef("pair".to_string()),
                    vec![HTerm::Local(0), HTerm::Local(1), HTerm::Local(2)],
                )),
                3,
            ),
            (
                "HA_app_ok, no arguments at all",
                HAssert::AppOk(HFnRef("pair".to_string()), vec![]),
                0,
            ),
            (
                "HA_app_ok, one argument too many",
                HAssert::AppOk(
                    HFnRef("pair".to_string()),
                    vec![HTerm::Local(0), HTerm::Local(1), HTerm::Local(2)],
                ),
                3,
            ),
        ] {
            let (spec_funcs, hspecs) = spec_maps(
                "Applied",
                vec![],
                vec![HSpecEntry::new(
                    HFnRef("applier".to_string()),
                    payload,
                    SpecKind::Forall,
                )],
            );
            let msg = translate(module, &spec_funcs, &hspecs)
                .err()
                .unwrap_or_else(|| panic!("{label}: a wrong-arity application must be rejected"))
                .to_string();
            assert!(
                msg.contains(&format!(
                    "to {arity} argument(s), but the function it names takes 2"
                )) && msg.contains("pair"),
                "{label}: the rejection must name the symbol and both counts; got: {msg}",
            );
        }

        // The control: the same symbol at its real arity translates.
        let (spec_funcs, hspecs) = spec_maps(
            "Applied",
            vec![],
            vec![HSpecEntry::new(
                HFnRef("applier".to_string()),
                HAssert::Defined(HTerm::App(
                    HFnRef("pair".to_string()),
                    vec![HTerm::Local(0), HTerm::Local(1)],
                )),
                SpecKind::Forall,
            )],
        );
        translate(module, &spec_funcs, &hspecs)
            .expect("an application at the function's own arity translates");

        // One symbol applied at two arities: the collection de-duplicates whole
        // applications, not symbols, so the wrong-arity one is still seen. Keyed
        // on the symbol alone it would be absorbed by the correct application
        // that sorts before it and reach emission unchecked.
        let (spec_funcs, hspecs) = spec_maps(
            "Applied",
            vec![],
            vec![
                HSpecEntry::new(
                    HFnRef("applier".to_string()),
                    HAssert::Defined(HTerm::App(
                        HFnRef("pair".to_string()),
                        vec![HTerm::Local(0), HTerm::Local(1)],
                    )),
                    SpecKind::Forall,
                ),
                HSpecEntry::new(
                    HFnRef("applier".to_string()),
                    HAssert::Defined(HTerm::App(
                        HFnRef("pair".to_string()),
                        vec![HTerm::Local(0), HTerm::Local(1), HTerm::Local(2)],
                    )),
                    SpecKind::Forall,
                ),
            ],
        );
        let err = translate(module, &spec_funcs, &hspecs)
            .expect_err("a wrong-arity application beside a correct one must still be rejected");
        assert!(
            err.to_string().contains("to 3 argument(s)"),
            "the rejection must name the wrong-arity application; got: {err}",
        );
    }

    /// Reachability classification hard-depends on the name section: a module
    /// with `exists`/`unique` obligations but no function names is rejected,
    /// where a forall-only module without `T_app` still translates.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn absent_name_section_rejects_reachability_obligations() {
        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![0], vec![exists_entry("ex_fn", 0, vec![0])]);
        // No `$` identifiers, so `wat` emits no name section.
        let err = translate("(module (func (param i32)))", &spec_funcs, &hspecs)
            .expect_err("a nameless module cannot resolve a reachability target");
        let msg = err.to_string();
        assert!(
            msg.contains("carries no function names"),
            "the rejection must name the missing name section; got: {msg}",
        );

        // The forall-only control: same nameless module, universal obligation,
        // translates fine.
        let (spec_funcs, hspecs) = spec_maps(
            "Only",
            vec![0],
            vec![HSpecEntry::new(
                HFnRef("ghost".to_string()),
                HAssert::Defined(HTerm::Local(0)),
                SpecKind::Forall,
            )],
        );
        translate("(module (func (param i32)))", &spec_funcs, &hspecs)
            .expect("a forall obligation needs no symbol resolution");
    }

    /// A reachability symbol no defined function carries is rejected.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn unresolvable_reachability_symbol_is_rejected() {
        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![0], vec![exists_entry("ghost", 0, vec![0])]);
        let err = translate("(module (func $ex_fn (param i32)))", &spec_funcs, &hspecs)
            .expect_err("an unresolvable reachability symbol must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("no defined function in the module carries") && msg.contains("ghost"),
            "the rejection must name the unresolvable symbol; got: {msg}",
        );
    }

    /// A reachability symbol shared by several defined functions is ambiguous
    /// and rejected. The name-section collision cannot be written in WAT, so
    /// the section is appended by hand.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn ambiguous_reachability_symbol_is_rejected() {
        let skeleton = wat::parse_str(
            r#"
            (module
              (func (param i32))
              (func (param i32)))
            "#,
        )
        .expect("ambiguity skeleton assembles");

        // name section: both function indices carry the identical name
        // `ex_probe`.
        let func_name = b"ex_probe";
        let mut func_subsec = Vec::new();
        func_subsec.push(2u8);
        for idx in 0u8..2 {
            func_subsec.push(idx);
            func_subsec.push(func_name.len() as u8);
            func_subsec.extend_from_slice(func_name);
        }
        let mut name_payload = Vec::new();
        name_payload.push(0x04);
        name_payload.extend_from_slice(b"name");
        name_payload.push(0x01);
        name_payload.push(func_subsec.len() as u8);
        name_payload.extend_from_slice(&func_subsec);
        let mut bytes = skeleton;
        bytes.push(0x00);
        bytes.push(name_payload.len() as u8);
        bytes.extend_from_slice(&name_payload);

        let (spec_funcs, hspecs) = spec_maps(
            "Reach",
            vec![0, 1],
            vec![exists_entry("ex_probe", 0, vec![0])],
        );
        let err = translate_bytes("Prog", &bytes, &spec_funcs, &hspecs)
            .expect_err("an ambiguous reachability symbol must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("2 defined") && msg.contains("ambiguous"),
            "the rejection must report the ambiguity; got: {msg}",
        );
    }

    /// A reachability obligation resolving to a function `inference.spec_funcs`
    /// does not list under its spec is rejected: retention may only ever move
    /// a spec function.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn reachability_target_outside_the_spec_index_list_is_rejected() {
        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![1], vec![exists_entry("exec", 0, vec![0])]);
        let err = translate(
            r#"
            (module
              (func $exec (param i32))
              (func $ex_fn (param i32)))
            "#,
            &spec_funcs,
            &hspecs,
        )
        .expect_err("a reachability target outside the spec's index list must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("does not list under that spec"),
            "the rejection must name the spec_funcs disagreement; got: {msg}",
        );
    }

    /// `entry_arity` exceeding the retained function's parameter count is
    /// rejected before emission: the choice suffix can only extend the source
    /// parameters, never shrink them, so an oversized arity is producer drift.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn entry_arity_exceeding_the_param_count_is_rejected() {
        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![0], vec![exists_entry("ex_fn", 2, vec![0])]);
        let err = translate("(module (func $ex_fn (param i32)))", &spec_funcs, &hspecs)
            .expect_err("an oversized entry arity must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("declares entry arity 2") && msg.contains("parameter count is 1"),
            "the rejection must report the arity overflow; got: {msg}",
        );
    }

    /// A `visible_locs` slot outside the retained function's frame
    /// (parameters + declared locals) is rejected before emission.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn visible_loc_outside_the_frame_is_rejected() {
        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![0], vec![unique_entry("uq_fn", 0, vec![2])]);
        // One parameter plus one declared local: frame = 2, so slot 2 is out.
        let err = translate(
            "(module (func $uq_fn (param i32) (local i32)))",
            &spec_funcs,
            &hspecs,
        )
        .expect_err("an out-of-frame visible slot must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("source-visible slot 2")
                && msg.contains("frame size (parameters + locals) is 2"),
            "the rejection must report the out-of-frame slot; got: {msg}",
        );

        // The in-frame control: slots 0 and 1 both fit.
        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![0], vec![unique_entry("uq_fn", 0, vec![0, 1])]);
        translate(
            "(module (func $uq_fn (param i32) (local i32)))",
            &spec_funcs,
            &hspecs,
        )
        .expect("in-frame visible slots pass the bounds check");
    }

    /// The explicit-vs-embedded `inference.hspecs` reconciliation covers the
    /// kind and reachability-metadata fields: byte-identical maps agree, a map
    /// differing only in `visible_locs` is a mismatch, and empty explicit maps
    /// adopt the embedded section all the way through reachability emission.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn embedded_hspecs_round_trip_covers_reachability_metadata() {
        fn leb128_u32(mut value: u32) -> Vec<u8> {
            let mut out = Vec::new();
            loop {
                let byte = (value & 0x7f) as u8;
                value >>= 7;
                if value == 0 {
                    out.push(byte);
                    break;
                }
                out.push(byte | 0x80);
            }
            out
        }
        fn append_custom_section(bytes: &mut Vec<u8>, name: &str, payload: &[u8]) {
            let mut content = Vec::new();
            content.extend_from_slice(&leb128_u32(u32::try_from(name.len()).unwrap()));
            content.extend_from_slice(name.as_bytes());
            content.extend_from_slice(payload);
            bytes.push(0x00);
            bytes.extend_from_slice(&leb128_u32(u32::try_from(content.len()).unwrap()));
            bytes.extend_from_slice(&content);
        }

        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![0], vec![exists_entry("ex_fn", 0, vec![0])]);

        // The embedded twin of the explicit maps: an `inference.spec_funcs`
        // payload (version, one pair) plus the canonical hspecs encoding.
        let mut spec_funcs_payload = Vec::new();
        spec_funcs_payload.extend(leb128_u32(super::SPEC_FUNCS_SECTION_VERSION));
        spec_funcs_payload.extend(leb128_u32(1));
        spec_funcs_payload.extend(leb128_u32(5));
        spec_funcs_payload.extend_from_slice(b"Reach");
        spec_funcs_payload.extend(leb128_u32(1));
        spec_funcs_payload.extend(leb128_u32(0));

        let mut bytes =
            wat::parse_str("(module (func $ex_fn (param i32)))").expect("fixture assembles");
        append_custom_section(
            &mut bytes,
            super::SPEC_FUNCS_SECTION_NAME,
            &spec_funcs_payload,
        );
        append_custom_section(
            &mut bytes,
            inference_hassert::HSPECS_SECTION_NAME,
            &inference_hassert::encode(&hspecs),
        );

        // Empty explicit maps adopt the embedded sections; the adopted kind
        // reaches emission.
        let adopted = translate_bytes("Prog", &bytes, &FxHashMap::default(), &HSpecMap::default())
            .expect("embedded sections are adopted");
        assert!(
            adopted.contains("Definition Prog__Reach__ex_specs : list reachability_spec :="),
            "the adopted exists-kind entry must reach reachability emission:\n{adopted}",
        );

        // Byte-identical explicit maps agree with the embedded sections.
        let agreed = translate_bytes("Prog", &bytes, &spec_funcs, &hspecs)
            .expect("matching explicit maps agree with the embedded sections");
        assert_eq!(adopted, agreed, "both paths must emit identical output");

        // A map differing ONLY in the reachability metadata is a mismatch:
        // the equality got stricter with the kind fields.
        let (_, divergent) = spec_maps("Reach", vec![0], vec![exists_entry("ex_fn", 0, vec![])]);
        let err = translate_bytes("Prog", &bytes, &spec_funcs, &divergent)
            .expect_err("a visible_locs disagreement must be a mismatch");
        assert!(
            matches!(
                err.downcast_ref::<WasmToVError>(),
                Some(WasmToVError::EmbeddedHspecsMismatch { .. })
            ),
            "the disagreement must surface as EmbeddedHspecsMismatch; got: {err:?}",
        );
    }

    /// Residual non-determinism in a retained body is rejected: the
    /// reachability lowering is vanilla WASM by construction, so a 0xfc opcode
    /// in a retained body is a corrupt or foreign artifact.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn residual_nondet_in_a_retained_body_is_rejected() {
        // `wat` cannot assemble the custom 0xfc opcodes, so splice a raw
        // `i32.uzumaki` (0xfc 0x31) body into a named one-function module.
        let mut bytes = vec![
            0x00, 0x61, 0x73, 0x6d, // magic
            0x01, 0x00, 0x00, 0x00, // version
            // type section: one () -> () type
            0x01, 0x04, 0x01, 0x60, 0x00, 0x00,
            // function section: one function of type 0
            0x03, 0x02, 0x01, 0x00, // code section: one body: uzumaki, drop, end
            0x0a, 0x07, 0x01, 0x05, 0x00, 0xfc, 0x31, 0x1a, 0x0b,
        ];
        // name section naming function 0 `ex_fn`.
        let func_name = b"ex_fn";
        let mut func_subsec = Vec::new();
        func_subsec.push(1u8);
        func_subsec.push(0u8);
        func_subsec.push(func_name.len() as u8);
        func_subsec.extend_from_slice(func_name);
        let mut name_payload = Vec::new();
        name_payload.push(0x04);
        name_payload.extend_from_slice(b"name");
        name_payload.push(0x01);
        name_payload.push(func_subsec.len() as u8);
        name_payload.extend_from_slice(&func_subsec);
        bytes.push(0x00);
        bytes.push(name_payload.len() as u8);
        bytes.extend_from_slice(&name_payload);

        let (spec_funcs, hspecs) =
            spec_maps("Reach", vec![0], vec![exists_entry("ex_fn", 0, vec![])]);
        let err = translate_bytes("Prog", &bytes, &spec_funcs, &hspecs)
            .expect_err("residual non-determinism in a retained body must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains(
                "non-deterministic instruction in a function body the emitted module retains"
            ),
            "the rejection must use the retention-aware wording; got: {msg}",
        );
    }
}

/// Collisions between the names one emitted `.v` gives its top-level constructs.
///
/// A generated `.v` puts everything in one flat namespace: the helper
/// `Definition`s the preamble always opens with, one `Definition` per retained
/// function, the `Definition <module> : module` record, and the
/// `Theorem valid_<module>` that judges it. Rocq definitions are not
/// overloadable, so a name claimed twice makes `coqc` reject the whole file
/// with `<name> already exists` — nothing in it elaborates, including the
/// definitions that were fine. Emission never noticed, so the failure surfaced
/// only when someone tried to check the proof.
///
/// The two halves take opposite answers, and these tests pin both. A *function*
/// name is disambiguated, because the emitted spelling is read only from
/// `mod_funcs` — that is what lets an entry file named `main.inf` contain
/// `fn main`, the standard shape in every language. The *module* name is
/// rejected, because it is the artifact's identity: the `.v`'s subject, the
/// `ValidModule` argument, and the prefix of every spec-derived proof name.
///
/// The rename assertions deliberately do not pin the disambiguated spelling.
/// They assert the property that matters — no top-level name appears twice, and
/// `mod_funcs` names the same definition the file defines — so a future change
/// to the suffix scheme stays free.
#[cfg(test)]
mod emitted_name_collisions {
    use super::errors::WasmToVError;
    use super::wasm_parser::translate_bytes;
    use rustc_hash::{FxHashMap, FxHashSet};

    /// Every name the preamble occupies, in emission order. Spelled out here
    /// rather than read from the translator's own list so a name silently
    /// dropped from that list fails this suite instead of hiding in it.
    const PREAMBLE_HELPERS: &[&str] = &["Vi32", "Vi64", "Mt", "Mm", "Mg", "Mi", "Me", "Ma"];

    fn translate(mod_name: &str, bytes: &[u8]) -> anyhow::Result<String> {
        translate_bytes(
            mod_name,
            bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
    }

    /// The whole point of the guard, asserted directly: no `Definition` or
    /// `Theorem` name occurs twice in one emitted module. This is exactly what
    /// `coqc` refuses, so a module passing this cannot be refused for a name.
    fn assert_no_duplicate_top_level_names(v: &str) {
        let mut seen: FxHashSet<&str> = FxHashSet::default();
        for line in v.lines() {
            let Some(rest) = line
                .strip_prefix("Definition ")
                .or_else(|| line.strip_prefix("Theorem "))
            else {
                continue;
            };
            let name = rest
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .next()
                .unwrap_or_default();
            assert!(
                seen.insert(name),
                "`{name}` names two top-level definitions, which coqc refuses:\n{v}",
            );
        }
    }

    /// The names of the emitted `module_func` definitions, in emission order.
    fn module_func_names(v: &str) -> Vec<&str> {
        v.lines()
            .filter_map(|line| {
                let rest = line.strip_prefix("Definition ")?;
                let (name, tail) = rest.split_once(' ')?;
                tail.starts_with(": module_func").then_some(name)
            })
            .collect()
    }

    /// Asserts that translating `bytes` under `mod_name` is rejected as a
    /// preamble shadow naming the contested name and the rename that frees it.
    fn assert_module_shadow(
        mod_name: &str,
        bytes: &[u8],
        expected_name: &str,
        expected_hint: &str,
    ) {
        let err = translate(mod_name, bytes).expect_err("a shadowed module name must be rejected");
        let Some(WasmToVError::ModuleNameShadowsPreambleHelper { name, fix_hint }) =
            err.downcast_ref::<WasmToVError>()
        else {
            panic!("expected ModuleNameShadowsPreambleHelper, got {err:?}");
        };
        assert_eq!(name, expected_name, "the contested name");
        assert_eq!(fix_hint, expected_hint, "the offered rename");
    }

    /// Translates a one-function module whose function is named `func_name`,
    /// asserts the file is duplicate-free, and returns the name the function
    /// was actually emitted under.
    fn emitted_name_of_the_only_function(mod_name: &str, func_name: &str) -> String {
        let out = translate(mod_name, &module_with_function_named(func_name))
            .expect("a function-name collision is renamed, not rejected");
        assert_no_duplicate_top_level_names(&out);
        let funcs = module_func_names(&out);
        assert_eq!(funcs.len(), 1, "one function was emitted:\n{out}");
        let emitted = funcs[0].to_string();
        assert!(
            out.contains(&format!("{emitted} ::")),
            "mod_funcs must reference the definition the file defines:\n{out}",
        );
        emitted
    }

    fn leb128_u32(mut value: u32) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                break;
            }
            out.push(byte | 0x80);
        }
        out
    }

    /// Length-prefixes `text` the way every WASM name is encoded.
    fn wasm_name(text: &str) -> Vec<u8> {
        let mut out = leb128_u32(u32::try_from(text.len()).expect("fixture name fits"));
        out.extend_from_slice(text.as_bytes());
        out
    }

    /// Wraps `body` as one subsection of the `name` custom section.
    fn name_subsection(id: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![id];
        out.extend(leb128_u32(
            u32::try_from(body.len()).expect("fixture subsection fits"),
        ));
        out.extend_from_slice(body);
        out
    }

    /// The `name` custom section carrying `subsections` verbatim.
    fn name_section(subsections: &[u8]) -> Vec<u8> {
        let mut content = wasm_name("name");
        content.extend_from_slice(subsections);
        let mut out = vec![0x00];
        out.extend(leb128_u32(
            u32::try_from(content.len()).expect("fixture section fits"),
        ));
        out.extend_from_slice(&content);
        out
    }

    /// A module of `count` trivial functions carrying no name section, built
    /// with `wat` so only the naming under test is hand-encoded.
    fn skeleton(count: usize) -> Vec<u8> {
        let wat = format!("(module {})", "(func) ".repeat(count));
        wat::parse_str(&wat).expect("skeleton assembles")
    }

    /// A module whose `name` section names function `i` `names[i]`. Hand-encoded
    /// because `wat` derives function names from symbolic identifiers, which
    /// cannot repeat and cannot spell every symbol a foreign binary carries.
    fn module_with_functions_named(names: &[&str]) -> Vec<u8> {
        let mut entries = leb128_u32(u32::try_from(names.len()).expect("fixture count fits"));
        for (index, name) in names.iter().enumerate() {
            entries.extend(leb128_u32(
                u32::try_from(index).expect("fixture index fits"),
            ));
            entries.extend(wasm_name(name));
        }
        let mut bytes = skeleton(names.len());
        bytes.extend(name_section(&name_subsection(0x01, &entries)));
        bytes
    }

    fn module_with_function_named(name: &str) -> Vec<u8> {
        module_with_functions_named(&[name])
    }

    /// A function named after a preamble helper moves off that name, and the
    /// preamble keeps it. Every helper is covered: the preamble writes all eight
    /// unconditionally, so each one is equally contested.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_function_named_as_a_preamble_helper_is_renamed() {
        for helper in PREAMBLE_HELPERS {
            let emitted = emitted_name_of_the_only_function("Prog", helper);
            assert_ne!(
                emitted, *helper,
                "the function must not keep the preamble's name",
            );
        }
    }

    /// A function sharing the module's name moves off it, and the module record
    /// keeps it — the shape `calc.inf` containing `pub fn calc()` produces, and
    /// the one `main.inf` containing `pub fn main()` produces.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_function_named_as_the_module_record_is_renamed() {
        let emitted = emitted_name_of_the_only_function("calc", "calc");
        assert_ne!(
            emitted, "calc",
            "the function must not keep the record's name"
        );
    }

    /// A function named `valid_<module>` moves off the module's validity
    /// theorem, which is a top-level name like any `Definition`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_function_named_as_the_module_theorem_is_renamed() {
        let emitted = emitted_name_of_the_only_function("prog", "valid_prog");
        assert_ne!(
            emitted, "valid_prog",
            "the function must not keep the theorem's name",
        );
    }

    /// Control: seeding the disambiguator with the module's reserved names must
    /// not disturb the collision it already resolved. A static merge folds an
    /// external library's private function next to a same-named main-module
    /// function, and neither name is the user's to change.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn two_ordinary_functions_sharing_a_name_still_disambiguate() {
        let out = translate("Prog", &module_with_functions_named(&["helper", "helper"]))
            .expect("a function-vs-function collision is renamed, not rejected");
        assert_no_duplicate_top_level_names(&out);
        let funcs = module_func_names(&out);
        assert_eq!(funcs.len(), 2, "both functions were emitted:\n{out}");
        assert_ne!(funcs[0], funcs[1], "the two must not share one name");
    }

    /// A module name that is itself a preamble helper is rejected at the public
    /// API boundary, before any parsing. Unlike a function, it has nowhere to
    /// move to.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_module_named_as_a_preamble_helper_is_rejected() {
        for helper in PREAMBLE_HELPERS {
            assert_module_shadow(
                helper,
                &module_with_function_named("f"),
                helper,
                &format!("{helper}_module"),
            );
        }
    }

    /// The embedded `name` section overrides the caller's module name *after*
    /// the API-boundary check has run, so the decode boundary must re-check it —
    /// otherwise a hand-crafted binary smuggles the collision straight past.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_module_renamed_by_the_name_section_is_rejected() {
        let mut bytes = skeleton(1);
        bytes.extend(name_section(&name_subsection(0x00, &wasm_name("Me"))));
        assert_module_shadow("Prog", &bytes, "Me", "Me_module");
    }

    /// Control: a module contesting nothing keeps every name unchanged. This is
    /// what pins the seeding as inert for ordinary input — the overwhelming
    /// majority of modules, whose `.v` must not move a byte.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_module_with_no_collision_is_untouched() {
        let out = translate("Prog", &module_with_function_named("add_three"))
            .expect("a clean module translates");
        assert_no_duplicate_top_level_names(&out);
        for helper in PREAMBLE_HELPERS {
            assert!(
                out.contains(&format!("Definition {helper} ")),
                "the preamble must still define `{helper}`:\n{out}",
            );
        }
        assert_eq!(
            module_func_names(&out),
            vec!["add_three"],
            "an uncontested function keeps its own name:\n{out}",
        );
        assert!(out.contains("Definition Prog : module :="), "{out}");
        assert!(
            out.contains("Theorem valid_Prog : ValidModule Prog."),
            "{out}"
        );
    }
}

/// Malformed function bodies reach a clean rejection, never an abort.
///
/// A `.wasm` arriving through the public library API is not the linker's own
/// output: it can be truncated, hand-crafted, or adversarial. Every failure
/// below used to end the *process* rather than the translation — four `unwrap`s
/// on reader results, plus an unbounded expansion of a declared locals count
/// that the OS resolves with SIGKILL. None of those is recoverable by a caller,
/// and a SIGKILL is not even observable as an error.
///
/// Each row here pins the recoverable outcome and greps a prefix this crate
/// owns (`function body:`, `function locals:`, `function section:`) rather than
/// the parser's wording, which belongs to the parser and moves with it. The
/// positive companions matter as much as the rejections: the shapes next to
/// each malformed one are legal, and a fix that rejected them too would be a
/// worse bug than the panic.
#[cfg(test)]
mod malformed_bodies {
    use super::errors::WasmToVError;
    use super::wasm_parser::translate_bytes;
    use inference_hassert::{HAssert, HFnRef, HSpecEntry, HSpecMap, HTerm, ReachMeta, SpecKind};
    use rustc_hash::FxHashMap;

    fn translate(bytes: &[u8]) -> anyhow::Result<String> {
        translate_bytes("Prog", bytes, &FxHashMap::default(), &HSpecMap::default())
    }

    /// Wraps `body` — the raw bytes *after* a code entry's length prefix, i.e.
    /// the locals vector followed by the operator stream — into a one-function
    /// module. Hand-encoded because every shape under test is one `wat` refuses
    /// to assemble.
    pub(super) fn module_with_body(body: &[u8]) -> Vec<u8> {
        let mut code = vec![0x01]; // one function body
        code.push(u8::try_from(body.len()).expect("fixture body fits one byte"));
        code.extend_from_slice(body);

        let mut module = vec![
            0x00, 0x61, 0x73, 0x6d, // magic
            0x01, 0x00, 0x00, 0x00, // version
            0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // type section: () -> ()
            0x03, 0x02, 0x01, 0x00, // function section: one func, type 0
        ];
        module.push(0x0a); // code section id
        module.push(u8::try_from(code.len()).expect("fixture code section fits one byte"));
        module.extend_from_slice(&code);
        module
    }

    /// The contract every rejection row shares: a recoverable
    /// [`WasmToVError::WasmParse`] whose message contains each needle.
    ///
    /// The sibling of `unsupported_surface::assert_rejected`, which hard-requires
    /// `UnsupportedFeature` and would report these as the wrong variant. The two
    /// stay separate because the distinction is the point: a malformed binary and
    /// a well-formed one outside the proof model need different guidance.
    ///
    /// Deliberately no `catch_unwind`: an `unwrap` still reachable for one of
    /// these fails the test as a panic, which is exactly what this module exists
    /// to rule out.
    fn assert_parse_rejected(label: &str, bytes: &[u8], needles: &[&str]) {
        let err = translate(bytes)
            .map(|v| panic!("{label}: must be rejected, but a `.v` was emitted:\n{v}"))
            .unwrap_err();

        let Some(WasmToVError::WasmParse(message)) = err.downcast_ref::<WasmToVError>() else {
            panic!("{label}: expected WasmParse, got {err:?}");
        };
        for needle in needles {
            assert!(
                message.contains(needle),
                "{label}: rejection must name `{needle}`; got: {message}",
            );
        }
    }

    /// A locals vector whose count LEB128 runs off the end of the body. The
    /// reader cannot even be constructed; the failure used to be swallowed by an
    /// `if let Ok(..)`, emitting `modfunc_locals := nil` for a function whose
    /// locals were never read — a module record describing a different program,
    /// at exit 0.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_truncated_locals_count_is_rejected() {
        assert_parse_rejected(
            "truncated locals count",
            &module_with_body(&[0x80]),
            &["function locals"],
        );
    }

    /// A locals vector that declares one group and then ends before its value
    /// type. The reader constructs; the entry fails. This is the `local.unwrap()`
    /// that used to abort the process.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_truncated_locals_entry_is_rejected() {
        assert_parse_rejected(
            "truncated locals entry",
            &module_with_body(&[0x01, 0x01]),
            &["function locals"],
        );
    }

    /// A single locals group declaring 200 000 repetitions. Six bytes of input
    /// used to drive the emission loop into an unbounded `String`; the cap
    /// rejects it before anything is materialized.
    ///
    /// 200 000 rather than a headline `u32::MAX`: the point is to exceed the
    /// engine limit, and a count near `u32::MAX` OOM-kills the test runner
    /// outright if the cap ever regresses — SIGKILL reads as infrastructure
    /// flake rather than as this test failing.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_oversized_locals_count_is_rejected() {
        assert_parse_rejected(
            "200000 declared locals",
            // count 1; reps 200000 (LEB `c0 9a 0c`); i32; end
            &module_with_body(&[0x01, 0xc0, 0x9a, 0x0c, 0x7f, 0x0b]),
            &["function locals", "exceeds"],
        );
    }

    /// The cap is on the function's *total* declared locals, so groups that are
    /// individually well under it are rejected once they sum past it. Checking
    /// per group would let this through.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn locals_groups_summing_past_the_cap_are_rejected() {
        let mut body = vec![0x03]; // three groups
        for _ in 0..3 {
            body.extend_from_slice(&[0xc0, 0x9a, 0x0c, 0x7f]); // 200000 x i32
        }
        body.push(0x0b);
        assert_parse_rejected(
            "three groups of 200000",
            &module_with_body(&body),
            &["exceeds"],
        );
    }

    /// An operator stream that ends mid-instruction: `i32.const` with no
    /// immediate. This is the `next_operator.as_ref().unwrap()` that used to
    /// abort the process.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_truncated_operator_is_rejected() {
        assert_parse_rejected(
            "i32.const with no immediate",
            &module_with_body(&[0x00, 0x41]),
            &["function body"],
        );
    }

    /// An `if` whose arm never closes. The recursion returns an empty arm, and
    /// `last_part().unwrap()` used to abort on it.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_unterminated_if_is_rejected() {
        assert_parse_rejected(
            "unterminated if",
            // 0 locals; i32.const 0; if (empty blocktype); EOF
            &module_with_body(&[0x00, 0x41, 0x00, 0x04, 0x40]),
            // The distinctive phrase, so this can only match the empty-arm
            // rejection and never the generic truncated-operator one, which
            // shares the `function body` prefix.
            &["ends without its terminating"],
        );
    }

    /// The positive companion to the row above, and the one a naive fix breaks:
    /// an `if` with an empty `then` arm is legal, and its terminating `end` is
    /// consumed by the inner recursion — so the arm is *not* empty and must
    /// still translate.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_empty_then_arm_still_translates() {
        let bytes = wat::parse_str("(module (func i32.const 0 if end))")
            .expect("an empty-then `if` assembles");
        let out = translate(&bytes).expect("a legal empty-then `if` must still translate");
        assert!(
            out.contains("BI_if"),
            "the `if` must reach the emitted body:\n{out}",
        );
    }

    /// A function section advertising one entry and carrying none. This is the
    /// `f.unwrap()` inside the `for_each` closure that used to abort the process.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_truncated_function_section_is_rejected() {
        let bytes = vec![
            0x00, 0x61, 0x73, 0x6d, // magic
            0x01, 0x00, 0x00, 0x00, // version
            0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // type section: () -> ()
            0x03, 0x01, 0x01, // function section: count 1, no entries
        ];
        assert_parse_rejected("truncated function section", &bytes, &["function section"]);
    }

    /// The neighbours of every row above: a body with real locals and a real
    /// operator stream still translates, and its locals reach the record.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_well_formed_body_with_locals_still_translates() {
        // one group of two i32 locals, then `end`
        let out = translate(&module_with_body(&[0x01, 0x02, 0x7f, 0x0b]))
            .expect("a well-formed body translates");
        assert!(
            out.contains("modfunc_locals := T_num T_i32 :: T_num T_i32 :: nil"),
            "both declared locals must reach the record:\n{out}",
        );
    }

    /// One malformed locals vector, translated twice: once with a reachability
    /// obligation naming the function and once without.
    ///
    /// The two paths read the same bytes through different error policies —
    /// `defined_func_local_count` is fail-closed (`.ok()?`), the emission loop
    /// was not — so the same input was a clean rejection or a process abort
    /// depending on whether an unrelated `exists`/`unique` obligation happened
    /// to be present. Both are now recoverable and both name the locals; the
    /// wording still differs by design, because the obligation path can name the
    /// obligation it is about and the emission path can only name the body.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_malformed_locals_vector_rejects_the_same_way_with_and_without_an_obligation() {
        let bytes = named_module_with_body("target", &[0x01, 0x01]);

        let without = translate(&bytes).expect_err("no obligation: must be rejected");
        let with = {
            let mut spec_funcs: FxHashMap<String, Vec<u32>> = FxHashMap::default();
            spec_funcs.insert("Reach".to_string(), vec![0]);
            let mut hspecs = HSpecMap::default();
            hspecs.insert(
                "Reach".to_string(),
                vec![HSpecEntry::new(
                    HFnRef("target".to_string()),
                    HAssert::Defined(HTerm::Local(0)),
                    SpecKind::Exists(ReachMeta {
                        entry_arity: 0,
                        visible_locs: Vec::new(),
                    }),
                )],
            );
            translate_bytes("Prog", &bytes, &spec_funcs, &hspecs)
                .expect_err("with an obligation: must be rejected")
        };

        for (label, err) in [("without an obligation", &without), ("with one", &with)] {
            assert!(
                err.downcast_ref::<WasmToVError>().is_some(),
                "{label}: must be a recoverable typed rejection, got {err:?}",
            );
            assert!(
                err.to_string().contains("locals"),
                "{label}: the rejection must name the locals; got: {err}",
            );
        }
    }

    /// [`module_with_body`] plus a `name` section naming function 0, so an
    /// obligation can refer to it by symbol.
    pub(super) fn named_module_with_body(func_name: &str, body: &[u8]) -> Vec<u8> {
        let mut entries = vec![0x01, 0x00];
        entries.push(u8::try_from(func_name.len()).expect("fixture name fits one byte"));
        entries.extend_from_slice(func_name.as_bytes());

        let mut subsection = vec![0x01];
        subsection.push(u8::try_from(entries.len()).expect("fixture subsection fits one byte"));
        subsection.extend_from_slice(&entries);

        let mut content = vec![0x04];
        content.extend_from_slice(b"name");
        content.extend_from_slice(&subsection);

        let mut bytes = module_with_body(body);
        bytes.push(0x00);
        bytes.push(u8::try_from(content.len()).expect("fixture name section fits one byte"));
        bytes.extend_from_slice(&content);
        bytes
    }
}

/// The type section, read fail-closed.
///
/// `mod_types` position N must be WASM type index N: every consumer indexes it
/// that way, and the emitter did not provide it. A recursion group was rendered
/// as one list element regardless of how many `SubType`s it flattens to, so an
/// empty `(rec)` claimed an index it does not have, a multi-member group ran its
/// entries together into a single over-applied `Tf`, and every index after
/// either one was wrong. Composites with no `function_type` representation —
/// GC aggregates, continuations, declared subtypes, shared types — were skipped
/// silently, which is the same index shift with no output to notice it by.
///
/// The rejections and the restructure are tested apart on purpose: a fix that
/// rejects GC but keeps the concatenation passes every rejection row here, and
/// only the positional tests catch it.
#[cfg(test)]
mod type_section {
    use super::errors::WasmToVError;
    use super::wasm_parser::translate_bytes;
    use rustc_hash::FxHashMap;

    fn translate(bytes: &[u8]) -> anyhow::Result<String> {
        translate_bytes(
            "Prog",
            bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
    }

    fn assert_rejected(label: &str, bytes: &[u8], needles: &[&str]) {
        let err = translate(bytes)
            .map(|v| panic!("{label}: must be rejected, but a `.v` was emitted:\n{v}"))
            .unwrap_err();
        let Some(WasmToVError::UnsupportedFeature { description }) =
            err.downcast_ref::<WasmToVError>()
        else {
            panic!("{label}: expected UnsupportedFeature, got {err:?}");
        };
        for needle in needles {
            assert!(
                description.contains(needle),
                "{label}: rejection must name `{needle}`; got: {description}",
            );
        }
    }

    fn assert_wat_rejected(label: &str, wat: &str, needles: &[&str]) {
        let bytes = wat::parse_str(wat).expect("fixture WAT assembles");
        assert_rejected(label, &bytes, needles);
    }

    /// The emitted `mod_types` list, one string per **list element**.
    ///
    /// Counting `Tf` occurrences instead would be blind to the defect this
    /// module is about: concatenating a rec group's members into one element
    /// leaves every `Tf` substring present and only the element *boundaries*
    /// wrong. `mod_types` is indexed positionally, so the boundaries are the
    /// whole invariant.
    fn mod_types_entries(v: &str) -> Vec<&str> {
        let block = v
            .split_once("mod_types :=\n")
            .expect("every module emits a mod_types list")
            .1
            .split_once(";\n")
            .expect("the mod_types list is terminated")
            .0;
        block
            .lines()
            .map(|line| line.trim().trim_end_matches("::").trim())
            .filter(|entry| !entry.is_empty() && *entry != "nil")
            .collect()
    }

    /// A GC struct in the type section has no `function_type` to become.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_struct_type_is_rejected() {
        assert_wat_rejected(
            "struct type",
            r#"(module (type $s (struct (field i32))))"#,
            &["aggregate in the type section"],
        );
    }

    /// The same for a GC array.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_array_type_is_rejected() {
        assert_wat_rejected(
            "array type",
            r#"(module (type $a (array i32)))"#,
            &["aggregate in the type section"],
        );
    }

    /// A continuation type, from stack switching.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_continuation_type_is_rejected() {
        assert_wat_rejected(
            "cont type",
            r#"(module (type $ft (func)) (type $ct (cont $ft)))"#,
            &["continuation type in the type section"],
        );
    }

    /// A type declaring participation in a subtyping hierarchy. The model's
    /// `function_type` is a bare parameter/result pair with no subtype relation.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_declared_subtype_is_rejected() {
        assert_wat_rejected(
            "sub func",
            r#"(module (rec (type $base (sub (func (param i32)))))
                 (type $derived (sub $base (func (param i32)))))"#,
            &["declared subtyping"],
        );
    }

    /// An empty recursion group, hand-encoded because `wat` will not emit one.
    /// It occupies no type index, so it must contribute no `mod_types` element —
    /// and contributing one produced a dangling cons (`::` with nothing before
    /// it), which is not parseable Gallina at all.
    ///
    /// This is a *separate* source of that dangling cons from the GC composites:
    /// neutralize the composite rejections alone and this row still fails.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_empty_rec_group_contributes_no_entry() {
        // type section: one entry, `rec` (0x4e) with zero members
        let bytes = [
            0x00, 0x61, 0x73, 0x6d, // magic
            0x01, 0x00, 0x00, 0x00, // version
            0x01, 0x03, 0x01, 0x4e, 0x00, // type section: (rec)
        ];
        let v = translate(&bytes).expect("an empty rec group is legal and contributes nothing");
        assert!(
            mod_types_entries(&v).is_empty(),
            "an empty rec group occupies no type index, so it must contribute no \
             `mod_types` entry:\n{v}",
        );
        assert!(
            !v.contains("    ::"),
            "a dangling `::` is unparseable Gallina:\n{v}",
        );
    }

    /// A shared composite, hand-encoded because the `wat` in the lock file emits
    /// composite prefixes this fork rejects for the `shared` form.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_shared_composite_is_rejected() {
        // type section: one entry, `shared` (0x65) wrapping `func () -> ()`
        let bytes = [
            0x00, 0x61, 0x73, 0x6d, // magic
            0x01, 0x00, 0x00, 0x00, // version
            0x01, 0x05, 0x01, 0x65, 0x60, 0x00, 0x00, // type section: (shared (func))
        ];
        assert_rejected("shared composite", &bytes, &["shared composite"]);
    }

    /// The loud leg of the multi-member group defect: two `Func` members ran
    /// together with no separator, which Rocq reads as an over-applied `Tf`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_multi_member_rec_group_emits_separate_entries() {
        let bytes = wat::parse_str(
            r#"(module (rec (type $a (func (param i32))) (type $b (func (result i32)))))"#,
        )
        .expect("a multi-member rec group assembles");
        let v = translate(&bytes).expect("a rec group of `Func` members translates");
        assert!(
            !v.contains(")Tf"),
            "rec-group members must not run together into one element:\n{v}",
        );
        assert_eq!(
            mod_types_entries(&v).len(),
            2,
            "each member must be its own `mod_types` element:\n{v}",
        );
    }

    /// The silent leg, and the only one that catches a fix which rejects GC but
    /// keeps the concatenation: a function typed by the *second* member of a
    /// recursion group. `modfunc_type` is a positional index into `mod_types`,
    /// so if the group collapsed into one element the index would name the
    /// wrong type — or no type at all — with nothing in the output looking
    /// wrong.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_function_typed_by_a_later_rec_member_keeps_its_index() {
        let bytes = wat::parse_str(
            r#"(module
                 (rec (type $a (func (param i32))) (type $b (func (result i32))))
                 (func (type $b) i32.const 7))"#,
        )
        .expect("a function typed by the second rec member assembles");
        let v = translate(&bytes).expect("it translates");
        assert!(
            v.contains("modfunc_type := 1%N"),
            "the function must keep type index 1, the group's second member:\n{v}",
        );
        // Asserted as list *elements*, so a concatenated group fails here: both
        // `Tf` substrings survive concatenation, only the boundary between them
        // is lost, and that boundary is what `modfunc_type := 1%N` indexes into.
        assert_eq!(
            mod_types_entries(&v),
            vec![
                "Tf (T_num T_i32 :: nil) (nil)",
                "Tf (nil) (T_num T_i32 :: nil)"
            ],
            "each member must be its own positional entry:\n{v}",
        );
    }

    /// The invariant behind the whole class, asserted directly over the shapes
    /// the emitter is expected to handle: the number of emitted `Tf` forms
    /// equals the number of flattened `SubType`s, so `mod_types` position is the
    /// WASM type index. Every defect in this module is one violation of it.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn emitted_type_count_matches_the_flattened_subtype_count() {
        for (label, wat) in [
            ("no types", "(module)"),
            ("one func", "(module (type (func)))"),
            (
                "three funcs",
                "(module (type (func)) (type (func (param i32))) (type (func (result i32))))",
            ),
            (
                "a rec group beside a bare type",
                "(module (rec (type (func)) (type (func (param i64)))) (type (func (result i64))))",
            ),
        ] {
            let bytes = wat::parse_str(wat).expect("fixture assembles");
            let expected = inf_wasmparser::Parser::new(0)
                .parse_all(&bytes)
                .filter_map(|payload| match payload {
                    Ok(inf_wasmparser::Payload::TypeSection(reader)) => Some(reader),
                    _ => None,
                })
                .flat_map(|reader| reader.into_iter().collect::<Vec<_>>())
                .filter_map(std::result::Result::ok)
                .map(|group| group.types().count())
                .sum::<usize>();
            let v = translate(&bytes).expect("fixture translates");
            let entries = mod_types_entries(&v);
            assert_eq!(
                entries.len(),
                expected,
                "{label}: one `mod_types` entry per flattened SubType:\n{v}",
            );
            for entry in &entries {
                assert_eq!(
                    entry.matches("Tf (").count(),
                    1,
                    "{label}: an entry carrying two `Tf` forms is one over-applied \
                     `Tf` to Rocq, and shifts every later type index:\n{v}",
                );
            }
        }
    }
}

/// Every malformed input this crate is known to reject leaves through the typed
/// error channel.
///
/// The property `translate_bytes` owes its callers: a rejection is always a
/// downcastable [`WasmToVError`], never a bare parser error the CLI can only
/// print through its generic "translation failed" line, and never a panic.
///
/// This is the totality claim that a blanket wrap at the `translate()` seam
/// would have made unfalsifiably. Written as a corpus sweep instead: remove any
/// one of the per-site wraps and this reddens, and every shape a later phase
/// adds to the corpus extends the claim rather than restating it.
#[cfg(test)]
mod rejection_totality {
    use super::errors::WasmToVError;
    use super::malformed_bodies::module_with_body;
    use super::wasm_parser::translate_bytes;
    use rustc_hash::FxHashMap;

    /// A module of the given `(section id, body)` pairs, lengths encoded as a
    /// single LEB byte — sufficient for every fixture here.
    fn raw(sections: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        for (id, body) in sections {
            out.push(*id);
            out.push(u8::try_from(body.len()).expect("fixture section is small"));
            out.extend_from_slice(body);
        }
        out
    }

    /// A two-memory module whose single function body is `bytes` then `end`, so
    /// a multi-memory immediate names a memory that exists.
    fn two_memory_body(bytes: &[u8]) -> Vec<u8> {
        let mut body = vec![0x00];
        body.extend_from_slice(bytes);
        body.push(0x0b);
        let mut code = vec![0x01];
        code.push(u8::try_from(body.len()).expect("fixture body is small"));
        code.extend_from_slice(&body);
        raw(&[
            (1, vec![0x01, 0x60, 0x00, 0x00]),
            (3, vec![0x01, 0x00]),
            (5, vec![0x02, 0x00, 0x01, 0x00, 0x01]),
            (10, code),
        ])
    }

    /// Every malformed shape the crate rejects, as (label, module bytes).
    /// Extended by each phase that adds a rejection.
    fn corpus() -> Vec<(&'static str, Vec<u8>)> {
        let wat = |source: &str| wat::parse_str(source).expect("fixture WAT assembles");
        vec![
            // Malformed bodies.
            ("truncated locals count", module_with_body(&[0x80])),
            ("truncated locals entry", module_with_body(&[0x01, 0x01])),
            (
                "oversized locals count",
                module_with_body(&[0x01, 0xc0, 0x9a, 0x0c, 0x7f, 0x0b]),
            ),
            ("truncated operator", module_with_body(&[0x00, 0x41])),
            (
                "unterminated if",
                module_with_body(&[0x00, 0x41, 0x00, 0x04, 0x40]),
            ),
            (
                "truncated function section",
                vec![
                    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00,
                    0x00, 0x03, 0x01, 0x01,
                ],
            ),
            // Type-section shapes with no `function_type` representation.
            (
                "struct type",
                wat(r#"(module (type $s (struct (field i32))))"#),
            ),
            ("array type", wat(r#"(module (type $a (array i32)))"#)),
            (
                "cont type",
                wat(r#"(module (type $ft (func)) (type $ct (cont $ft)))"#),
            ),
            (
                "declared subtype",
                wat(r#"(module (rec (type $base (sub (func (param i32)))))
                         (type $derived (sub $base (func (param i32)))))"#),
            ),
            (
                "shared composite",
                vec![
                    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x65, 0x60,
                    0x00, 0x00,
                ],
            ),
            // Operators outside the proof contract.
            (
                "ref.null",
                wat(r#"(module (func (result i32) ref.null func drop i32.const 0))"#),
            ),
            ("f32 arithmetic", wat(r#"(module (func f32.const 1 drop))"#)),
            // Shapes the module record has no field to carry.
            ("table64", raw(&[(4, vec![0x01, 0x70, 0x04, 0x01])])),
            ("shared table", raw(&[(4, vec![0x01, 0x70, 0x02, 0x01])])),
            (
                "table element initializer",
                wat(r#"(module (func $g) (table 1 funcref (ref.func $g)))"#),
            ),
            (
                "multi-memory load",
                two_memory_body(&[0x41, 0x00, 0x28, 0x42, 0x01, 0x00]),
            ),
            (
                "multi-memory fill",
                two_memory_body(&[0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0b, 0x01]),
            ),
            (
                "tag section",
                wat(r#"(module (tag $e (param i32)) (func nop))"#),
            ),
            ("unknown section", raw(&[(0x0e, vec![0xaa])])),
            // Structure the binary asserts about itself.
            (
                "component binary",
                vec![0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00],
            ),
            (
                "unknown core version",
                vec![0x00, 0x61, 0x73, 0x6d, 0x02, 0x00, 0x00, 0x00],
            ),
            (
                "duplicate type section",
                raw(&[
                    (1, vec![0x01, 0x60, 0x00, 0x00]),
                    (1, vec![0x01, 0x60, 0x01, 0x7f, 0x00]),
                ]),
            ),
            (
                "sections out of order",
                raw(&[
                    (5, vec![0x01, 0x00, 0x01]),
                    (4, vec![0x01, 0x70, 0x00, 0x01]),
                ]),
            ),
            (
                "data count mismatch",
                raw(&[
                    (5, vec![0x01, 0x00, 0x01]),
                    (12, vec![0x03]),
                    (11, vec![0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x78]),
                ]),
            ),
            (
                "function and code lengths disagree",
                raw(&[
                    (1, vec![0x01, 0x60, 0x00, 0x00]),
                    (3, vec![0x02, 0x00, 0x00]),
                    (10, vec![0x01, 0x02, 0x00, 0x0b]),
                ]),
            ),
            (
                "operators after the terminating end",
                raw(&[
                    (1, vec![0x01, 0x60, 0x00, 0x01, 0x7f]),
                    (3, vec![0x01, 0x00]),
                    (10, vec![0x01, 0x07, 0x00, 0x41, 0x01, 0x0b, 0x41, 0x02, 0x0b]),
                ]),
            ),
        ]
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn every_rejection_is_a_typed_error() {
        for (label, bytes) in corpus() {
            let err = translate_bytes(
                "Prog",
                &bytes,
                &FxHashMap::default(),
                &inference_hassert::HSpecMap::default(),
            )
            .map(|v| panic!("{label}: expected a rejection, but a `.v` was emitted:\n{v}"))
            .unwrap_err();
            assert!(
                err.downcast_ref::<WasmToVError>().is_some(),
                "{label}: every rejection must be downcastable, so the CLI can \
                 classify it; got an untyped {err:?}",
            );
        }
    }
}

/// Import names, export names, and name-section local names are *data* copied
/// out of a `.wasm` into Gallina syntax — a string literal for the first two, a
/// `(* … *)` comment for the third. A WASM name may carry any byte, including
/// the delimiters that end those constructs, so an unescaped name does not
/// merely look wrong: it closes its construct early and the remainder is read
/// as Gallina, fabricating module content the binary never contained.
///
/// Each row below asserts on what the injection actually *produces* rather than
/// on the delimiter it uses. A quote count or a substring search stays green
/// under the very defect it is written for, because the injected text keeps
/// those substrings intact and only the construct boundary moves.
#[cfg(test)]
mod gallina_escaping {
    use super::errors::WasmToVError;
    use super::wasm_parser::translate_bytes;
    use rustc_hash::FxHashMap;

    fn translate(bytes: &[u8]) -> String {
        translate_bytes(
            "Prog",
            bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
        .unwrap_or_else(|e| panic!("fixture must translate, got {e:?}"))
    }

    fn wat(source: &str) -> Vec<u8> {
        wat::parse_str(source).expect("fixture WAT assembles")
    }

    /// Returns `v` with every string literal emptied and every comment removed,
    /// leaving only what Rocq reads as code.
    ///
    /// This is what makes the injection rows non-vacuous. An injection payload
    /// necessarily contains the very text it is trying to smuggle in, so a
    /// substring search over the raw `.v` finds it whether the escape worked or
    /// not — the payload is simply quoted instead of executed. Counting only
    /// outside literals and comments is the difference between "the bytes appear
    /// somewhere" and "the bytes became module content", which is the defect.
    ///
    /// The literal scan honours Coq's doubled-quote escape, so an escaped `""`
    /// stays inside its literal rather than ending it — the precise behaviour
    /// under test.
    fn code_outside_literals_and_comments(v: &str) -> String {
        let chars: Vec<char> = v.chars().collect();
        let mut out = String::new();
        let mut index = 0;
        let mut comment_depth = 0usize;
        while index < chars.len() {
            if comment_depth == 0 && chars[index] == '"' {
                index += 1;
                while index < chars.len() {
                    if chars[index] == '"' {
                        if chars.get(index + 1) == Some(&'"') {
                            index += 2;
                            continue;
                        }
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                out.push_str("\"\"");
                continue;
            }
            if chars[index] == '(' && chars.get(index + 1) == Some(&'*') {
                comment_depth += 1;
                index += 2;
                continue;
            }
            if comment_depth > 0 && chars[index] == '*' && chars.get(index + 1) == Some(&')') {
                comment_depth -= 1;
                index += 2;
                continue;
            }
            if comment_depth == 0 {
                out.push(chars[index]);
            }
            index += 1;
        }
        out
    }

    fn leb128_u32(mut value: u32) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                break;
            }
            out.push(byte | 0x80);
        }
        out
    }

    fn wasm_name(text: &str) -> Vec<u8> {
        let mut out = leb128_u32(u32::try_from(text.len()).expect("fixture name fits"));
        out.extend_from_slice(text.as_bytes());
        out
    }

    /// A module whose single function has one named local, the name hand-encoded
    /// because `wat` derives local names from symbolic identifiers and cannot
    /// spell the delimiters under test.
    fn module_with_local_named(name: &str) -> Vec<u8> {
        let mut naming = leb128_u32(1);
        naming.extend(leb128_u32(0));
        naming.extend(wasm_name(name));

        let mut per_function = leb128_u32(1);
        per_function.extend(leb128_u32(0));
        per_function.extend(naming);

        let mut subsection = vec![0x02];
        subsection.extend(leb128_u32(
            u32::try_from(per_function.len()).expect("fixture subsection fits"),
        ));
        subsection.extend(per_function);

        let mut content = wasm_name("name");
        content.extend(subsection);

        let mut section = vec![0x00];
        section.extend(leb128_u32(
            u32::try_from(content.len()).expect("fixture section fits"),
        ));
        section.extend(content);

        let mut bytes = wat(r#"(module (func (local i32) local.get 0 drop))"#);
        bytes.extend(section);
        bytes
    }

    /// The payload closes the string literal and continues in Gallina, so the
    /// emitted record gains an export the module does not have. One `MED_func`
    /// is the assertion with teeth: the quote survives either way, the
    /// fabricated entry does not.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_export_name_cannot_fabricate_a_second_export() {
        let v = translate(&wat(
            r#"(module (func) (export "a\22 (MED_func 99%N) :: Me \22b" (func 0)))"#,
        ));
        let code = code_outside_literals_and_comments(&v);
        assert_eq!(
            code.matches("MED_func").count(),
            1,
            "one export was declared, so one `MED_func` may reach the record; \
             the rest must stay quoted:\n{v}",
        );
        assert!(
            !code.contains("99%N"),
            "the payload's function index must stay inside the literal:\n{v}",
        );
    }

    /// Both interpolation points of `Mi` are covered. A fix that escapes only
    /// the module argument leaves the field-name leg open, so the two legs are
    /// separate rows rather than one.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn neither_import_name_can_fabricate_a_second_import() {
        let module_leg = translate(&wat(
            r#"(module (import "m\22 (MID_func 0%N) :: Mi \22x" "f" (func)))"#,
        ));
        assert_eq!(
            code_outside_literals_and_comments(&module_leg)
                .matches("MID_func")
                .count(),
            1,
            "one import was declared:\n{module_leg}",
        );

        let field_leg = translate(&wat(
            r#"(module (import "m" "f\22 (MID_func 0%N) :: Mi \22g" (func)))"#,
        ));
        assert_eq!(
            code_outside_literals_and_comments(&field_leg)
                .matches("MID_func")
                .count(),
            1,
            "the field name is the second interpolation point:\n{field_leg}",
        );
    }

    /// A quote reaches the `.v` doubled, which is Coq's own escape, and the
    /// surrounding bytes are untouched — the name has to round-trip through
    /// `list_byte_of_string` exactly.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_quote_in_a_name_is_doubled_not_rewritten() {
        let v = translate(&wat(r#"(module (func) (export "a\22b" (func 0)))"#));
        assert!(
            v.contains(r#"Me "a""b""#),
            "the quote must double and nothing else may move:\n{v}",
        );
    }

    /// Coq comments nest, so an unbalanced `(*` swallows the rest of the file
    /// and a `*)` closes early and lets what follows be read as Gallina.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_local_name_cannot_inject_an_instruction() {
        let injected = translate(&module_with_local_named("*) :: BI_unreachable :: (*"));
        let ordinary = translate(&module_with_local_named("x"));

        // A local name is comment prose. Whatever it contains, the code the
        // comments sit in must be the same module — which is a stronger claim
        // than "the payload's text is absent", and the only one that survives
        // the payload necessarily containing its own text.
        assert_eq!(
            code_outside_literals_and_comments(&injected),
            code_outside_literals_and_comments(&ordinary),
            "a local name may change comment text and nothing else:\n{injected}",
        );
        assert!(
            !code_outside_literals_and_comments(&injected).contains("BI_unreachable"),
            "the module contains no `unreachable`, so the emitted code must not:\n{injected}",
        );
    }

    /// The other half: an opener with no closer.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_unclosed_comment_opener_in_a_local_name_is_neutralized() {
        let v = translate(&module_with_local_named("(*"));
        assert_eq!(
            v.matches("(*").count(),
            v.matches("*)").count(),
            "an opener in a local name must not leave the file unbalanced:\n{v}",
        );
    }

    /// The byte-identity control. `__frame_ptr` is the local codegen emits for
    /// every array-using program; routing local names through identifier
    /// sanitization would render it `f_frame_ptr` and move every byte-compared
    /// `.v` golden. Only the two comment delimiters may ever be touched.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_ordinary_local_name_renders_unchanged() {
        let v = translate(&module_with_local_named("__frame_ptr"));
        assert!(
            v.contains("(*__frame_ptr*)"),
            "an ordinary local name must reach the comment verbatim:\n{v}",
        );
    }

    /// Ordinary names must survive untouched, or the escapes would be rewriting
    /// data rather than protecting it.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn ordinary_import_and_export_names_are_unchanged() {
        let v = translate(&wat(
            r#"(module (import "env" "memcpy" (func)) (func) (export "run" (func 1)))"#,
        ));
        assert!(v.contains(r#"Mi "env" "memcpy""#), "{v}");
        assert!(v.contains(r#"Me "run""#), "{v}");
    }

    /// A name carrying a quote is still a translation, not a rejection: the
    /// escape is the fix, so nothing here may become a `WasmToVError`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_escaped_name_is_not_a_rejection() {
        let bytes = wat(r#"(module (func) (export "a\22b" (func 0)))"#);
        let out = translate_bytes(
            "Prog",
            &bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        );
        assert!(
            out.is_ok(),
            "a quoted name is escapable, so it must translate: {:?}",
            out.err().and_then(|e| e
                .downcast_ref::<WasmToVError>()
                .map(std::string::ToString::to_string)),
        );
    }
}

/// Shapes the emitted module record has no field to carry.
///
/// Each one was previously emitted as its nearest representable neighbour — a
/// 64-bit table as a 32-bit one, an access to memory 1 as an access to memory 0,
/// a tag section as no tag section at all. That is the failure mode this issue
/// exists to close: not a `.v` that fails to compile, but one that compiles and
/// describes a different program than the `.wasm` it came from.
///
/// Every row is paired with a positive control on the neighbouring legal shape.
/// Without them a guard that rejected the whole family — every table, every
/// load — would satisfy the rejection rows while breaking the translator.
#[cfg(test)]
mod unrepresentable_shapes {
    use super::errors::WasmToVError;
    use super::wasm_parser::translate_bytes;
    use rustc_hash::FxHashMap;

    fn translate(bytes: &[u8]) -> anyhow::Result<String> {
        translate_bytes(
            "Prog",
            bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
    }

    fn wat(source: &str) -> Vec<u8> {
        wat::parse_str(source).expect("fixture WAT assembles")
    }

    /// Asserts a recoverable `UnsupportedFeature` whose text contains every
    /// needle, compared lowercased.
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
                "{label}: the description must name `{needle}`; got {description:?}",
            );
        }
    }

    fn section(id: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![id];
        out.push(u8::try_from(body.len()).expect("fixture section is small"));
        out.extend_from_slice(body);
        out
    }

    fn module(sections: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        for s in sections {
            out.extend_from_slice(s);
        }
        out
    }

    /// A one-function module whose body is `bytes` followed by `end`, with a
    /// two-memory memory section so the multi-memory immediates below name a
    /// memory that exists.
    fn two_memory_module_with_body(bytes: &[u8]) -> Vec<u8> {
        let mut body = vec![0x00];
        body.extend_from_slice(bytes);
        body.push(0x0b);
        let mut code = vec![0x01];
        code.push(u8::try_from(body.len()).expect("fixture body is small"));
        code.extend_from_slice(&body);
        module(&[
            section(1, &[0x01, 0x60, 0x00, 0x00]),
            section(3, &[0x01, 0x00]),
            section(5, &[0x02, 0x00, 0x01, 0x00, 0x01]),
            section(10, &code),
        ])
    }

    /// `table64` changes the index type of every table operation, and the
    /// emitted limits record has no field for it. Hand-encoded: the table's
    /// flags byte is what carries the bit.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_64_bit_table_is_rejected() {
        let bytes = module(&[section(4, &[0x01, 0x70, 0x04, 0x01])]);
        assert_rejected("table64", &bytes, &["table64"]);
    }

    /// The shared-everything-threads sibling of the same flags byte.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_shared_table_is_rejected() {
        let bytes = module(&[section(4, &[0x01, 0x70, 0x02, 0x01])]);
        assert_rejected("shared table", &bytes, &["shared table"]);
    }

    /// A table whose slots start at something other than null is modelled by an
    /// all-null table, so it is refused instead.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_table_with_an_element_initializer_is_rejected() {
        let bytes = wat(r#"(module (func $g) (table 1 funcref (ref.func $g)))"#);
        assert_rejected("table init expr", &bytes, &["initializer"]);
    }

    /// Ordinary tables — defined and imported — must keep translating, or the
    /// three rejections above would be satisfied by a guard that broke tables
    /// outright.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn ordinary_tables_still_translate() {
        let defined = translate(&wat(r#"(module (table 1 1 funcref))"#))
            .expect("a plain bounded table translates");
        assert!(
            defined.contains("lim_min := 1%N") && defined.contains("lim_max := Some(1%N)"),
            "{defined}",
        );
        translate(&wat(r#"(module (import "env" "t" (table 1 funcref)))"#))
            .expect("an imported table translates");
    }

    /// `Ma` carries no memory index, so an access to a memory other than the
    /// first cannot be represented. Hand-encoded: `wat` will not spell a
    /// multi-memory `memarg` in every form, and the flag lives in the alignment
    /// byte's bit 6.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_load_from_a_second_memory_is_rejected() {
        // i32.const 0; i32.load align=2|0x40 memidx=1 offset=0
        let bytes = two_memory_module_with_body(&[0x41, 0x00, 0x28, 0x42, 0x01, 0x00]);
        assert_rejected("multi-memory load", &bytes, &["multi-memory"]);
    }

    /// The three bulk-memory operators name their memories directly rather than
    /// through a `memarg`, so each carries its own check.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn bulk_memory_operators_naming_a_second_memory_are_rejected() {
        // memory.copy dst=1 src=0
        assert_rejected(
            "memory.copy destination",
            &two_memory_module_with_body(&[
                0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0a, 0x01, 0x00,
            ]),
            &["multi-memory", "destination"],
        );
        // memory.copy dst=0 src=1
        assert_rejected(
            "memory.copy source",
            &two_memory_module_with_body(&[
                0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0a, 0x00, 0x01,
            ]),
            &["multi-memory", "source"],
        );
        // memory.fill mem=1
        assert_rejected(
            "memory.fill",
            &two_memory_module_with_body(&[0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x0b, 0x01]),
            &["multi-memory", "memory.fill"],
        );
    }

    /// The positive control for the memory guards: the same operators against
    /// memory 0 still emit their constructors. Without this, rejecting the whole
    /// bulk-memory family would pass every row above.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn memory_zero_operations_still_translate() {
        let load = translate(&wat(
            r#"(module (memory 1) (func (result i32) i32.const 0 i32.load))"#,
        ))
        .expect("a load from the only memory translates");
        assert!(load.contains("BI_load"), "{load}");

        let copy = translate(&wat(
            r#"(module (memory 1) (func i32.const 0 i32.const 0 i32.const 0 memory.copy))"#,
        ))
        .expect("memory.copy on the only memory translates");
        assert!(copy.contains("BI_memory_copy"), "{copy}");

        let fill = translate(&wat(
            r#"(module (memory 1) (func i32.const 0 i32.const 0 i32.const 0 memory.fill))"#,
        ))
        .expect("memory.fill on the only memory translates");
        assert!(fill.contains("BI_memory_fill"), "{fill}");
    }

    /// A tag section means the module uses exception handling. The fixture
    /// carries no `throw` or `try_table`, so no operator arm can produce this
    /// rejection — only the section arm can.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_tag_section_is_rejected() {
        let bytes = wat(r#"(module (tag $e (param i32)) (func (export "f") nop))"#);
        assert_rejected("tag section", &bytes, &["tag section"]);
    }

    /// An unrecognised section id carries content the `.v` cannot account for.
    /// Asserting the id appears is what stops this passing for some other reason.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_unknown_section_is_rejected() {
        let bytes = module(&[section(0x0e, &[0xaa])]);
        assert_rejected("unknown section", &bytes, &["unknown wasm section", "14"]);
    }
}

/// Structure the binary asserts about itself, checked against what it actually
/// carries.
///
/// The translator constructs no `Validator` — the rejection fixtures elsewhere
/// in this file are deliberately stack-invalid and must still translate — so
/// nothing else on this path notices a module whose sections contradict each
/// other. Each shape below previously produced a `.v`: a component binary became
/// an empty core module complete with its `ValidModule` theorem, a body with
/// operators after its terminating `end` was emitted truncated, and a code
/// section longer than its function section had its extra bodies typed by a
/// fabricated default.
#[cfg(test)]
mod section_consistency {
    use super::errors::WasmToVError;
    use super::wasm_parser::translate_bytes;
    use rustc_hash::FxHashMap;

    fn translate(bytes: &[u8]) -> anyhow::Result<String> {
        translate_bytes(
            "Prog",
            bytes,
            &FxHashMap::default(),
            &inference_hassert::HSpecMap::default(),
        )
    }

    /// Asserts a recoverable rejection of either kind whose text names every
    /// needle. Both variants appear here: a contradiction inside the binary is a
    /// `WasmParse`, a construct outside the contract is an `UnsupportedFeature`.
    fn assert_rejected(label: &str, bytes: &[u8], needles: &[&str]) {
        let err = translate(bytes)
            .map(|v| panic!("{label}: must be rejected, but a `.v` was emitted:\n{v}"))
            .unwrap_err();
        let text = match err.downcast_ref::<WasmToVError>() {
            Some(e) => e.to_string(),
            None => panic!("{label}: rejection must be typed, got {err:?}"),
        };
        let lowered = text.to_lowercase();
        for needle in needles {
            assert!(
                lowered.contains(needle),
                "{label}: the rejection must name `{needle}`; got {text:?}",
            );
        }
    }

    fn section(id: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![id];
        out.push(u8::try_from(body.len()).expect("fixture section is small"));
        out.extend_from_slice(body);
        out
    }

    fn module(sections: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        for s in sections {
            out.extend_from_slice(s);
        }
        out
    }

    const TYPE_UNIT: [u8; 4] = [0x01, 0x60, 0x00, 0x00];

    /// A component shares the core preamble's first four bytes and differs only
    /// in the layer field, so it used to arrive as an empty core module — and be
    /// emitted as one, theorem included.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_component_binary_is_rejected() {
        let bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00];
        assert_rejected("component preamble", &bytes, &["component"]);
    }

    /// A core binary of an unknown version is not this format.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_unknown_core_version_is_rejected() {
        let bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x02, 0x00, 0x00, 0x00];
        assert_rejected("core version 2", &bytes, &["version 2"]);
    }

    /// Two type sections used to concatenate, silently shifting every type index
    /// that follows the first.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_repeated_section_is_rejected() {
        let bytes = module(&[
            section(1, &TYPE_UNIT),
            section(1, &[0x01, 0x60, 0x01, 0x7f, 0x00]),
        ]);
        assert_rejected("two type sections", &bytes, &["duplicate", "1"]);
    }

    /// Order carries meaning — a code section preceding its function section
    /// describes a different module than the same sections in sequence.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn sections_out_of_order_are_rejected() {
        // memory (id 5) before table (id 4)
        let bytes = module(&[
            section(5, &[0x01, 0x00, 0x01]),
            section(4, &[0x01, 0x70, 0x00, 0x01]),
        ]);
        assert_rejected("memory before table", &bytes, &["out of order"]);
    }

    /// The data count section is a claim about the data section.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_data_count_disagreeing_with_the_data_section_is_rejected() {
        let bytes = module(&[
            section(5, &[0x01, 0x00, 0x01]),
            section(12, &[0x03]),
            section(11, &[0x01, 0x00, 0x41, 0x00, 0x0b, 0x01, 0x78]),
        ]);
        assert_rejected("data count 3 vs 1 segment", &bytes, &["data count", "3"]);
    }

    /// Every defined function's signature comes from its function-section entry.
    /// Both directions of the mismatch are covered: the short function section
    /// used to have its missing entries defaulted to type 0.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_function_and_code_section_of_different_lengths_are_rejected() {
        let more_bodies = module(&[
            section(1, &TYPE_UNIT),
            section(3, &[0x01, 0x00]),
            section(10, &[0x02, 0x02, 0x00, 0x0b, 0x02, 0x00, 0x0b]),
        ]);
        assert_rejected(
            "one declared, two bodies",
            &more_bodies,
            &["function section declares"],
        );

        let more_declared = module(&[
            section(1, &TYPE_UNIT),
            section(3, &[0x02, 0x00, 0x00]),
            section(10, &[0x01, 0x02, 0x00, 0x0b]),
        ]);
        assert_rejected(
            "two declared, one body",
            &more_declared,
            &["function section declares"],
        );
    }

    /// A body used to be emitted truncated at its first top-level `end`, with
    /// whatever followed silently dropped.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn operators_after_the_terminating_end_are_rejected() {
        let bytes = module(&[
            section(1, &[0x01, 0x60, 0x00, 0x01, 0x7f]),
            section(3, &[0x01, 0x00]),
            section(10, &[0x01, 0x07, 0x00, 0x41, 0x01, 0x0b, 0x41, 0x02, 0x0b]),
        ]);
        assert_rejected(
            "trailing operators",
            &bytes,
            &["operators follow the terminating"],
        );
    }

    /// The ordering rule's positive control, and the reason it is a rank table
    /// rather than an id comparison: the data count section carries id 12 but
    /// belongs between element (9) and code (10). Comparing ids directly would
    /// reject this module — every module using bulk memory — as out of order.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_module_with_a_data_count_section_still_translates() {
        let bytes = wat::parse_str(
            r#"(module (memory 1) (data $d "x")
                 (func i32.const 0 i32.const 0 i32.const 1 (memory.init $d)))"#,
        )
        .expect("bulk-memory fixture assembles");
        assert!(
            bytes.windows(2).any(|w| w[0] == 0x0c),
            "the fixture must actually carry a data count section",
        );
        let v = translate(&bytes).expect("a data-count-bearing module must still translate");
        assert!(v.contains("BI_memory_init"), "{v}");
    }

    /// The consistency rules must not disturb an ordinary module: one of every
    /// core section, in order, each appearing once.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_module_using_every_section_in_order_still_translates() {
        let bytes = wat::parse_str(
            r#"(module
                 (import "env" "log" (func $log (param i32)))
                 (memory 1)
                 (table 1 funcref)
                 (global $g (mut i32) (i32.const 3))
                 (data (i32.const 0) "x")
                 (elem (i32.const 0) $run)
                 (func $run (result i32) global.get $g)
                 (export "run" (func $run)))"#,
        )
        .expect("full-section fixture assembles");
        let v = translate(&bytes).expect("an ordinary module must translate");
        for expected in ["Mi \"env\"", "Mm ", "Mt ", "Mg ", "moddata_init", "Me \"run\""] {
            assert!(v.contains(expected), "missing {expected}:\n{v}");
        }
    }
}
