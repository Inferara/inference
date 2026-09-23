//! WebAssembly code generation for the Inference compiler.
//!
//! This crate provides WebAssembly binary generation from Inference's typed AST
//! using `wasm-encoder`. The generated WASM binary is returned as a [`CodegenOutput`]
//! struct, which carries the binary bytes along with compilation metadata.
//!
//! # Architecture
//!
//! ```text
//! Typed AST (TypedContext)
//!         |
//!         v
//!   codegen(tc, module_name, CodegenOptions { target, mode, opt_level, features, layout })
//!         |
//!         v
//!   CodegenOutput { wasm, target, mode, opt_level, module_name, has_main }
//! ```
//!
//! # Non-Deterministic Extensions
//!
//! The compiler supports Inference's non-deterministic constructs through custom
//! WebAssembly instructions in the 0xfc prefix space:
//!
//! - `uzumaki()` - Non-deterministic value generation
//! - `forall { }` - Universal quantification blocks
//! - `exists { }` - Existential quantification blocks
//! - `assume { }` - Precondition assumption blocks
//! - `unique { }` - Uniqueness constraint blocks
//!
//! These extensions enable formal verification by preserving non-deterministic semantics
//! through the compilation pipeline.
//!
//! # Compilation Modes
//!
//! - **`Compile`** mode: Produces production binaries. Spec nodes are stripped.
//! - **`Proof`** mode: Produces WASM for Rocq formalization. All code is emitted,
//!   including spec functions with non-deterministic instructions. Always uses
//!   the `Wasm32` target, since the custom non-deterministic instructions it
//!   emits require that target.
//!
//! # Module Organization
//!
//! - [`compiler`] - WASM binary generation via wasm-encoder (private)
//! - [`memory`] - Linear memory infrastructure for stack-allocated compound types (private)
//! - [`output`] - `CodegenOutput` struct definition
//! - [`target`] - `Target`, `CompilationMode`, `OptLevel`, and `MemoryLayout`

#![warn(clippy::pedantic)]

use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::Def;
use inference_fn_key::FnKey;
use inference_type_checker::typed_context::TypedContext;
use inference_type_checker::{ExternKind, ExternOrigin};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::compiler::{Compiler, FunctionOrigin};
use crate::errors::CodegenError;

#[cfg(test)]
mod arith_polarity_tests;
mod checked_section;
mod choice;
#[cfg(test)]
mod choice_lowering_tests;
mod compiler;
mod errors;
pub use errors::NAMED_ANALYSIS_RULES;
mod hassert;
mod hspecs_section;
mod memory;
mod overflow_guard;
pub mod output;
mod spec_section;
pub mod target;

pub use output::{AbiParam, AbiReturn, AbiType, CodegenOutput, ExportSignature};
pub use target::{
    CodegenOptions, CompilationMode, EmitFeatures, MemoryLayout, MemoryLayoutError,
    MemoryLayoutSource, OptLevel, Target,
};

/// Re-exports of the `hassert` obligation IR, so a consumer of
/// [`CodegenOutput::hspecs`] can name the assertion tree it returns without a
/// separate dependency on `inference-hassert`.
pub use inference_hassert::{
    HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HSpecEntry, HSpecMap, HTerm, ReachMeta,
    SpecKind,
};

/// Single source of truth for the custom WASM section name that carries
/// per-spec function indices. Consumed by both `core/wasm-codegen` (encoder)
/// and `core/wasm-to-v` (decoder) so the wire-format constant lives in one
/// place.
pub use crate::spec_section::SECTION_NAME as SPEC_FUNCS_SECTION_NAME;

/// Wire-format version of the `inference.spec_funcs` custom section payload.
/// Decoders must reject payloads whose leading varuint32 does not equal this
/// constant; bumping the value is a breaking change to the section format.
pub use crate::spec_section::SECTION_VERSION as SPEC_FUNCS_SECTION_VERSION;

/// The custom WASM section name that lists the functions carrying an overflow
/// guard, as this crate emits it.
///
/// Published for the test suite, which reads the section back out of a compiled
/// module, and for the cross-crate test that holds this spelling and the static
/// merge linker's to agreement. The linker keeps a hand-synchronised copy rather
/// than depend on this crate, exactly as it does for `inference.spec_funcs`; it
/// is not a constant it imports from here.
pub use crate::checked_section::SECTION_NAME as CHECKED_SECTION_NAME;

/// Wire-format version of the `inference.checked` custom section payload, as
/// this crate emits it. Published for the same two readers as
/// [`CHECKED_SECTION_NAME`].
///
/// A decoder must reject a payload whose leading varuint32 it does not
/// recognise; bumping this value is a breaking change to the section format.
pub use crate::checked_section::SECTION_VERSION as CHECKED_SECTION_VERSION;

/// Generates WebAssembly binary from a typed AST for the specified target and compilation mode.
///
/// `module_name` is written into the WASM module-name subsection and flows
/// downstream to the Rocq translator, which uses it as the top-level module
/// identifier. The CLI derives this from the input file stem; library
/// callers can pass any [`validate_rocq_identifier`]-compatible name.
///
/// [`validate_rocq_identifier`]: inference_wasm_to_v_translator::validate_rocq_identifier
///
/// `options` carries the full compilation configuration; see [`CodegenOptions`]
/// for the field-by-field contract. Its `features` apply identically in both
/// compilation modes, so the `.v` always describes the same program as the
/// `.wasm`; [`CodegenOptions::default()`] compiles an executable Wasm32 module
/// inside WebAssembly 1.0 at the target's default optimization level, into a
/// single all-stack page of linear memory.
///
/// The memory layout needs no check here: [`MemoryLayout`]'s fields are private
/// and [`MemoryLayout::resolve`] refuses anything the emitter could not lower, so
/// a layout that reaches this function is one code generation can honor. That is
/// a stronger guarantee than a refusal at this boundary was — it holds for every
/// caller, including one that never passes through here.
///
/// The target-specific export check runs on what [`emit`] returns rather than
/// on a half-built module; [`emit`] carries why that position changes no
/// outcome. Every other refusal runs before emission, which is what decides
/// which of two applicable refusals a user reads: a host-import program whose
/// exported signature the Stellar target also refuses is refused for the host
/// import, because the binding is inadmissible whatever the exported signature
/// turns out to be, so it is the change the author has to make first.
///
/// # Errors
///
/// Returns an error if:
/// - Validation fails: a feature the target does not accept, `proof` mode at a
///   target that refuses it, a non-deterministic construct in a function that
///   ships at a target that refuses those, a host import in a `proof` build or
///   at a target that refuses host imports, or an export the Stellar target
///   cannot carry (see [`check_stellar_exports`])
/// - Code generation fails
pub fn codegen(
    typed_context: &TypedContext,
    module_name: &str,
    options: CodegenOptions,
) -> anyhow::Result<CodegenOutput> {
    let CodegenOptions {
        target,
        mode,
        opt_level,
        features,
        layout,
    } = options;

    // Refuse a feature the target's runtime does not accept before a single byte
    // is emitted: a build-time refusal names the manifest entry to remove, where
    // the same module rejected at deploy time names nothing.
    //
    // Every target name in the four configuration refusals is rendered through
    // `as_str()`, which is the spelling `--target` and `Inference.toml` accept.
    // A user reading one has to be able to paste the name back into the command
    // that produced it, which the `Debug` form does not allow. Three of the four
    // are below; the fourth is the host-import target gate in
    // `check_host_import_target_support`, whose proof-mode sibling is
    // deliberately outside the set, because it names no target at all -- what
    // it refuses is the mode.
    if let Some(feature) = features.first_rejected_by(target) {
        cov_mark::hit!(wasm_codegen_target_rejects_feature);
        let name = target.as_str();
        return Err(anyhow::anyhow!(
            "The `{name}` target does not support the '{feature}' WebAssembly feature. \
             This target is pinned to the WebAssembly 1.0 instruction set, so a module \
             using the instructions '{feature}' adds is refused here rather than at \
             deployment; drop '{feature}' from the requested features to build for \
             `{name}`."
        ));
    }

    if mode == CompilationMode::Proof && !target.supports_proof_mode() {
        cov_mark::hit!(wasm_codegen_proof_mode_rejected_non_wasm32);
        let name = target.as_str();
        let proof_target = Target::Wasm32.as_str();
        return Err(anyhow::anyhow!(
            "Proof mode requires the `{proof_target}` target. Proof mode emits custom \
             0xfc non-deterministic instructions that only `{proof_target}` accepts; the \
             `{name}` runtime rejects a module carrying them. Build the proof at \
             `--target {proof_target}`: code generation is target-blind, so the module a \
             `{name}` build starts from is the `{proof_target}` build's."
        ));
    }

    let arena = typed_context.arena();

    if !target.supports_non_det_functions() {
        // Analysis rules A006 and A042 are the primary rejection and are the only
        // reading that reports a source location -- A042 refuses a
        // non-deterministic block outside a `spec`, and A006 the bare `@`, which
        // A042 leaves to it. This is the backstop for a caller reaching code
        // generation without having run analysis, and it is total: over
        // definitions it descends a struct's methods, so a function in no file's
        // top-level `defs` is asked about too, and within one it descends every
        // block of every kind, every statement and every operand of every
        // expression. A `false` from it is therefore a decision and not an
        // approximation, which is what a backstop against instructions the
        // runtime cannot decode has to be. It stops at two definitions that ship
        // no instruction for it to be wrong about: a `Def::Spec`, whose body
        // compile mode strips before emission, and a module-scope `const`, which
        // emission drops rather than lowering.
        for source_file in typed_context.source_files() {
            if let Some(def_id) = arena.first_non_det_def_deep(&source_file.defs) {
                cov_mark::hit!(wasm_codegen_target_rejects_nondet_function);
                let name = target.as_str();
                let proof_target = Target::Wasm32.as_str();
                let fn_name = arena.def_name(def_id);
                return Err(anyhow::anyhow!(
                    "The `{name}` target does not support non-deterministic operations. \
                     Function '{fn_name}' contains non-deterministic constructs (uzumaki, \
                     forall, exists, assume, or unique blocks), which compile to custom \
                     0xfc WebAssembly instructions the `{name}` runtime rejects. \
                     Non-deterministic code is specification code: move it into a `spec` \
                     block, which compile mode strips from the artifact, or build this \
                     program with `--target {proof_target}`."
                ));
            }
        }
    }

    check_host_import_support(typed_context, mode, target)?;

    let emitted = emit(typed_context, module_name, mode, features, layout)?;

    if target == Target::Stellar {
        check_stellar_exports(&emitted.export_signatures)?;
    }

    Ok(CodegenOutput::new(
        emitted.wasm,
        target,
        mode,
        opt_level,
        module_name.to_string(),
        emitted.has_main,
        emitted.spec_func_indices_by_spec,
    )
    .with_frame_sizes(emitted.frame_sizes)
    .with_guarded_functions(emitted.guarded_functions)
    .with_export_signatures(emitted.export_signatures)
    .with_hspecs(emitted.hspecs))
}

/// Refuses a program whose host bindings this configuration cannot carry.
///
/// Two refusals rather than one, because they answer different questions about
/// the same binding and a program can meet either alone. Proof mode refuses one
/// at every target: the body is the embedder's, so it is outside the artifact
/// the proof is written about and the translation has no way to state an
/// assumption about it. A target refuses one when its calling convention is
/// unbound here, which is a claim about the runtime and not about the mode; that
/// half is [`check_host_import_target_support`].
///
/// Neither can fire unless the mode or the target asks for it, and both of
/// those are a read of the configuration, so the bound externs are walked only
/// where the answer could change the outcome: a compile build at a target that
/// binds a host -- what most builds are -- asks nothing at all.
///
/// The mode is answered first, and that is an ordering rather than a
/// precedence: a build reaching here in `proof` mode is at the default target,
/// the configuration refusal above this one having already refused the mode
/// everywhere else, and the default target binds a host.
///
/// # Errors
///
/// Returns the refusal, naming the first host binding the program carries.
fn check_host_import_support(
    typed_context: &TypedContext,
    mode: CompilationMode,
    target: Target,
) -> anyhow::Result<()> {
    if mode == CompilationMode::Proof
        && let Some(origin) = first_host_import(typed_context)
    {
        let fn_name = typed_context.arena().def_name(origin.decl);
        let clause = origin.source_spelling();
        cov_mark::hit!(wasm_codegen_proof_mode_rejects_host_import);
        return Err(anyhow::anyhow!(
            "Host imports are not yet modeled in the proof translation. `external fn \
             {fn_name}` is bound to `{clause}`, and its body is supplied by the embedder, so \
             its behavior is outside the artifact a proof is written about, and the \
             translation has no way to state an assumption about it. Build this program with \
             `--mode compile`, or remove the host imports from the proof build."
        ));
    }
    check_host_import_target_support(typed_context, target)
}

/// Refuses a program that binds a host import at a target whose host-call
/// convention this toolchain does not bind.
///
/// Public, where its mode-keyed sibling is not, for a driver that resolves a
/// program's externals, or holds them to a policy, before it calls
/// [`codegen`]. Those steps have refusals of their own, and at such a target
/// their remedies lead into this one: a program binding both kinds of extern is
/// told to bind every one of them to the host, and a build whose policy does
/// not admit an import is told to admit it — each advice this refusal then
/// makes void. Asked first, the reader hears the refusal that governs the
/// build. [`codegen`] still asks it, so a caller that does not is refused all
/// the same, in the same words.
///
/// The mode half stays private because a driver writing a proof artifact
/// refuses it in terms of its own flags, which this crate does not know.
///
/// # Errors
///
/// Returns the refusal, naming the first host binding the program carries,
/// when `target` binds no host and the program binds at least one.
pub fn check_host_import_target_support(
    typed_context: &TypedContext,
    target: Target,
) -> anyhow::Result<()> {
    if target.supports_host_imports() {
        return Ok(());
    }
    let Some(origin) = first_host_import(typed_context) else {
        return Ok(());
    };
    let fn_name = typed_context.arena().def_name(origin.decl);
    let clause = origin.source_spelling();

    cov_mark::hit!(wasm_codegen_target_rejects_host_import);
    let name = target.as_str();
    let host_target = Target::Wasm32.as_str();
    // The explanation belongs to the Soroban calling convention, which is the
    // one target answering `false` today. A second refusing target would not
    // share it: what makes a host import inadmissible is a property of the
    // runtime's own convention, so its reason would have to be chosen here
    // rather than inherited from this sentence.
    //
    // It says what this toolchain does not do rather than what the reader's
    // declaration carries, which is what keeps it true of every signature they
    // can write: `external fn commit();` declares no value in either position,
    // and a sentence about the values that would cross would be about nothing
    // at all.
    //
    // Dropping the binding leads because the reader named this target to get a
    // contract out, and the other way out gives that up.
    Err(anyhow::anyhow!(
        "The `{name}` target does not support host imports. `external fn {fn_name}` is bound \
         to `{clause}`, and the Soroban host-call convention is not bound by this toolchain \
         yet: a contract reaches storage, ledger access and every host object through host \
         functions the host defines, each taking and returning the host's 64-bit tagged \
         word, and nothing here maps an `external fn` onto one of them. Remove the host \
         binding to build a contract, or build this program for the `{host_target}` target, \
         where a host import is supported; the Stellar host binding is tracked under issue \
         #324."
    ))
}

