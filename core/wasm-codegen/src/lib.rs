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
//!   codegen(tc, target, mode, opt_level)
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
//!   `Wasm32` target (Decision #32).
//!
//! # Module Organization
//!
//! - [`compiler`] - WASM binary generation via wasm-encoder (private)
//! - [`memory`] - Linear memory infrastructure for stack-allocated compound types (private)
//! - [`output`] - `CodegenOutput` struct definition
//! - [`target`] - `Target`, `CompilationMode`, and `OptLevel` enums

#![warn(clippy::pedantic)]

use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::Def;
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashMap;

use crate::compiler::{Compiler, FunctionOrigin};
use crate::errors::CodegenError;

mod compiler;
mod errors;
mod memory;
pub mod output;
mod spec_section;
pub mod target;

pub use output::CodegenOutput;
pub use target::{CompilationMode, OptLevel, Target};

/// Single source of truth for the custom WASM section name that carries
/// per-spec function indices. Consumed by both `core/wasm-codegen` (encoder)
/// and `core/wasm-to-v` (decoder) so the wire-format constant lives in one
/// place.
pub use crate::spec_section::SECTION_NAME as SPEC_FUNCS_SECTION_NAME;

/// Wire-format version of the `inference.spec_funcs` custom section payload.
/// Decoders must reject payloads whose leading varuint32 does not equal this
/// constant; bumping the value is a breaking change to the section format.
pub use crate::spec_section::SECTION_VERSION as SPEC_FUNCS_SECTION_VERSION;

/// Generates WebAssembly binary from a typed AST for the specified target and compilation mode.
///
/// `module_name` is written into the WASM module-name subsection and flows
/// downstream to the Rocq translator, which uses it as the top-level module
/// identifier. The CLI derives this from the input file stem; library
/// callers can pass any [`validate_rocq_identifier`]-compatible name.
///
/// [`validate_rocq_identifier`]: inference_wasm_to_v_translator::validate_rocq_identifier
///
/// # Errors
///
/// Returns an error if:
/// - Validation fails (proof + non-Wasm32, or Soroban + non-det)
/// - Code generation fails
pub fn codegen(
    typed_context: &TypedContext,
    target: Target,
    mode: CompilationMode,
    opt_level: OptLevel,
    module_name: &str,
) -> anyhow::Result<CodegenOutput> {
    if mode == CompilationMode::Proof && !target.supports_proof_mode() {
        cov_mark::hit!(wasm_codegen_proof_mode_rejected_non_wasm32);
        return Err(anyhow::anyhow!(
            "Proof mode requires Wasm32 target. Proof mode emits custom 0xfc \
             non-deterministic instructions that only the Wasm32 target supports; \
             the {target:?} target cannot process these."
        ));
    }

    let arena = typed_context.arena();

    if target == Target::Soroban {
        for source_file in typed_context.source_files() {
            for &def_id in &source_file.defs {
                if arena.def_is_non_det(def_id) {
                    cov_mark::hit!(wasm_codegen_soroban_rejects_nondet_function);
                    let fn_name = arena.def_name(def_id);
                    return Err(anyhow::anyhow!(
                        "Soroban target does not support non-deterministic operations. \
                         Function '{fn_name}' contains non-deterministic constructs (uzumaki, \
                         forall, exists, assume, or unique blocks) that produce custom \
                         0xfc WebAssembly instructions incompatible with the Soroban VM.",
                    ));
                }
            }
        }
    }

    let mut compiler = Compiler::new(module_name);

    // Runtime array bounds checks are emitted for every Compile-mode build
    // (Debug and Release, Wasm32 and Soroban): the executed/deployed artifact is
    // always checked so a dynamic out-of-range access traps cleanly instead of
    // corrupting adjacent frame slots. `OptLevel` no longer influences this.
    // Proof mode is left unguarded pending the proof-obligation path (#212),
    // which discharges dynamic bounds as Rocq obligations rather than runtime
    // traps; the `emit_index_offset` choke point is the seam where it hooks in.
    compiler.set_emit_bounds_checks(mode == CompilationMode::Compile);

    if typed_context.source_files().len() > 0 {
        traverse_t_ast_with_compiler(typed_context, &mut compiler, mode)?;
    }

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

    // Snapshot `has_main` before `finish_and_take` consumes the compiler:
    // the section is emitted in a single pass that moves out the recorded
    // spec map alongside the WASM bytes.
    let has_main = compiler.has_main();
    let (wasm, spec_func_indices_by_spec, frame_sizes) = compiler.finish_and_take();
    debug_assert!(
        mode != CompilationMode::Compile || spec_func_indices_by_spec.is_empty(),
        "compile mode must not record any spec function indices"
    );

    Ok(CodegenOutput::new(
        wasm,
        target,
        mode,
        opt_level,
        module_name.to_string(),
        has_main,
        spec_func_indices_by_spec,
    )
    .with_frame_sizes(frame_sizes))
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
) -> Result<(), CodegenError> {
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

    // Stage 2: Compile bodies in the same order as registration.
    for entry in &buckets.funcs {
        compiler.visit_function_definition(
            entry.def_id,
            arena,
            typed_context,
            None,
            &entry.module_path,
            &FunctionOrigin::TopLevel,
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
        )?;
    }
    for entry in &buckets.spec_funcs {
        compiler.visit_function_definition(
            entry.def_id,
            arena,
            typed_context,
            None,
            &entry.module_path,
            &FunctionOrigin::SpecInner(qualified_spec_name(
                &entry.module_path,
                &entry.spec_name,
            )),
        )?;
    }
    for entry in &buckets.spec_methods {
        compiler.visit_function_definition(
            entry.def_id,
            arena,
            typed_context,
            Some(&entry.struct_name),
            &entry.module_path,
            &FunctionOrigin::SpecInner(qualified_spec_name(
                &entry.module_path,
                &entry.spec_name,
            )),
        )?;
    }
    Ok(())
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
pub(crate) fn qualified_spec_name(module_path: &[String], spec_name: &str) -> String {
    if module_path.is_empty() {
        spec_name.to_string()
    } else {
        format!("{}_{spec_name}", module_path.join("_"))
    }
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
/// `spec` (a path stem, then the spec name) that fabricates or carries a `__`
/// run, or `None` when the joined name has no `__` run. The path stems are
/// checked before the spec name so a file-stem offense (the common case) names
/// the file.
///
/// Gated on the joined name actually carrying a `__` run: a leading `_` on the
/// *first* segment only makes the whole name start with `_` (a non-letter start,
/// not a `__` run), which the generic Rocq-identifier check reports instead.
fn spec_reserves_separator(spec: &VisitedSpec) -> Option<CodegenError> {
    let qualified = qualified_spec_name(&spec.module_path, &spec.spec_name);
    if !qualified.contains("__") {
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
            spec_source: spec.render_source(),
            join_lhs,
            qualified,
            offender_kind: offender_kind.to_string(),
            offender: offender.to_string(),
            offender_cause: offender_cause.to_string(),
            fix_hint,
        },
    )))
}

