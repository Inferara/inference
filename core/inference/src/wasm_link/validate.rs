//! Compile-time validation of an `external fn` declaration against the real
//! `.wasm` module that is expected to provide it.
//!
//! After a logical module reference is [resolved](super::resolve), the compiler
//! must confirm two things about the resolved binary before trusting the binding:
//!
//! 1. the named `export_field` is actually an **exported function**, and
//! 2. that function's WASM signature **matches** the lowering of the
//!    `external fn` declaration (parameter and result value types, in order).
//!
//! The two failure modes carry **distinct** error variants
//! ([`ValidateError::ExportNotFound`] vs [`ValidateError::SignatureMismatch`]) so
//! callers can report precisely what went wrong.
//!
//! ## Signature lowering
//!
//! Inference primitive types lower to WASM value types as `wasm-codegen` does:
//! `bool`, `i8`/`u8`, `i16`/`u16`, `i32`/`u32`, arrays, and struct/enum pointers
//! become `i32`; `i64`/`u64` become `i64`; `unit` produces no value. Keeping this
//! in lock-step with codegen is what makes validation meaningful — a mismatch
//! here is a real mismatch at link time.
//!
//! The two lowerings are not yet identical, and what keeps the difference
//! harmless is a rejection elsewhere rather than agreement here. Codegen lowers a
//! `::`-qualified type that resolves to a struct or enum to an `i32` pointer,
//! where this module has no arm for one and reports it unsupported; and codegen
//! errors on a `Custom` name it cannot resolve, where this module lowers any
//! `Custom` to `i32` on sight. Neither divergence is reachable today, because a
//! `::`-qualified type on an `external fn` is rejected by the type checker before
//! validation runs, and an unknown type name is rejected outright. That standoff
//! is what [#425](https://github.com/Inferara/inference/issues/425) tracks: the
//! rejection and these two arms have to move together, since lifting the
//! rejection on its own would admit a declaration this module refuses and codegen
//! accepts.

use inf_wasmparser::{
    CompositeInnerType, Export, ExternalKind, FuncType, Parser, Payload, RecGroup, ValType,
};

use inference_ast::arena::AstArena;
use inference_ast::ids::TypeId;
use inference_ast::nodes::{ArgKind, SimpleTypeKind, TypeNode};

/// Maximum number of exported function names listed in an
/// [`ValidateError::ExportNotFound`] hint before the rest are summarized as a
/// count. Bounds the diagnostic against a module exporting thousands of names.
const MAX_LISTED_EXPORTS: usize = 20;

/// A WASM value type, restricted to the kinds Inference codegen emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmValType {
    I32,
    I64,
}

impl WasmValType {
    fn from_parser(val: ValType) -> Option<Self> {
        match val {
            ValType::I32 => Some(WasmValType::I32),
            ValType::I64 => Some(WasmValType::I64),
            _ => None,
        }
    }
}

impl std::fmt::Display for WasmValType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmValType::I32 => write!(f, "i32"),
            WasmValType::I64 => write!(f, "i64"),
        }
    }
}

/// The lowered WASM signature of an `external fn` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredSignature {
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
}

/// Reason an `external fn` type could not be lowered to a WASM value type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerSignatureError {
    /// A parameter was declared `unit`, which has no WASM value representation.
    UnitParameter,
    /// A type form this lowering does not map to a scalar value type
    /// (e.g. a generic or function type) appeared in the signature.
    UnsupportedType { rendered: String },
}

impl std::fmt::Display for LowerSignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerSignatureError::UnitParameter => {
                write!(f, "`unit` cannot appear as an external function parameter")
            }
            LowerSignatureError::UnsupportedType { rendered } => {
                write!(f, "unsupported type in external function signature: {rendered}")
            }
        }
    }
}

impl std::error::Error for LowerSignatureError {}