/// The first import an embedder would have to satisfy for this program to
/// instantiate.
///
/// One rather than the inventory, because a refusal listing ten bindings would
/// be stating the consequences of one decision ten times over. Neither message
/// asks for anything to be done to the binding it names in particular — the
/// restriction is all-or-nothing, so the ways out are to drop the host bindings
/// or to move the build — and the one named is there to put the reader in the
/// right place in their source rather than to single it out from the rest.
/// Which one that is stays stable across runs, since
/// [`TypedContext::extern_origins`] is already ordered; it is not, though, the
/// first the author wrote, that ordering being by module and field name.
///
/// The whole origin rather than the two strings a message interpolates, so that
/// neither is derived twice. The `from` clause a reader wrote is
/// [`ExternOrigin::source_spelling`], the one place that knows the `host`
/// segment the classifier stripped belongs back in front of the module string;
/// and the declaration a reader would go and edit is named off
/// [`decl`](ExternOrigin::decl), since `export_field` is the name the emitted
/// import carries and the data model keeps the two free to diverge.
fn first_host_import(typed_context: &TypedContext) -> Option<ExternOrigin> {
    typed_context
        .extern_origins()
        .into_iter()
        .find(|origin| origin.kind == ExternKind::Host)
}

/// One run of [`emit`]: the module bytes, and the metadata the traversal
/// produced alongside them.
///
/// Named fields rather than a seven-wide tuple: four are handed to
/// differently-named `with_*` calls in [`codegen`] and three become positional
/// arguments of [`CodegenOutput::new`], where a name at both ends is what keeps
/// the mapping checkable without counting positions.
struct Emitted {
    wasm: Vec<u8>,
    spec_func_indices_by_spec: FxHashMap<String, Vec<u32>>,
    frame_sizes: FxHashMap<FnKey, u32>,
    hspecs: HSpecMap,
    has_main: bool,
    guarded_functions: Vec<FnKey>,
    export_signatures: Vec<ExportSignature>,
}

/// Assembles the WASM module for `typed_context`, with the emission metadata
/// that travels with it.
///
/// The parameter list is the contract. Emitted bytes are a function of the
/// typed context, the module name, the compilation mode, the requested
/// features and the memory layout — and of nothing else: no parameter carries
/// the target a build asked for, and this crate holds no ambient state one
/// could be read from. A proof is written about the bytes the default target
/// produces, and a module deployed to any other target has to be those same
/// bytes; an emission path that could consult the requested target would
/// separate the artifact that was proven from the artifact that ships, and
/// would do it silently. A target read in here would therefore have to be
/// conjured rather than handed in — which is what the signature buys, not an
/// impossibility: [`Target`] is in scope for every item in this module, so the
/// backstop for the hard way is the identity test that builds sources at
/// another target and compares the bytes with the default build:
/// `every_target_emits_what_the_default_target_emits`, in
/// `tests/src/codegen/wasm/target_identity.rs`, which runs every non-default
/// target over the whole single-file codegen corpus.
///
/// A target-specific acceptance check therefore runs in [`codegen`], on what
/// this function returns rather than on a half-built module: see
/// [`check_stellar_exports`], which reads the export descriptor alone, before
/// linking, and emits nothing. Which program it refuses does not depend on
/// where it sits: it still runs after the two refusals that follow a
/// traversal — the spec name cap and the `inference.hspecs` payload check,
/// both inside this function — and the steps between those and it, reading the
/// descriptor off the compiler and assembling the module, cannot fail. Placing
/// the check before those steps instead would change no outcome and would put a
/// target read inside the one region this signature exists to keep free of one.
///
/// # Errors
///
/// Returns an error if the traversal cannot lower the program, if a spec name
/// exceeds the byte cap both `inference.spec_funcs` decoders enforce, or if an
/// obligation would not survive the `inference.hspecs` decoder.
fn emit(
    typed_context: &TypedContext,
    module_name: &str,
    mode: CompilationMode,
    features: EmitFeatures,
    layout: MemoryLayout,
) -> anyhow::Result<Emitted> {
    let mut compiler = Compiler::new(module_name);
    compiler.set_emit_features(features);
    compiler.set_memory_layout(layout);

    // Bounds checks are on for every build, in either compilation mode and at
    // every target and optimization level: no input reaching here can turn them
    // off. `Compiler::set_emit_bounds_checks` carries why.
    compiler.set_emit_bounds_checks(true);

    let hspecs = if typed_context.source_files().next().is_some() {
        traverse_t_ast_with_compiler(typed_context, &mut compiler, mode)?
    } else {
        HSpecMap::default()
    };

    // Reject any spec name that would overflow the byte cap both
    // `inference.spec_funcs` decoders enforce, before the section is emitted.
    // Surfacing it here yields a clean codegen diagnostic instead of an
    // artifact that fails its own downstream link/translate step.
    if let Err(too_long) = spec_section::check_spec_name_lengths(compiler.spec_func_indices()) {
        cov_mark::hit!(wasm_codegen_spec_name_too_long);
        return Err(CodegenError::SpecNameTooLong {
            name: too_long.name,
            len: too_long.len,
            max: spec_section::MAX_SPEC_NAME_LEN,
        }
        .into());
    }

    // Refuse to write an `inference.hspecs` section the codec's own decoder
    // would reject: `inference_hassert::encode` is infallible, but its hardened
    // decoder enforces an input contract (bounded tree depth, non-empty names
    // within a byte cap), so an unchecked obligation would serialize into a
    // corrupt-at-decode artifact. Gating on the shared validator here names the
    // offending spec and identifier instead of leaving a `.wasm` that fails its
    // own downstream link/translate step.
    hspecs_section::check_payload(&hspecs)?;

    // Snapshot `has_main` before `finish_and_take` consumes the compiler:
    // the section is emitted in a single pass that moves out the recorded
    // spec map alongside the WASM bytes. The obligation map is borrowed for the
    // `inference.hspecs` section and retained here to attach to the output.
    let has_main = compiler.has_main();
    let guarded_functions = compiler.guarded_functions();
    let export_signatures = compiler.export_signatures();

    let (wasm, spec_func_indices_by_spec, frame_sizes) = compiler.finish_and_take(&hspecs);
    debug_assert!(
        mode != CompilationMode::Compile
            || (spec_func_indices_by_spec.is_empty() && hspecs.is_empty()),
        "compile mode must not record any spec function indices or hspec obligations"
    );

    Ok(Emitted {
        wasm,
        spec_func_indices_by_spec,
        frame_sizes,
        hspecs,
        has_main,
        guarded_functions,
        export_signatures,
    })
}

/// The most parameters an exported function may take at the Stellar target.
///
/// The host checks an invocation's argument count against its own limit of 32.
/// A wider method could never be called.
const STELLAR_MAX_EXPORT_PARAMS: usize = 32;

/// The longest exported name the Stellar target accepts, in bytes.
///
/// The host turns a method name into a symbol, and exactly 32 bytes is the
/// largest it can hold. A longer name uploads and is then unreachable, because
/// no caller can express it.
const STELLAR_MAX_EXPORT_NAME_BYTES: usize = 32;

/// The prefix the Stellar host reserves for itself.
const STELLAR_RESERVED_EXPORT_PREFIX: &str = "__";

/// The longest parameter name an exported function may have at the Stellar
/// target, in bytes.
///
/// The contract's `contractspecv0` section records each parameter's name in an
/// XDR `string<30>` field, the `name` of `SCSpecFunctionInputV0`. The name has
/// to be recorded whole and as written, because `stellar contract invoke`
/// derives from it the `--<name>` flag a caller passes the argument by.
const STELLAR_MAX_INPUT_NAME_BYTES: usize = 30;

/// The set an exported function may be built from at the Stellar target, as
/// every refusal restates it.
const STELLAR_SCALAR_SET: &str = "This target currently carries only the scalar set: an \
     exported parameter is 'u32', 'i32' or 'bool', and an exported return is one of those \
     or unit.";

/// Why a compound type cannot cross the contract boundary, and where the work
/// that would let it is tracked.
///
/// It states the refusal and not only the unbound convention, because a reader
/// told that a contract builds host objects "through host functions it imports"
/// has been handed their next move: declare those imports. This target refuses
/// that binding, so the sentence has to say so, or the advice is a dead end the
/// compiler closes on the following build.
const STELLAR_COMPOUND_NEXT_STEP: &str = "A compound value crosses the contract boundary as a \
     host object, which a contract has to build and read through host functions it imports; \
     this target refuses a host binding, because the Soroban host-call convention is not \
     bound here yet, and issue #324 is where that work is tracked.";

/// Why a 64-bit integer cannot cross it either. Same machinery, different
/// reason for needing it, and the same reason for naming the refusal.
const STELLAR_WIDE_NEXT_STEP: &str = "A 64-bit integer needs the same machinery: the host's \
     word is 64 bits wide and spends part of it on a tag, so no 64-bit value fits in one, \
     and it travels as a host object built through host functions this target refuses to \
     import, their call convention not being bound here yet — issue #324.";

/// Why an integer narrower than 32 bits is held back, which is not the reason
/// the others are.
const STELLAR_NARROW_NEXT_STEP: &str = "An integer narrower than 32 bits is held back for a \
     different reason: the host has no narrower word, so what an exported 'u8' does with a \
     caller-supplied 300 is a language question rather than a layout one, and it is not \
     settled. Widen the declaration to 'u32' or 'i32'.";

/// The other way out of a type or parameter-name refusal, since not every
/// `pub fn` is meant to be a contract method.
const STELLAR_UNEXPORT_HINT: &str = "If the function is not meant to be a contract method, \
     remove 'pub': only an entry-file top-level 'pub fn' is exported.";

/// Why the Stellar target cannot carry `ty` across the contract boundary, or
/// `None` for a type it can.
///
/// One exhaustive match answers both halves, so the admissibility rule and the
/// explanation a refusal gives cannot come apart — a type added to the
/// descriptor has to be classified here before it compiles, and whatever
/// classification it gets is the one the message reports.
fn stellar_refusal_reason(ty: &AbiType) -> Option<&'static str> {
    match ty {
        AbiType::Bool | AbiType::I32 | AbiType::U32 => None,
        AbiType::I8 | AbiType::U8 | AbiType::I16 | AbiType::U16 => Some(STELLAR_NARROW_NEXT_STEP),
        AbiType::I64 | AbiType::U64 => Some(STELLAR_WIDE_NEXT_STEP),
        AbiType::Enum { .. } | AbiType::Struct { .. } | AbiType::Array { .. } => {
            Some(STELLAR_COMPOUND_NEXT_STEP)
        }
    }
}

/// The source spelling of a described type, as a refusal names it back to the
/// author.
fn render_abi_type(ty: &AbiType) -> String {
    match ty {
        AbiType::Bool => "bool".to_string(),
        AbiType::I8 => "i8".to_string(),
        AbiType::U8 => "u8".to_string(),
        AbiType::I16 => "i16".to_string(),
        AbiType::U16 => "u16".to_string(),
        AbiType::I32 => "i32".to_string(),
        AbiType::U32 => "u32".to_string(),
        AbiType::I64 => "i64".to_string(),
        AbiType::U64 => "u64".to_string(),
        AbiType::Enum { name } | AbiType::Struct { name } => name.clone(),
        AbiType::Array { elem, len } => format!("[{}; {len}]", render_abi_type(elem)),
    }
}

/// How a refusal points at one parameter: its one-based position, and the name
/// the author gave it when there is one — the name its descriptor entry
/// carries, which is `None` for a parameter written `_`.
fn stellar_parameter_label(name: Option<&str>, index: usize) -> String {
    let position = index + 1;
    match name {
        Some(name) => format!("parameter {position} '{name}'"),
        None => format!("parameter {position}"),
    }
}

/// Refuses an exported function the Stellar target cannot carry, reading the
/// source types off the export descriptor.
///
/// Every refusal below opens with the fixed prose `Stellar target: `, rather
/// than the interpolated `Target::as_str()` the four configuration gates in
/// [`codegen`] render. This gate is only ever reached at that one target, so
/// there is no variant name to hand the reader back.
///
/// The rules run in one order, export by export: the name, the parameter
/// count, every parameter's type, the return, then every parameter's name. A
/// program breaking two of them hears about the earlier one first, and a type
/// refusal always precedes a name refusal, so a parameter written `_` with a
/// type the target refuses is reported for its type, which is the change the
/// author has to make regardless.
///
/// # The overlap with the Val-ABI rewriter is deliberate
///
/// `inference-stellar-abi` refuses the same shapes again, after linking. That is
/// not a redundant check but a different one at a different vantage. Both read
/// the same descriptor, parameter names included; what differs is when and
/// against what. This gate runs on the descriptor of the program the author
/// wrote, before linking and before any file is produced, so it can phrase a
/// refusal as a declaration — the function, the parameter and the declared
/// type — and offer the repair. The rewriter runs on the linked module beside
/// that same descriptor, so it is the net, positioned where things this gate
/// cannot see arrive: an export introduced by linking, a descriptor that does
/// not match the module it is paired with, a caller that reaches the rewriter
/// without passing through here at all. Neither replaces the other, and the two
/// must agree on the rules: a change to what is admissible has to be made in
/// both places, or a build is accepted here and refused there with a message
/// about bytes.
fn check_stellar_exports(exports: &[ExportSignature]) -> anyhow::Result<()> {
    if exports.is_empty() {
        cov_mark::hit!(wasm_codegen_stellar_gate_no_exports);
        return Err(anyhow::anyhow!(
            "Stellar target: this module exports no function, so the contract would upload \
             with no method to call. A contract's methods are the entry file's top-level \
             'pub fn' declarations; declare at least one."
        ));
    }

    for signature in exports {
        check_stellar_export_name(&signature.name)?;
        check_stellar_export_arity(signature)?;
        check_stellar_export_types(signature)?;
        check_stellar_parameter_names(signature)?;
    }
    Ok(())
}

