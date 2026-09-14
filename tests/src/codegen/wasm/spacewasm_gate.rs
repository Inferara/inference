//! The acceptance envelope of the `SpaceWasm` target.
//!
//! `SpaceWasm` is a flight interpreter that decodes WebAssembly 1.0 and nothing
//! else, so unlike the Stellar target it imposes no calling convention and has
//! no export gate: what it narrows is the *instruction set* a build may reach
//! for. Three configuration refusals are its whole source-level envelope —
//! `proof` mode, a post-MVP feature request, and a non-deterministic construct
//! in a function that ships — and each of the three is refused before a byte is
//! emitted. The third is the one whose code-generation gate is a backstop rather
//! than the decision: analysis rules A042 and A006 are what refuse
//! non-determinism with a source location — A042 the non-deterministic blocks,
//! A006 the bare `@` A042 leaves to it — and every `infc` build runs both. The
//! gate reaches the same programs and says nothing about where in them, which is
//! what "backstop" means here: it is for the caller that drove code generation
//! without running analysis, and it is a decision rather than an approximation,
//! because a module carrying instructions this runtime cannot decode is not a
//! thing to write and then hope somebody notices.
//!
//! What is pinned here is the matrix of that envelope: the metadata an accepted
//! build records, the exact text of each refusal, the slots the walk descends
//! and the two it deliberately does not — into a struct's methods, and not into
//! a `spec` — and that the bytes an accepted build produces actually decode.
//!
//! The byte-identity claim — that this target's module *is* the default
//! target's — is not here. It is the corpus-wide sweep in `target_identity`,
//! which states it for every non-default target over every single-file codegen
//! fixture rather than for the handful of sources this module happens to write.
//! One row below is the exception, because the corpus cannot state it: no
//! single-file codegen fixture carries a `spec` block.

#[cfg(test)]
mod spacewasm_gate_tests {
    use crate::utils::{
        AnalysisMode, codegen_impl_with_features, codegen_with_target_mode,
        codegen_with_target_mode_no_analysis,
    };
    use inference_wasm_codegen::{CompilationMode, EmitFeatures, OptLevel, Target};

    /// A program with no non-determinism and nothing post-MVP in it: the
    /// ordinary case the target exists to build.
    const ORDINARY: &str = "pub fn answer(x: u32) -> u32 { return x + 1; }";

    /// Compiles `source` for `SpaceWasm`, expecting the envelope to refuse it, and
    /// returns the message.
    ///
    /// Analysis is skipped so a fixture written to exercise one refusal is not
    /// intercepted by an unrelated rule — A042 rejects a non-deterministic block
    /// outside a `spec`, and A006 a bare `@` outside such a block, both before
    /// code generation ever runs — while the gate under test runs inside
    /// `codegen` either way.
    fn refused(source: &str) -> String {
        codegen_with_target_mode_no_analysis(source, Target::SpaceWasm, CompilationMode::Compile)
            .expect_err("this program must be refused at the SpaceWasm target")
            .to_string()
    }

    // Row 1: what an accepted build records ---

    /// The metadata an accepted build carries, which is the target's whole
    /// user-visible output besides the bytes: the target it was built for, the
    /// mode, and `Os` — the level a flight computer's scarce resource argues
    /// for, recorded for a post-build tool because emission applies none.
    ///
    /// Fails if `default_opt_level` or the recorded target ever stops agreeing
    /// with what was asked for.
    #[test]
    fn a_compile_build_records_the_target_the_mode_and_the_size_level() {
        let output =
            codegen_with_target_mode(ORDINARY, Target::SpaceWasm, CompilationMode::Compile)
                .expect("an ordinary program is admissible at the SpaceWasm target");

        assert_eq!(output.target(), Target::SpaceWasm);
        assert_eq!(output.mode(), CompilationMode::Compile);
        assert_eq!(output.opt_level(), OptLevel::Os);
        assert_eq!(output.opt_level(), Target::SpaceWasm.default_opt_level());
        assert!(!output.wasm().is_empty());
    }

