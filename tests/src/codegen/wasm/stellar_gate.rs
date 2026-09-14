//! The source-level admissibility gates for the Stellar target.
//!
//! A Stellar build's artifact is a contract, and a contract's method signature
//! is narrower than an Inference function's: every parameter and every return
//! has to fit in the host's tagged word without a host object behind it. The
//! gate in `codegen()` is where a program that does not fit is refused, against
//! the source the author wrote rather than against bytes. A second gate, over
//! the same source, refuses a program that binds a host import; it has its own
//! heading below.
//!
//! The rules themselves are unit-tested where they live, against hand-built
//! descriptors that can express shapes no source can — an empty export name, a
//! hyphen in one. What is tested here is the half that only real source can
//! answer: that an ordinary program reaches the gate at all, that the refusal
//! names the declaration the author would go and edit, and that an admissible
//! program is untouched by the target it is built for.
//!
//! # The identity theorem
//!
//! Emission reads no target. For any program both targets accept,
//! `codegen(Stellar)` and `codegen(Wasm32)` are the same bytes — each built at
//! its own default optimization level, which is what the two command lines
//! really produce. That is what lets a project prove the default build and
//! deploy the Stellar one: the contract wrappers are added afterwards, by the
//! Val-ABI rewriter, to a module the proof already describes.
//!
//! # The host-import gate
//!
//! The target's other source-level gate lives here too, though it reads no
//! export descriptor. `Target::supports_host_imports()` answers `false` at this
//! target and at no other, so a program binding `use { f } from host::<module>;`
//! is refused — and refused *before* `emit()`, where the export gate above runs
//! after it. That ordering is the whole of what the doubly-inadmissible row
//! pins: the corpus's own host program is inadmissible twice over, and nothing
//! but the position of the two checks decides which message its author reads.

#[cfg(test)]
mod stellar_gate_tests {
    use crate::utils::{codegen_with_target_mode, codegen_with_target_mode_no_analysis};
    use inference_wasm_codegen::{CompilationMode, Target};

    /// Compiles `source` for Stellar, expecting the gate to accept it.
    fn accepted(source: &str) -> Vec<u8> {
        codegen_with_target_mode(source, Target::Stellar, CompilationMode::Compile)
            .expect("this program is admissible at the Stellar target")
            .wasm()
            .to_vec()
    }

    /// Compiles `source` for Stellar, expecting the gate to refuse it, and
    /// returns the message.
    ///
    /// Analysis is skipped so that a fixture written to exercise one refusal is
    /// not intercepted by an unrelated rule; the gate runs inside `codegen`
    /// either way.
    fn refused(source: &str) -> String {
        codegen_with_target_mode_no_analysis(source, Target::Stellar, CompilationMode::Compile)
            .expect_err("this program must be refused at the Stellar target")
            .to_string()
    }

    // What the target accepts ---

    /// The scalar set in every position an exported function has, compiled end
    /// to end. Each of these is a contract method the ABI can encode.
    #[test]
    fn the_scalar_set_compiles_for_stellar() {
        for source in [
            "pub fn answer() -> i32 { return 42; }",
            "pub fn identity(x: u32) -> u32 { return x; }",
            "pub fn flag(b: bool) -> bool { return b; }",
            "pub fn mixed(a: u32, b: i32, c: bool) -> i32 { if c { return b; } return 0; }",
            "pub fn nothing(x: u32) { let _y: u32 = x; }",
        ] {
            let wasm = accepted(source);
            assert!(!wasm.is_empty(), "no bytes for: {source}");
            inf_wasmparser::validate(&wasm)
                .unwrap_or_else(|e| panic!("invalid module for `{source}`: {e}"));
        }
    }

    /// Only the entry file's top-level `pub fn` declarations are exported, so a
    /// private helper carrying an inadmissible type is none of the gate's
    /// business. Without this the target would refuse programs whose contract
    /// surface is perfectly fine.
    #[test]
    fn an_inadmissible_type_inside_a_private_function_is_not_the_gate_s_business() {
        let source = "fn wide(x: u64) -> u64 { return x; } \
                      pub fn narrow(x: u32) -> u32 { return x; }";
        assert!(!accepted(source).is_empty());
    }