/// Refuses an exported name the host cannot dispatch to.
///
/// Every rule here is about reachability rather than taste: a name the host
/// rejects, or reserves, produces a contract that uploads and then fails every
/// call, which is a worse outcome than not building.
fn check_stellar_export_name(name: &str) -> anyhow::Result<()> {
    let refusal = if name.is_empty() {
        "an exported function has an empty name, which the host cannot turn into a method \
         symbol"
            .to_string()
    } else if name.len() > STELLAR_MAX_EXPORT_NAME_BYTES {
        format!(
            "the exported function '{name}' has a name of {} bytes, and the host holds a \
             method name of at most {STELLAR_MAX_EXPORT_NAME_BYTES}. A longer one is not \
             expressible by any caller, so the method would upload and be unreachable — \
             rename it",
            name.len()
        )
    } else if name.starts_with(STELLAR_RESERVED_EXPORT_PREFIX) {
        format!(
            "the exported function '{name}' starts with '{STELLAR_RESERVED_EXPORT_PREFIX}', a \
             prefix the host reserves. Such a method uploads and then refuses every call with \
             \"can't invoke a reserved function directly\" — rename it without the leading \
             underscores"
        )
    } else if let Some(offending) = name
        .chars()
        .find(|ch| !ch.is_ascii_alphanumeric() && *ch != '_')
    {
        format!(
            "the exported function '{name}' contains '{offending}', and a method name may hold \
             only letters, digits and '_'. No caller can express a name with anything else, so \
             the method would upload and be unreachable — rename it"
        )
    } else {
        return Ok(());
    };
    cov_mark::hit!(wasm_codegen_stellar_gate_export_name);
    Err(anyhow::anyhow!("Stellar target: {refusal}."))
}

/// Refuses an exported function with more parameters than the host will pass.
fn check_stellar_export_arity(signature: &ExportSignature) -> anyhow::Result<()> {
    let count = signature.params.len();
    if count <= STELLAR_MAX_EXPORT_PARAMS {
        return Ok(());
    }
    cov_mark::hit!(wasm_codegen_stellar_gate_param_count);
    let name = &signature.name;
    Err(anyhow::anyhow!(
        "Stellar target: exported function '{name}' takes {count} parameters, and a contract \
         method takes at most {STELLAR_MAX_EXPORT_PARAMS}. The host checks an invocation's \
         argument count against that limit, so a wider method could never be called."
    ))
}

/// Refuses an exported parameter or return outside the scalar set this target
/// carries.
fn check_stellar_export_types(signature: &ExportSignature) -> anyhow::Result<()> {
    let name = &signature.name;
    for (index, param) in signature.params.iter().enumerate() {
        if let Some(next_step) = stellar_refusal_reason(&param.ty) {
            cov_mark::hit!(wasm_codegen_stellar_gate_param_type);
            let parameter = stellar_parameter_label(param.name.as_deref(), index);
            let declared = render_abi_type(&param.ty);
            return Err(anyhow::anyhow!(
                "Stellar target: exported function '{name}' cannot be a contract method \
                 because {parameter} is declared '{declared}'. {STELLAR_SCALAR_SET} \
                 {next_step} {STELLAR_UNEXPORT_HINT}"
            ));
        }
    }

    match &signature.ret {
        AbiReturn::Unit => Ok(()),
        AbiReturn::Scalar(ty) => {
            let Some(next_step) = stellar_refusal_reason(ty) else {
                return Ok(());
            };
            cov_mark::hit!(wasm_codegen_stellar_gate_return_type);
            let declared = render_abi_type(ty);
            Err(anyhow::anyhow!(
                "Stellar target: exported function '{name}' cannot be a contract method \
                 because it returns '{declared}'. {STELLAR_SCALAR_SET} {next_step} \
                 {STELLAR_UNEXPORT_HINT}"
            ))
        }
        AbiReturn::Sret(ty) => {
            cov_mark::hit!(wasm_codegen_stellar_gate_compound_return);
            let declared = render_abi_type(ty);
            Err(anyhow::anyhow!(
                "Stellar target: exported function '{name}' cannot be a contract method \
                 because it returns '{declared}', which the caller receives through a hidden \
                 pointer into linear memory rather than as a value. A contract method hands \
                 back one host word and has no pointer to give. {STELLAR_SCALAR_SET} \
                 {STELLAR_COMPOUND_NEXT_STEP} {STELLAR_UNEXPORT_HINT}"
            ))
        }
    }
}

/// Refuses an exported parameter the contract's `contractspecv0` section cannot
/// record.
///
/// A caller reaches a contract method's parameter by its name, so every
/// parameter needs one, and the name must fit the spec's input-name field
/// whole. Each rule runs over every parameter before the next begins, so a
/// signature breaking both is refused for its unnamed parameter wherever the
/// over-long one sits.
///
/// An empty name counts as none, as it does in the Val-ABI rewriter. No
/// identifier is empty, so only a hand-built descriptor carries one — the
/// footing of the empty export name, which both gates refuse — and the empty
/// case is refused here only to keep this rule set identical to the
/// rewriter's. The message assumes a descriptor built from source, where the
/// one parameter without a name is one written `_`, and says so; the
/// rewriter's message names both causes. Two parameters sharing a name are
/// admitted: uniqueness is a producer invariant the type checker enforces with
/// `DuplicateParameterName`, so a duplicate reaches this gate only from a
/// hand-built descriptor, and neither gate yet refuses it.
fn check_stellar_parameter_names(signature: &ExportSignature) -> anyhow::Result<()> {
    let name = &signature.name;
    if let Some(index) = signature
        .params
        .iter()
        .position(|param| param.name.as_deref().is_none_or(str::is_empty))
    {
        cov_mark::hit!(wasm_codegen_stellar_gate_unnamed_param);
        let parameter = stellar_parameter_label(None, index);
        return Err(anyhow::anyhow!(
            "Stellar target: exported function '{name}' cannot be a contract method because \
             {parameter} is written '_', which gives it no name. A contract's `contractspecv0` \
             section describes each method's parameters by name, and `stellar contract invoke` \
             takes each argument as a `--<name>` flag built from it; the compiler does not \
             invent a name you did not write. Name the parameter — `_unused` if the body does \
             not read it; a leading underscore is kept as written. {STELLAR_UNEXPORT_HINT}"
        ));
    }

    let too_long = signature.params.iter().enumerate().find_map(|(index, param)| {
        let param_name = param.name.as_deref()?;
        (param_name.len() > STELLAR_MAX_INPUT_NAME_BYTES).then_some((index, param_name))
    });
    let Some((index, param_name)) = too_long else {
        return Ok(());
    };
    cov_mark::hit!(wasm_codegen_stellar_gate_param_name_length);
    let parameter = stellar_parameter_label(Some(param_name), index);
    let len = param_name.len();
    Err(anyhow::anyhow!(
        "Stellar target: exported function '{name}' cannot be a contract method because \
         {parameter} has a name of {len} bytes, and a contract method's parameter name is at \
         most {STELLAR_MAX_INPUT_NAME_BYTES} bytes: the contract's `contractspecv0` section \
         records each name whole in a field that wide, and a longer one would leave the \
         section unreadable to `stellar contract invoke` and every other tool that reads it. \
         Shorten the name. {STELLAR_UNEXPORT_HINT}"
    ))
}

/// Traverses every source file's typed AST and compiles all function and
/// method definitions into one flat WASM module.
///
/// Emittable items from all files are first collected into a single set of
/// buckets, in canonical file order (entry first, then by module path). The
/// traversal then proceeds in two stages over those combined buckets so all
/// WASM function indices are globally unique and known before any body is
/// compiled (required for forward references, including cross-file calls):
///
/// 1. **Index registration** -- `build_func_name_to_idx` registers top-level
///    functions, then `build_method_name_to_idx` registers struct methods.
///    Items defined in an imported file get a file-qualified mangled name
///    (`lib.arith.add`, `lib.arith.Point.new`); entry-file items stay
///    unqualified, so single-file output is byte-identical.
/// 2. **Body compilation** -- bodies are compiled in registration order, each
///    with its defining file's module path so that struct/enum metadata
///    resolves relative to the file the body lives in, and its
///    `method_struct_name` so `self` handling knows the struct in scope.
fn traverse_t_ast_with_compiler(
    typed_context: &TypedContext,
    compiler: &mut Compiler,
    mode: CompilationMode,
) -> Result<HSpecMap, CodegenError> {
    let arena = typed_context.arena();

    // Collect emittable items from every source file into one set of buckets,
    // in canonical file order (entry first, then by module path). Registration
    // and body compilation then run once over the combined buckets so WASM
    // function indices are globally unique and deterministic across files; a
    // per-file registration pass would reset the index bases and collide.
    let mut buckets = EmittableFunctions::default();
    for source_file in typed_context.source_files() {
        collect_emittable_functions(
            arena,
            &source_file.defs,
            &source_file.module_path,
            mode,
            &mut buckets,
        )?;
    }

    // Plan the choice lowering of every specification function before any body
    // is compiled: each `@` becomes one hidden trailing parameter (one per
    // scalar leaf for an aggregate), so a specification body compiles to
    // vanilla WebAssembly. The planner iterates the same buckets body
    // compilation does, so its scope cannot drift from code generation's. In
    // compile mode the spec buckets are empty, so the plan set is too.
    //
    // The reachability view then selects the `exists`/`unique` free functions,
    // whose obligation payloads denote against the real activation frame, and
    // enforces the no-return rule that keeps those frame indices valid. It
    // fails here, before a single byte is emitted. Running both after
    // `collect_emittable_functions` means a program carrying both a nested spec
    // and a reachability-return fault reports the nested spec first — the more
    // fundamental structural error, and the only interaction, since
    // `NestedSpecsNotSupported` is that function's sole error.
    let choice_plans = choice::plan_choice_lowering(typed_context, &buckets);
    let reach_plans = hassert::reach::reachability_plans(typed_context, &buckets, &choice_plans)?;

    // Reject two specs whose file-qualified names collide under the `_` join
    // before any are recorded; the spec map is keyed by the joined name, so a
    // post-join check could not tell a collision from a single entry.
    check_spec_name_collisions(&buckets.visited_specs)?;

    // Reject a spec whose file-qualified name is not a legal Rocq identifier
    // (chiefly a leading-underscore spec name, which the module-path join turns
    // into a `__` run) before any artifact is written. Running it here — rather
    // than letting the downstream Rocq translator reject the already-written
    // `.wasm` — keeps a bad spec name from leaving a stale `.wasm` behind and
    // points the diagnostic at the source spec the user wrote.
    check_spec_names_valid(&buckets.visited_specs)?;

    // Register every visited spec (even with zero emittable inner defs) so
    // user-authored `spec MySpec { }` still surfaces a per-spec entry that
    // the Rocq translator turns into `Definition output__MySpec_specs` and
    // `Theorem valid_output__MySpec`. The spec is keyed by its file-qualified
    // name so two files may each define a `spec MySpec`.
    for visited in &buckets.visited_specs {
        compiler.ensure_spec_registered(&qualified_spec_name(
            &visited.module_path,
            &visited.spec_name,
        ));
    }

    register_function_indices(arena, compiler, typed_context, &buckets)?;

    // Stage 2: Compile bodies in the same order as registration. Only a
    // specification function carries a choice plan, so the top-level buckets
    // pass `None` rather than performing a lookup that can never hit.
    for entry in &buckets.funcs {
        compiler.visit_function_definition(
            entry.def_id,
            arena,
            typed_context,
            None,
            &entry.module_path,
            &FunctionOrigin::TopLevel,
            None,
        )?;
    }
    for entry in &buckets.methods {
        compiler.visit_function_definition(
            entry.def_id,
            arena,
            typed_context,
            Some(&entry.struct_name),
            &entry.module_path,
            &FunctionOrigin::TopLevel,
            None,
        )?;
    }
    for entry in &buckets.spec_funcs {
        compiler.visit_function_definition(
            entry.def_id,
            arena,
            typed_context,
            None,
            &entry.module_path,
            &FunctionOrigin::SpecInner(entry.spec_name.clone()),
            choice_plans.get(entry.def_id),
        )?;
    }
    for entry in &buckets.spec_methods {
        compiler.visit_function_definition(
            entry.def_id,
            arena,
            typed_context,
            Some(&entry.struct_name),
            &entry.module_path,
            &FunctionOrigin::SpecInner(entry.spec_name.clone()),
            choice_plans.get(entry.def_id),
        )?;
    }

    // Proof-mode only: derive each spec function's `hassert` obligation. This
    // runs after every body is compiled so the WASM bytes are already settled;
    // the pass reads the AST, type information, and the buckets, never the
    // compiler's output, so proof-mode bytes are unchanged. In compile mode the
    // spec buckets are empty, so the obligation map is empty.
    //
    // The obligation is a required proof-mode deliverable: a spec function that
    // cannot be translated (a `P0xx` diagnostic) fails code generation rather
    // than silently emitting a module whose specifications are unverifiable.
    // Every diagnostic is collected first, so a spec with several mistakes
    // surfaces them all at once.
    if mode == CompilationMode::Proof {
        let (hspecs, diagnostics) = hassert::translate_spec_fns(
            typed_context,
            &buckets,
            &reach_plans,
            compiler.default_arith_mode(),
        );
        if !diagnostics.is_empty() {
            let rendered = diagnostics
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            return Err(CodegenError::UntranslatableSpec(rendered));
        }

        // Refuse a program whose own functions share a `name`-section symbol an
        // obligation applies. The obligations have to exist first: the section
        // is only a namespace to the extent something resolves a name through
        // it, so the applied set is half of the question and is settled here.
        // Nothing is written yet — the caller writes the artifact only once this
        // function returns — so rejecting this late still leaves no stale
        // `.wasm` behind.
        check_name_section_symbols(arena, &buckets, &hspecs)?;
        Ok(hspecs)
    } else {
        Ok(HSpecMap::default())
    }
}