    // Row 2: proof mode ---

    /// Pinned whole rather than by substring. The message renders both target
    /// names through `Target::as_str()`, the spelling `--target` accepts, and
    /// its last sentence is an instruction the reader is meant to act on — so a
    /// substring check that omitted the names would keep passing while the
    /// message told somebody to build at a target that does not exist.
    #[test]
    fn proof_mode_is_refused_and_names_the_target_to_build_the_proof_at() {
        cov_mark::check!(wasm_codegen_proof_mode_rejected_non_wasm32);
        let err = codegen_with_target_mode(ORDINARY, Target::SpaceWasm, CompilationMode::Proof)
            .expect_err("proof mode is refused at the SpaceWasm target");
        assert_eq!(
            err.to_string(),
            "Proof mode requires the `wasm32` target. Proof mode emits custom \
             0xfc non-deterministic instructions that only `wasm32` accepts; the \
             `spacewasm` runtime rejects a module carrying them. Build the proof at \
             `--target wasm32`: code generation is target-blind, so the module a \
             `spacewasm` build starts from is the `wasm32` build's."
        );
    }

    // Row 3: a post-MVP feature request ---

    /// The decoder has not implemented the bulk-memory proposal, so the request
    /// is refused at build time rather than producing a module that fails to
    /// load. Pinned whole for the reason above.
    #[test]
    fn a_bulk_memory_request_is_refused() {
        cov_mark::check!(wasm_codegen_target_rejects_feature);
        let err = codegen_impl_with_features(
            ORDINARY,
            Target::SpaceWasm,
            CompilationMode::Compile,
            Target::SpaceWasm.default_opt_level(),
            AnalysisMode::Run,
            EmitFeatures { bulk_memory: true },
        )
        .expect_err("SpaceWasm does not accept bulk memory");
        assert_eq!(
            err.to_string(),
            "The `spacewasm` target does not support the 'bulk-memory' WebAssembly feature. \
             This target is pinned to the WebAssembly 1.0 instruction set, so a module \
             using the instructions 'bulk-memory' adds is refused here rather than at \
             deployment; drop 'bulk-memory' from the requested features to build for \
             `spacewasm`."
        );
    }

    // Row 4: a non-deterministic function that ships ---

    /// The refusal names the function, and then names the way forward: the
    /// construct is specification code, and a `spec` block is where compile mode
    /// drops it. A refusal that only said "not supported" would read as "this
    /// target cannot run Inference programs", which is the opposite of true.
    #[test]
    fn a_non_deterministic_function_is_refused_with_the_spec_block_remedy() {
        cov_mark::check!(wasm_codegen_target_rejects_nondet_function);
        let message = refused("pub fn guess() -> i32 { return @; }");
        assert_eq!(
            message,
            "The `spacewasm` target does not support non-deterministic operations. \
             Function 'guess' contains non-deterministic constructs (uzumaki, \
             forall, exists, assume, or unique blocks), which compile to custom \
             0xfc WebAssembly instructions the `spacewasm` runtime rejects. \
             Non-deterministic code is specification code: move it into a `spec` \
             block, which compile mode strips from the artifact, or build this \
             program with `--target wasm32`."
        );
    }

    // Row 5: a specification is not executable code ---