/// A concrete renamed form to point the user at: trims boundary underscores and
/// collapses internal `__` runs, suffixing `.inf` for a file stem so the hint
/// reads as a filename (`x_` -> `x.inf`).
fn suggest_clean_segment(offender_kind: &str, offender: &str) -> String {
    let mut cleaned = offender.trim_matches('_').to_string();
    while cleaned.contains("__") {
        cleaned = cleaned.replace("__", "_");
    }
    if offender_kind == "file stem" {
        format!("{offender}.inf -> {cleaned}.inf")
    } else {
        format!("{offender} -> {cleaned}")
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
}

#[derive(Default)]
struct EmittableFunctions {
    /// Top-level `external fn` declarations, emitted as WASM function imports
    /// at indices `0..N` ahead of every local function (see
    /// [`Compiler::register_imports`]).
    imports: Vec<DefId>,
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
            Def::ExternFunction { .. } => buckets.imports.push(def_id),
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
mod spec_name_tests {
    use super::{check_spec_name_collisions, check_spec_names_valid, qualified_spec_name, VisitedSpec};
    use crate::errors::CodegenError;

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
                assert_eq!(d.spec_source, "lib::geo::_S");
                assert_eq!(d.qualified, "lib_geo__S");
                assert_eq!(d.offender_kind, "spec name");
                assert_eq!(d.offender, "_S");
                assert_eq!(d.offender_cause, "begins with `_`");
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
                assert_eq!(d.qualified, "lib_x__S");
                assert_eq!(d.offender_kind, "file stem");
                assert_eq!(d.offender, "x_");
                assert_eq!(d.offender_cause, "ends with `_`");
                assert_eq!(d.fix_hint, "x_.inf -> x.inf");
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