/// File-qualifies a spec name by prefixing its defining file's module-path
/// segments, joined with `_` (`lib_geometry_MySpec`). A spec in the entry file
/// (empty `module_path`) keeps its bare name, so single-file proof-mode output
/// is unchanged.
///
/// The `_` join keeps the result a legal Rocq identifier (`.` is not), so the
/// spec key passes the wasm-to-v identifier validator unchanged and travels
/// intact into the `<module>__<spec>_specs` theorem grammar. The join is not
/// injective when a segment itself ends or begins with `_`; [`check_spec_name_collisions`]
/// rejects the rare resulting clash rather than letting two specs merge.
///
/// Delegates to [`inference_fn_key::fold_spec_name`], the single implementation
/// of the fold, so the proof grammar here and the spec [`FnKey`] identity the
/// analysis passes build stay byte-identical.
pub(crate) fn qualified_spec_name(module_path: &[String], spec_name: &str) -> String {
    inference_fn_key::fold_spec_name(module_path, spec_name)
}

/// Rejects two distinct `(module_path, spec_name)` pairs that collapse to the
/// same [`qualified_spec_name`]. The underscore join is not injective —
/// `["lib","checks"]` + `S` and `["lib_checks"]` + `S` both yield `lib_checks_S` —
/// so two specs from different files could otherwise share one
/// `inference.spec_funcs` map key, silently dropping one spec's obligations.
///
/// Checks the pre-join pairs (where each spec's identity is still distinct), so
/// the collision is caught before the lossy join, and returns a deterministic
/// error naming both originating specs (the `::`-rendered source path) and the
/// shared qualified name.
fn check_spec_name_collisions(specs: &[VisitedSpec]) -> Result<(), CodegenError> {
    let mut seen: FxHashMap<String, &VisitedSpec> = FxHashMap::default();
    for spec in specs {
        let qualified = qualified_spec_name(&spec.module_path, &spec.spec_name);
        if let Some(previous) = seen.insert(qualified.clone(), spec)
            && (previous.module_path != spec.module_path
                || previous.spec_name != spec.spec_name)
        {
            // Render both with the lower-numbered source first so the message
            // is stable regardless of file iteration order.
            let (first, second) = {
                let a = previous.render_source();
                let b = spec.render_source();
                if a <= b { (a, b) } else { (b, a) }
            };
            return Err(CodegenError::SpecNameCollision {
                first,
                second,
                qualified,
            });
        }
    }
    Ok(())
}

/// Rejects a program two of whose own functions render one `name`-section
/// symbol *and* whose obligations apply that symbol.
///
/// The symbol is [`FnKey::name_section_symbol`], the same rendering code
/// generation writes, computed here from the same buckets body compilation
/// iterates — so this decides on exactly the strings that will be emitted
/// rather than on a parallel derivation of them.
///
/// The applied set is the other half of the question, and is what keeps this
/// from refusing programs that were never at risk. A shared symbol is dangerous
/// only where something resolves a function *by* it: the proof translator does
/// that for an obligation's `T_app` / `HA_app_ok` head and nowhere else. It
/// emits the two functions themselves under distinct `Definition` names either
/// way, appending the WASM function index to the second — so with nothing
/// applying the symbol there is no lookup to misresolve, and a program that
/// built before the obligations existed keeps building.
///
/// Only non-specification functions are compared. A spec-inner function's
/// symbol is deliberately left unqualified by its defining file, so it may
/// legitimately coincide with a program function's or with another spec's; the
/// proof translator resolves that coincidence by dropping specification
/// functions from an applied symbol's candidate set, and `inference.spec_funcs`
/// is what tells it which those are. Only the two non-spec keys can collide:
/// `Method` on a struct `mid` in file `lib` and `Free` in file `lib/mid` both
/// join to `lib.mid.make`, because the struct join and the path join are the
/// same `.`.
fn check_name_section_symbols(
    arena: &AstArena,
    buckets: &EmittableFunctions,
    hspecs: &HSpecMap,
) -> Result<(), CodegenError> {
    let applied = applied_obligation_symbols(hspecs);
    if applied.is_empty() {
        return Ok(());
    }

    let mut seen: FxHashMap<String, SymbolSource<'_>> = FxHashMap::default();
    for source in symbol_sources(arena, buckets) {
        let symbol = source.symbol();
        if let Some(previous) = seen.get(&symbol)
            && applied.contains(&symbol)
        {
            // Ordered so the message does not depend on which file was walked
            // first.
            let (a, b) = (previous.render(), source.render());
            let (first, second) = if a <= b { (a, b) } else { (b, a) };
            return Err(CodegenError::NameSectionSymbolCollision {
                symbol,
                first,
                second,
            });
        }
        seen.insert(symbol, source);
    }
    Ok(())
}

/// Every function symbol the program's obligations apply.
///
/// An applied symbol is a `T_app`'s or an `HA_app_ok`'s head — the two positions
/// in which an obligation names a function the proof resolves through the `name`
/// section. An entry's own `fn_symbol` is deliberately not one: it names a
/// specification function, which the translator finds through the
/// `inference.spec_funcs` index list rather than by a lookup over the whole
/// section.
///
/// The matches are exhaustive on purpose: a variant added without a case here
/// would quietly stop being collected, and a compile error is the only notice
/// that carries.
fn applied_obligation_symbols(hspecs: &HSpecMap) -> FxHashSet<String> {
    fn walk_assert(assert: &HAssert, acc: &mut FxHashSet<String>) {
        match assert {
            HAssert::True | HAssert::False => {}
            HAssert::Not(inner) | HAssert::Ex(inner) | HAssert::All(inner) => {
                walk_assert(inner, acc);
            }
            HAssert::And(left, right) | HAssert::Imp(left, right) | HAssert::Or(left, right) => {
                walk_assert(left, acc);
                walk_assert(right, acc);
            }
            HAssert::TermEq(left, right) => {
                walk_term(left, acc);
                walk_term(right, acc);
            }
            HAssert::HasType(term, _) | HAssert::Defined(term) => walk_term(term, acc),
            HAssert::AppOk(symbol, args) => {
                acc.insert(symbol.0.clone());
                for arg in args {
                    walk_term(arg, acc);
                }
            }
        }
    }
    fn walk_term(term: &HTerm, acc: &mut FxHashSet<String>) {
        match term {
            HTerm::Const(_) | HTerm::LVar(_) | HTerm::Local(_) => {}
            HTerm::App(symbol, args) => {
                acc.insert(symbol.0.clone());
                for arg in args {
                    walk_term(arg, acc);
                }
            }
            HTerm::Binop(_, _, left, right) | HTerm::Relop(_, _, left, right) => {
                walk_term(left, acc);
                walk_term(right, acc);
            }
        }
    }

    let mut acc = FxHashSet::default();
    for entries in hspecs.values() {
        for entry in entries {
            walk_assert(&entry.hassert, &mut acc);
        }
    }
    acc
}

/// Every compiled non-specification function, in the order code generation
/// registers them: free functions first, then struct methods.
fn symbol_sources<'a>(
    arena: &'a AstArena,
    buckets: &'a EmittableFunctions,
) -> impl Iterator<Item = SymbolSource<'a>> {
    let funcs = buckets.funcs.iter().map(|entry| SymbolSource::Free {
        module_path: &entry.module_path,
        name: arena.def_name(entry.def_id),
    });
    let methods = buckets.methods.iter().map(|entry| SymbolSource::Method {
        module_path: &entry.module_path,
        struct_name: &entry.struct_name,
        name: arena.def_name(entry.def_id),
    });
    funcs.chain(methods)
}

/// One compiled function as both halves of the collision report: the
/// `name`-section symbol it is compared by, and the source terms it is named in
/// if the comparison fails.
///
/// The two are kept apart because only the first is needed on every proof-mode
/// build. Rendering the human description costs a `String` per function and is
/// read only when a collision is both found and applied, so it is produced from
/// the borrowed parts on that path alone.
enum SymbolSource<'a> {
    Free {
        module_path: &'a [String],
        name: &'a str,
    },
    Method {
        module_path: &'a [String],
        struct_name: &'a str,
        name: &'a str,
    },
}

impl SymbolSource<'_> {
    /// The string code generation writes into the `name` section for this
    /// function, produced by the renderer that writes it.
    fn symbol(&self) -> String {
        match self {
            Self::Free { module_path, name } => {
                FnKey::free_in(module_path.to_vec(), *name).name_section_symbol()
            }
            Self::Method {
                module_path,
                struct_name,
                name,
            } => FnKey::method_in(module_path.to_vec(), *struct_name, *name).name_section_symbol(),
        }
    }

    /// The function named the way its author wrote it: `function 'add' in file
    /// 'lib::arith'`, or `method 'make' on struct 'mid' in file 'lib'`, without
    /// the file clause in the entry file.
    fn render(&self) -> String {
        match self {
            Self::Free { module_path, name } => match file_clause(module_path) {
                Some(file) => format!("function '{name}' in file '{file}'"),
                None => format!("function '{name}'"),
            },
            Self::Method {
                module_path,
                struct_name,
                name,
            } => match file_clause(module_path) {
                Some(file) => {
                    format!("method '{name}' on struct '{struct_name}' in file '{file}'")
                }
                None => format!("method '{name}' on struct '{struct_name}'"),
            },
        }
    }
}

/// The `::`-joined source path of a defining file, or `None` for the entry file.
fn file_clause(module_path: &[String]) -> Option<String> {
    if module_path.is_empty() {
        None
    } else {
        Some(module_path.join("::"))
    }
}

/// Rejects any spec whose file-qualified name is not a legal Rocq identifier.
///
/// The file-qualified name (`qualified_spec_name`) is what the Rocq translator
/// emits into its `<module>__<spec>_specs` definition and theorem, so it must
/// satisfy the translator's identifier rules. Checking here lets codegen surface
/// a clean, source-level diagnostic (naming the file or spec the user wrote, not
/// the joined internal key) and — crucially — fail *before* any `.wasm` is
/// written, so a rejected spec name never leaves a stale artifact behind.
///
/// Two failure families are distinguished so each gets the right message:
///
/// 1. A `__`-run fabricated by the underscore join. A path segment (file stem)
///    or the spec name that begins or ends with `_`, or carries a `__` run in the
///    source itself, makes the joined name reserve Rocq's `<module>__<spec>`
///    separator. This is reported per offending segment with a
///    [`CodegenError::SpecNameReservesSeparator`] that names the file/spec and
///    shows the flattening, because the join is unchanged (kept readable) and the
///    fix is a rename. Single underscores *inside* a segment are fine — the join
///    only fabricates a run at a boundary.
/// 2. Any other Rocq invalidity of the joined name (an invalid character, or a
///    non-letter, non-`_` start), reported with the generic
///    [`CodegenError::SpecNameInvalid`].
fn check_spec_names_valid(specs: &[VisitedSpec]) -> Result<(), CodegenError> {
    for spec in specs {
        if let Some(err) = spec_reserves_separator(spec) {
            return Err(err);
        }
        let qualified = qualified_spec_name(&spec.module_path, &spec.spec_name);
        if let Some(reason) = spec_section::spec_name_rocq_invalidity_reason(&qualified) {
            return Err(CodegenError::SpecNameInvalid {
                spec: spec.render_source(),
                reason,
            });
        }
    }
    Ok(())
}

/// Whether a path segment or spec name would fabricate (or carry) a `__` run when
/// joined, returning the offense phrasing for the diagnostic. A leading or
/// trailing `_` lands next to the join separator (or the next segment's leading
/// `_`); a `__` run in the source is carried verbatim. A single underscore in the
/// interior is fine — it never abuts a join boundary.
fn segment_reserves_separator(segment: &str) -> Option<&'static str> {
    if segment.contains("__") {
        Some("contains a `__` run")
    } else if segment.starts_with('_') {
        Some("begins with `_`")
    } else if segment.ends_with('_') {
        Some("ends with `_`")
    } else {
        None
    }
}

/// Builds a [`CodegenError::SpecNameReservesSeparator`] for the first segment of
/// `spec` (a path stem, then the spec name) that fabricates or carries the
/// reserved `__` separator, or `None` when no segment offends. The path stems are
/// checked before the spec name so a file-stem offense (the common case) names
/// the file.
///
/// Two offense shapes are caught here, both before any artifact is written so the
/// diagnostic names the source the user wrote rather than the flattened key:
///
/// 1. The codegen `_`-join itself carries a `__` run (`qualified.contains("__")`):
///    a leading `_` on a non-first segment, a trailing `_` on a non-last segment,
///    or a `__` run anywhere in the source. A leading `_` on the *first* segment
///    is excluded — it only makes the whole name start with `_` (a non-letter
///    start, not a `__` run), which the generic Rocq-identifier check reports.
/// 2. An imported file's spec name (the last segment) *ends* with `_` while the
///    `_`-join carries no `__`. The trailing `_` is the final character of
///    `qualified`, so it abuts nothing in the codegen join — but the Rocq
///    translator joins the file-qualified name into `<module>__<spec>_specs` and
///    `valid_<module>__<spec>`, where that trailing `_` lands next to the reserved
///    `__` separator. Caught here (not only downstream) so the message names the
///    source spec and its file instead of the joined key the translator sees.
///    The entry case (empty module path) is left to the translator, which has the
///    output module name needed to render its own join.
fn spec_reserves_separator(spec: &VisitedSpec) -> Option<CodegenError> {
    let qualified = qualified_spec_name(&spec.module_path, &spec.spec_name);
    let trailing_underscore_spec_in_subfile =
        !spec.module_path.is_empty() && spec.spec_name.ends_with('_');
    if !qualified.contains("__") && !trailing_underscore_spec_in_subfile {
        return None;
    }

    let segments: Vec<(&str, &str)> = spec
        .module_path
        .iter()
        .map(|s| ("file stem", s.as_str()))
        .chain(std::iter::once(("spec name", spec.spec_name.as_str())))
        .collect();

    let (offender_kind, offender, offender_cause) = segments
        .iter()
        .find_map(|(kind, seg)| segment_reserves_separator(seg).map(|cause| (*kind, *seg, cause)))?;

    // `dir / stem / spec`, the visual the message renders as the join's left side.
    let join_lhs = segments
        .iter()
        .map(|(_, seg)| *seg)
        .collect::<Vec<_>>()
        .join(" / ");
    let fix_hint = suggest_clean_segment(offender_kind, offender);

    Some(CodegenError::SpecNameReservesSeparator(Box::new(
        crate::errors::SpecNameSeparatorDetails {
            spec_name: spec.spec_name.clone(),
            file_label: spec.file_label(),
            join_lhs,
            qualified,
            offender_kind: offender_kind.to_string(),
            offender: offender.to_string(),
            offender_cause: offender_cause.to_string(),
            fix_hint,
        },
    )))
}