    /// A `spec` block is non-deterministic by construction and compile mode
    /// strips it, so a program carrying one is admissible here and its module is
    /// the module the same program compiles to at the default target — the spec
    /// contributed nothing to either.
    ///
    /// This row is inline rather than a corpus fixture because the corpus cannot
    /// state it: every spec-bearing codegen fixture in the tree lives under a
    /// multi-file `src` directory, which the single-file walk skips, so the
    /// corpus sweep in `target_identity` compares no program containing a `spec`
    /// at all.
    ///
    /// Fails if the non-determinism walk ever descends `Def::Spec`: the program
    /// would be refused rather than compared.
    #[test]
    fn a_program_carrying_a_spec_block_compiles_to_the_default_target_s_module() {
        let source = "\
fn double(n: i32) -> i32 { return wrapping(n + n); }

spec Doubling {
    fn is_modular(lo: i32) forall {
        let n: i32 = @;
        assert(double(n) == wrapping(n + n));
        assert(lo == lo);
    }
}

pub fn twice(n: i32) -> i32 { return double(n); }
";
        let spacewasm = codegen_with_target_mode_no_analysis(
            source,
            Target::SpaceWasm,
            CompilationMode::Compile,
        )
        .expect("a `spec` block is specification code, not shipped code");
        let wasm32 =
            codegen_with_target_mode_no_analysis(source, Target::Wasm32, CompilationMode::Compile)
                .expect("the same program at the default target");

        assert_ne!(
            spacewasm.opt_level(),
            wasm32.opt_level(),
            "the two sides must differ in configuration, or this compares a build with itself"
        );
        assert!(
            !spacewasm.wasm().is_empty(),
            "an empty module would make the comparison vacuous"
        );
        assert_eq!(
            spacewasm.wasm(),
            wasm32.wasm(),
            "a stripped `spec` must leave the same module at both targets"
        );
    }

    // Row 6: the walk is total inside a body ---

    /// Every shape a partial walk would have let through.
    ///
    /// The gate is a backstop, and a backstop that covers the statement kinds
    /// somebody enumerated is not one: analysis is what refuses these with a
    /// source location, and the whole point of asking again in `codegen` is the
    /// caller that did not run analysis. Every row is refused here with no
    /// analysis run at all.
    ///
    /// Most of them are slots the earlier, partial walk did not read: every
    /// operand of every expression — an `@` under an operator, in a call
    /// argument, in an array or struct literal, as an index, on the right of an
    /// assignment — and a non-deterministic block in a loop body, which that
    /// walk skipped along with the whole loop. Two are not new, and are here as
    /// regression guards rather than as evidence: the partial walk already
    /// descended a nested plain block and both arms of an `if`, and the deep
    /// walk re-implements those arms rather than reusing them, so a row that
    /// would have passed before is still a row that can fail now.
    ///
    /// Driven as a table because the failure mode is a slot being forgotten, and
    /// a slot forgotten is invisible in a test that exercises the three somebody
    /// thought of. The exhaustive matches in `core/ast` are what make a *new*
    /// node kind a compile error; this table is what covers the kinds that
    /// already exist, at the source level where they can be written down.
    ///
    /// Two slots are covered in `core/ast`'s own tables instead of here, because
    /// no source spells them: an `@` directly under `assert` has no type the
    /// checker can give it and never reaches code generation, and a local
    /// `const` whose initializer is non-deterministic is refused the same way.
    /// Both arms are driven against a hand-built arena there.
    ///
    /// The coverage mark is checked once per row rather than once for the test,
    /// so each row pins the gate as the site that refused it and not merely the
    /// sentence it was refused with.
    ///
    /// Fails if the walk stops descending any of these slots: the program would
    /// compile, and a module carrying custom `0xfc` instructions would be
    /// written for a runtime that decodes WebAssembly 1.0 and nothing else.
    #[test]
    fn non_determinism_below_a_statement_or_an_operand_is_refused_too() {
        for (label, source) in [
            (
                "an uzumaki under a binary operator",
                "pub fn sum() -> i32 { let n: i32 = @ + 1; return n; }",
            ),
            (
                "an uzumaki as a call argument",
                "fn id(n: i32) -> i32 { return n; } \
                 pub fn call() -> i32 { let v: i32 = id(@); return v; }",
            ),
            (
                "an uzumaki in an array literal",
                "pub fn arr() -> i32 { let xs: [i32; 2] = [1, @]; return xs[0]; }",
            ),
            (
                "an uzumaki as an array index",
                "pub fn ix() -> i32 { let xs: [i32; 2] = [1, 2]; return xs[@]; }",
            ),
            (
                "an uzumaki in a struct literal field",
                "struct P { x: i32; } pub fn f() -> i32 { let p: P = P { x: @ }; return p.x; }",
            ),
            (
                "an uzumaki in an assignment's right side",
                "pub fn assign() -> i32 { let mut n: i32 = 0; n = @ + 1; return n; }",
            ),
            (
                "a forall block inside a loop body",
                "pub fn looped() { let mut i: i32 = 0; loop (i < 1) { forall { let n: i32 = @; \
                 assert(n == n); } i = i + 1; } }",
            ),
            (
                "an assume block inside an if branch",
                "pub fn branched(c: bool) { if c { assume { let n: i32 = @; assert(n == n); } } }",
            ),
            (
                "an exists block inside a nested plain block",
                "pub fn nested() { { exists { let n: i32 = @; assert(n == n); } } }",
            ),
            (
                "a unique block inside a struct method's loop",
                "struct Holder { v: i32; \
                 fn hidden(self) { let mut i: i32 = 0; loop (i < 1) { \
                 unique { let n: i32 = @; assert(n == n); } i = i + 1; } } } \
                 pub fn use_it() { let h: Holder = Holder { v: 1 }; h.hidden(); }",
            ),
        ] {
            cov_mark::check!(wasm_codegen_target_rejects_nondet_function);
            let message = refused(source);
            assert!(
                message.contains("does not support non-deterministic operations"),
                "{label} must be refused by the non-determinism gate: {message}"
            );
        }
    }