/// Lowers an Inference type to its WASM value type, mirroring
/// `wasm-codegen`'s `val_type_from_type_id`. `unit` lowers to `None`
/// (no value); a struct or enum name lowers to an `i32` pointer. The mirror is
/// not yet exact; see the module documentation for where it parts company and
/// why that is currently unobservable.
fn lower_value_type(arena: &AstArena, ty: TypeId) -> Result<Option<WasmValType>, LowerSignatureError> {
    match &arena[ty].kind {
        TypeNode::Simple(SimpleTypeKind::Unit) => Ok(None),
        TypeNode::Simple(
            SimpleTypeKind::Bool
            | SimpleTypeKind::I8
            | SimpleTypeKind::U8
            | SimpleTypeKind::I16
            | SimpleTypeKind::U16
            | SimpleTypeKind::I32
            | SimpleTypeKind::U32,
        )
        | TypeNode::Array { .. }
        // Struct / enum values are i32 pointers into linear memory, matching codegen.
        | TypeNode::Custom(_) => Ok(Some(WasmValType::I32)),
        TypeNode::Simple(SimpleTypeKind::I64 | SimpleTypeKind::U64) => Ok(Some(WasmValType::I64)),
        // What remains: `Generic` and `Function`, which codegen reaches only as a
        // `todo!()`; `QualifiedName`, the dead AST variant codegen also rejects;
        // and `Qualified`, which codegen *does* lower, to an `i32` pointer, once
        // the path resolves to a struct or enum. Refusing the last of these is a
        // divergence the type checker keeps out of reach — see the module
        // documentation. Erroring beats guessing a representation.
        other => Err(LowerSignatureError::UnsupportedType {
            rendered: format!("{other:?}"),
        }),
    }
}

/// An `external fn` declaration, lowered into the WASM parameter space.
///
/// The signature and the write set are produced by **one walk over one argument
/// slice**, which is what makes the parameter indices in `mut_params` mean the
/// same thing as the positions in `signature.params`. Deriving them separately
/// would put two walks in the tree that agree only by inspection, and the whole
/// value of the write set is that its coordinates are the linker's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredExtern {
    /// The declared signature, compared against the resolved library's export.
    pub signature: DeclaredSignature,
    /// WASM parameter indices declared `mut`: the write set the merged body is
    /// held to. Only a named argument can carry `mut`; the unnamed forms occupy
    /// a parameter slot and contribute nothing.
    pub mut_params: Vec<u32>,
    /// Declared parameter names, positionally. `None` for an argument written in
    /// an unnamed form, which the linker quotes to teach the name-it-first fix.
    pub param_names: Vec<Option<String>>,
}

/// Lowers an `external fn`'s declared arguments and return type into WASM value
/// types, together with the write set its `mut` annotations declare.
///
/// Every argument form but `self` occupies one WASM parameter slot, in
/// declaration order, so the index of an argument in `args` is the index of its
/// parameter — the correspondence codegen's `import_param_types` also relies on.
/// `self` is unreachable here: the type checker rejects a receiver on an
/// `external fn`, so no `self` can shift the indices apart. That is asserted
/// rather than assumed, because if it ever became reachable the write set would
/// silently name the wrong parameters.
///
/// # Errors
///
/// Returns [`LowerSignatureError`] if a parameter is `unit` or a type form is
/// not lowerable to a scalar value type. A `unit` return is valid and yields an
/// empty `results` list.
pub fn lower_extern_signature(
    arena: &AstArena,
    args: &[inference_ast::nodes::ArgData],
    returns: Option<TypeId>,
) -> Result<LoweredExtern, LowerSignatureError> {
    let mut params = Vec::with_capacity(args.len());
    let mut mut_params = Vec::new();
    let mut param_names = Vec::with_capacity(args.len());
    for arg in args {
        let (ty, is_mut, name) = match arg.kind {
            ArgKind::Named { ty, is_mut, name } => (ty, is_mut, Some(arena[name].name.clone())),
            // The unnamed forms occupy a real parameter slot but carry no
            // mutability field, and the grammar has no slot for one — so an
            // external that writes through such a parameter cannot express it,
            // and the linker's rejection has to say so.
            ArgKind::Ignored { ty } | ArgKind::TypeOnly(ty) => (ty, false, None),
            // `external fn` declarations have no receiver; the type-checker now
            // rejects a `self` here (H7). Drop it with no param so this validator
            // genuinely agrees with codegen — which also emits no receiver — and
            // a mismatching export is reported as a `SignatureMismatch` rather
            // than silently validating against an extra i32 the call never pushes.
            ArgKind::SelfRef { .. } => {
                debug_assert!(
                    false,
                    "an `external fn` cannot declare a receiver, so no `self` may shift the \
                     parameter indices the write set is phrased in"
                );
                continue;
            }
        };
        match lower_value_type(arena, ty)? {
            Some(val) => {
                if is_mut {
                    mut_params.push(u32::try_from(params.len()).unwrap_or(u32::MAX));
                }
                params.push(val);
                param_names.push(name);
            }
            None => return Err(LowerSignatureError::UnitParameter),
        }
    }

    let results = match returns {
        Some(ty) => lower_value_type(arena, ty)?.into_iter().collect(),
        None => Vec::new(),
    };

    Ok(LoweredExtern {
        signature: DeclaredSignature { params, results },
        mut_params,
        param_names,
    })
}