/// An imperative fix naming the offender and the concrete rename: trims boundary
/// underscores and collapses internal `__` runs to get the clean form, then
/// phrases it as `rename the file 'x_.inf' to 'x.inf' ...` or `rename the spec
/// 'Invariant_' to 'Invariant' ...`, with a parenthetical naming the exact edit
/// (drop the trailing `_`, drop the leading `_`, or collapse the `__` run).
fn suggest_clean_segment(offender_kind: &str, offender: &str) -> String {
    let mut cleaned = offender.trim_matches('_').to_string();
    while cleaned.contains("__") {
        cleaned = cleaned.replace("__", "_");
    }
    let edit = if offender.contains("__") {
        "collapse the '__' run"
    } else if offender.starts_with('_') {
        "drop the leading '_'"
    } else {
        "drop the trailing '_'"
    };
    if offender_kind == "file stem" {
        format!("rename the file '{offender}.inf' to '{cleaned}.inf' ({edit}).")
    } else {
        format!("rename the spec '{offender}' to '{cleaned}' ({edit}).")
    }
}

/// Stage 1: register every WASM function index up front so forward references
/// resolve correctly during body compilation. Index order:
///   imports (base 0) → regular fns → regular methods → spec fns → spec methods.
///
/// Imported `external fn`s occupy the lowest WASM function indices, so every
/// local function is shifted by the import count. `set_local_func_base` seeds the
/// body-compilation index counter past the imports to keep it in lockstep with
/// the `func_name_to_idx` entries.
fn register_function_indices(
    arena: &AstArena,
    compiler: &mut Compiler,
    typed_context: &TypedContext,
    buckets: &EmittableFunctions,
) -> Result<(), CodegenError> {
    let import_count = compiler.register_imports(arena, &buckets.imports, typed_context)?;
    compiler.set_local_func_base(import_count);

    let toplevel_count =
        u32::try_from(buckets.funcs.len()).expect("more than u32::MAX top-level functions");
    let method_count =
        u32::try_from(buckets.methods.len()).expect("more than u32::MAX top-level methods");

    compiler.build_func_name_to_idx(arena, &buckets.funcs, typed_context, import_count)?;
    let method_base_idx = compiler.func_idx_after_toplevel(toplevel_count);
    compiler.build_method_name_to_idx(
        arena,
        &buckets.methods,
        typed_context,
        method_base_idx,
    )?;

    let spec_func_base = import_count + toplevel_count + method_count;
    let spec_func_indices = compiler.build_func_name_to_idx_with_spec_names(
        arena,
        &buckets.spec_funcs,
        typed_context,
        spec_func_base,
    )?;
    assert_eq!(
        buckets.spec_funcs.len(),
        spec_func_indices.len(),
        "spec-funcs zip length mismatch: bucket has {} entries, registration returned {} indices",
        buckets.spec_funcs.len(),
        spec_func_indices.len(),
    );
    for (entry, assigned_idx) in buckets.spec_funcs.iter().zip(spec_func_indices.iter()) {
        compiler.record_spec_index(
            &qualified_spec_name(&entry.module_path, &entry.spec_name),
            *assigned_idx,
        );
    }

    let spec_func_indices_len = u32::try_from(spec_func_indices.len())
        .expect("more than u32::MAX spec-inner functions");
    let spec_method_base = spec_func_base + spec_func_indices_len;
    let spec_method_indices = compiler.build_method_name_to_idx_with_spec_names(
        arena,
        &buckets.spec_methods,
        typed_context,
        spec_method_base,
    )?;
    assert_eq!(
        buckets.spec_methods.len(),
        spec_method_indices.len(),
        "spec-methods zip length mismatch: bucket has {} entries, registration returned {} indices",
        buckets.spec_methods.len(),
        spec_method_indices.len(),
    );
    for (entry, assigned_idx) in buckets.spec_methods.iter().zip(spec_method_indices.iter()) {
        compiler.record_spec_index(
            &qualified_spec_name(&entry.module_path, &entry.spec_name),
            *assigned_idx,
        );
    }

    // Verify Stage 1 produced the expected number of index entries.
    // Catches index calculation bugs before they manifest as wrong `call` targets.
    debug_assert_eq!(
        compiler.registered_function_count(),
        buckets.funcs.len()
            + buckets.methods.len()
            + buckets.spec_funcs.len()
            + buckets.spec_methods.len(),
        "func_name_to_idx entry count after Stage 1 registration does not match \
         expected count (top-level functions: {}, methods: {}, spec functions: {}, \
         spec methods: {})",
        buckets.funcs.len(),
        buckets.methods.len(),
        buckets.spec_funcs.len(),
        buckets.spec_methods.len(),
    );
    Ok(())
}

/// A top-level free function to emit, tagged with its defining file's module
/// path (empty for the entry file). The module path file-qualifies the
/// function's flat WASM name so two files can each define a same-named function.
pub(crate) struct EmittableFn {
    pub(crate) module_path: Vec<String>,
    pub(crate) def_id: DefId,
}

/// An `external fn` declaration to emit as a WASM function import, tagged with
/// its defining file's module path (empty for the entry file). An extern's
/// signature may name a struct or an enum, and the name is written in — and
/// resolves from — the file that declares the extern, not the entry file.
pub(crate) struct EmittableExtern {
    pub(crate) module_path: Vec<String>,
    pub(crate) def_id: DefId,
}

/// A struct method to emit. `module_path` is the **struct's** defining file —
/// the method's mangled name is qualified by where its struct lives, not where
/// it is called.
pub(crate) struct EmittableMethod {
    pub(crate) module_path: Vec<String>,
    pub(crate) struct_name: String,
    pub(crate) def_id: DefId,
}

/// A spec-inner free function to emit, tagged with its spec and defining file.
pub(crate) struct EmittableSpecFn {
    pub(crate) module_path: Vec<String>,
    pub(crate) spec_name: String,
    pub(crate) def_id: DefId,
}

/// A spec-inner struct method to emit, tagged with its spec, struct, and
/// defining file.
pub(crate) struct EmittableSpecMethod {
    pub(crate) module_path: Vec<String>,
    pub(crate) spec_name: String,
    pub(crate) struct_name: String,
    pub(crate) def_id: DefId,
}

/// A spec block visited in proof mode, tagged with its defining file so its
/// per-spec Rocq entry can be file-qualified consistently with its inner
/// functions.
struct VisitedSpec {
    module_path: Vec<String>,
    spec_name: String,
}

impl VisitedSpec {
    /// Renders the spec's source identity for diagnostics: `spec S` in the
    /// entry file, `lib::checks::S` in an imported file. Uses `::` (the source
    /// path syntax) rather than the joined codegen key so the message points at
    /// what the user wrote.
    fn render_source(&self) -> String {
        if self.module_path.is_empty() {
            self.spec_name.clone()
        } else {
            format!("{}::{}", self.module_path.join("::"), self.spec_name)
        }
    }

    /// The `::`-joined source path of the file the spec is declared in
    /// (`lib::checks`), or `None` for the entry file (which has no path prefix).
    /// Lets a diagnostic name the file separately from the spec, so the message
    /// reads `spec 'S' in file 'lib::checks'` rather than splicing them.
    fn file_label(&self) -> Option<String> {
        file_clause(&self.module_path)
    }
}

#[derive(Default)]
struct EmittableFunctions {
    /// Top-level `external fn` declarations, emitted as WASM function imports
    /// at indices `0..N` ahead of every local function (see
    /// [`Compiler::register_imports`]).
    imports: Vec<EmittableExtern>,
    funcs: Vec<EmittableFn>,
    methods: Vec<EmittableMethod>,
    spec_funcs: Vec<EmittableSpecFn>,
    spec_methods: Vec<EmittableSpecMethod>,
    /// Every spec block visited in proof mode, even if it contributes no
    /// `spec_funcs` / `spec_methods` entries. Drives `ensure_spec_registered`
    /// so an empty user `spec MySpec { }` still surfaces a per-spec
    /// `Definition` and `Theorem` in the Rocq output.
    visited_specs: Vec<VisitedSpec>,
}