    /// The row above would be satisfied by a gate that refused everything, so
    /// this is what says it is not: the same shapes with the non-determinism
    /// taken out compile.
    ///
    /// Committed rather than argued, because "the gate got wider" and "the gate
    /// refuses everything" produce the same green table one test up.
    ///
    /// Fails if the deep walk ever answers `true` for a deterministic body.
    #[test]
    fn the_same_shapes_without_non_determinism_still_compile() {
        for (label, source) in [
            ("an operand under an operator", "pub fn sum() -> i32 { return 1 + 1; }"),
            (
                "a call argument",
                "fn id(n: i32) -> i32 { return n; } pub fn call() -> i32 { return id(2); }",
            ),
            (
                "an array literal",
                "pub fn arr() -> i32 { let xs: [i32; 2] = [1, 2]; return xs[0]; }",
            ),
            (
                "a plain block inside a loop body",
                "pub fn looped() { let mut i: i32 = 0; loop (i < 1) { { i = i + 1; } } }",
            ),
            (
                "a struct method's loop",
                "struct Holder { v: i32; \
                 fn hidden(self) { let mut i: i32 = 0; loop (i < 1) { i = i + 1; } } } \
                 pub fn use_it() { let h: Holder = Holder { v: 1 }; h.hidden(); }",
            ),
        ] {
            let output = codegen_with_target_mode_no_analysis(
                source,
                Target::SpaceWasm,
                CompilationMode::Compile,
            )
            .unwrap_or_else(|e| panic!("{label} is deterministic and must compile: {e}"));
            assert!(!output.wasm().is_empty(), "{label} must produce a module");
        }
    }

    // Rows 7 and 8: the shape of the walk ---

    /// A struct's methods are ordinary functions that appear in no file's
    /// top-level `defs` list, so a gate that looked only at that list would
    /// accept this program *here* and emit an instruction the decoder cannot
    /// read. Analysis rejects it on every `infc` path — A006 and A042 both
    /// descend a struct's methods — so what this row pins is the backstop for a
    /// caller reaching code generation without analysis, which is the route
    /// under test.
    ///
    /// Fails if the walk stops descending `Def::Struct`: the program would
    /// compile. The refusal naming `hidden` rather than `Holder` is what says
    /// the walk returned the offending *function*.
    #[test]
    fn a_non_deterministic_struct_method_is_refused_by_name() {
        cov_mark::check!(wasm_codegen_target_rejects_nondet_function);
        let message = refused(
            "struct Holder { v: i32; fn hidden(self) -> i32 { return @; } } \
             pub fn use_it() -> i32 { let h: Holder = Holder { v: 1 }; return h.hidden(); }",
        );
        assert!(
            message.contains("Function 'hidden'"),
            "the method, not its struct, is what the author edits: {message}"
        );
    }

