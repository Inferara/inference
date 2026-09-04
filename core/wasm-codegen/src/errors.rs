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
    #[error(
        "cycle detected in struct layout for '{name}' -- the struct transitively contains itself"
    )]
    CycleInStructLayout { name: String },
    /// A struct name referenced during layout computation was not found in the type context.
    #[error(
        "struct '{name}' not found in type context -- the type checker should have caught this"
    )]
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
    /// Two of the program's own functions would be written into the WASM `name`
    /// section under one symbol, in proof mode, and an obligation applies that
    /// symbol — so the claim it carries names both functions at once.
    ///
    /// The section joins a function's defining file to its source name with
    /// `.`, and joins a struct to its method with the same `.`, so the two
    /// spellings meet: a free `helper` in `lib.inf` and a `helper` method on an
    /// entry-file struct `lib` both render `lib.helper`, as do a free `make` in
    /// `lib/mid.inf` and a `make` method on a struct `mid` in `lib.inf`.
    /// Nothing downstream can tell the two apart — an obligation applying the
    /// symbol names a string, and both functions answer to it — so rather than
    /// let one silently win, codegen names both and asks for a rename.
    ///
    /// A shared symbol *no* obligation applies is left alone: the two functions
    /// still receive distinct Rocq definitions, and nothing resolves them by the
    /// shared string. Compile mode is unaffected for the same reason — there the
    /// section carries debug names only, and no obligation reads it.
    #[error(
        "proof-mode symbol collision: {first} and {second} are both recorded as \
         `{symbol}` in the verification artifact, and a proof obligation applies that \
         symbol — so the claim it carries names both. The two joins are spelled alike: \
         `.` separates a struct from its method, and equally separates a defining file \
         from the name it declares. Rename the struct, the function, or the file"
    )]
    NameSectionSymbolCollision {
        symbol: String,
        first: String,
        second: String,
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

    /// A proof-mode spec's file-qualified name would fabricate (or carry) a `__`
    /// run, which Rocq reserves as the `<module>__<spec>` separator. The run is
    /// fabricated when a path segment (file stem) or the spec name begins or ends
    /// with `_` — the `_` that joins the segments, or that abuts the translator's
    /// own `<module>__<spec>_specs` join, lands next to the boundary `_` — or when
    /// a segment carries a `__` run in the source itself. Reported at the source
    /// level: it leads with the spec and its file as the user wrote them
    /// (`spec 'S' in file 'lib::checks'`), and shows the *generated* Rocq name only
    /// as the consequence, never as the subject. Caught before any artifact is
    /// written so a rejected name leaves no stale `.wasm`/`.v`.
    ///
    /// Deliberately a rejection rather than an auto-escape: the file-qualified
    /// name is emitted verbatim into the proof artifact (`Definition
    /// <module>__<name>_specs`, `Theorem valid_<module>__<name>`), so an escaped
    /// form like `lib_x_1U_S` would make the proof harder to read. Renaming keeps
    /// the proof names legible.
    #[error(
        "spec '{spec_name}'{in_file} has a name that collides with the reserved \
         '__' separator in generated Rocq proof names.\n\
         \n\
         In proof mode each spec is given a file-qualified name so two specs with \
         the same name in different files do not collide in the generated Rocq \
         (.v). That name joins the path segments with '_', then the translator \
         joins it with the module name to form the proof definitions:\n\
         \n    {join_lhs} -> proof name '<module>__{qualified}_specs'\n\
         \n\
         Rocq reserves '__' there as the module/spec separator, so a file-qualified \
         name that begins or ends with '_', or carries a '__' run, fabricates that \
         separator and is rejected. The {offender_kind} '{offender}' (which \
         {offender_cause}) is what creates it.\n\
         \n\
         How to fix: {fix_hint}\n\
         \n\
         note: proof-mode names appear verbatim in your generated .v, so they are \
         renamed rather than escaped into noise.",
        spec_name = .0.spec_name,
        in_file = .0.in_file_clause(),
        join_lhs = .0.join_lhs,
        qualified = .0.qualified,
        offender_kind = .0.offender_kind,
        offender = .0.offender,
        offender_cause = .0.offender_cause,
        fix_hint = .0.fix_hint,
    )]
    SpecNameReservesSeparator(Box<SpecNameSeparatorDetails>),

    /// A spec function's translated `hassert` obligation nests deeper than the
    /// `inference.hspecs` codec's decode-time depth cap. The encoder is
    /// infallible, so an over-deep tree would serialize into a section that the
    /// codec's own hardened decoder — in the linker and the Rocq translator —
    /// rejects as corrupt. Codegen refuses to write such an artifact, naming the
    /// offending spec and function. Realistic specifications never approach the
    /// cap; only a pathologically long statement chain can reach it.
    #[error(
        "the verification obligation for function '{function}' in spec '{spec}' nests deeper \
         than the maximum of {max} the inference.hspecs section supports; \
         simplify the specification"
    )]
    HspecTreeTooDeep {
        spec: String,
        function: String,
        max: usize,
    },

    /// A spec name or a spec-function symbol carried in a `hassert` obligation
    /// falls outside the `inference.hspecs` codec's name-length contract (at
    /// most `max` bytes; a non-empty minimum that source identifiers always
    /// meet). The encoder is infallible, so an out-of-range name would serialize
    /// into a section the codec's own decoder — in the linker and the Rocq
    /// translator — rejects. Codegen refuses to write such an artifact, naming
    /// the offending identifier and its spec. Only a pathologically long
    /// identifier reaches this; realistic names are far shorter.
    #[error(
        "the inference.hspecs name '{name}' in spec '{spec}' is {len} bytes, outside the \
         1..={max} bytes the section permits; shorten the identifier"
    )]
    HspecNameTooLong {
        spec: String,
        name: String,
        len: usize,
        max: usize,
    },

    /// A reachability obligation's metadata (`entry_arity`/`visible_locs`)
    /// falls outside the `inference.hspecs` codec's contract — visible locals
    /// out of strictly ascending order, or a count or slot index past the
    /// section's cap. The encoder is infallible, so such metadata would
    /// serialize into a section the codec's own decoder — in the linker and
    /// the Rocq translator — rejects as corrupt. Codegen refuses to write the
    /// artifact; the detail is the codec's own report. Reaching this indicates
    /// a compiler bug: the metadata is computed, never user-authored.
    #[error(
        "the verification obligation for function '{function}' in spec '{spec}' carries \
         reachability metadata the inference.hspecs section rejects: {detail}"
    )]
    HspecReachMetaInvalid {
        spec: String,
        function: String,
        detail: String,
    },

    /// An `exists`/`unique`-quantified specification function declares a
    /// return type or contains a `return` statement. Its obligation is
    /// discharged by running the compiled body under the verifier's vanilla
    /// reduction, which observes only a body that exits by falling off its
    /// end: a `return` instruction can never take a reduction step there, so
    /// the obligation would be silently unprovable, and a declared compound
    /// result would introduce an sret pointer parameter that shifts every
    /// slot index the obligation's payload depends on. Analysis rules A005
    /// and A007 reject both shapes with source locations when analysis runs;
    /// this error is the defense for pipelines that go straight from type
    /// checking to code generation, so a misaligned or unprovable obligation
    /// can never be emitted silently.
    #[error(
        "spec function '{function}' in spec '{spec}' is '{kind}'-quantified and {offense}; \
         an '{kind}'-quantified specification is proven by running its compiled body, whose \
         only supported exit is falling off the end — make the function void, let the body \
         end by falling through, and state the property with an `assert` inside the body"
    )]
    ReachabilitySpecReturns {
        spec: String,
        function: String,
        kind: &'static str,
        offense: &'static str,
    },

    /// A specification function's declared parameters plus the hidden choice
    /// parameters its `@`s lower to would exceed WebAssembly's implementation
    /// limit on parameter count. Refusing here names the specification function
    /// that overflowed; emitting it produces a module this compiler's own
    /// verification step reports as a malformed binary, blaming the user for a
    /// compiler limit.
    #[error(
        "specification function '{function}' in spec '{spec}' needs {base} leading parameter slot(s) \
         plus {choices} hidden choice parameter(s), which exceeds WebAssembly's limit of {max} \
         parameters per function — each `@` in a specification body becomes one parameter (one \
         per scalar leaf for an array or struct `@`), so draw fewer values or split the property \
         across several specification functions"
    )]
    ChoiceSuffixTooLarge {
        spec: String,
        function: String,
        /// Parameter slots already registered when the choice suffix is
        /// appended. This is the observed frame position, not the source
        /// arity: a method's receiver and a compound return's `sret` pointer
        /// occupy slots here without being declared parameters.
        base: u32,
        choices: usize,
        max: usize,
    },

    /// One or more specification functions could not be translated into a
    /// `hassert` verification obligation: a construct with no assertion
    /// encoding (`loop`, `break`, a `unique` block, `**`, memory access), an
    /// `exists`/`unique`/`assume`-quantified body, a reassignment, a non-scalar
    /// term or `@`, an untranslatable call, or a quantified spec method. In
    /// proof mode the obligation is a required deliverable, so codegen refuses
    /// to emit a module whose specifications are silently unverifiable. The
    /// message is the newline-joined `P0xx` diagnostics, each in the same
    /// `[file:]line:col: error[P00x]: message` shape analysis diagnostics use.
    #[error("{0}")]
    UntranslatableSpec(String),
}