    // What the target refuses ---

    /// The refusal a user is most likely to meet, with the three things they
    /// need: which function, which parameter, and what it was declared as.
    #[test]
    fn a_wide_parameter_names_the_function_the_parameter_and_the_type() {
        cov_mark::check!(wasm_codegen_stellar_gate_param_type);
        let message = refused("pub fn transfer(to: u32, amount: u64) -> bool { return true; }");
        assert!(message.contains("exported function 'transfer'"), "{message}");
        assert!(message.contains("parameter 2 'amount'"), "{message}");
        assert!(message.contains("declared 'u64'"), "{message}");
        assert!(message.contains("issue #324"), "{message}");
    }

    /// The parameter name comes from the source, not from the descriptor, so a
    /// parameter the source did not name has to degrade to its position rather
    /// than to a wrong name or a panic.
    #[test]
    fn an_unnamed_parameter_is_reported_by_position() {
        let message = refused("pub fn ignore(_: u64) -> u32 { return 1; }");
        assert!(message.contains("parameter 1 is declared 'u64'"), "{message}");
    }

    /// A same-named function in an imported file must not lend its parameter
    /// names to the entry file's export. The lookup is restricted to the entry
    /// file for exactly this case: an imported `pub fn` is not exported, so it
    /// has no entry in the descriptor and no business naming one.
    ///
    /// Driven through `codegen` directly, because the multi-file helpers in the
    /// test utilities build for the default target only.
    #[test]
    fn a_same_named_import_does_not_lend_its_parameter_names() {
        let typed_context = crate::utils::try_type_check_multi_file(&[
            (Vec::new(), "pub fn transfer(amount: u64) -> u32 { return 1; }"),
            (
                vec!["lib"],
                "pub fn transfer(elsewhere: u64) -> u32 { return 2; }",
            ),
        ])
        .expect("the fixture type-checks");

        let message = inference_wasm_codegen::codegen(
            &typed_context,
            "output",
            inference_wasm_codegen::CodegenOptions {
                target: Target::Stellar,
                mode: CompilationMode::Compile,
                opt_level: Target::Stellar.default_opt_level(),
                ..inference_wasm_codegen::CodegenOptions::default()
            },
        )
        .expect_err("the entry file's export takes a `u64`")
        .to_string();

        assert!(message.contains("parameter 1 'amount'"), "{message}");
        assert!(
            !message.contains("elsewhere"),
            "an imported file's parameter name reached an export it does not describe: {message}"
        );
    }

    #[test]
    fn a_narrow_parameter_is_refused_and_says_what_to_write_instead() {
        let message = refused("pub fn clamp(x: u8) -> u32 { return 1; }");
        assert!(message.contains("parameter 1 'x' is declared 'u8'"), "{message}");
        assert!(
            message.contains("Widen the declaration to 'u32' or 'i32'."),
            "{message}"
        );
    }

    #[test]
    fn a_struct_parameter_is_refused_as_a_host_object() {
        let message = refused(
            "struct Point { x: i32; y: i32; } pub fn origin(p: Point) -> i32 { return p.x; }",
        );
        assert!(message.contains("parameter 1 'p' is declared 'Point'"), "{message}");
        assert!(message.contains("A compound value"), "{message}");
        assert!(message.contains("issue #324"), "{message}");
    }

    #[test]
    fn an_array_parameter_is_refused() {
        let message = refused("pub fn total(xs: [u32; 4]) -> u32 { return xs[0]; }");
        assert!(
            message.contains("parameter 1 'xs' is declared '[u32; 4]'"),
            "{message}"
        );
    }

    #[test]
    fn a_wide_return_is_refused() {
        cov_mark::check!(wasm_codegen_stellar_gate_return_type);
        let message = refused("pub fn total() -> u64 { return 1; }");
        assert!(message.contains("exported function 'total'"), "{message}");
        assert!(message.contains("it returns 'u64'"), "{message}");
    }