/// A WASM signature mismatch, rendered for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureMismatch {
    pub declared: DeclaredSignature,
    pub found_params: Vec<WasmValType>,
    pub found_results: Vec<WasmValType>,
}

/// Failure of [`validate_extern`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidateError {
    /// The `.wasm` bytes could not be parsed.
    Parse(String),
    /// No exported **function** named `export_field` exists in the module.
    ExportNotFound {
        export_field: String,
        /// Names of the functions the module *does* export, for a helpful hint.
        available_functions: Vec<String>,
    },
    /// The export exists and is a function, but its signature differs from the
    /// lowered `external fn` declaration.
    SignatureMismatch {
        export_field: String,
        mismatch: SignatureMismatch,
    },
    /// The exported function's signature uses a WASM value type Inference does
    /// not model (e.g. `f64`), so it cannot back an `external fn`.
    UnsupportedExportType { export_field: String },
}

impl std::fmt::Display for ValidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidateError::Parse(msg) => write!(f, "failed to parse external `.wasm`: {msg}"),
            ValidateError::ExportNotFound {
                export_field,
                available_functions,
            } => {
                write!(
                    f,
                    "external module has no exported function `{export_field}`"
                )?;
                if !available_functions.is_empty() {
                    // Cap the hint so an adversarial module exporting thousands of
                    // functions cannot flood stderr; the count covers the rest.
                    let shown = available_functions
                        .iter()
                        .take(MAX_LISTED_EXPORTS)
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", ");
                    write!(f, " (exported functions: {shown}")?;
                    let hidden = available_functions.len().saturating_sub(MAX_LISTED_EXPORTS);
                    if hidden > 0 {
                        write!(f, ", ... and {hidden} more")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            ValidateError::SignatureMismatch {
                export_field,
                mismatch,
            } => {
                write!(
                    f,
                    "signature mismatch for external function `{export_field}`: declared {}, found {}",
                    render_signature(&mismatch.declared.params, &mismatch.declared.results),
                    render_signature(&mismatch.found_params, &mismatch.found_results),
                )
            }
            ValidateError::UnsupportedExportType { export_field } => write!(
                f,
                "exported function `{export_field}` uses a WASM value type Inference does not model"
            ),
        }
    }
}

impl std::error::Error for ValidateError {}

fn render_signature(params: &[WasmValType], results: &[WasmValType]) -> String {
    let p = params
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let r = results
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!("({p}) -> ({r})")
}