/// Folds one source file's top-level defs into `buckets`, tagging each entry
/// with `module_path` (the file's source-root-relative segments, empty for the
/// entry file). Called once per file in canonical order, accumulating into a
/// single set of buckets so Stage 1 assigns globally unique, deterministic WASM
/// function indices across the whole multi-file program.
///
/// Top-level `Def::ExternFunction` declarations land in the `imports` bucket and
/// are emitted as WASM function imports at indices `0..N` ahead of every local
/// function. Spec-inner externs are still skipped — when they are wired through,
/// they will need to either join `spec_funcs` or be surfaced in a sibling
/// `<mod>_spec_imports` list in the Rocq output.
///
/// In `compile` mode the spec buckets stay empty (specs are stripped). In `proof`
/// mode, top-level `Def::Spec.defs` is recursed one level deep to surface inner
/// functions and inner struct methods. Nested specs and module-nested specs are
/// out of scope until those constructs are wired through codegen.
fn collect_emittable_functions(
    arena: &AstArena,
    defs: &[DefId],
    module_path: &[String],
    mode: CompilationMode,
    buckets: &mut EmittableFunctions,
) -> Result<(), CodegenError> {
    for &def_id in defs {
        match &arena[def_id].kind {
            Def::ExternFunction { .. } => buckets.imports.push(EmittableExtern {
                module_path: module_path.to_vec(),
                def_id,
            }),
            Def::Function { .. } => buckets.funcs.push(EmittableFn {
                module_path: module_path.to_vec(),
                def_id,
            }),
            Def::Struct { name, methods, .. } => {
                let struct_name = arena[*name].name.clone();
                for &method_def_id in methods {
                    buckets.methods.push(EmittableMethod {
                        module_path: module_path.to_vec(),
                        struct_name: struct_name.clone(),
                        def_id: method_def_id,
                    });
                }
            }
            Def::Spec {
                name,
                defs: inner,
                ..
            } if mode == CompilationMode::Proof => {
                let spec_name = arena[*name].name.clone();
                buckets.visited_specs.push(VisitedSpec {
                    module_path: module_path.to_vec(),
                    spec_name: spec_name.clone(),
                });
                for &inner_id in inner {
                    match &arena[inner_id].kind {
                        Def::Function { .. } => {
                            buckets.spec_funcs.push(EmittableSpecFn {
                                module_path: module_path.to_vec(),
                                spec_name: spec_name.clone(),
                                def_id: inner_id,
                            });
                        }
                        Def::Struct { name, methods, .. } => {
                            let struct_name = arena[*name].name.clone();
                            for &method_def_id in methods {
                                buckets.spec_methods.push(EmittableSpecMethod {
                                    module_path: module_path.to_vec(),
                                    spec_name: spec_name.clone(),
                                    struct_name: struct_name.clone(),
                                    def_id: method_def_id,
                                });
                            }
                        }
                        Def::Spec { name: inner_name, .. } => {
                            return Err(CodegenError::NestedSpecsNotSupported {
                                outer_spec: spec_name,
                                inner_spec: arena[*inner_name].name.clone(),
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod memory_layout_tests {
    use super::{CodegenOptions, MemoryLayout, MemoryLayoutSource, codegen};
    use inference_type_checker::typed_context::TypedContext;

    /// Every layout `codegen` can be handed compiles, which is what replaced the
    /// refusal this module used to test.
    ///
    /// The unbuildable cases are gone rather than moved: outside
    /// `inference-compiler-interface` a rejected layout has no representation, so
    /// there is nothing left here to hand `codegen`. The rejection itself is
    /// tested where it now lives, against the constructor.
    #[test]
    fn a_constructible_layout_compiles() {
        for (pages, stack_size) in [(1, 65_536), (2, 32_768), (4, 131_072)] {
            let layout =
                MemoryLayout::resolve(Some(pages), Some(stack_size), MemoryLayoutSource::Flag)
                    .expect("these layouts are admissible");
            assert!(
                codegen(
                    &TypedContext::default(),
                    "output",
                    CodegenOptions {
                        layout,
                        ..CodegenOptions::default()
                    },
                )
                .is_ok(),
                "{pages} pages / {stack_size} bytes must compile"
            );
        }
    }
}

#[cfg(test)]
mod stellar_gate_tests {
    use super::{
        AbiParam, AbiReturn, AbiType, ExportSignature, STELLAR_COMPOUND_NEXT_STEP,
        STELLAR_MAX_EXPORT_NAME_BYTES, STELLAR_MAX_EXPORT_PARAMS, STELLAR_MAX_INPUT_NAME_BYTES,
        check_stellar_exports, render_abi_type, stellar_parameter_label,
    };

    fn signature(name: &str, params: Vec<AbiParam>, ret: AbiReturn) -> ExportSignature {
        ExportSignature {
            name: name.to_string(),
            params,
            ret,
        }
    }

    /// Parameters of the given types, named `p0`, `p1`, … in declaration order —
    /// the shape an ordinary source is described by. The position-only label a
    /// parameter written `_` gets has tests of its own.
    fn named_params(types: impl IntoIterator<Item = AbiType>) -> Vec<AbiParam> {
        types
            .into_iter()
            .enumerate()
            .map(|(index, ty)| AbiParam::named(format!("p{index}"), ty))
            .collect()
    }

    /// One admissible method, as a base to vary.
    fn admissible() -> ExportSignature {
        signature(
            "add",
            vec![
                AbiParam::named("a", AbiType::U32),
                AbiParam::named("b", AbiType::I32),
                AbiParam::named("c", AbiType::Bool),
            ],
            AbiReturn::Scalar(AbiType::U32),
        )
    }

    /// The gate against a hand-built descriptor, which is everything it reads —
    /// a refused parameter's label included — so a shape no source can spell is
    /// as reachable here as one it can.
    fn gate(exports: &[ExportSignature]) -> anyhow::Result<()> {
        check_stellar_exports(exports)
    }

    fn refusal(exports: &[ExportSignature]) -> String {
        gate(exports)
            .expect_err("this descriptor must be refused")
            .to_string()
    }

    #[test]
    fn the_scalar_set_in_every_position_is_admissible() {
        for ret in [
            AbiReturn::Unit,
            AbiReturn::Scalar(AbiType::U32),
            AbiReturn::Scalar(AbiType::I32),
            AbiReturn::Scalar(AbiType::Bool),
        ] {
            let sig = signature("method", admissible().params, ret.clone());
            assert!(gate(&[sig]).is_ok(), "{ret:?} must be admissible");
        }
        assert!(gate(&[signature("no_params", Vec::new(), AbiReturn::Unit)]).is_ok());
    }

    /// A contract that uploads with nothing to call is a build worth refusing:
    /// every later stage would accept it, and the failure would surface only as
    /// a deployed address nobody can invoke.
    #[test]
    fn a_module_that_exports_nothing_is_refused() {
        cov_mark::check!(wasm_codegen_stellar_gate_no_exports);
        assert_eq!(
            refusal(&[]),
            "Stellar target: this module exports no function, so the contract would upload \
             with no method to call. A contract's methods are the entry file's top-level \
             'pub fn' declarations; declare at least one."
        );
    }

    /// Each name rule, pinned character for character. These messages are the
    /// only copy of their wording and a user acts on them directly.
    #[test]
    fn each_name_rule_refuses_with_its_own_words() {
        cov_mark::check_count!(wasm_codegen_stellar_gate_export_name, 4);
        assert_eq!(
            refusal(&[signature("", Vec::new(), AbiReturn::Unit)]),
            "Stellar target: an exported function has an empty name, which the host cannot \
             turn into a method symbol."
        );

        let too_long = "n".repeat(STELLAR_MAX_EXPORT_NAME_BYTES + 1);
        assert_eq!(
            refusal(&[signature(&too_long, Vec::new(), AbiReturn::Unit)]),
            format!(
                "Stellar target: the exported function '{too_long}' has a name of 33 bytes, \
                 and the host holds a method name of at most 32. A longer one is not \
                 expressible by any caller, so the method would upload and be unreachable — \
                 rename it."
            )
        );

        assert_eq!(
            refusal(&[signature("__reserved", Vec::new(), AbiReturn::Unit)]),
            "Stellar target: the exported function '__reserved' starts with '__', a prefix \
             the host reserves. Such a method uploads and then refuses every call with \
             \"can't invoke a reserved function directly\" — rename it without the leading \
             underscores."
        );

        assert_eq!(
            refusal(&[signature("two-words", Vec::new(), AbiReturn::Unit)]),
            "Stellar target: the exported function 'two-words' contains '-', and a method \
             name may hold only letters, digits and '_'. No caller can express a name with \
             anything else, so the method would upload and be unreachable — rename it."
        );
    }

    /// Two boundaries the host measured exactly: a name of 32 bytes is a
    /// symbol, and a single leading underscore is not the reserved prefix.
    /// Without these the length rule could be off by one and the prefix rule
    /// could refuse an ordinary private-looking name, and every other test here
    /// would still pass.
    #[test]
    fn the_name_rules_are_exact_at_their_boundaries() {
        let longest = "n".repeat(STELLAR_MAX_EXPORT_NAME_BYTES);
        assert!(
            gate(&[signature(&longest, Vec::new(), AbiReturn::Unit)]).is_ok(),
            "exactly {STELLAR_MAX_EXPORT_NAME_BYTES} bytes is accepted"
        );
        assert!(gate(&[signature("_single", Vec::new(), AbiReturn::Unit)]).is_ok());
        assert!(gate(&[signature("a1_B2", Vec::new(), AbiReturn::Unit)]).is_ok());
    }

    #[test]
    fn the_arity_rule_is_exact_at_its_boundary() {
        let widest = named_params(vec![AbiType::U32; STELLAR_MAX_EXPORT_PARAMS]);
        assert!(gate(&[signature("wide", widest, AbiReturn::Unit)]).is_ok());

        cov_mark::check!(wasm_codegen_stellar_gate_param_count);
        let too_wide = named_params(vec![AbiType::U32; STELLAR_MAX_EXPORT_PARAMS + 1]);
        assert_eq!(
            refusal(&[signature("wider", too_wide, AbiReturn::Unit)]),
            "Stellar target: exported function 'wider' takes 33 parameters, and a contract \
             method takes at most 32. The host checks an invocation's argument count against \
             that limit, so a wider method could never be called."
        );
    }

    /// The whole parameter refusal, pinned character for character on one case,
    /// so the sentence order and the joins are fixed and not merely the clauses.
    #[test]
    fn a_refused_parameter_names_its_position_type_and_next_step() {
        cov_mark::check!(wasm_codegen_stellar_gate_param_type);
        assert_eq!(
            refusal(&[signature(
                "transfer",
                vec![
                    AbiParam::named("to", AbiType::U32),
                    AbiParam::named("amount", AbiType::U64),
                ],
                AbiReturn::Unit
            )]),
            "Stellar target: exported function 'transfer' cannot be a contract method \
             because parameter 2 'amount' is declared 'u64'. This target currently carries only \
             the scalar set: an exported parameter is 'u32', 'i32' or 'bool', and an exported \
             return is one of those or unit. A 64-bit integer needs the same machinery: the \
             host's word is 64 bits wide and spends part of it on a tag, so no 64-bit value \
             fits in one, and it travels as a host object built through host functions this \
             target refuses to import, their call convention not being bound here yet — \
             issue #324. If the function is not meant to be a contract method, remove 'pub': \
             only an entry-file top-level 'pub fn' is exported."
        );
    }

    /// Every inadmissible type reaches a refusal, each family reaches the
    /// diagnosis that is true of it, and each says what to do next. A single
    /// shared sentence would send the author of a `u8` parameter to a
    /// host-object issue that is not what is stopping them.
    #[test]
    fn each_type_family_earns_the_next_step_that_fits_it() {
        const WIDEN: &str = "Widen the declaration to 'u32' or 'i32'.";
        const TRACKED: &str = "issue #324";
        let cases = [
            (AbiType::U64, "64-bit integer", TRACKED),
            (AbiType::I64, "64-bit integer", TRACKED),
            (AbiType::U8, "narrower than 32 bits", WIDEN),
            (AbiType::I8, "narrower than 32 bits", WIDEN),
            (AbiType::U16, "narrower than 32 bits", WIDEN),
            (AbiType::I16, "narrower than 32 bits", WIDEN),
            (
                AbiType::Struct {
                    name: "Point".to_string(),
                },
                "A compound value",
                TRACKED,
            ),
            (
                AbiType::Enum {
                    name: "Colour".to_string(),
                },
                "A compound value",
                TRACKED,
            ),
            (
                AbiType::Array {
                    elem: Box::new(AbiType::I32),
                    len: 4,
                },
                "A compound value",
                TRACKED,
            ),
        ];
        for (ty, diagnosis, next_step) in cases {
            let rendered = render_abi_type(&ty);
            let message = refusal(&[signature("m", named_params([ty.clone()]), AbiReturn::Unit)]);
            assert!(
                message.contains(&format!("is declared '{rendered}'")),
                "the refusal of {ty:?} does not name the declared type: {message}"
            );
            assert!(
                message.contains(diagnosis),
                "the refusal of {ty:?} does not carry `{diagnosis}`: {message}"
            );
            assert!(
                message.contains(next_step),
                "the refusal of {ty:?} does not point at `{next_step}`: {message}"
            );
        }
    }

    /// The narrow-integer refusal points at the language question rather than at
    /// host objects, and says so — it is the one family whose next step is not
    /// issue #324.
    #[test]
    fn a_narrow_integer_is_refused_for_its_own_reason() {
        let message = refusal(&[signature(
            "clamp",
            named_params([AbiType::U8]),
            AbiReturn::Unit,
        )]);
        assert!(message.contains("Widen the declaration to 'u32' or 'i32'."), "{message}");
        assert!(
            !message.contains(STELLAR_COMPOUND_NEXT_STEP),
            "a narrow integer is not a host-object problem: {message}"
        );
    }

    #[test]
    fn a_returned_wide_integer_is_refused() {
        cov_mark::check!(wasm_codegen_stellar_gate_return_type);
        let message = refusal(&[signature(
            "total",
            Vec::new(),
            AbiReturn::Scalar(AbiType::U64),
        )]);
        assert!(message.contains("it returns 'u64'"), "{message}");
        assert!(message.contains("#324"), "{message}");
    }

    /// A compound return arrives through a hidden pointer, which is a different
    /// refusal from a value the host cannot encode: there is nothing for a
    /// contract method to hand back at all.
    #[test]
    fn a_compound_return_is_refused_as_a_pointer_convention() {
        cov_mark::check!(wasm_codegen_stellar_gate_compound_return);
        let message = refusal(&[signature(
            "corners",
            Vec::new(),
            AbiReturn::Sret(AbiType::Array {
                elem: Box::new(AbiType::I32),
                len: 4,
            }),
        )]);
        assert!(message.contains("it returns '[i32; 4]'"), "{message}");
        assert!(
            message.contains("hidden pointer into linear memory"),
            "{message}"
        );
    }

    /// A descriptor that breaks several rules at once reports the name first,
    /// then the arity, then the parameter types, then the return, then the
    /// parameter names. The order is what a user experiences as "fix one thing
    /// and the next appears", so it is fixed here rather than left to the order
    /// the checks happen to be written in — and it is the Val-ABI rewriter's
    /// order too, so both gates refuse one program for the same rule.
    #[test]
    fn the_refusal_order_is_name_arity_types_return_then_parameter_names() {
        let everything_wrong = signature(
            "__wide-and-long",
            vec![AbiParam::unnamed(AbiType::U64); STELLAR_MAX_EXPORT_PARAMS + 1],
            AbiReturn::Scalar(AbiType::U64),
        );
        assert!(
            refusal(std::slice::from_ref(&everything_wrong)).contains("starts with '__'"),
            "the name rule reports first"
        );

        let named = ExportSignature {
            name: "wide_and_long".to_string(),
            ..everything_wrong
        };
        assert!(
            refusal(std::slice::from_ref(&named)).contains("takes 33 parameters"),
            "the arity rule reports before the types"
        );

        let narrow = ExportSignature {
            params: named_params([AbiType::U64]),
            ..named
        };
        let message = refusal(&[narrow]);
        assert!(
            message.contains("is declared 'u64'"),
            "a parameter reports before the return: {message}"
        );

        let returns_wide = signature(
            "f",
            vec![AbiParam::unnamed(AbiType::U32)],
            AbiReturn::Scalar(AbiType::U64),
        );
        let message = refusal(&[returns_wide]);
        assert!(
            message.contains("it returns 'u64'"),
            "the return reports before the parameter names: {message}"
        );

        let typed = signature(
            "f",
            vec![
                AbiParam::unnamed(AbiType::U32),
                AbiParam::named("amount", AbiType::U64),
            ],
            AbiReturn::Unit,
        );
        let message = refusal(&[typed]);
        assert!(
            message.contains("parameter 2 'amount' is declared 'u64'"),
            "every parameter's type reports before any parameter's name: {message}"
        );

        let too_long = "n".repeat(STELLAR_MAX_INPUT_NAME_BYTES + 1);
        for (params, expected) in [
            (
                vec![
                    AbiParam::unnamed(AbiType::U32),
                    AbiParam::named(too_long.clone(), AbiType::U32),
                ],
                "parameter 1 is written '_'",
            ),
            (
                vec![
                    AbiParam::named(too_long.clone(), AbiType::U32),
                    AbiParam::unnamed(AbiType::U32),
                ],
                "parameter 2 is written '_'",
            ),
        ] {
            let message = refusal(&[signature("f", params, AbiReturn::Unit)]);
            assert!(
                message.contains(expected),
                "every parameter is asked for a name before any is measured: {message}"
            );
        }
    }

    /// The whole refusal of `transfer(to: u32, _: u32)`, which the two
    /// unnamed-parameter tests below both pin.
    const UNNAMED_SECOND_PARAMETER_OF_TRANSFER: &str = "Stellar target: exported function \
         'transfer' cannot be a contract method because parameter 2 is written '_', which \
         gives it no name. A contract's `contractspecv0` section describes each method's \
         parameters by name, and `stellar contract invoke` takes each argument as a \
         `--<name>` flag built from it; the compiler does not invent a name you did not write. \
         Name the parameter — `_unused` if the body does not read it; a leading underscore is \
         kept as written. If the function is not meant to be a contract method, remove 'pub': \
         only an entry-file top-level 'pub fn' is exported.";

    /// The unnamed-parameter refusal, pinned character for character: a
    /// parameter written `_` has nothing for the spec to record and nothing for
    /// a caller to pass it by.
    #[test]
    fn an_unnamed_parameter_is_refused_with_its_own_words() {
        cov_mark::check!(wasm_codegen_stellar_gate_unnamed_param);
        assert_eq!(
            refusal(&[signature(
                "transfer",
                vec![
                    AbiParam::named("to", AbiType::U32),
                    AbiParam::unnamed(AbiType::U32),
                ],
                AbiReturn::Unit
            )]),
            UNNAMED_SECOND_PARAMETER_OF_TRANSFER
        );
    }

    /// An empty name is no name, as the rewriter treats it: only a hand-built
    /// descriptor can carry one, and it earns the unnamed refusal, word for
    /// word, rather than passing here to be written as a zero-length name.
    ///
    /// That refusal says the parameter "is written '_'": its wording assumes a
    /// descriptor built from source, where no name is empty. This case exists
    /// only to keep the gate's rule set identical to the rewriter's, whose
    /// message names both causes.
    #[test]
    fn an_empty_parameter_name_is_refused_as_unnamed() {
        assert_eq!(
            refusal(&[signature(
                "transfer",
                vec![
                    AbiParam::named("to", AbiType::U32),
                    AbiParam::named("", AbiType::U32),
                ],
                AbiReturn::Unit
            )]),
            UNNAMED_SECOND_PARAMETER_OF_TRANSFER
        );
    }

    /// Thirty bytes is the width of the spec's input-name field and is
    /// admitted; thirty-one is refused, naming the parameter, its length and
    /// the limit. A leading underscore is part of a name, not a way of leaving
    /// one out, so `_x` is admitted as written.
    #[test]
    fn the_parameter_name_rule_is_exact_at_its_boundary() {
        assert_eq!(STELLAR_MAX_INPUT_NAME_BYTES, 30, "the XDR bound, `string name<30>`");
        let widest = "n".repeat(30);
        assert!(
            gate(&[signature(
                "f",
                vec![AbiParam::named(widest, AbiType::U32)],
                AbiReturn::Unit
            )])
            .is_ok()
        );
        assert!(
            gate(&[signature(
                "f",
                vec![AbiParam::named("_x", AbiType::U32)],
                AbiReturn::Unit
            )])
            .is_ok()
        );

        cov_mark::check!(wasm_codegen_stellar_gate_param_name_length);
        let too_long = "n".repeat(31);
        assert_eq!(
            refusal(&[signature(
                "f",
                vec![
                    AbiParam::named("a", AbiType::U32),
                    AbiParam::named(too_long.clone(), AbiType::Bool),
                ],
                AbiReturn::Unit
            )]),
            format!(
                "Stellar target: exported function 'f' cannot be a contract method because \
                 parameter 2 '{too_long}' has a name of 31 bytes, and a contract method's \
                 parameter name is at most 30 bytes: the contract's `contractspecv0` section \
                 records each name whole in a field that wide, and a longer one would leave the \
                 section unreadable to `stellar contract invoke` and every other tool that reads \
                 it. Shorten the name. If the function is not meant to be a contract method, \
                 remove 'pub': only an entry-file top-level 'pub fn' is exported."
            )
        );
    }

    /// The name bound is a count of bytes, not of characters: fifteen `é`,
    /// two bytes each, are thirty bytes and admitted, and sixteen are
    /// thirty-two and refused with that length. The parity test cannot state
    /// this row, because an Inference identifier is ASCII; only a hand-built
    /// descriptor carries such a name.
    #[test]
    fn a_parameter_name_is_measured_in_bytes_not_characters() {
        let at_bound = "é".repeat(15);
        assert_eq!(at_bound.len(), 30);
        assert!(
            gate(&[signature(
                "f",
                vec![AbiParam::named(at_bound, AbiType::U32)],
                AbiReturn::Unit
            )])
            .is_ok()
        );

        let over_bound = "é".repeat(16);
        let message = refusal(&[signature(
            "f",
            vec![AbiParam::named(over_bound.clone(), AbiType::U32)],
            AbiReturn::Unit,
        )]);
        assert!(
            message.contains(&format!("parameter 1 '{over_bound}' has a name of 32 bytes")),
            "{message}"
        );
    }

    /// The first inadmissible export decides the message, so a module with a
    /// good method and a bad one is still refused.
    #[test]
    fn one_bad_export_refuses_the_module() {
        let message = refusal(&[
            admissible(),
            signature("wide", named_params([AbiType::I64]), AbiReturn::Unit),
        ]);
        assert!(message.contains("exported function 'wide'"), "{message}");
    }

    /// The label of a refused parameter comes from its own descriptor entry: its
    /// name when the source gave one, its position alone when the source wrote
    /// `_`. A neighbour's name must never stand in for it, whichever of the two
    /// is the unnamed one. The type refusal and the length refusal render a
    /// name when there is one, so a borrowed name would show in either. The
    /// unnamed refusal is labelled `None` by construction; its row holds it to
    /// the same position-alone spelling as a type refusal of `_`.
    #[test]
    fn a_refused_parameter_is_labelled_by_the_name_its_descriptor_carries() {
        let cases = [
            (
                vec![
                    AbiParam::named("to", AbiType::U32),
                    AbiParam::named("amount", AbiType::U64),
                ],
                "because parameter 2 'amount' is declared 'u64'.",
            ),
            (
                vec![
                    AbiParam::unnamed(AbiType::U32),
                    AbiParam::named("amount", AbiType::U64),
                ],
                "because parameter 2 'amount' is declared 'u64'.",
            ),
            (
                vec![
                    AbiParam::named("to", AbiType::U32),
                    AbiParam::unnamed(AbiType::U64),
                ],
                "because parameter 2 is declared 'u64'.",
            ),
            (
                vec![
                    AbiParam::named("to", AbiType::U32),
                    AbiParam::unnamed(AbiType::U32),
                ],
                "because parameter 2 is written '_', which gives it no name.",
            ),
            (
                vec![
                    AbiParam::named("to", AbiType::U32),
                    AbiParam::named("n".repeat(31), AbiType::U32),
                ],
                "because parameter 2 'nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn' has a name of 31 bytes,",
            ),
        ];
        for (params, label) in cases {
            let message = refusal(&[signature("transfer", params, AbiReturn::Unit)]);
            assert!(message.contains(label), "expected `{label}` in: {message}");
        }
    }

    #[test]
    fn a_parameter_label_falls_back_to_its_position() {
        assert_eq!(
            stellar_parameter_label(Some("amount"), 0),
            "parameter 1 'amount'"
        );
        assert_eq!(stellar_parameter_label(None, 1), "parameter 2");
        assert_eq!(stellar_parameter_label(None, 9), "parameter 10");
    }

    #[test]
    fn a_nested_array_renders_as_its_source_spelling() {
        assert_eq!(
            render_abi_type(&AbiType::Array {
                elem: Box::new(AbiType::Array {
                    elem: Box::new(AbiType::U32),
                    len: 3,
                }),
                len: 2,
            }),
            "[[u32; 3]; 2]"
        );
        assert_eq!(
            render_abi_type(&AbiType::Struct {
                name: "geom::Point".to_string()
            }),
            "geom::Point"
        );
    }
}

#[cfg(test)]
mod feature_validation_tests {
    use super::{CodegenOptions, CompilationMode, EmitFeatures, Target, codegen};
    use inference_type_checker::typed_context::TypedContext;

    /// One admissible contract method, for the acceptance cases. The Stellar
    /// target refuses a module that exports nothing, so `compile_empty` can only
    /// witness a refusal there, never an acceptance.
    const ONE_EXPORT: &str = "pub fn answer() -> i32 { return 42; }";

    /// The refusal is reached before anything is emitted, so an empty program is
    /// enough to exercise it.
    fn compile_empty(
        target: Target,
        mode: CompilationMode,
        features: EmitFeatures,
    ) -> anyhow::Result<crate::CodegenOutput> {
        let typed_context = TypedContext::default();
        codegen(
            &typed_context,
            "output",
            CodegenOptions {
                target,
                mode,
                opt_level: target.default_opt_level(),
                features,
                layout: crate::MemoryLayout::default(),
            },
        )
    }

    /// Pinned whole rather than by substring: the refusal renders the target
    /// through `Target::as_str()`, so the user-visible text moves whenever that
    /// spelling does while every substring that omits the name keeps passing.
    /// The name is also what a user pastes back into `--target`, so a wrong one
    /// is a wrong instruction, not a cosmetic slip.
    #[test]
    fn stellar_rejects_a_bulk_memory_request() {
        cov_mark::check!(wasm_codegen_target_rejects_feature);
        let err = compile_empty(
            Target::Stellar,
            CompilationMode::Compile,
            EmitFeatures { bulk_memory: true },
        )
        .expect_err("Stellar does not accept bulk memory");
        assert_eq!(
            err.to_string(),
            "The `stellar` target does not support the 'bulk-memory' WebAssembly feature. \
             This target is pinned to the WebAssembly 1.0 instruction set, so a module \
             using the instructions 'bulk-memory' adds is refused here rather than at \
             deployment; drop 'bulk-memory' from the requested features to build for \
             `stellar`."
        );
    }

    /// The feature check sits ahead of the mode checks deliberately: a build that
    /// is wrong about its instruction set should be told that, not sent to fix an
    /// unrelated mode conflict first. `Stellar` + `Proof` violates both rules at
    /// once, so the message that comes back is what pins the order.
    #[test]
    fn the_feature_refusal_precedes_the_proof_mode_refusal() {
        let err = compile_empty(
            Target::Stellar,
            CompilationMode::Proof,
            EmitFeatures { bulk_memory: true },
        )
        .expect_err("both rules reject this build");
        assert!(
            err.to_string()
                .contains("'bulk-memory' WebAssembly feature"),
            "the feature refusal must win, got: {err}"
        );
    }

    /// Parses and type-checks `source` into the context `codegen` takes.
    pub(super) fn type_check(source: &str) -> TypedContext {
        let parsed = inference_parser::parse(source);
        assert!(parsed.errors.is_empty(), "fixture does not parse: {source}");
        inference_type_checker::TypeCheckerBuilder::build_typed_context(parsed.arena)
            .expect("fixture does not type-check")
            .typed_context()
    }

    #[test]
    fn stellar_accepts_the_default_feature_set() {
        assert!(
            codegen(
                &type_check(ONE_EXPORT),
                "output",
                CodegenOptions {
                    target: Target::Stellar,
                    mode: CompilationMode::Compile,
                    opt_level: Target::Stellar.default_opt_level(),
                    features: EmitFeatures::default(),
                    layout: crate::MemoryLayout::default(),
                },
            )
            .is_ok(),
            "the WebAssembly 1.0 default must be accepted by every target"
        );
    }

    #[test]
    fn wasm32_accepts_a_bulk_memory_request() {
        assert!(
            compile_empty(
                Target::Wasm32,
                CompilationMode::Compile,
                EmitFeatures { bulk_memory: true }
            )
            .is_ok(),
            "Wasm32 permits bulk memory"
        );
    }
}

#[cfg(test)]
mod spec_name_tests {
    use super::feature_validation_tests::type_check;
    use super::{
        check_spec_name_collisions, check_spec_names_valid, codegen, qualified_spec_name,
        CodegenOptions, CompilationMode, VisitedSpec,
    };
    use crate::errors::CodegenError;
    use crate::spec_section::MAX_SPEC_NAME_LEN;

    fn visited(segments: &[&str], spec: &str) -> VisitedSpec {
        VisitedSpec {
            module_path: segments.iter().map(|s| (*s).to_string()).collect(),
            spec_name: spec.to_string(),
        }
    }

    #[test]
    fn entry_file_spec_keeps_bare_name() {
        // An entry-file spec (empty module path) keeps its bare name, so
        // single-file proof output is byte-identical to the pre-multi-file world.
        assert_eq!(qualified_spec_name(&[], "LibSpec"), "LibSpec");
    }

    #[test]
    fn non_entry_spec_joins_path_with_underscore() {
        // `.` is illegal in a Rocq identifier; the `_` join keeps the key valid.
        assert_eq!(
            qualified_spec_name(&["lib".to_string(), "checks".to_string()], "LibSpec"),
            "lib_checks_LibSpec"
        );
    }

    #[test]
    fn single_segment_path_joins() {
        assert_eq!(
            qualified_spec_name(&["math".to_string()], "Sp"),
            "math_Sp"
        );
    }

    #[test]
    fn distinct_specs_without_collision_pass() {
        let specs = vec![
            visited(&[], "EntrySpec"),
            visited(&["lib", "checks"], "LibSpec"),
            visited(&["lib", "geo"], "GeoSpec"),
        ];
        assert!(check_spec_name_collisions(&specs).is_ok());
    }

    #[test]
    fn same_spec_recorded_twice_is_not_a_collision() {
        // The same (module_path, spec_name) appearing twice (e.g. revisited)
        // is the same spec, not a clash — only DISTINCT pairs that join to one
        // key are rejected.
        let specs = vec![
            visited(&["lib", "checks"], "LibSpec"),
            visited(&["lib", "checks"], "LibSpec"),
        ];
        assert!(check_spec_name_collisions(&specs).is_ok());
    }

    #[test]
    fn underscore_segment_collision_is_rejected() {
        // `["lib","checks"]` + `S` and `["lib_checks"]` + `S` both join to
        // `lib_checks_S`. Distinct specs, one key — a hard error, never a
        // silent merge.
        let specs = vec![
            visited(&["lib", "checks"], "S"),
            visited(&["lib_checks"], "S"),
        ];
        let err = check_spec_name_collisions(&specs)
            .expect_err("colliding distinct specs must be rejected");
        match err {
            CodegenError::SpecNameCollision {
                first,
                second,
                qualified,
            } => {
                assert_eq!(qualified, "lib_checks_S");
                // Both source identities are named, sorted for determinism.
                assert_eq!(first, "lib::checks::S");
                assert_eq!(second, "lib_checks::S");
            }
            other => panic!("expected SpecNameCollision, got {other:?}"),
        }
    }

    #[test]
    fn trailing_underscore_segment_collision_is_rejected() {
        // `["a_"]` + `b` and `["a"]` + `_b` both join to `a__b`.
        let specs = vec![visited(&["a_"], "b"), visited(&["a"], "_b")];
        let err = check_spec_name_collisions(&specs)
            .expect_err("trailing-underscore collision must be rejected");
        assert!(matches!(err, CodegenError::SpecNameCollision { .. }));
    }

    #[test]
    fn valid_spec_names_pass_validity_check() {
        let specs = vec![
            visited(&[], "EntrySpec"),
            visited(&["lib", "geo"], "GeoSpec"),
            visited(&["math"], "Sp"),
        ];
        assert!(check_spec_names_valid(&specs).is_ok());
    }

    #[test]
    fn leading_underscore_spec_name_in_subfile_reserves_separator() {
        // `spec _S` in `lib/geo.inf` joins to `lib_geo__S`: the join `_` lands
        // next to the spec name's leading `_`, fabricating a reserved `__`. The
        // diagnostic names the SOURCE spec and blames the spec name, not the
        // flattened key.
        let specs = vec![visited(&["lib", "geo"], "_S")];
        let err = check_spec_names_valid(&specs)
            .expect_err("a leading-underscore spec name must be rejected");
        match err {
            CodegenError::SpecNameReservesSeparator(d) => {
                assert_eq!(d.spec_name, "_S");
                assert_eq!(d.file_label.as_deref(), Some("lib::geo"));
                assert_eq!(d.qualified, "lib_geo__S");
                assert_eq!(d.offender_kind, "spec name");
                assert_eq!(d.offender, "_S");
                assert_eq!(d.offender_cause, "begins with `_`");
                assert_eq!(d.fix_hint, "rename the spec '_S' to 'S' (drop the leading '_').");
            }
            other => panic!("expected SpecNameReservesSeparator, got {other:?}"),
        }
    }

    #[test]
    fn trailing_underscore_file_stem_reserves_separator() {
        // `spec S` in `lib/x_.inf` joins to `lib_x__S`: the stem's trailing `_`
        // lands next to the join `_`. The diagnostic blames the FILE stem.
        let specs = vec![visited(&["lib", "x_"], "S")];
        let err = check_spec_names_valid(&specs)
            .expect_err("a trailing-underscore file stem must be rejected");
        match err {
            CodegenError::SpecNameReservesSeparator(d) => {
                assert_eq!(d.spec_name, "S");
                assert_eq!(d.file_label.as_deref(), Some("lib::x_"));
                assert_eq!(d.qualified, "lib_x__S");
                assert_eq!(d.offender_kind, "file stem");
                assert_eq!(d.offender, "x_");
                assert_eq!(d.offender_cause, "ends with `_`");
                assert_eq!(d.fix_hint, "rename the file 'x_.inf' to 'x.inf' (drop the trailing '_').");
            }
            other => panic!("expected SpecNameReservesSeparator, got {other:?}"),
        }
    }

    #[test]
    fn internal_double_underscore_segment_reserves_separator() {
        // A `__` run is legal in an Inference identifier, so a file stem `a__b`
        // or a spec `S__T` carries the reserved run into the joined name verbatim
        // and must be rejected. Here the stem offends.
        let specs = vec![visited(&["lib", "a__b"], "S")];
        let err = check_spec_names_valid(&specs)
            .expect_err("an internal `__` run must be rejected");
        match err {
            CodegenError::SpecNameReservesSeparator(d) => {
                assert_eq!(d.offender_kind, "file stem");
                assert_eq!(d.offender, "a__b");
                assert_eq!(d.offender_cause, "contains a `__` run");
            }
            other => panic!("expected SpecNameReservesSeparator, got {other:?}"),
        }
    }

    #[test]
    fn internal_double_underscore_spec_name_reserves_separator() {
        // The spec name itself carries the run when no path stem offends first.
        let specs = vec![visited(&["lib", "geo"], "S__T")];
        let err = check_spec_names_valid(&specs)
            .expect_err("a `__` run in the spec name must be rejected");
        match err {
            CodegenError::SpecNameReservesSeparator(d) => {
                assert_eq!(d.offender_kind, "spec name");
                assert_eq!(d.offender, "S__T");
            }
            other => panic!("expected SpecNameReservesSeparator, got {other:?}"),
        }
    }

    #[test]
    fn interior_single_underscore_segment_is_fine() {
        // A single underscore in the interior of a segment never abuts the join
        // boundary, so it does not fabricate a `__` run: `lib_my_geo_MySpec` is a
        // legal Rocq identifier.
        let specs = vec![visited(&["lib", "my_geo"], "MySpec")];
        assert!(
            check_spec_names_valid(&specs).is_ok(),
            "an interior single `_` must not be rejected"
        );
    }

    #[test]
    fn trailing_underscore_spec_name_in_subfile_reserves_separator() {
        // `spec Invariant_` in `lib/geom.inf` joins to `lib_geom_Invariant_`,
        // which carries no `__` of its own — the trailing `_` is the final
        // character. But the Rocq translator joins it into `<module>__<spec>_specs`
        // and `valid_<module>__<spec>`, where that trailing `_` abuts the reserved
        // `__` separator. Codegen catches it here so the diagnostic names the
        // SOURCE spec (`lib::geom::Invariant_`) the user wrote, not the flattened
        // key the translator would otherwise report.
        let specs = vec![visited(&["lib", "geom"], "Invariant_")];
        let err = check_spec_names_valid(&specs)
            .expect_err("a trailing-underscore spec name in an imported file must be rejected");
        match err {
            CodegenError::SpecNameReservesSeparator(d) => {
                assert_eq!(d.spec_name, "Invariant_");
                assert_eq!(d.file_label.as_deref(), Some("lib::geom"));
                assert_eq!(d.qualified, "lib_geom_Invariant_");
                assert_eq!(d.offender_kind, "spec name");
                assert_eq!(d.offender, "Invariant_");
                assert_eq!(d.offender_cause, "ends with `_`");
                assert_eq!(
                    d.fix_hint,
                    "rename the spec 'Invariant_' to 'Invariant' (drop the trailing '_')."
                );
            }
            other => panic!("expected SpecNameReservesSeparator, got {other:?}"),
        }
    }

    #[test]
    fn entry_file_trailing_underscore_spec_name_left_to_translator() {
        // An entry-file `spec Spec_` keeps its bare name `Spec_` (empty module
        // path), so codegen's `_`-join produces no `__` and the trailing `_` only
        // abuts the translator's `<module>__<spec>_specs` join, which needs the
        // output module name codegen does not have. Codegen passes it through; the
        // translator's `validate_spec_join_boundary` rejects it with the output
        // module name in hand. The subfile case is the one codegen owns.
        let specs = vec![visited(&[], "Spec_")];
        assert!(
            check_spec_names_valid(&specs).is_ok(),
            "an entry-file trailing-underscore spec is left to the translator's join check"
        );
    }

    #[test]
    fn entry_file_leading_underscore_spec_name_rejected() {
        // An entry-file `spec _S` keeps its bare name `_S`: there is no join, so
        // no `__` run is fabricated — it is simply a non-letter start, which the
        // generic Rocq-identifier check rejects.
        let specs = vec![visited(&[], "_S")];
        let err = check_spec_names_valid(&specs)
            .expect_err("a bare leading-underscore spec name must be rejected");
        match err {
            CodegenError::SpecNameInvalid { spec, reason } => {
                assert_eq!(spec, "_S");
                assert!(
                    reason.contains("start with a letter"),
                    "reason must explain the leading non-letter, got: {reason}"
                );
            }
            other => panic!("expected SpecNameInvalid, got {other:?}"),
        }
    }

    /// The byte cap both `inference.spec_funcs` decoders enforce is checked on
    /// the way out of emission, so an over-long spec name comes back as a
    /// codegen diagnostic instead of an artifact that fails its own downstream
    /// decode. Falsified by dropping the early return that raises
    /// `SpecNameTooLong`, or by changing the message or the length it reports.
    /// Where the check sits relative to section encoding is not pinned here and
    /// cannot be — this entry point hands back no bytes on either error path, so
    /// the ordering is unobservable from outside it.
    #[test]
    fn a_spec_name_over_the_byte_cap_is_refused() {
        cov_mark::check!(wasm_codegen_spec_name_too_long);
        let name = "S".repeat(MAX_SPEC_NAME_LEN + 1);
        let source = format!(
            "fn helper() -> i32 {{ return 2; }}
             spec {name} {{
                 fn claim() forall {{ assert(helper() == 2); }}
             }}"
        );
        let typed_context = type_check(&source);
        let err = codegen(
            &typed_context,
            "output",
            CodegenOptions {
                mode: CompilationMode::Proof,
                ..CodegenOptions::default()
            },
        )
        .expect_err("a spec name past the cap must be refused");
        let len = MAX_SPEC_NAME_LEN + 1;
        assert_eq!(
            err.to_string(),
            format!(
                "spec name is {len} bytes, which exceeds the maximum of \
                 {MAX_SPEC_NAME_LEN} bytes: '{name}'"
            )
        );
    }

    #[test]
    fn entry_file_internal_double_underscore_spec_name_reserves_separator() {
        // An entry-file `spec S__T` carries the `__` run with no join at all, so
        // it is reported as reserving the separator (the spec name is the
        // offender).
        let specs = vec![visited(&[], "S__T")];
        let err = check_spec_names_valid(&specs)
            .expect_err("a bare `__`-run spec name must be rejected");
        match err {
            CodegenError::SpecNameReservesSeparator(d) => {
                assert_eq!(d.offender_kind, "spec name");
                assert_eq!(d.offender, "S__T");
            }
            other => panic!("expected SpecNameReservesSeparator, got {other:?}"),
        }
    }
}

/// The `name`-section symbol collision codegen refuses in proof mode.
///
/// The section is one namespace with two joins in it — a struct's
/// `<struct>.<method>` and a defining file's `<file>.<function>` — and they are
/// spelled alike, so one program can put two functions under one symbol. That
/// symbol is what a proof obligation resolves an applied function name through,
/// so a program whose obligations *do* apply it is refused while both
/// definitions can still be named — and a program whose obligations do not is
/// left alone, because nothing there resolves anything by the shared string.
#[cfg(test)]
mod name_section_symbol_tests {
    use super::{CodegenOptions, CompilationMode, codegen};
    use inference_ast::arena::AstArena;
    use inference_type_checker::TypeCheckerBuilder;
    use inference_type_checker::typed_context::TypedContext;

    /// The entry file declares a struct named `lib` carrying a `helper` method;
    /// `lib.inf` declares a free `helper`. `Method{[], "lib", "helper"}` and
    /// `Free{["lib"], "helper"}` both render `lib.helper`.
    const ENTRY: &str = r"
        struct lib {
            fn helper() -> i32 { return 1; }
        }
        fn main() -> i32 { return 0; }
    ";

    /// `lib.inf` with a spec whose obligation applies the shared `lib.helper`.
    const LIB_APPLIED: &str = r"
        fn helper() -> i32 { return 2; }
        fn other() -> i32 { return 3; }
        spec LibSpec {
            fn claim() forall { assert(helper() == 2); }
        }
    ";

    /// The same file and the same collision, with the obligation applying the
    /// file's *other* function instead. The program still carries obligations,
    /// so this isolates the applied symbol as the thing that decides.
    const LIB_UNAPPLIED: &str = r"
        fn helper() -> i32 { return 2; }
        fn other() -> i32 { return 3; }
        spec LibSpec {
            fn claim() forall { assert(other() == 3); }
        }
    ";

    fn type_check_multi(files: &[(&[&str], &str)]) -> TypedContext {
        let mut arena = AstArena::default();
        for (module_path, source) in files {
            let module_path: Vec<String> = module_path.iter().map(|s| (*s).to_string()).collect();
            let parsed = inference_parser::parse_into(arena, source, module_path);
            assert!(
                parsed.errors.is_empty(),
                "parse errors: {:?}",
                parsed.errors
            );
            arena = parsed.arena;
        }
        TypeCheckerBuilder::build_typed_context(arena)
            .expect("multi-file type checking should succeed")
            .typed_context()
    }

    fn compile(
        files: &[(&[&str], &str)],
        mode: CompilationMode,
    ) -> anyhow::Result<crate::CodegenOutput> {
        let ctx = type_check_multi(files);
        codegen(
            &ctx,
            "output",
            CodegenOptions {
                mode,
                ..CodegenOptions::default()
            },
        )
    }

    /// The collision with an obligation that applies it.
    fn compile_applied(mode: CompilationMode) -> anyhow::Result<crate::CodegenOutput> {
        compile(&[(&[], ENTRY), (&["lib"], LIB_APPLIED)], mode)
    }

    #[test]
    fn a_method_and_a_free_function_under_one_applied_symbol_are_refused_in_proof_mode() {
        let err = compile_applied(CompilationMode::Proof)
            .expect_err("an obligation over two functions of one symbol must be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("`lib.helper`"),
            "the message must name the shared symbol; got: {msg}"
        );
        assert!(
            msg.contains("method 'helper' on struct 'lib'")
                && msg.contains("function 'helper' in file 'lib'"),
            "the message must name both definitions as their author wrote them; got: {msg}"
        );
    }

    /// The diagnostic is one flowing sentence, not a string with the line
    /// continuations left out of it.
    ///
    /// A message assembled from a multi-line literal loses its `\` and ships
    /// with runs of source indentation inside it. Every substring an assertion
    /// would naturally reach for sits *inside* one of those runs, so the garbled
    /// form passes a substring check; only a phrase that spans a line boundary,
    /// and a direct look for a double space, can see it.
    #[test]
    fn the_collision_message_reads_as_one_sentence() {
        let err = compile_applied(CompilationMode::Proof)
            .expect_err("an obligation over two functions of one symbol must be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("recorded as `lib.helper` in the verification artifact"),
            "the phrase spanning the literal's line break must read continuously; got: {msg}"
        );
        assert!(
            !msg.contains("  "),
            "no run of source indentation may survive into the message; got: {msg}"
        );
    }

    /// The same collision, in a program whose obligations name something else,
    /// builds.
    ///
    /// Nothing resolves a function by the shared symbol here, and the two
    /// functions still receive distinct Rocq definitions — so refusing this
    /// would cost a program that was never at risk.
    #[test]
    fn the_same_collision_builds_when_no_obligation_applies_the_symbol() {
        compile(
            &[(&[], ENTRY), (&["lib"], LIB_UNAPPLIED)],
            CompilationMode::Proof,
        )
        .expect("a shared symbol no obligation applies is not a proof-mode error");
    }

    /// A program with no specification at all — the shape a collision cannot
    /// endanger, because there is no obligation to misresolve.
    ///
    /// Written as the deeper `lib.inf` / `lib/mid.inf` pair, the other way the
    /// two joins meet: `Method{["lib"], "mid", "make"}` and
    /// `Free{["lib", "mid"], "make"}` both render `lib.mid.make`.
    #[test]
    fn a_collision_in_a_specless_program_builds_in_both_modes() {
        let files: &[(&[&str], &str)] = &[
            (
                &[],
                r"
                use lib;
                use lib::mid;
                fn main() -> i32 { return lib::helper(1); }
                ",
            ),
            (
                &["lib"],
                r"
                pub struct mid {
                    x: i32;
                    pub fn make(v: i32) -> i32 { return v + 1; }
                }
                pub fn helper(v: i32) -> i32 { return v + 2; }
                ",
            ),
            (
                &["lib", "mid"],
                r"pub fn make(v: i32) -> i32 { return v + 3; }",
            ),
        ];
        compile(files, CompilationMode::Proof)
            .expect("a specless program has nothing to misresolve");
        compile(files, CompilationMode::Compile).expect("and compile mode never read the section");
    }

    /// Compile mode writes the same two names, but nothing reads the section as a
    /// namespace there, so even the applied program still compiles.
    #[test]
    fn the_applied_program_still_compiles_in_compile_mode() {
        compile_applied(CompilationMode::Compile)
            .expect("a colliding debug name is not a compile-mode error");
    }
}