    /// A compound return is passed through a hidden pointer into linear memory,
    /// which is a different refusal from a value the host cannot encode: there
    /// is no value at all for a contract method to hand back.
    #[test]
    fn a_compound_return_is_refused_as_a_pointer_convention() {
        cov_mark::check!(wasm_codegen_stellar_gate_compound_return);
        let message = refused("pub fn corners() -> [i32; 4] { return [1, 2, 3, 4]; }");
        assert!(message.contains("it returns '[i32; 4]'"), "{message}");
        assert!(
            message.contains("hidden pointer into linear memory"),
            "{message}"
        );
    }

    /// A contract with no method uploads and can never be called, so a program
    /// with nothing public is refused rather than shipped.
    #[test]
    fn a_program_with_no_public_function_is_refused() {
        cov_mark::check!(wasm_codegen_stellar_gate_no_exports);
        let message = refused("fn helper(x: u32) -> u32 { return x; }");
        assert!(message.contains("exports no function"), "{message}");
        assert!(message.contains("'pub fn'"), "{message}");
    }

    /// `main` is exported like any other `pub fn`, so it is a contract method
    /// and is held to the same rules. Suppressing it would be silent; refusing
    /// it would be a style rule enforced as a build error.
    #[test]
    fn main_is_a_contract_method_like_any_other() {
        assert!(!accepted("pub fn main() -> i32 { return 0; }").is_empty());
        let message = refused("pub fn main(x: u64) -> i32 { return 0; }");
        assert!(message.contains("exported function 'main'"), "{message}");
    }

    /// A name of exactly 32 bytes is a symbol the host can hold; 33 is not, and
    /// the method would upload unreachable. The pair is what makes the boundary
    /// exact rather than approximately right.
    #[test]
    fn an_over_long_export_name_is_refused_at_thirty_three_bytes() {
        cov_mark::check!(wasm_codegen_stellar_gate_export_name);
        let longest = "n".repeat(32);
        assert!(
            !accepted(&format!("pub fn {longest}() -> i32 {{ return 1; }}")).is_empty(),
            "a 32-byte name is accepted"
        );

        let too_long = "n".repeat(33);
        let message = refused(&format!("pub fn {too_long}() -> i32 {{ return 1; }}"));
        assert!(message.contains("a name of 33 bytes"), "{message}");
    }

    /// The host reserves the `__` prefix: such a method uploads and then refuses
    /// every call, which is the failure a build-time refusal exists to prevent.
    #[test]
    fn a_double_underscore_export_name_is_refused() {
        let message = refused("pub fn __hidden() -> i32 { return 1; }");
        assert!(message.contains("starts with '__'"), "{message}");
        assert!(message.contains("reserves"), "{message}");
    }

    /// A single leading underscore is not the reserved prefix, and refusing it
    /// would reject an ordinary name.
    #[test]
    fn a_single_underscore_export_name_is_accepted() {
        assert!(!accepted("pub fn _internal() -> i32 { return 1; }").is_empty());
    }