/// Validates that `wasm_bytes` exports a function named `export_field` whose
/// signature equals `declared_sig`.
///
/// # Errors
///
/// - [`ValidateError::Parse`] if the bytes are not a valid WASM module.
/// - [`ValidateError::ExportNotFound`] if no exported *function* of that name
///   exists (a non-function export of the same name is treated as "not found").
/// - [`ValidateError::SignatureMismatch`] if the function exists but its
///   parameters or results differ from `declared_sig`.
/// - [`ValidateError::UnsupportedExportType`] if the export uses a value type
///   Inference does not model.
pub fn validate_extern(
    wasm_bytes: &[u8],
    export_field: &str,
    declared_sig: &DeclaredSignature,
) -> Result<(), ValidateError> {
    let module = ParsedModule::parse(wasm_bytes)?;

    let Some(func_index) = module.exported_function_index(export_field) else {
        return Err(ValidateError::ExportNotFound {
            export_field: export_field.to_string(),
            available_functions: module.exported_function_names(),
        });
    };

    let func_type = module
        .function_type(func_index)
        .ok_or_else(|| ValidateError::Parse(format!(
            "export `{export_field}` references function index {func_index} with no type"
        )))?;

    let found_params = to_val_types(func_type.params(), export_field)?;
    let found_results = to_val_types(func_type.results(), export_field)?;

    if found_params == declared_sig.params && found_results == declared_sig.results {
        Ok(())
    } else {
        Err(ValidateError::SignatureMismatch {
            export_field: export_field.to_string(),
            mismatch: SignatureMismatch {
                declared: declared_sig.clone(),
                found_params,
                found_results,
            },
        })
    }
}

fn to_val_types(
    types: &[ValType],
    export_field: &str,
) -> Result<Vec<WasmValType>, ValidateError> {
    types
        .iter()
        .map(|&v| {
            WasmValType::from_parser(v).ok_or_else(|| ValidateError::UnsupportedExportType {
                export_field: export_field.to_string(),
            })
        })
        .collect()
}

/// The subset of a parsed WASM module needed for export-signature validation:
/// the function-type table, the per-function type indices (imports first, then
/// locally-defined functions), and the function exports.
struct ParsedModule {
    /// Types indexed by their position in the module's type section. Non-function
    /// composite types occupy a `None` slot so that every function-section type
    /// index stays aligned with the section it came from.
    types: Vec<Option<FuncType>>,
    /// Type index for each function, ordered by function index. Imported
    /// functions occupy the lowest indices, then locally-defined functions.
    func_type_indices: Vec<u32>,
    /// `export name → function index` for every function export.
    function_exports: Vec<(String, u32)>,
}