    /// The walk's one non-descent, stated at the source level: a `spec` block
    /// sits beside a struct whose method is ordinary, and the program compiles.
    /// If the walk descended specs, every program that writes a specification
    /// would be refused at this target — including the one in the row above.
    ///
    /// A `spec` *inside* a struct method is unspellable: a struct body admits
    /// fields and functions only, so `spec` inside one is a parse error. The
    /// nested arena is therefore pinned where it can be built directly —
    /// `the_deep_walk_keeps_both_of_the_shallow_walk_s_carve_outs`, in
    /// `core/ast`, constructs a spec under a struct's methods and asserts
    /// `first_non_det_def_deep` returns `None`. This row is the reachable half.
    #[test]
    fn a_spec_beside_a_struct_leaves_both_untouched() {
        let source = "\
struct Counter {
    v: i32;
    fn bump(self) -> i32 { return self.v + 1; }
}

spec Bumping {
    fn grows(start: i32) forall {
        let n: i32 = @;
        assert(n == n);
        assert(start == start);
    }
}

pub fn run() -> i32 { let c: Counter = Counter { v: 1 }; return c.bump(); }
";
        let output = codegen_with_target_mode_no_analysis(
            source,
            Target::SpaceWasm,
            CompilationMode::Compile,
        )
        .expect("a specification beside a struct is not shipped code");
        assert!(!output.wasm().is_empty());
    }

    // Row 9: the bytes decode ---

    /// The target's whole premise is that its output survives a strict
    /// WebAssembly 1.0 decoder, and every row above asserts only what the
    /// compiler *recorded*. This one asserts what it produced: the module
    /// validates and carries the WebAssembly magic number.
    ///
    /// The validator is the unmodified upstream `wasmparser` at
    /// `WasmFeatures::WASM1` — the W3C 1.0 level, the MVP plus mutable globals —
    /// which is the instruction set this interpreter decodes. The in-tree
    /// `inf_wasmparser` fork would be the wrong tool here: it accepts the
    /// compiler's own custom 0xfc opcodes, so it cannot testify that a module is
    /// something a foreign decoder will read.
    ///
    /// Decode-time *limits*, as against the instruction set, are not this row's
    /// business and are checked nowhere yet; a module passing here is one whose
    /// instructions the interpreter covers, which is not the same statement as
    /// one it will load.
    ///
    /// Fails the moment an emission path reaches for a post-1.0 instruction at
    /// this target, whether or not the feature gate was asked about it.
    #[test]
    fn an_accepted_build_produces_a_module_that_decodes() {
        use wasmparser::{Validator, WasmFeatures};

        for source in [
            ORDINARY,
            "pub fn nothing() {}",
            "pub fn indexed(i: u32) -> i32 { let xs: [i32; 4] = [1, 2, 3, 4]; return xs[i]; }",
            "pub fn guarded(x: i32) -> i32 { assert(x > 0); return x; }",
        ] {
            let wasm = codegen_with_target_mode(source, Target::SpaceWasm, CompilationMode::Compile)
                .unwrap_or_else(|e| panic!("admissible at the SpaceWasm target: {source}: {e}"))
                .wasm()
                .to_vec();
            assert!(
                wasm.len() > 8,
                "a SpaceWasm module must be non-trivial: {source}"
            );
            assert_eq!(
                &wasm[0..4],
                b"\0asm",
                "a SpaceWasm module must start with the WebAssembly magic number: {source}"
            );
            Validator::new_with_features(WasmFeatures::WASM1)
                .validate_all(&wasm)
                .unwrap_or_else(|e| {
                    panic!("`{source}` does not validate as WebAssembly 1.0: {e}")
                });
        }
    }
}