    /// The host passes at most 32 arguments. 32 is a method; 33 could never be
    /// called.
    #[test]
    fn the_parameter_limit_is_exact_at_thirty_two() {
        cov_mark::check!(wasm_codegen_stellar_gate_param_count);
        let params = |count: usize| {
            (0..count)
                .map(|i| format!("p{i}: u32"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        assert!(
            !accepted(&format!(
                "pub fn wide({}) -> u32 {{ return p0; }}",
                params(32)
            ))
            .is_empty(),
            "32 parameters is a callable method"
        );

        let message = refused(&format!(
            "pub fn wider({}) -> u32 {{ return p0; }}",
            params(33)
        ));
        assert!(message.contains("takes 33 parameters"), "{message}");
        assert!(message.contains("at most 32"), "{message}");
    }

    /// The gate is the Stellar target's alone. The same program builds for the
    /// default target, which imposes no calling convention of its own — so this
    /// is a narrowing of one target, not a new restriction on the language.
    #[test]
    fn the_default_target_accepts_what_stellar_refuses() {
        for source in [
            "pub fn transfer(amount: u64) -> u32 { return 1; }",
            "pub fn corners() -> [i32; 4] { return [1, 2, 3, 4]; }",
            "fn helper(x: u32) -> u32 { return x; }",
            "pub fn __hidden() -> i32 { return 1; }",
        ] {
            assert!(
                codegen_with_target_mode_no_analysis(
                    source,
                    Target::Wasm32,
                    CompilationMode::Compile
                )
                .is_ok(),
                "the default target must still accept: {source}"
            );
        }
    }

    // The identity theorem ---

    /// For a program the Stellar target accepts, its module is byte-for-byte the
    /// default target's — each side built at its own default optimization level,
    /// which is what the two command lines really produce.
    ///
    /// This is the theorem the whole "prove the default build, deploy the
    /// Stellar build" story rests on. Nothing on the emission path reads a
    /// target, and the contract calling convention is added afterwards by the
    /// Val-ABI rewriter, so the module a Rocq proof describes is the module the
    /// wrappers are wrapped around. If this ever fails, the proof and the
    /// deployed contract have come apart and no downstream test would say so.
    ///
    /// The two sides are not the same build: they record different optimization
    /// levels, asserted below so this cannot pass by comparing a configuration
    /// with itself.
    #[test]
    fn a_stellar_build_is_byte_identical_to_the_default_build() {
        assert_ne!(
            Target::Stellar.default_opt_level(),
            Target::Wasm32.default_opt_level(),
            "the two sides must differ in configuration, or this compares a build with itself"
        );

        let sources = [
            "pub fn answer() -> i32 { return 42; }",
            "pub fn identity(x: u32) -> u32 { return x; }",
            "pub fn flag(b: bool) -> bool { return !b; }",
            "pub fn mixed(a: u32, b: i32, c: bool) -> i32 { if c { return b; } return 0; }",
            "pub fn arithmetic(a: u32, b: u32) -> u32 { return a + b * 2; }",
            "pub fn indexed(i: u32) -> i32 { let xs: [i32; 4] = [1, 2, 3, 4]; return xs[i]; }",
            "struct P { x: i32; y: i32; } \
             pub fn sum(a: i32, b: i32) -> i32 { let p: P = P { x: a, y: b }; return p.x + p.y; }",
            "fn helper(x: u32) -> u32 { return x + 1; } \
             pub fn call(x: u32) -> u32 { return helper(x); }",
        ];

        for source in sources {
            let stellar =
                codegen_with_target_mode(source, Target::Stellar, CompilationMode::Compile)
                    .expect("admissible at the Stellar target");
            let wasm32 = codegen_with_target_mode(source, Target::Wasm32, CompilationMode::Compile)
                .expect("admissible at the default target");

            assert_eq!(
                stellar.opt_level(),
                Target::Stellar.default_opt_level(),
                "each side is built at its own default optimization level"
            );
            assert_eq!(wasm32.opt_level(), Target::Wasm32.default_opt_level());
            assert_ne!(
                stellar.opt_level(),
                wasm32.opt_level(),
                "the two builds must differ in configuration for the identity to say anything"
            );

            assert!(
                !stellar.wasm().is_empty(),
                "an empty module would make this vacuous: {source}"
            );
            assert_eq!(
                stellar.wasm(),
                wasm32.wasm(),
                "the Stellar build differs from the default build for: {source}"
            );
        }
    }

    /// The identity above would pass on any two builds that happen to coincide,
    /// so this is what says the comparison has teeth: two programs that differ
    /// produce different modules, at the Stellar target as much as at the
    /// default one.
    #[test]
    fn the_identity_comparison_can_tell_two_modules_apart() {
        let one = accepted("pub fn answer() -> i32 { return 42; }");
        let other = accepted("pub fn answer() -> i32 { return 43; }");
        assert_ne!(one, other, "the byte comparison must be able to fail");
    }

    // Host imports ---

    /// A program whose only inadmissible element is a host binding.
    ///
    /// Its exported signature is scalar on purpose. A host-bound program whose
    /// export this target would also refuse — the corpus fixtures return `i64` —
    /// is refused twice over, and then only the order of the two checks decides
    /// which message a user reads: a test written on one of those would still
    /// pass with the host-import refusal deleted, having fallen through to the
    /// export gate's own words. [`HOST_IMPORT_WIDE_EXPORT`] is that program, and
    /// it is here to pin the order rather than the refusal.
    const HOST_IMPORT: &str = "external fn put(k: i32, v: i32) -> i32; \
                               use { put } from host::l; \
                               pub fn store(k: i32, v: i32) -> i32 { return put(k, v); }";

    /// The same exported signature with the binding taken away, and nothing
    /// else changed.
    ///
    /// The negative control: every other row in this section asserts a refusal,
    /// so a gate that refused every Stellar build would satisfy the lot.
    const NO_HOST_IMPORT: &str = "pub fn store(k: i32, v: i32) -> i32 { \
                                  if k > 0 { return v; } return 0; }";

    /// A host binding whose declaration carries no value in either position.
    ///
    /// The refusal is written about what this toolchain does not map onto a
    /// host function rather than about what a declaration sends across the
    /// boundary, and this is the declaration that tells the two apart: there is
    /// nothing here to send. Every earlier wording described the reader's own
    /// parameters and return, and every one of them passed the rest of this
    /// section.
    const HOST_IMPORT_NO_VALUES: &str = "external fn commit(); \
                                         use { commit } from host::l; \
                                         pub fn save() { commit(); }";

    /// A program this target refuses twice over: it binds a host import *and*
    /// exports a `u64`-family return the contract boundary cannot carry.
    ///
    /// The same program as the corpus's own `extern_import/host_import` fixture,
    /// respelled on one line as the sources in this file are. That is the shape
    /// a user meets first, and it is the one that tells the two refusals apart —
    /// the host gate runs before `emit()` and the export gate after it, so
    /// moving either would silently swap the message a real program gets while
    /// every other row here stayed green.
    const HOST_IMPORT_WIDE_EXPORT: &str = "external fn clock_ms() -> i64; \
                                           use { clock_ms } from host::env; \
                                           pub fn now() -> i64 { return clock_ms(); }";

    /// Two host bindings, with the one the refusal names written second.
    ///
    /// The enumeration behind the choice is ordered by module and field name,
    /// so `host::a` is reached before `host::z` however the file is written;
    /// declaring `zulu` first is what makes source order visible as a wrong
    /// answer rather than as the same answer.
    const TWO_HOST_IMPORTS: &str = "external fn zulu() -> i32; \
                                    external fn alpha() -> i32; \
                                    use { zulu } from host::z; \
                                    use { alpha } from host::a; \
                                    pub fn both() -> i32 { return zulu() + alpha(); }";

    /// A host binding is refused at this target, pinned whole: the first
    /// sentence is the family the configuration refusals share, and everything
    /// after it is the reason and the two ways out, which are the half a reader
    /// acts on.
    ///
    /// The `external fn` is named rather than a `use` clause quoted back,
    /// because one directive may import several fields and a synthesized
    /// single-field clause would be a line the author never wrote.
    ///
    /// The order of the two ways out is pinned with the words. A reader typed
    /// `--target stellar` because they want an uploadable contract, and
    /// building for `wasm32` is the one that gives that up, so it cannot lead.
    #[test]
    fn a_host_import_is_refused_at_stellar() {
        cov_mark::check!(wasm_codegen_target_rejects_host_import);
        assert_eq!(
            refused(HOST_IMPORT),
            "The `stellar` target does not support host imports. `external fn put` is bound \
             to `host::l`, and the Soroban host-call convention is not bound by this \
             toolchain yet: a contract reaches storage, ledger access and every host object \
             through host functions the host defines, each taking and returning the host's \
             64-bit tagged word, and nothing here maps an `external fn` onto one of them. \
             Remove the host binding to build a contract, or build this program for the \
             `wasm32` target, where a host import is supported; the Stellar host binding is \
             tracked under issue #324."
        );
    }

    /// A declaration carrying no value at all earns the same message, word for
    /// word.
    ///
    /// This is the row the wording is held to. Every clause has to be true of
    /// `external fn commit();`, which declares neither a parameter nor a
    /// return, and a refusal that reached for the values the declaration sends
    /// across the boundary would be describing an empty set while reading like
    /// a fact about this program.
    #[test]
    fn a_host_binding_declaring_no_value_earns_the_same_refusal() {
        assert_eq!(
            refused(HOST_IMPORT_NO_VALUES),
            "The `stellar` target does not support host imports. `external fn commit` is \
             bound to `host::l`, and the Soroban host-call convention is not bound by this \
             toolchain yet: a contract reaches storage, ledger access and every host object \
             through host functions the host defines, each taking and returning the host's \
             64-bit tagged word, and nothing here maps an `external fn` onto one of them. \
             Remove the host binding to build a contract, or build this program for the \
             `wasm32` target, where a host import is supported; the Stellar host binding is \
             tracked under issue #324."
        );
    }

    /// A program both gates refuse is refused for the host import, and the whole
    /// message is pinned because that is the half a reader acts on.
    ///
    /// The order is what rides on this row: the host gate runs before `emit()`
    /// and the export gate after it, so a refusal moved below emission would
    /// leave every other row in this file green and quietly hand the corpus's
    /// own host fixture a message about its `i64` return instead of about the
    /// binding that has to change first.
    #[test]
    fn a_doubly_inadmissible_program_is_refused_for_its_host_import() {
        assert_eq!(
            refused(HOST_IMPORT_WIDE_EXPORT),
            "The `stellar` target does not support host imports. `external fn clock_ms` is \
             bound to `host::env`, and the Soroban host-call convention is not bound by this \
             toolchain yet: a contract reaches storage, ledger access and every host object \
             through host functions the host defines, each taking and returning the host's \
             64-bit tagged word, and nothing here maps an `external fn` onto one of them. \
             Remove the host binding to build a contract, or build this program for the \
             `wasm32` target, where a host import is supported; the Stellar host binding is \
             tracked under issue #324."
        );
    }

    /// Two bindings earn one refusal, and the one it names is decided by the
    /// program rather than by the run.
    ///
    /// Both halves of that are a choice nothing else would notice. One binding
    /// is named rather than the inventory, because the way out is the same for
    /// all of them and a list would state one decision once per clause; and the
    /// same one is named every time, because the enumeration is ordered by
    /// module and field name — `host::a` here, though `host::z` is bound first
    /// in the source. A message reading off a hash map would name either
    /// binding on either run of one unchanged program.
    #[test]
    fn two_host_bindings_earn_one_refusal_and_it_names_the_same_one_every_time() {
        let message = refused(TWO_HOST_IMPORTS);
        assert!(
            message.contains("`external fn alpha` is bound to `host::a`"),
            "{message}"
        );
        assert!(
            !message.contains("zulu"),
            "the refusal names one binding, not the inventory: {message}"
        );
    }

    /// The gate refuses a binding rather than a target: the same exported
    /// signature with no `external fn` behind it still compiles here.
    #[test]
    fn a_stellar_program_with_no_host_binding_still_compiles() {
        assert!(!accepted(NO_HOST_IMPORT).is_empty());
    }

    /// The refusal belongs to this target and not to the language: the same
    /// program builds wherever an embedder registers a host against the
    /// WebAssembly signature the import declares.
    ///
    /// Both other targets are named because they answer for different reasons —
    /// one narrows nothing, the other narrows the instruction set and still
    /// admits the binding — and a predicate that had been written as "the
    /// default target only" would pass every other test in this file.
    #[test]
    fn the_targets_that_bind_a_host_accept_what_stellar_refuses() {
        for target in [Target::Wasm32, Target::SpaceWasm] {
            assert!(
                codegen_with_target_mode(HOST_IMPORT, target, CompilationMode::Compile).is_ok(),
                "`{}` must accept a host import",
                target.as_str()
            );
        }
    }
}