impl ParsedModule {
    fn parse(wasm_bytes: &[u8]) -> Result<Self, ValidateError> {
        let mut types = Vec::new();
        let mut func_type_indices = Vec::new();
        let mut function_exports = Vec::new();

        for payload in Parser::new(0).parse_all(wasm_bytes) {
            let payload = payload.map_err(|e| ValidateError::Parse(e.to_string()))?;
            match payload {
                Payload::TypeSection(reader) => {
                    for group in reader {
                        let group = group.map_err(|e| ValidateError::Parse(e.to_string()))?;
                        collect_types(&group, &mut types);
                    }
                }
                Payload::ImportSection(reader) => {
                    for import in reader {
                        let import = import.map_err(|e| ValidateError::Parse(e.to_string()))?;
                        if let inf_wasmparser::TypeRef::Func(type_idx) = import.ty {
                            func_type_indices.push(type_idx);
                        }
                    }
                }
                Payload::FunctionSection(reader) => {
                    for type_idx in reader {
                        let type_idx = type_idx.map_err(|e| ValidateError::Parse(e.to_string()))?;
                        func_type_indices.push(type_idx);
                    }
                }
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export.map_err(|e| ValidateError::Parse(e.to_string()))?;
                        let Export { name, kind, index } = export;
                        if kind == ExternalKind::Func {
                            function_exports.push((name.to_string(), index));
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(ParsedModule {
            types,
            func_type_indices,
            function_exports,
        })
    }

    fn exported_function_index(&self, name: &str) -> Option<u32> {
        self.function_exports
            .iter()
            .find(|(export_name, _)| export_name == name)
            .map(|(_, index)| *index)
    }

    fn exported_function_names(&self) -> Vec<String> {
        self.function_exports
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn function_type(&self, func_index: u32) -> Option<&FuncType> {
        let type_index = *self.func_type_indices.get(func_index as usize)?;
        self.types.get(type_index as usize)?.as_ref()
    }
}

/// Appends each type in a `RecGroup` to `out` in type-section order, keeping a
/// `None` slot for non-function composite types so that function-section type
/// indices remain aligned with the type section they reference.
fn collect_types(group: &RecGroup, out: &mut Vec<Option<FuncType>>) {
    for sub_type in group.types() {
        match &sub_type.composite_type.inner {
            CompositeInnerType::Func(func_type) => out.push(Some(func_type.clone())),
            _ => out.push(None),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for signature lowering, the diagnostic `Display` impls, and the
    //! `f64`-export rejection — the parts the integration suite drives only for
    //! the happy path.

    use super::*;
    use crate::parse;
    use inference_ast::nodes::Def;

    /// Lowers the first `external fn` found in `source` (descending into specs).
    fn lower_first_extern(source: &str) -> Result<DeclaredSignature, LowerSignatureError> {
        let arena = parse(source).expect("source parses");
        let extern_def = arena
            .source_files()
            .flat_map(|file| file.defs.iter().copied())
            .find_map(|def_id| find_extern(&arena, def_id))
            .expect("an external fn");
        let Def::ExternFunction { args, returns, .. } = &arena[extern_def].kind else {
            unreachable!("find_extern only yields externs");
        };
        lower_extern_signature(&arena, args, *returns).map(|lowered| lowered.signature)
    }

    fn find_extern(
        arena: &inference_ast::arena::AstArena,
        def_id: inference_ast::ids::DefId,
    ) -> Option<inference_ast::ids::DefId> {
        match &arena[def_id].kind {
            Def::ExternFunction { .. } => Some(def_id),
            Def::Spec { defs, .. } => defs.iter().find_map(|&inner| find_extern(arena, inner)),
            _ => None,
        }
    }

    #[test]
    fn lowers_scalar_and_pointer_types_to_value_types() {
        // bool/i16/u32/array/struct-name all lower to i32; i64/u64 to i64.
        let sig = lower_first_extern(
            "struct P { x: i32; }\n\
             external fn f(a: bool, b: u32, c: i64, d: [i32; 4], e: P) -> u64;",
        )
        .expect("lowers");
        assert_eq!(
            sig.params,
            vec![
                WasmValType::I32,
                WasmValType::I32,
                WasmValType::I64,
                WasmValType::I32,
                WasmValType::I32,
            ],
            "bool/u32/array/struct lower to i32; i64 stays i64"
        );
        assert_eq!(sig.results, vec![WasmValType::I64]);
    }

    #[test]
    fn unit_return_lowers_to_no_results() {
        // The unit type is written `()`; a unit return produces no WASM result.
        let sig = lower_first_extern("external fn f(a: i32) -> ();").expect("lowers");
        assert_eq!(sig.params, vec![WasmValType::I32]);
        assert!(sig.results.is_empty(), "a unit return yields no result value");
    }

    #[test]
    fn unit_parameter_is_rejected() {
        let err = lower_first_extern("external fn f(a: ()) -> i32;")
            .expect_err("unit parameter must be rejected");
        assert_eq!(err, LowerSignatureError::UnitParameter);
    }

    #[test]
    fn f64_export_is_an_unsupported_export_type() {
        // The module exports a function taking `f64` — a value type Inference does
        // not model — so validation must reject it as unsupported, not as a
        // signature mismatch.
        let mut module = wasm_encoder::Module::new();
        let mut types = wasm_encoder::TypeSection::new();
        types
            .ty()
            .function([wasm_encoder::ValType::F64], [wasm_encoder::ValType::F64]);
        module.section(&types);
        let mut funcs = wasm_encoder::FunctionSection::new();
        funcs.function(0);
        module.section(&funcs);
        let mut exports = wasm_encoder::ExportSection::new();
        exports.export("f", wasm_encoder::ExportKind::Func, 0);
        module.section(&exports);
        let mut code = wasm_encoder::CodeSection::new();
        let mut func = wasm_encoder::Function::new([]);
        func.instruction(&wasm_encoder::Instruction::LocalGet(0));
        func.instruction(&wasm_encoder::Instruction::End);
        code.function(&func);
        module.section(&code);
        let bytes = module.finish();

        let declared = DeclaredSignature {
            params: vec![WasmValType::I64],
            results: vec![WasmValType::I64],
        };
        let err = validate_extern(&bytes, "f", &declared).unwrap_err();
        match err {
            ValidateError::UnsupportedExportType { export_field } => {
                assert_eq!(export_field, "f");
            }
            other => panic!("expected UnsupportedExportType, got {other:?}"),
        }
    }

    #[test]
    fn value_type_display_renders_keywords() {
        assert_eq!(WasmValType::I32.to_string(), "i32");
        assert_eq!(WasmValType::I64.to_string(), "i64");
    }

    #[test]
    fn lower_signature_error_display_is_descriptive() {
        assert!(LowerSignatureError::UnitParameter
            .to_string()
            .contains("unit"));
        assert!(LowerSignatureError::UnsupportedType {
            rendered: "Generic".into(),
        }
        .to_string()
        .contains("Generic"));
    }

    #[test]
    fn export_not_found_display_lists_available_functions() {
        let rendered = ValidateError::ExportNotFound {
            export_field: "product".into(),
            available_functions: vec!["sum".into(), "diff".into()],
        }
        .to_string();
        assert!(rendered.contains("product"), "names the missing export");
        assert!(rendered.contains("sum, diff"), "lists what is available");

        // With nothing exported, the hint is omitted.
        let bare = ValidateError::ExportNotFound {
            export_field: "x".into(),
            available_functions: Vec::new(),
        }
        .to_string();
        assert!(!bare.contains("exported functions:"), "no hint when empty");
    }

    #[test]
    fn export_not_found_caps_the_listed_functions() {
        // L2: an adversarial module exporting thousands of functions must not
        // flood the diagnostic. At most MAX_LISTED_EXPORTS names appear, the rest
        // summarized as a count.
        let available: Vec<String> = (0..1000).map(|i| format!("f{i}")).collect();
        let rendered = ValidateError::ExportNotFound {
            export_field: "target".into(),
            available_functions: available,
        }
        .to_string();

        assert!(rendered.contains("f0"), "lists the first names: {rendered}");
        assert!(
            rendered.contains(&format!("... and {} more", 1000 - MAX_LISTED_EXPORTS)),
            "summarizes the remainder: {rendered}"
        );
        // The last name must NOT appear in full — it is past the cap.
        assert!(!rendered.contains("f999"), "caps the listing: {rendered}");

        // Exactly the cap many names: every name shown, no "more" suffix.
        let exact: Vec<String> = (0..MAX_LISTED_EXPORTS).map(|i| format!("g{i}")).collect();
        let rendered_exact = ValidateError::ExportNotFound {
            export_field: "x".into(),
            available_functions: exact,
        }
        .to_string();
        assert!(
            !rendered_exact.contains("more"),
            "no remainder suffix when nothing is hidden: {rendered_exact}"
        );
    }

    #[test]
    fn signature_mismatch_display_shows_both_signatures() {
        let rendered = ValidateError::SignatureMismatch {
            export_field: "sum".into(),
            mismatch: SignatureMismatch {
                declared: DeclaredSignature {
                    params: vec![WasmValType::I32],
                    results: vec![WasmValType::I32],
                },
                found_params: vec![WasmValType::I32, WasmValType::I32],
                found_results: vec![WasmValType::I64],
            },
        }
        .to_string();
        assert!(rendered.contains("declared (i32) -> (i32)"), "{rendered}");
        assert!(rendered.contains("found (i32, i32) -> (i64)"), "{rendered}");
    }

    #[test]
    fn other_validate_error_displays_render() {
        assert!(ValidateError::Parse("boom".into())
            .to_string()
            .contains("boom"));
        assert!(ValidateError::UnsupportedExportType {
            export_field: "g".into(),
        }
        .to_string()
        .contains('g'));
    }
}
