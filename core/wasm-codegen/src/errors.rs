use thiserror::Error;

/// Error returned when a function call expression cannot be lowered by the codegen pass.
///
/// This is an internal error type used by [`super::compiler::Compiler::lower_function_call`]
/// and sret return lowering. Callers convert it to a `panic!` depending on whether the
/// case indicates a type-checker inconsistency.
#[derive(Debug, Error)]
#[must_use = "errors must not be silently ignored"]
pub(crate) enum CodegenError {
    /// The function name was not found in the pre-built index map.
    /// This should never happen if the type-checker ran successfully.
    #[error(
        "function '{0}' not found in module — the type-checker should have caught undefined functions"
    )]
    UnknownFunction(String),
    /// The return expression in an sret function is not a supported form.
    /// Supported forms: identifier, array literal, or call to another sret function.
    #[error(
        "unsupported sret return expression in function — expected identifier, array literal, or array-returning function call"
    )]
    UnsupportedSretReturnExpression,
    /// An array (or nested array) has too many total elements for uzumaki
    /// unrolling. Each element produces several WASM instructions, so
    /// unbounded unrolling leads to O(n) instruction explosion.
    #[error(
        "array has {total_elements} elements which exceeds the maximum of {max} for uzumaki unrolling"
    )]
    ArrayTooLargeForUzumaki { total_elements: u32, max: u32 },
    /// Cycle detected in struct layout computation. The type checker should
    /// prevent recursive struct definitions, so this variant is defense-in-depth.
    #[error("cycle detected in struct layout for '{name}' -- the struct transitively contains itself")]
    CycleInStructLayout { name: String },
    /// A struct name referenced during layout computation was not found in the type context.
    #[error("struct '{name}' not found in type context -- the type checker should have caught this")]
    StructNotFoundInTypeContext { name: String },
    /// A `spec` block contained another `spec` block. Nested specs have no
    /// defined Rocq emission; the codegen pipeline refuses rather than
    /// silently dropping the inner definitions.
    #[error("nested specs are not supported: spec '{outer_spec}' contains spec '{inner_spec}'")]
    NestedSpecsNotSupported {
        outer_spec: String,
        inner_spec: String,
    },
    /// A type in a signature has no WASM value-type representation. The
    /// type-checker rejects unknown types before codegen, so reaching this is a
    /// defense-in-depth failure rather than a normal diagnostic path; emitting
    /// an error keeps codegen from `todo!()`-panicking on a malformed type.
    #[error("unsupported type in WASM codegen: {rendered}")]
    UnsupportedType { rendered: String },
    /// A spec name exceeds the byte cap that both `inference.spec_funcs`
    /// decoders enforce (the linker and the Rocq translator). Emitting it would
    /// produce a `.wasm` artifact that fails its own downstream link/translate
    /// step, so codegen refuses up front with an actionable diagnostic.
    #[error("spec name is {len} bytes, which exceeds the maximum of {max} bytes: '{name}'")]
    SpecNameTooLong {
        name: String,
        len: usize,
        max: usize,
    },
    /// Two specs in distinct files (or a distinct file/name combination)
    /// produce the same file-qualified key after the module path is joined to
    /// the spec name. Because module-path segments may themselves contain `_`,
    /// the underscore join is not injective: `lib/checks` + `S` and a file
    /// `lib_checks` + `S` both yield `lib_checks_S`. Merging them in the
    /// `inference.spec_funcs` map would silently drop one spec's proof
    /// obligations, so codegen refuses and names both originating specs.
    #[error(
        "spec name collision: '{first}' and '{second}' both map to the qualified name '{qualified}'; \
         rename one spec or its containing file to avoid the clash"
    )]
    SpecNameCollision {
        first: String,
        second: String,
        qualified: String,
    },
    /// A spec's file-qualified name is not a legal Rocq identifier, so the
    /// emitted `<module>__<spec>_specs` definition and `valid_<module>__<spec>`
    /// theorem would be rejected by the Rocq translator. The most common cause is
    /// a leading underscore in the spec name, which the module-path join turns
    /// into a `__` run (`spec _S` in `lib/geo.inf` → `lib_geo__S`). The diagnostic
    /// names the source spec (`lib::geo::_S`), not the joined internal key, so the
    /// user sees what they wrote. Caught in codegen before any artifact is
    /// written, so a rejected spec name leaves no stale `.wasm` behind.
    #[error(
        "spec name `{spec}` is not a valid name for Rocq translation ({reason}); \
         rename the spec (Rocq names must start with a letter and contain no `__` run)"
    )]
    SpecNameInvalid { spec: String, reason: String },

    /// A proof-mode spec's file-qualified name would carry a `__` run, which Rocq
    /// reserves as the `<module>__<spec>` separator. The run is fabricated when a
    /// path segment (file stem) or the spec name begins or ends with `_` — the
    /// `_` that joins the segments lands next to the boundary `_` — or carries a
    /// `__` run in the source itself. Reported at the source level, naming the
    /// offending file or spec rather than the flattened internal key, and before
    /// any artifact is written so a rejected name leaves no stale `.wasm`/`.v`.
    ///
    /// Deliberately a rejection rather than an auto-escape: the file-qualified
    /// name is emitted verbatim into the proof artifact (`Definition
    /// output__<name>_specs`, `Theorem valid_output__<name>`), so an escaped form
    /// like `lib_x_1U_S` would make the proof harder to read. Renaming keeps the
    /// proof names legible.
    #[error(
        "proof-mode spec name reserves `__`\n\
         \n\
         The spec `{spec_source}` is fine, but in proof mode each spec is given a \
         file-qualified name so two specs with the same name in different files do \
         not collide in the generated Rocq (.v). That name joins the path segments \
         with `_`:\n\
         \n    {join_lhs} -> {qualified}\n\
         \n\
         The `__` run in `{qualified}` is reserved in Rocq as the module/spec \
         separator, so the name is ambiguous and rejected. The {offender_kind} \
         `{offender}` (which {offender_cause}) is what creates it.\n\
         \n\
         How to fix: rename so no path segment or spec name begins with `_`, ends \
         with `_`, or contains a `__` run (e.g. `{fix_hint}`).\n\
         \n\
         Why not auto-encode: proof-mode names appear verbatim in your .v file, so \
         they are kept readable rather than escaped into noise.",
        spec_source = .0.spec_source,
        join_lhs = .0.join_lhs,
        qualified = .0.qualified,
        offender_kind = .0.offender_kind,
        offender = .0.offender,
        offender_cause = .0.offender_cause,
        fix_hint = .0.fix_hint,
    )]
    SpecNameReservesSeparator(Box<SpecNameSeparatorDetails>),
}

/// The boxed payload of [`CodegenError::SpecNameReservesSeparator`]. Boxed so the
/// seven diagnostic strings do not enlarge `CodegenError` (and every
/// `Result<_, CodegenError>` it flows through) — the variant is rare and only ever
/// rendered, so the indirection costs nothing on the hot path.
#[derive(Debug)]
pub(crate) struct SpecNameSeparatorDetails {
    /// The source-level spec identity (`lib::x_::S`), for the lead line.
    pub(crate) spec_source: String,
    /// The `dir / stem / spec` rendering of the join inputs.
    pub(crate) join_lhs: String,
    /// The flattened qualified name that carries the `__` run.
    pub(crate) qualified: String,
    /// Whether the offender is a `file stem` or a `spec name`.
    pub(crate) offender_kind: String,
    /// The offending segment text.
    pub(crate) offender: String,
    /// The phrasing of why it offends — "ends with an underscore", and so on.
    pub(crate) offender_cause: String,
    /// A concrete renamed form to point at.
    pub(crate) fix_hint: String,
}