/// The boxed payload of [`CodegenError::SpecNameReservesSeparator`]. Boxed so the
/// diagnostic strings do not enlarge `CodegenError` (and every
/// `Result<_, CodegenError>` it flows through) — the variant is rare and only ever
/// rendered, so the indirection costs nothing on the hot path.
#[derive(Debug)]
pub(crate) struct SpecNameSeparatorDetails {
    /// The source spec name as the user wrote it (`Invariant_`), for the lead line.
    pub(crate) spec_name: String,
    /// The `::`-joined source path of the declaring file (`lib::checks`), or `None`
    /// for the entry file (which has no path prefix). Rendered as ` in file 'X'`.
    pub(crate) file_label: Option<String>,
    /// The `dir / stem / spec` rendering of the join inputs.
    pub(crate) join_lhs: String,
    /// The flattened file-qualified name, shown only as the generated consequence.
    pub(crate) qualified: String,
    /// Whether the offender is a `file stem` or a `spec name`.
    pub(crate) offender_kind: String,
    /// The offending segment text.
    pub(crate) offender: String,
    /// The phrasing of why it offends — "ends with `_`", and so on.
    pub(crate) offender_cause: String,
    /// An imperative fix, e.g. `rename the spec 'Invariant_' to 'Invariant' (drop
    /// the trailing '_')`.
    pub(crate) fix_hint: String,
}

impl SpecNameSeparatorDetails {
    /// The ` in file 'lib::checks'` clause for an imported file, or the empty
    /// string for an entry-file spec (which has no file path to name).
    fn in_file_clause(&self) -> String {
        match &self.file_label {
            Some(label) => format!(" in file '{label}'"),
            None => String::new(),
        }
    }
}
