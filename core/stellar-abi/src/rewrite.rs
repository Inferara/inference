//! The rewrite itself: parse, refuse what the Val ABI cannot carry and what the
//! contract spec cannot record, synthesize one wrapper per exported function,
//! rebuild the module around them, and append the three contract custom
//! sections.
//!
//! # Why the wrappers go at the end
//!
//! Every existing function keeps its index. Nothing is renumbered, so nothing
//! that names a function index has to be remapped — and the module carries two
//! such records that this crate does not decode: `inference.checked`, the list
//! of functions whose arithmetic traps on overflow, and the name section's
//! function names. Appending leaves the first byte-identical and lets the second
//! be extended rather than rewritten. It is also *true*: a wrapper contains no
//! arithmetic that could overflow, so its absence from the guard record is the
//! correct statement rather than a convenient one.
//!
//! # Why an export is matched by name
//!
//! The static-merge linker renumbers functions when it folds an external module
//! in, so a function index recorded at code generation is stale by the time this
//! pass runs. The export section is what survives, and the descriptor is
//! documented as name-keyed for exactly this reason. Matching is therefore total
//! in both directions and fails closed on either mismatch: an exported function
//! with no descriptor would otherwise ship as a method that is reachable and
//! always broken, and a descriptor with no export would ship a contract silently
//! missing a method.

use std::collections::BTreeMap;
use std::ops::Range;

use inference_target_conformance::check_wasm1;
use inference_wasm_codegen::{AbiParam, AbiReturn, ExportSignature};
use wasm_encoder::{Encode, ExportKind, ExportSection, Instruction, Module, RawSection};
use wasmparser::{
    CompositeInnerType, ExternalKind, ImportSectionReader, Parser, Payload, SubType, TypeRef,
    ValType,
};

use crate::error::StellarAbiError;
use crate::meta::{
    ENV_META_SECTION_NAME, STELLAR_ENV_PRE_RELEASE, STELLAR_ENV_PROTOCOL, metadata_payload,
    metadata_section,
};
use crate::spec::{
    CONTRACT_META_SECTION_NAME, CONTRACT_META_TOOLCHAIN_VERSION, MAX_INPUT_NAME_BYTES,
    SPEC_SECTION_NAME, contract_meta_payload, contract_meta_section, function_entry,
    spec_section,
};
use crate::val::{ValReturn, ValScalar, encode, render_return, render_type, unwrap_parameter,
    wrap_return};

/// The most parameters a contract method may take.
///
/// The host checks an invocation's argument count against `MAX_VM_ARGS`, which
/// is 32. A wrapper's single result is counted separately and is never at risk.
pub const MAX_VAL_PARAMETERS: usize = 32;

/// The longest export name the host can turn into a `Symbol`. Exactly 32 is
/// accepted; 33 is not nameable by any caller.
pub const MAX_EXPORT_NAME_BYTES: usize = 32;

/// The prefix the host reserves for itself.
const RESERVED_EXPORT_PREFIX: &str = "__";

/// The custom sections this pass writes, and so the ones an input may not
/// already carry.
const CONTRACT_SECTION_NAMES: [&str; 3] = [
    SPEC_SECTION_NAME,
    CONTRACT_META_SECTION_NAME,
    ENV_META_SECTION_NAME,
];

/// The name of the standard WebAssembly name section.
const NAME_SECTION_NAME: &str = "name";

/// The name-section subsection holding function names.
const FUNCTION_NAMES_SUBSECTION: u8 = 1;

/// The suffix a wrapper's name-section entry carries, so a disassembly tells the
/// wrapper apart from the function whose export name it took.
const WRAPPER_NAME_SUFFIX: &str = "$val";

/// The `functype` prefix byte, which opens every entry of a WebAssembly 1.0 type
/// section.
const FUNC_TYPE_PREFIX: u8 = 0x60;

/// The `i64` value type, the only one a Val-ABI wrapper mentions.
const VAL_TYPE_I64: u8 = 0x7e;

/// Section ids, named where they are spliced rather than passed as literals.
const TYPE_SECTION_ID: u8 = 1;
const FUNCTION_SECTION_ID: u8 = 3;
const CODE_SECTION_ID: u8 = 10;
const CUSTOM_SECTION_ID: u8 = 0;

/// Turns `wasm` into an uploadable Soroban contract.
///
/// See the crate documentation for the algorithm and the refusal set.
///
/// # Errors
///
/// Returns the [`StellarAbiError`] naming the export and the element that made
/// the module inadmissible, the reason it is not a WebAssembly 1.0 module
/// before or after the rewrite, or [`StellarAbiError::ProtocolPredatesSoroban`]
/// when `protocol` is older than the environment Soroban shipped in.
pub fn rewrite(
    wasm: &[u8],
    exports: &[ExportSignature],
    protocol: u32,
) -> Result<Vec<u8>, StellarAbiError> {
    check_protocol(protocol)?;
    check_wasm1(wasm).map_err(|reason| StellarAbiError::InputNotWasm1 { reason })?;

    let module = ParsedModule::parse(wasm)?;
    let wrappers = plan_wrappers(&module, exports)?;
    let rewritten = assemble(wasm, &module, &wrappers, protocol);

    check_wasm1(&rewritten).map_err(|reason| StellarAbiError::RewrittenNotWasm1 { reason })?;
    Ok(rewritten)
}

/// Checks that `protocol` is one a Soroban host could ever have run.
///
/// The metadata section is well formed for any number, so a protocol below the
/// one Soroban arrived in produces an artifact nothing refuses until upload,
/// where it is refused by every network there has ever been. Above the floor
/// there is nothing to check here: a protocol newer than a given ledger's is a
/// deliberate choice about which networks an artifact targets.
fn check_protocol(protocol: u32) -> Result<(), StellarAbiError> {
    if protocol < STELLAR_ENV_PROTOCOL {
        return Err(StellarAbiError::ProtocolPredatesSoroban { protocol });
    }
    Ok(())
}

/// One section of the input, in the order it appeared.
///
/// The four sections the rewrite splices into are placeholders: they are rebuilt
/// from their own bytes plus the new entries, so every pre-existing entry
/// survives verbatim. Everything else is a byte range copied through untouched.
#[derive(Debug)]
enum Piece {
    Raw { id: u8, range: Range<usize> },
    Types,
    Functions,
    Exports,
    Code,
    Names,
}

/// One entry of the input's export section.
#[derive(Debug)]
struct ExportEntry<'a> {
    name: &'a str,
    kind: ExportKind,
    index: u32,
}

/// What the parse pass learned about the module being rewritten.
#[derive(Debug)]
struct ParsedModule<'a> {
    pieces: Vec<Piece>,
    /// The type section's entries, without the leading count, and how many.
    types: Entries<'a>,
    /// The function section's entries, without the leading count, and how many.
    functions: Entries<'a>,
    /// The code section's function bodies, without the leading count.
    code: Entries<'a>,
    /// The name section's whole body, name string included.
    name_section: Option<&'a [u8]>,
    /// How many functions the module imports.
    ///
    /// WebAssembly puts imports first in the function index space, so this is
    /// the base every defined function's index is measured from. It is zero for
    /// every module that gets past [`ParsedModule::parse`], which refuses an
    /// import — but the two places that read it, [`ParsedModule::signature_of`]
    /// and the wrapper index in [`plan_wrappers`], are the only two in this
    /// crate, and both are correct for a nonzero base the day this target admits
    /// host imports.
    imported_functions: u32,
    exports: Vec<ExportEntry<'a>>,
    /// The parameter and result types of every type-section entry, in index
    /// order. Read to check an export against the descriptor that claims to
    /// describe it, and to reuse a wrapper signature the module already has.
    type_shapes: Vec<(Vec<ValType>, Vec<ValType>)>,
    /// The type index of every function, in function-index order.
    function_type_indices: Vec<u32>,
}

impl ParsedModule<'_> {
    /// The index of an existing `(i64 × arity) -> i64` type entry, if the module
    /// already carries one.
    fn val_wrapper_type(&self, arity: usize) -> Option<u32> {
        self.type_shapes
            .iter()
            .position(|(params, results)| {
                params.len() == arity
                    && results.len() == 1
                    && params.iter().chain(results).all(|ty| *ty == ValType::I64)
            })
            .and_then(|index| u32::try_from(index).ok())
    }

    /// The signature of the function at `index`, or `None` when the module does
    /// not describe one there — which includes an imported function, whose type
    /// lives in the import section rather than the function section.
    fn signature_of(&self, index: u32) -> Option<&(Vec<ValType>, Vec<ValType>)> {
        let defined = index.checked_sub(self.imported_functions)?;
        let type_index = *self.function_type_indices.get(defined as usize)?;
        self.type_shapes.get(type_index as usize)
    }
}

/// A section's entries with their count, or an absent section.
#[derive(Debug, Default, Clone, Copy)]
struct Entries<'a> {
    bytes: &'a [u8],
    count: u32,
}

impl<'a> ParsedModule<'a> {
    fn parse(wasm: &'a [u8]) -> Result<Self, StellarAbiError> {
        let mut pieces = Vec::new();
        let mut types = Entries::default();
        let mut functions = Entries::default();
        let mut code = Entries::default();
        let mut name_section = None;
        let mut imported_functions = 0u32;
        let mut exports = Vec::new();
        let mut type_shapes = Vec::new();
        let mut function_type_indices = Vec::new();

        for payload in Parser::new(0).parse_all(wasm) {
            let payload = payload.map_err(|err| StellarAbiError::MalformedModule {
                reason: err.to_string(),
            })?;
            let section = payload.as_section();
            match &payload {
                Payload::ImportSection(reader) => {
                    imported_functions = imported_function_count(reader)?;
                    push_raw(&mut pieces, section);
                }
                Payload::StartSection { func, .. } => {
                    return Err(StellarAbiError::StartSectionPresent { function: *func });
                }
                Payload::TypeSection(reader) => {
                    for group in reader.clone() {
                        let group = group.map_err(|err| StellarAbiError::MalformedModule {
                            reason: err.to_string(),
                        })?;
                        for subtype in group.into_types() {
                            type_shapes.push(function_shape(&subtype));
                        }
                    }
                    types = split_count(wasm, section.as_ref())?;
                    pieces.push(Piece::Types);
                }
                Payload::FunctionSection(reader) => {
                    for type_index in reader.clone() {
                        function_type_indices.push(type_index.map_err(|err| {
                            StellarAbiError::MalformedModule {
                                reason: err.to_string(),
                            }
                        })?);
                    }
                    functions = split_count(wasm, section.as_ref())?;
                    pieces.push(Piece::Functions);
                }
                Payload::ExportSection(reader) => {
                    for export in reader.clone() {
                        let export = export.map_err(|err| StellarAbiError::MalformedModule {
                            reason: err.to_string(),
                        })?;
                        exports.push(ExportEntry {
                            name: export.name,
                            kind: export_kind(export.kind)?,
                            index: export.index,
                        });
                    }
                    pieces.push(Piece::Exports);
                }
                Payload::CodeSectionStart { .. } => {
                    code = split_count(wasm, section.as_ref())?;
                    pieces.push(Piece::Code);
                }
                Payload::CustomSection(reader) => {
                    if CONTRACT_SECTION_NAMES.contains(&reader.name()) {
                        return Err(StellarAbiError::AlreadyAContract {
                            section: reader.name().to_string(),
                        });
                    }
                    if reader.name() == NAME_SECTION_NAME {
                        if name_section.is_some() {
                            return Err(StellarAbiError::MultipleNameSections);
                        }
                        name_section = section.as_ref().map(|(_, range)| &wasm[range.clone()]);
                        pieces.push(Piece::Names);
                    } else {
                        push_raw(&mut pieces, section);
                    }
                }
                _ => push_raw(&mut pieces, section),
            }
        }

        Ok(Self {
            pieces,
            types,
            functions,
            code,
            name_section,
            imported_functions,
            exports,
            type_shapes,
            function_type_indices,
        })
    }
}

/// How many functions the import section declares, or the first import as a
/// refusal — which is what every module carrying one gets today.
///
/// The count and the refusal are one pass so that the count is already right on
/// the day the refusal is relaxed for host functions: WebAssembly puts imports
/// first in the function index space, and this is the base two index
/// computations in this crate are measured from.
fn imported_function_count(reader: &ImportSectionReader<'_>) -> Result<u32, StellarAbiError> {
    let mut functions = 0;
    let mut first: Option<(String, String)> = None;
    for import in reader.clone().into_imports() {
        let import = import.map_err(|err| StellarAbiError::MalformedModule {
            reason: err.to_string(),
        })?;
        if matches!(import.ty, TypeRef::Func(_)) {
            functions += 1;
        }
        if first.is_none() {
            first = Some((import.module.to_string(), import.name.to_string()));
        }
    }
    if let Some((module, name)) = first {
        return Err(StellarAbiError::ImportsUnsupported { module, name });
    }
    Ok(functions)
}

/// Records a section this pass does not touch, by the byte range it occupies.
fn push_raw(pieces: &mut Vec<Piece>, section: Option<(u8, Range<usize>)>) {
    if let Some((id, range)) = section {
        pieces.push(Piece::Raw { id, range });
    }
}

/// The parameters and results of `subtype`, or two empty lists when it is not a
/// function type — which WebAssembly 1.0 validation has already ruled out.
fn function_shape(subtype: &SubType) -> (Vec<ValType>, Vec<ValType>) {
    match &subtype.composite_type.inner {
        CompositeInnerType::Func(func) => (func.params().to_vec(), func.results().to_vec()),
        _ => (Vec::new(), Vec::new()),
    }
}

/// A signature as a reader would write it, for a message that has to show two
/// of them side by side.
fn render_signature(params: &[ValType], results: &[ValType]) -> String {
    let render = |types: &[ValType]| {
        types
            .iter()
            .map(|ty| format!("{ty}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match results {
        [] => format!("({})", render(params)),
        _ => format!("({}) -> {}", render(params), render(results)),
    }
}

/// Splits a section body into its leading entry count and the entries after it.
///
/// The count is what a splice has to rewrite; the entries after it are what a
/// splice copies verbatim, which is how every pre-existing entry keeps its exact
/// bytes.
fn split_count<'a>(
    wasm: &'a [u8],
    section: Option<&(u8, Range<usize>)>,
) -> Result<Entries<'a>, StellarAbiError> {
    let Some((_, range)) = section else {
        return Ok(Entries::default());
    };
    let body = &wasm[range.clone()];
    let mut offset = 0;
    let count = read_u32_leb(body, &mut offset)?;
    Ok(Entries {
        bytes: &body[offset..],
        count,
    })
}

/// Reads one unsigned LEB128 `u32` at `offset`, advancing it.
fn read_u32_leb(bytes: &[u8], offset: &mut usize) -> Result<u32, StellarAbiError> {
    let mut result = 0u32;
    let mut shift = 0u32;
    loop {
        let byte = *bytes
            .get(*offset)
            .ok_or_else(|| StellarAbiError::MalformedModule {
                reason: "a section ended in the middle of a LEB128 value".to_string(),
            })?;
        *offset += 1;
        result |= u32::from(byte & 0x7f)
            .checked_shl(shift)
            .ok_or_else(|| StellarAbiError::MalformedModule {
                reason: "a LEB128 value does not fit in 32 bits".to_string(),
            })?;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
    }
}

/// One wrapper to synthesize, and the method the contract spec describes it as.
///
/// The spec entry is built from these same fields, so it describes exactly what
/// the wrapper marshals: a parameter's scalar here is both the tag its unwrap
/// demands and the type code the spec publishes for it.
#[derive(Debug)]
struct Wrapper {
    /// The contract method name, which the wrapper takes over from the function
    /// it wraps.
    export_name: String,
    /// The function the wrapper calls, in the input's index space, which is also
    /// the output's.
    inner: u32,
    /// The wrapper's own index, past every existing function.
    index: u32,
    /// The type index of its `(i64 × n) -> i64` signature.
    type_index: u32,
    params: Vec<WrapperParam>,
    ret: ValReturn,
}

/// One parameter of a wrapped method.
#[derive(Debug)]
struct WrapperParam {
    /// The name the source declared, which the contract spec records and a
    /// caller passes the argument by.
    name: String,
    /// The scalar the argument's `Val` must hold.
    scalar: ValScalar,
}

/// Checks every exported function against its descriptor and lays out the
/// wrappers, or returns the first refusal.
///
/// The rules run in one order, export by export: the method name, the
/// parameter count, every parameter's type, the return, every parameter's
/// name, and last whether the descriptor matches the module's own signature.
/// Each rule is checked over all parameters before the next begins, so a
/// descriptor breaking two of them is refused for the earlier rule wherever in
/// the list the two parameters sit. The source-level gate in
/// `inference-wasm-codegen` states the same rules in the same order, which is
/// what lets one program earn the same refusal from both.
fn plan_wrappers(
    module: &ParsedModule<'_>,
    exports: &[ExportSignature],
) -> Result<Vec<Wrapper>, StellarAbiError> {
    let exported_functions: Vec<&ExportEntry<'_>> = module
        .exports
        .iter()
        .filter(|entry| entry.kind == ExportKind::Func)
        .collect();
    if exported_functions.is_empty() {
        return Err(StellarAbiError::NoExportedFunctions);
    }

    let mut wrappers: Vec<Wrapper> = Vec::with_capacity(exported_functions.len());
    let mut next_type_index = module.types.count;
    let mut wrapper_types: BTreeMap<usize, u32> = BTreeMap::new();

    for entry in exported_functions {
        let signature = exports
            .iter()
            .find(|candidate| candidate.name == entry.name)
            .ok_or_else(|| StellarAbiError::UnknownExport {
                export: entry.name.to_string(),
            })?;
        check_export_name(entry.name)?;

        if signature.params.len() > MAX_VAL_PARAMETERS {
            return Err(StellarAbiError::TooManyParameters {
                export: entry.name.to_string(),
                count: signature.params.len(),
            });
        }

        let mut scalars = Vec::with_capacity(signature.params.len());
        for (index, param) in signature.params.iter().enumerate() {
            let scalar = ValScalar::from_abi(&param.ty).ok_or_else(|| {
                StellarAbiError::UnsupportedParameter {
                    export: entry.name.to_string(),
                    position: index + 1,
                    ty: render_type(&param.ty),
                }
            })?;
            scalars.push(scalar);
        }

        if let AbiReturn::Sret(ty) = &signature.ret {
            return Err(StellarAbiError::CompoundReturn {
                export: entry.name.to_string(),
                ty: render_type(ty),
            });
        }
        let ret = ValReturn::from_abi(&signature.ret).ok_or_else(|| {
            StellarAbiError::UnsupportedReturn {
                export: entry.name.to_string(),
                ty: render_return(&signature.ret),
            }
        })?;

        let names = check_parameter_names(entry.name, &signature.params)?;
        check_signature_matches(module, entry, &scalars, ret)?;

        let arity = scalars.len();
        let type_index = match module.val_wrapper_type(arity) {
            Some(existing) => existing,
            None => *wrapper_types.entry(arity).or_insert_with(|| {
                let index = next_type_index;
                next_type_index += 1;
                index
            }),
        };

        let index = module
            .imported_functions
            .checked_add(module.functions.count)
            .and_then(|existing| existing.checked_add(u32::try_from(wrappers.len()).ok()?))
            .ok_or_else(|| StellarAbiError::MalformedModule {
                reason: "the module exports more functions than an index can name".to_string(),
            })?;
        let params = names
            .into_iter()
            .zip(scalars)
            .map(|(name, scalar)| WrapperParam { name, scalar })
            .collect();
        wrappers.push(Wrapper {
            export_name: entry.name.to_string(),
            inner: entry.index,
            index,
            type_index,
            params,
            ret,
        });
    }

    for signature in exports {
        if !module
            .exports
            .iter()
            .any(|entry| entry.kind == ExportKind::Func && entry.name == signature.name)
        {
            return Err(StellarAbiError::MissingExport {
                export: signature.name.clone(),
            });
        }
    }

    Ok(wrappers)
}

/// Checks that every parameter has a name the contract spec can record, and
/// returns the names in declaration order.
///
/// Two rules, each over every parameter before the next: a parameter written
/// `_` has no name to record, and a name wider than the spec's input-name field
/// cannot be recorded whole. Truncating it would not do either, because the
/// name is also the `--<name>` flag `stellar contract invoke` passes the
/// argument by, and a caller writes the name the author did.
///
/// An empty name counts as none. No identifier is empty, so only a hand-built
/// descriptor carries one, and this pass is the net for exactly that caller:
/// written out, it would be a zero-length input name, from which the CLI
/// derives the flag `--`. Two parameters sharing a name are admitted:
/// uniqueness is a producer invariant the type checker enforces with
/// `DuplicateParameterName`, so a duplicate reaches this pass only from a
/// hand-built descriptor, and this pass does not yet net it.
fn check_parameter_names(
    export: &str,
    params: &[AbiParam],
) -> Result<Vec<String>, StellarAbiError> {
    let names = params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            param
                .name
                .as_deref()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| StellarAbiError::UnnamedParameter {
                    export: export.to_string(),
                    position: index + 1,
                })
        })
        .collect::<Result<Vec<&str>, _>>()?;

    if let Some((index, name)) = names
        .iter()
        .enumerate()
        .find(|(_, name)| name.len() > MAX_INPUT_NAME_BYTES)
    {
        return Err(StellarAbiError::ParameterNameTooLong {
            export: export.to_string(),
            position: index + 1,
            name: (*name).to_string(),
            len: name.len(),
        });
    }
    Ok(names.into_iter().map(str::to_string).collect())
}

/// Checks that the module's own signature for an export is the one the
/// descriptor describes.
///
/// The two come from the same compilation, so a disagreement is a defect
/// upstream of here rather than anything a user wrote — but it would otherwise
/// surface as a stack-height complaint from the validator, naming an offset and
/// no export. Every admissible scalar lowers to `i32`, so the check is over the
/// shape rather than over the source types.
fn check_signature_matches(
    module: &ParsedModule<'_>,
    entry: &ExportEntry<'_>,
    params: &[ValScalar],
    ret: ValReturn,
) -> Result<(), StellarAbiError> {
    let expected_params = vec![ValType::I32; params.len()];
    let expected_results = match ret {
        ValReturn::Void => Vec::new(),
        ValReturn::Scalar(_) => vec![ValType::I32],
    };
    let found = module.signature_of(entry.index);
    if found.is_some_and(|(found_params, found_results)| {
        *found_params == expected_params && *found_results == expected_results
    }) {
        return Ok(());
    }
    Err(StellarAbiError::DescriptorDisagreesWithModule {
        export: entry.name.to_string(),
        expected: render_signature(&expected_params, &expected_results),
        found: found.map_or_else(
            || "no function".to_string(),
            |(found_params, found_results)| render_signature(found_params, found_results),
        ),
    })
}

/// Checks that `name` is a contract method name the host can reach.
///
/// Every rule here bites at *call* time rather than at upload: a module with an
/// unusable export name uploads cleanly and is broken later, either in the
/// host's `Symbol` constructor — so no caller can even express the name — or in
/// its dispatch, for the reserved prefix. Refusing at build time is what turns a
/// silent, undiagnosable failure into a message.
fn check_export_name(name: &str) -> Result<(), StellarAbiError> {
    if name.is_empty() {
        return Err(StellarAbiError::EmptyExportName);
    }
    if name.len() > MAX_EXPORT_NAME_BYTES {
        return Err(StellarAbiError::ExportNameTooLong {
            export: name.to_string(),
            len: name.len(),
        });
    }
    if name.starts_with(RESERVED_EXPORT_PREFIX) {
        return Err(StellarAbiError::ReservedExportName {
            export: name.to_string(),
        });
    }
    if let Some(offending) = name
        .chars()
        .find(|ch| !ch.is_ascii_alphanumeric() && *ch != '_')
    {
        return Err(StellarAbiError::ExportNameNotASymbol {
            export: name.to_string(),
            offending,
        });
    }
    Ok(())
}

/// Rebuilds the module: every untouched section copied through by byte range,
/// the four spliced sections rebuilt around their own entries, and three custom
/// sections appended — `contractspecv0`, then `contractmetav0`, then the
/// environment metadata section `contractenvmetav0` last.
///
/// The host imposes no order on custom sections; the environment metadata goes
/// last so that every contract still ends with the exact bytes measured before
/// the other two existed.
fn assemble(wasm: &[u8], module: &ParsedModule<'_>, wrappers: &[Wrapper], protocol: u32) -> Vec<u8> {
    let type_body = rebuild_type_section(module, wrappers);
    let function_body = rebuild_function_section(module, wrappers);
    let code_body = rebuild_code_section(module, wrappers);
    let export_section = rebuild_export_section(module, wrappers);
    let name_body = module
        .name_section
        .map(|body| rebuild_name_section(body, wrappers));
    let spec = spec_payload(wrappers);
    let contract_meta = contract_meta_payload(CONTRACT_META_TOOLCHAIN_VERSION);
    let env_meta = metadata_payload(protocol, STELLAR_ENV_PRE_RELEASE);

    let mut out = Module::new();
    for piece in &module.pieces {
        match piece {
            Piece::Raw { id, range } => {
                out.section(&RawSection {
                    id: *id,
                    data: &wasm[range.clone()],
                });
            }
            Piece::Types => {
                out.section(&RawSection {
                    id: TYPE_SECTION_ID,
                    data: &type_body,
                });
            }
            Piece::Functions => {
                out.section(&RawSection {
                    id: FUNCTION_SECTION_ID,
                    data: &function_body,
                });
            }
            Piece::Exports => {
                out.section(&export_section);
            }
            Piece::Code => {
                out.section(&RawSection {
                    id: CODE_SECTION_ID,
                    data: &code_body,
                });
            }
            Piece::Names => {
                if let Some(body) = &name_body {
                    out.section(&RawSection {
                        id: CUSTOM_SECTION_ID,
                        data: body,
                    });
                }
            }
        }
    }
    out.section(&spec_section(&spec));
    out.section(&contract_meta_section(&contract_meta));
    out.section(&metadata_section(&env_meta));
    out.finish()
}

/// The `contractspecv0` body: one function entry per wrapper, in the order the
/// export section lists the methods.
fn spec_payload(wrappers: &[Wrapper]) -> Vec<u8> {
    let mut payload = Vec::new();
    for wrapper in wrappers {
        let inputs: Vec<(&str, ValScalar)> = wrapper
            .params
            .iter()
            .map(|param| (param.name.as_str(), param.scalar))
            .collect();
        payload.extend(function_entry(&wrapper.export_name, &inputs, wrapper.ret));
    }
    payload
}

/// The type section with one `(i64 × n) -> i64` entry appended for every arity
/// the wrappers need and the input did not already carry.
fn rebuild_type_section(module: &ParsedModule<'_>, wrappers: &[Wrapper]) -> Vec<u8> {
    let mut appended: Vec<(u32, usize)> = wrappers
        .iter()
        .filter(|wrapper| wrapper.type_index >= module.types.count)
        .map(|wrapper| (wrapper.type_index, wrapper.params.len()))
        .collect();
    appended.sort_unstable();
    appended.dedup();

    let mut body = Vec::new();
    let total = module.types.count + u32::try_from(appended.len()).unwrap_or(u32::MAX);
    total.encode(&mut body);
    body.extend_from_slice(module.types.bytes);
    for (_, arity) in appended {
        body.push(FUNC_TYPE_PREFIX);
        let params = u32::try_from(arity).unwrap_or(u32::MAX);
        params.encode(&mut body);
        body.extend(std::iter::repeat_n(VAL_TYPE_I64, arity));
        1u32.encode(&mut body);
        body.push(VAL_TYPE_I64);
    }
    body
}

/// The function section with one entry appended per wrapper.
fn rebuild_function_section(module: &ParsedModule<'_>, wrappers: &[Wrapper]) -> Vec<u8> {
    let mut body = Vec::new();
    let total = module.functions.count + wrapper_count(wrappers);
    total.encode(&mut body);
    body.extend_from_slice(module.functions.bytes);
    for wrapper in wrappers {
        wrapper.type_index.encode(&mut body);
    }
    body
}

/// The code section with one body appended per wrapper.
fn rebuild_code_section(module: &ParsedModule<'_>, wrappers: &[Wrapper]) -> Vec<u8> {
    let mut body = Vec::new();
    let total = module.code.count + wrapper_count(wrappers);
    total.encode(&mut body);
    body.extend_from_slice(module.code.bytes);
    for wrapper in wrappers {
        let function = wrapper_body(wrapper);
        let size = u32::try_from(function.len()).unwrap_or(u32::MAX);
        size.encode(&mut body);
        body.extend_from_slice(&function);
    }
    body
}

/// One wrapper's function body: an empty local declaration, the unwraps, the
/// call, the wrap, and the closing `end`.
///
/// No scratch local is declared. Each unwrap reads its parameter twice from the
/// local it already occupies, and the guard between the two reads is an
/// empty-blocktype `if` that consumes only its own condition, so the operands
/// earlier parameters left on the stack are undisturbed.
fn wrapper_body(wrapper: &Wrapper) -> Vec<u8> {
    let mut instructions: Vec<Instruction<'static>> = Vec::new();
    for (local, param) in wrapper.params.iter().enumerate() {
        instructions.extend(unwrap_parameter(
            param.scalar,
            u32::try_from(local).unwrap_or(u32::MAX),
        ));
    }
    instructions.push(Instruction::Call(wrapper.inner));
    instructions.extend(wrap_return(wrapper.ret));
    instructions.push(Instruction::End);

    let mut body = Vec::new();
    0u32.encode(&mut body);
    body.extend_from_slice(&encode(&instructions));
    body
}

/// The export section with every wrapped method pointing at its wrapper.
///
/// Only the target index moves. The name, the kind and their order are the
/// input's, and a non-function export — the linear memory, the shadow-stack
/// global — is re-encoded exactly as it arrived. Retargeting rather than adding
/// is what keeps the module uploadable: two exports sharing a name is an upload
/// refusal, so the wrapped function's own export has to give the name up.
fn rebuild_export_section(module: &ParsedModule<'_>, wrappers: &[Wrapper]) -> ExportSection {
    let mut section = ExportSection::new();
    for entry in &module.exports {
        let wrapper = wrappers
            .iter()
            .find(|wrapper| entry.kind == ExportKind::Func && wrapper.export_name == entry.name);
        let index = wrapper.map_or(entry.index, |wrapper| wrapper.index);
        section.export(entry.name, entry.kind, index);
    }
    section
}

/// The `wasm-encoder` spelling of a `wasmparser` export kind.
///
/// `FuncExact` belongs to a proposal outside WebAssembly 1.0, so validation has
/// already refused any module carrying one; it is reported rather than mapped so
/// that this conversion never has to guess.
fn export_kind(kind: ExternalKind) -> Result<ExportKind, StellarAbiError> {
    match kind {
        ExternalKind::Func => Ok(ExportKind::Func),
        ExternalKind::Table => Ok(ExportKind::Table),
        ExternalKind::Memory => Ok(ExportKind::Memory),
        ExternalKind::Global => Ok(ExportKind::Global),
        ExternalKind::Tag => Ok(ExportKind::Tag),
        ExternalKind::FuncExact => Err(StellarAbiError::MalformedModule {
            reason: "an export names a function with an exact type, which WebAssembly 1.0 has no \
                     encoding for"
                .to_string(),
        }),
    }
}

/// The name section with one function-name entry appended per wrapper.
///
/// Every subsection the rewrite does not understand is copied verbatim, and the
/// function-name map is extended rather than rebuilt. Its entries must ascend by
/// index, which appending satisfies for free: the wrappers hold the highest
/// indices in the module.
fn rebuild_name_section(body: &[u8], wrappers: &[Wrapper]) -> Vec<u8> {
    let mut offset = 0;
    let Ok(name_len) = read_u32_leb(body, &mut offset) else {
        return body.to_vec();
    };
    let header_end = offset + name_len as usize;
    if header_end > body.len() {
        return body.to_vec();
    }

    let mut subsections: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut cursor = header_end;
    while cursor < body.len() {
        let id = body[cursor];
        cursor += 1;
        let Ok(size) = read_u32_leb(body, &mut cursor) else {
            return body.to_vec();
        };
        let end = cursor + size as usize;
        if end > body.len() {
            return body.to_vec();
        }
        subsections.push((id, body[cursor..end].to_vec()));
        cursor = end;
    }

    let appended = wrapper_name_entries(wrappers);
    let added = wrapper_count(wrappers);
    if let Some((_, payload)) = subsections
        .iter_mut()
        .find(|(id, _)| *id == FUNCTION_NAMES_SUBSECTION)
    {
        let mut inner = 0;
        let Ok(count) = read_u32_leb(payload, &mut inner) else {
            return body.to_vec();
        };
        let mut extended = Vec::new();
        (count + added).encode(&mut extended);
        extended.extend_from_slice(&payload[inner..]);
        extended.extend_from_slice(&appended);
        *payload = extended;
    } else {
        let mut payload = Vec::new();
        added.encode(&mut payload);
        payload.extend_from_slice(&appended);
        let position = subsections
            .iter()
            .position(|(id, _)| *id > FUNCTION_NAMES_SUBSECTION)
            .unwrap_or(subsections.len());
        subsections.insert(position, (FUNCTION_NAMES_SUBSECTION, payload));
    }

    let mut rebuilt = Vec::new();
    rebuilt.extend_from_slice(&body[..header_end]);
    for (id, payload) in subsections {
        rebuilt.push(id);
        let Ok(size) = u32::try_from(payload.len()) else {
            return body.to_vec();
        };
        size.encode(&mut rebuilt);
        rebuilt.extend_from_slice(&payload);
    }
    rebuilt
}

/// How many wrappers there are, as a `u32`.
///
/// The count is bounded by the export section, which the module's own encoding
/// already holds in a `u32`, so the saturation can only be reached by a module
/// no decoder produced.
fn wrapper_count(wrappers: &[Wrapper]) -> u32 {
    u32::try_from(wrappers.len()).unwrap_or(u32::MAX)
}

/// The `(index, name)` entries the wrappers contribute to the function-name map.
fn wrapper_name_entries(wrappers: &[Wrapper]) -> Vec<u8> {
    let mut entries = Vec::new();
    for wrapper in wrappers {
        wrapper.index.encode(&mut entries);
        let name = format!("{}{WRAPPER_NAME_SUFFIX}", wrapper.export_name);
        name.as_str().encode(&mut entries);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_wasm_codegen::{AbiParam, AbiType};
    use wasm_encoder::{
        CodeSection, ConstExpr, CustomSection, EntityType, Function, FunctionSection,
        GlobalSection, GlobalType, ImportSection, IndirectNameMap, MemorySection, MemoryType,
        NameMap, NameSection, StartSection, TypeSection, ValType,
    };

    use crate::meta::STELLAR_ENV_PROTOCOL;

    /// A hand-built module, so that every test names exactly the shape it is
    /// about and no test needs the compiler.
    #[derive(Default)]
    struct Fixture {
        types: Vec<(Vec<ValType>, Vec<ValType>)>,
        functions: Vec<u32>,
        bodies: Vec<Vec<Instruction<'static>>>,
        exports: Vec<(&'static str, ExportKind, u32)>,
        imports: Vec<(&'static str, &'static str, u32)>,
        memory: bool,
        start: Option<u32>,
        function_names: Vec<(u32, &'static str)>,
        /// Emit a name section even with no function names, which is the only
        /// way to reach the branch that synthesizes the function-name
        /// subsection instead of extending one.
        module_name: bool,
        /// Local names, whose subsection id is above the function-name
        /// subsection's, so a synthesized one has to be placed before them.
        local_names: Vec<(u32, Vec<(u32, &'static str)>)>,
        checked: Option<Vec<u8>>,
    }

    impl Fixture {
        fn build(&self) -> Vec<u8> {
            let mut module = Module::new();

            let mut types = TypeSection::new();
            for (params, results) in &self.types {
                types
                    .ty()
                    .function(params.iter().copied(), results.iter().copied());
            }
            module.section(&types);

            if !self.imports.is_empty() {
                let mut imports = ImportSection::new();
                for (from, field, type_index) in &self.imports {
                    imports.import(from, field, EntityType::Function(*type_index));
                }
                module.section(&imports);
            }

            let mut functions = FunctionSection::new();
            for type_index in &self.functions {
                functions.function(*type_index);
            }
            module.section(&functions);

            if self.memory {
                let mut memories = MemorySection::new();
                memories.memory(MemoryType {
                    minimum: 1,
                    maximum: Some(1),
                    memory64: false,
                    shared: false,
                    page_size_log2: None,
                });
                module.section(&memories);

                let mut globals = GlobalSection::new();
                globals.global(
                    GlobalType {
                        val_type: ValType::I32,
                        mutable: true,
                        shared: false,
                    },
                    &ConstExpr::i32_const(1024),
                );
                module.section(&globals);
            }

            let mut exports = ExportSection::new();
            for (name, kind, index) in &self.exports {
                exports.export(name, *kind, *index);
            }
            if self.memory {
                exports.export("memory", ExportKind::Memory, 0);
                exports.export("__stack_pointer", ExportKind::Global, 0);
            }
            module.section(&exports);

            if let Some(function_index) = self.start {
                module.section(&StartSection { function_index });
            }

            let mut code = CodeSection::new();
            for body in &self.bodies {
                let mut function = Function::new(Vec::new());
                for instruction in body {
                    function.instruction(instruction);
                }
                code.function(&function);
            }
            module.section(&code);

            if !self.function_names.is_empty() || self.module_name || !self.local_names.is_empty()
            {
                let mut names = NameSection::new();
                names.module("fixture");
                if !self.function_names.is_empty() {
                    let mut map = NameMap::new();
                    for (index, name) in &self.function_names {
                        map.append(*index, name);
                    }
                    names.functions(&map);
                }
                if !self.local_names.is_empty() {
                    let mut locals = IndirectNameMap::new();
                    for (function, entries) in &self.local_names {
                        let mut map = NameMap::new();
                        for (index, name) in entries {
                            map.append(*index, name);
                        }
                        locals.append(*function, &map);
                    }
                    names.locals(&locals);
                }
                module.section(&names);
            }

            if let Some(payload) = &self.checked {
                module.section(&CustomSection {
                    name: "inference.checked".into(),
                    data: payload.as_slice().into(),
                });
            }

            module.finish()
        }
    }

    /// `add(i32, i32) -> i32`, exported, with a linear memory and a shadow-stack
    /// global — the shape every real Inference artifact has.
    fn add_fixture() -> Fixture {
        Fixture {
            types: vec![(vec![ValType::I32, ValType::I32], vec![ValType::I32])],
            functions: vec![0],
            bodies: vec![vec![
                Instruction::LocalGet(0),
                Instruction::LocalGet(1),
                Instruction::I32Add,
                Instruction::End,
            ]],
            exports: vec![("add", ExportKind::Func, 0)],
            memory: true,
            ..Fixture::default()
        }
    }

    fn add_descriptor() -> Vec<ExportSignature> {
        vec![ExportSignature {
            name: "add".to_string(),
            params: vec![
                AbiParam::named("a", AbiType::U32),
                AbiParam::named("b", AbiType::U32),
            ],
            ret: AbiReturn::Scalar(AbiType::U32),
        }]
    }

    /// A descriptor entry whose parameters have the given types, named as
    /// [`named_params`] names them.
    fn signature(name: &str, types: Vec<AbiType>, ret: AbiReturn) -> ExportSignature {
        ExportSignature {
            name: name.to_string(),
            params: named_params(types),
            ret,
        }
    }

    /// Parameters of the given types, named `p0`, `p1`, … in declaration
    /// order, the way a source that names every parameter is described.
    fn named_params(types: Vec<AbiType>) -> Vec<AbiParam> {
        types
            .into_iter()
            .enumerate()
            .map(|(index, ty)| AbiParam::named(format!("p{index}"), ty))
            .collect()
    }

    /// Every section as `(id, custom-section name, body bytes)`, in order. The
    /// name is empty for a non-custom section, whose id already identifies it.
    fn sections(wasm: &[u8]) -> Vec<(u8, String, Vec<u8>)> {
        let mut out = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            let payload = payload.expect("the fixture parses");
            let name = match &payload {
                Payload::CustomSection(reader) => reader.name().to_string(),
                _ => String::new(),
            };
            if let Some((id, range)) = payload.as_section() {
                out.push((id, name, wasm[range].to_vec()));
            }
        }
        out
    }

    fn section_body(wasm: &[u8], id: u8, name: &str) -> Vec<u8> {
        let found = sections(wasm)
            .into_iter()
            .find(|(section_id, section_name, _)| *section_id == id && section_name == name);
        match found {
            Some((_, _, body)) => body,
            None => panic!("section {id} `{name}` is present"),
        }
    }

    /// The exports of `wasm` as `(name, kind, index)`.
    fn exports_of(wasm: &[u8]) -> Vec<(String, ExternalKind, u32)> {
        let mut out = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::ExportSection(reader) = payload.expect("the module parses") {
                for export in reader {
                    let export = export.expect("the export decodes");
                    out.push((export.name.to_string(), export.kind, export.index));
                }
            }
        }
        out
    }

    /// The `(params, results)` of every type-section entry, in index order.
    fn types_of(wasm: &[u8]) -> Vec<(Vec<wasmparser::ValType>, Vec<wasmparser::ValType>)> {
        let mut out = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::TypeSection(reader) = payload.expect("the module parses") {
                for group in reader {
                    for subtype in group.expect("the rec group decodes").into_types() {
                        let wasmparser::CompositeInnerType::Func(func) =
                            &subtype.composite_type.inner
                        else {
                            continue;
                        };
                        out.push((func.params().to_vec(), func.results().to_vec()));
                    }
                }
            }
        }
        out
    }

    /// The type index of every function-section entry, in index order.
    fn function_types_of(wasm: &[u8]) -> Vec<u32> {
        let mut out = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::FunctionSection(reader) = payload.expect("the module parses") {
                for type_index in reader {
                    out.push(type_index.expect("the entry decodes"));
                }
            }
        }
        out
    }

    /// Every function body of `wasm`, as the raw bytes between its size prefix
    /// and the next entry.
    fn bodies_of(wasm: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::CodeSectionEntry(body) = payload.expect("the module parses") {
                let range = body.range();
                out.push(wasm[range].to_vec());
            }
        }
        out
    }

    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn unhex(text: &str) -> Vec<u8> {
        text.split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).expect("a hex byte"))
            .collect()
    }

    /// The measured metadata section, header included: the 32 bytes every
    /// contract ends with.
    const MEASURED_META_SECTION: &str = "00 1e 11 63 6f 6e 74 72 61 63 74 65 6e 76 6d 65 74 61 76 \
                                         30 00 00 00 00 00 00 00 14 00 00 00 00";

    #[test]
    fn an_exported_add_becomes_a_val_method_taking_two_words() {
        let input = add_fixture().build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");

        let exports = exports_of(&output);
        let (name, kind, index) = &exports[0];
        assert_eq!(name, "add");
        assert_eq!(*kind, ExternalKind::Func);
        assert_eq!(*index, 1, "the export names the wrapper, not the inner add");

        let types = types_of(&output);
        let wrapper_type = function_types_of(&output)[1] as usize;
        assert_eq!(
            types[wrapper_type],
            (
                vec![wasmparser::ValType::I64, wasmparser::ValType::I64],
                vec![wasmparser::ValType::I64]
            )
        );
    }

    /// The inner function keeps its index and loses its name: two exports
    /// sharing a name is an upload refusal, so the wrapper displaces the
    /// original export rather than joining it.
    #[test]
    fn the_wrapped_function_is_left_unexported_and_unmoved() {
        let input = add_fixture().build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");

        let exports = exports_of(&output);
        assert_eq!(
            exports
                .iter()
                .filter(|(_, kind, index)| *kind == ExternalKind::Func && *index == 0)
                .count(),
            0,
            "nothing exports the inner function any more"
        );
        assert_eq!(
            exports.iter().filter(|(name, ..)| name == "add").count(),
            1,
            "exactly one export carries the method name"
        );
        assert_eq!(bodies_of(&output)[0], bodies_of(&input)[0]);
    }

    /// The criterion, stated so that it is not self-contradictory: the rewrite
    /// splices into the type, function, code and export sections, so those four
    /// cannot be byte-identical. Everything else must be, and the only new
    /// sections are the three contract custom sections at the end, the
    /// environment metadata one last.
    #[test]
    fn every_section_the_rewrite_does_not_splice_is_byte_identical() {
        let input = add_fixture().build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");

        let spliced = [TYPE_SECTION_ID, FUNCTION_SECTION_ID, 7, CODE_SECTION_ID];
        let before: Vec<_> = sections(&input)
            .into_iter()
            .filter(|(id, ..)| !spliced.contains(id))
            .collect();
        let mut after: Vec<_> = sections(&output)
            .into_iter()
            .filter(|(id, ..)| !spliced.contains(id))
            .collect();

        let appended = after.split_off(after.len() - 3);
        assert_eq!(
            appended
                .iter()
                .map(|(id, name, _)| (*id, name.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (CUSTOM_SECTION_ID, "contractspecv0"),
                (CUSTOM_SECTION_ID, "contractmetav0"),
                (CUSTOM_SECTION_ID, "contractenvmetav0"),
            ]
        );
        assert_eq!(before, after);
    }

    /// And within the four spliced sections, every pre-existing entry survives
    /// verbatim: only the count changes and only new entries follow.
    #[test]
    fn every_pre_existing_entry_of_a_spliced_section_survives_verbatim() {
        let input = add_fixture().build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");

        for id in [TYPE_SECTION_ID, FUNCTION_SECTION_ID, CODE_SECTION_ID] {
            let before = section_body(&input, id, "");
            let after = section_body(&output, id, "");
            let mut before_offset = 0;
            let mut after_offset = 0;
            let before_count = read_u32_leb(&before, &mut before_offset).expect("a count");
            let after_count = read_u32_leb(&after, &mut after_offset).expect("a count");
            assert_eq!(after_count, before_count + 1, "section {id} grew by one");
            assert!(
                after[after_offset..].starts_with(&before[before_offset..]),
                "section {id} kept its entries verbatim"
            );
        }
    }

    /// The export section is the one splice that also rewrites an existing
    /// entry, and it rewrites nothing but the target index: the names, the
    /// kinds and their order are the input's, and the memory and shadow-stack
    /// exports come through untouched.
    #[test]
    fn the_export_section_moves_only_the_targets_of_wrapped_methods() {
        let input = add_fixture().build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");

        let before = exports_of(&input);
        let after = exports_of(&output);
        assert_eq!(before.len(), after.len());
        for (left, right) in before.iter().zip(&after) {
            assert_eq!(left.0, right.0);
            assert_eq!(left.1, right.1);
        }
        assert_eq!(
            after
                .iter()
                .filter(|(name, ..)| name == "memory" || name == "__stack_pointer")
                .map(|(name, kind, index)| (name.clone(), *kind, *index))
                .collect::<Vec<_>>(),
            vec![
                ("memory".to_string(), ExternalKind::Memory, 0),
                ("__stack_pointer".to_string(), ExternalKind::Global, 0),
            ]
        );

        let before_bytes = section_body(&input, 7, "");
        let after_bytes = section_body(&output, 7, "");
        assert_eq!(before_bytes.len(), after_bytes.len());
        let moved: Vec<usize> = before_bytes
            .iter()
            .zip(&after_bytes)
            .enumerate()
            .filter(|(_, (left, right))| left != right)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            moved.len(),
            1,
            "one byte moves: the target index of the one wrapped method"
        );
        assert_eq!(
            (before_bytes[moved[0]], after_bytes[moved[0]]),
            (0, 1),
            "and it moves from the inner function to its wrapper"
        );
    }

    /// The golden for the shape the reference contract uses: two `u32`
    /// parameters and a `u32` result, one measured sequence after another with
    /// no local declarations and no scratch.
    #[test]
    fn the_two_parameter_u32_wrapper_body_is_the_measured_bytes() {
        let input = add_fixture().build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");

        assert_eq!(
            hex(&bodies_of(&output)[1]),
            "00 \
             20 00 42 ff 01 83 42 04 52 04 40 00 0b 20 00 42 20 88 a7 \
             20 01 42 ff 01 83 42 04 52 04 40 00 0b 20 01 42 20 88 a7 \
             10 00 \
             ad 42 20 86 42 04 84 \
             0b"
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
        );
    }

    #[test]
    fn the_i32_wrapper_body_is_the_measured_bytes() {
        let fixture = Fixture {
            types: vec![(vec![ValType::I32], vec![ValType::I32])],
            functions: vec![0],
            bodies: vec![vec![Instruction::LocalGet(0), Instruction::End]],
            exports: vec![("negate", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        let output = rewrite(
            &fixture.build(),
            &[signature(
                "negate",
                vec![AbiType::I32],
                AbiReturn::Scalar(AbiType::I32),
            )],
            STELLAR_ENV_PROTOCOL,
        )
        .expect("rewrites");

        assert_eq!(
            hex(&bodies_of(&output)[1]),
            "00 20 00 42 ff 01 83 42 05 52 04 40 00 0b 20 00 42 20 88 a7 10 00 ad 42 20 86 42 05 \
             84 0b"
        );
    }

    /// The boolean golden, with an inner function returning 42 — a truthy value
    /// that is not exactly one. A bool-identity fixture would pin nothing: its
    /// inner value is already a valid `Val` body, so the normalization could be
    /// dropped and the golden would still describe a contract that works. With
    /// 42 the normalization is the only thing between the wrapper and an
    /// invoke-time refusal.
    #[test]
    fn the_bool_wrapper_body_is_the_measured_bytes_for_a_truthy_value_that_is_not_one() {
        let fixture = Fixture {
            types: vec![(vec![ValType::I32], vec![ValType::I32])],
            functions: vec![0],
            bodies: vec![vec![Instruction::I32Const(42), Instruction::End]],
            exports: vec![("truthy", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        let output = rewrite(
            &fixture.build(),
            &[signature(
                "truthy",
                vec![AbiType::Bool],
                AbiReturn::Scalar(AbiType::Bool),
            )],
            STELLAR_ENV_PROTOCOL,
        )
        .expect("rewrites");

        let body = bodies_of(&output)[1].clone();
        assert_eq!(
            hex(&body),
            "00 20 00 42 ff 01 83 42 01 56 04 40 00 0b 20 00 42 ff 01 83 a7 10 00 41 00 47 ad 0b"
        );
        assert!(
            body.windows(4).any(|window| window == [0x41, 0x00, 0x47, 0xad]),
            "the normalization must sit immediately before the extension"
        );
    }

    /// Void in return position: the inner function has no result and the
    /// wrapper still gives back one word.
    #[test]
    fn the_void_wrapper_body_is_the_measured_bytes() {
        let fixture = Fixture {
            types: vec![(Vec::new(), Vec::new())],
            functions: vec![0],
            bodies: vec![vec![Instruction::End]],
            exports: vec![("tick", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        let output = rewrite(
            &fixture.build(),
            &[signature("tick", Vec::new(), AbiReturn::Unit)],
            STELLAR_ENV_PROTOCOL,
        )
        .expect("rewrites");

        assert_eq!(hex(&bodies_of(&output)[1]), "00 10 00 42 02 0b");
    }

    /// A wrapper whose parameters are one of each admissible type, so that the
    /// three unwraps are pinned together in one body and in the order the
    /// source declared them.
    #[test]
    fn a_mixed_wrapper_unwraps_each_parameter_from_its_own_local() {
        let fixture = Fixture {
            types: vec![(
                vec![ValType::I32, ValType::I32, ValType::I32],
                vec![ValType::I32],
            )],
            functions: vec![0],
            bodies: vec![vec![Instruction::LocalGet(0), Instruction::End]],
            exports: vec![("mixed", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        let output = rewrite(
            &fixture.build(),
            &[signature(
                "mixed",
                vec![AbiType::U32, AbiType::Bool, AbiType::I32],
                AbiReturn::Scalar(AbiType::U32),
            )],
            STELLAR_ENV_PROTOCOL,
        )
        .expect("rewrites");

        assert_eq!(
            hex(&bodies_of(&output)[1]),
            "00 \
             20 00 42 ff 01 83 42 04 52 04 40 00 0b 20 00 42 20 88 a7 \
             20 01 42 ff 01 83 42 01 56 04 40 00 0b 20 01 42 ff 01 83 a7 \
             20 02 42 ff 01 83 42 05 52 04 40 00 0b 20 02 42 20 88 a7 \
             10 00 \
             ad 42 20 86 42 04 84 \
             0b"
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
        );
    }

    #[test]
    fn the_module_ends_with_the_measured_metadata_section() {
        let input = add_fixture().build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");

        let expected = unhex(MEASURED_META_SECTION);
        assert_eq!(expected.len(), 32);
        assert_eq!(output[output.len() - expected.len()..], expected[..]);
        assert_eq!(
            section_body(&output, CUSTOM_SECTION_ID, "contractenvmetav0")[18..],
            [0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0]
        );
    }

    /// The names of every custom section of `wasm`, in order.
    fn custom_section_names(wasm: &[u8]) -> Vec<String> {
        sections(wasm)
            .into_iter()
            .filter(|(id, ..)| *id == CUSTOM_SECTION_ID)
            .map(|(_, name, _)| name)
            .collect()
    }

    /// The three sections a contract carries go after every section the input
    /// had, the environment metadata one last. A name section and the
    /// overflow-guard record are in the fixture so that "after every section
    /// the input had" is measured against custom sections too.
    #[test]
    fn the_contract_sections_are_appended_spec_then_meta_then_environment_metadata() {
        let mut fixture = add_fixture();
        fixture.function_names = vec![(0, "add")];
        fixture.checked = Some(vec![1, 1, 0]);
        let output = rewrite(&fixture.build(), &add_descriptor(), STELLAR_ENV_PROTOCOL)
            .expect("rewrites");

        assert_eq!(
            custom_section_names(&output),
            vec![
                "name",
                "inference.checked",
                SPEC_SECTION_NAME,
                CONTRACT_META_SECTION_NAME,
                "contractenvmetav0",
            ]
        );
    }

    /// The reference contract's method, described by the sixty bytes derived
    /// by hand from the XDR definitions, reached through the whole rewrite.
    #[test]
    fn the_spec_section_describes_add_as_the_sixty_derived_bytes() {
        let output = rewrite(&add_fixture().build(), &add_descriptor(), STELLAR_ENV_PROTOCOL)
            .expect("rewrites");
        let spec = section_body(&output, CUSTOM_SECTION_ID, "contractspecv0");
        assert_eq!(
            hex(&spec[15..]),
            "00 00 00 00 00 00 00 00 00 00 00 03 61 64 64 00 00 00 00 02 \
             00 00 00 00 00 00 00 01 61 00 00 00 00 00 00 04 \
             00 00 00 00 00 00 00 01 62 00 00 00 00 00 00 04 \
             00 00 00 01 00 00 00 04"
        );
        assert_eq!(&spec[..15], b"\x0econtractspecv0", "the body follows the section name");
    }

    /// A method returning nothing is described with an empty outputs vector.
    /// Its wrapper still hands back a `Void` word — the Val ABI and the spec say
    /// "nothing" in different ways, and the spec's way is no output at all.
    #[test]
    fn a_method_returning_nothing_is_described_with_no_outputs() {
        let fixture = Fixture {
            types: vec![(Vec::new(), Vec::new())],
            functions: vec![0],
            bodies: vec![vec![Instruction::End]],
            exports: vec![("tick", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        let output = rewrite(
            &fixture.build(),
            &[signature("tick", Vec::new(), AbiReturn::Unit)],
            STELLAR_ENV_PROTOCOL,
        )
        .expect("rewrites");
        assert_eq!(
            hex(&section_body(&output, CUSTOM_SECTION_ID, "contractspecv0")[15..]),
            "00 00 00 00 00 00 00 00 00 00 00 04 74 69 63 6b 00 00 00 00 00 00 00 00"
        );
    }

    /// Two methods, two entries, in the order the export section lists them —
    /// not the order the descriptor happens to — and nothing between them: the
    /// section body is their concatenation. `one(p0: i32) -> i32` is 44 bytes
    /// and `two(p0: bool)` 40, each written out as derived from the XDR rules.
    #[test]
    fn the_spec_section_lists_every_method_in_export_order() {
        let fixture = Fixture {
            types: vec![
                (vec![ValType::I32], vec![ValType::I32]),
                (vec![ValType::I32], Vec::new()),
            ],
            functions: vec![0, 1],
            bodies: vec![
                vec![Instruction::LocalGet(0), Instruction::End],
                vec![Instruction::End],
            ],
            exports: vec![("one", ExportKind::Func, 0), ("two", ExportKind::Func, 1)],
            ..Fixture::default()
        };
        let output = rewrite(
            &fixture.build(),
            &[
                signature("two", vec![AbiType::Bool], AbiReturn::Unit),
                signature("one", vec![AbiType::I32], AbiReturn::Scalar(AbiType::I32)),
            ],
            STELLAR_ENV_PROTOCOL,
        )
        .expect("rewrites");

        assert_eq!(
            hex(&section_body(&output, CUSTOM_SECTION_ID, "contractspecv0")[15..]),
            "00 00 00 00 00 00 00 00 00 00 00 03 6f 6e 65 00 00 00 00 01 \
             00 00 00 00 00 00 00 02 70 30 00 00 00 00 00 05 \
             00 00 00 01 00 00 00 05 \
             00 00 00 00 00 00 00 00 00 00 00 03 74 77 6f 00 00 00 00 01 \
             00 00 00 00 00 00 00 02 70 30 00 00 00 00 00 01 \
             00 00 00 00"
        );
    }

    /// A parameter name reaches the spec exactly as the descriptor spells it,
    /// a leading underscore included.
    #[test]
    fn a_parameter_name_reaches_the_spec_as_spelled() {
        let output = rewrite_one(
            "f",
            vec![AbiParam::named("_amount", AbiType::U32)],
            AbiReturn::Scalar(AbiType::U32),
        )
        .expect("rewrites");
        let spec = section_body(&output, CUSTOM_SECTION_ID, "contractspecv0");
        assert!(
            spec.windows(12)
                .any(|window| window == b"\0\0\0\x07_amount\0"),
            "{}",
            hex(&spec)
        );
    }

    /// The meta section's one entry: kind zero, the key `infver`, and the
    /// toolchain's own version — read back from the bytes rather than compared
    /// with a pinned version, so a version bump moves nothing here.
    #[test]
    fn the_contract_meta_section_carries_the_toolchain_version() {
        let output = rewrite(&add_fixture().build(), &add_descriptor(), STELLAR_ENV_PROTOCOL)
            .expect("rewrites");
        let body = section_body(&output, CUSTOM_SECTION_ID, "contractmetav0");
        let entry = &body[15..];

        assert_eq!(entry[..4], [0, 0, 0, 0], "SC_META_V0");
        assert_eq!(&entry[4..16], b"\0\0\0\x06infver\0\0");
        let len = u32::from_be_bytes(entry[16..20].try_into().expect("four bytes")) as usize;
        assert_eq!(
            std::str::from_utf8(&entry[20..20 + len]),
            Ok(CONTRACT_META_TOOLCHAIN_VERSION)
        );
        assert!(entry[20 + len..].iter().all(|byte| *byte == 0));
        assert_eq!(entry.len() % 4, 0);
    }

    /// A one-method module whose function takes one `i32` per described
    /// parameter and returns an `i32`, rewritten against `params` and `ret`.
    fn rewrite_one(
        name: &'static str,
        params: Vec<AbiParam>,
        ret: AbiReturn,
    ) -> Result<Vec<u8>, StellarAbiError> {
        let fixture = Fixture {
            types: vec![(vec![ValType::I32; params.len()], vec![ValType::I32])],
            functions: vec![0],
            bodies: vec![vec![Instruction::I32Const(0), Instruction::End]],
            exports: vec![(name, ExportKind::Func, 0)],
            ..Fixture::default()
        };
        rewrite(
            &fixture.build(),
            &[ExportSignature {
                name: name.to_string(),
                params,
                ret,
            }],
            STELLAR_ENV_PROTOCOL,
        )
    }

    /// Thirty bytes is the width of the spec's input-name field and is
    /// accepted — and written whole; thirty-one is refused, naming the
    /// parameter by its position and the length it has.
    #[test]
    fn a_parameter_name_of_thirty_bytes_is_accepted_and_thirty_one_is_not() {
        assert_eq!(MAX_INPUT_NAME_BYTES, 30, "the XDR bound, `string name<30>`");

        let widest = "n".repeat(30);
        let output = rewrite_one(
            "f",
            vec![AbiParam::named(widest.clone(), AbiType::U32)],
            AbiReturn::Scalar(AbiType::U32),
        )
        .expect("a 30-byte name is accepted");
        let mut recorded = vec![0, 0, 0, 30];
        recorded.extend_from_slice(widest.as_bytes());
        recorded.extend_from_slice(&[0, 0, 0, 0, 0, 4]);
        assert!(
            section_body(&output, CUSTOM_SECTION_ID, "contractspecv0")
                .windows(recorded.len())
                .any(|window| window == recorded),
            "the 30-byte name is written whole, padded to 32, then its type code"
        );

        let too_wide = "n".repeat(31);
        assert_eq!(
            rewrite_one(
                "f",
                vec![
                    AbiParam::named("a", AbiType::U32),
                    AbiParam::named(too_wide.clone(), AbiType::U32),
                ],
                AbiReturn::Scalar(AbiType::U32),
            ),
            Err(StellarAbiError::ParameterNameTooLong {
                export: "f".to_string(),
                position: 2,
                name: too_wide,
                len: 31,
            })
        );
    }

    #[test]
    fn an_unnamed_parameter_is_refused_by_position() {
        assert_eq!(
            rewrite_one(
                "f",
                vec![
                    AbiParam::named("a", AbiType::U32),
                    AbiParam::unnamed(AbiType::Bool),
                ],
                AbiReturn::Scalar(AbiType::U32),
            ),
            Err(StellarAbiError::UnnamedParameter {
                export: "f".to_string(),
                position: 2,
            })
        );
    }

    /// An empty name is no name. Only a hand-built descriptor can carry one,
    /// and written out it would give a caller the flag `--`.
    #[test]
    fn an_empty_parameter_name_is_refused_as_unnamed() {
        assert_eq!(
            rewrite_one(
                "f",
                vec![
                    AbiParam::named("a", AbiType::U32),
                    AbiParam::named("", AbiType::U32),
                ],
                AbiReturn::Scalar(AbiType::U32),
            ),
            Err(StellarAbiError::UnnamedParameter {
                export: "f".to_string(),
                position: 2,
            })
        );
    }

    /// A one-method descriptor that breaks two rules, and the refusal for the
    /// earlier of them.
    type TwoRuleCase = (&'static str, Vec<AbiParam>, AbiReturn, StellarAbiError);

    /// Rewrites each case's descriptor through [`rewrite_one`] and requires the
    /// refusal the case names.
    fn assert_each_is_refused_for_the_earlier_rule(cases: Vec<TwoRuleCase>) {
        for (name, params, ret, expected) in cases {
            let described = format!("{params:?} -> {ret:?}");
            assert_eq!(rewrite_one(name, params, ret), Err(expected), "{name}: {described}");
        }
    }

    /// The four rules that run before any parameter's name — the method name,
    /// the parameter count, every parameter's type, the return — each row
    /// breaking two that are adjacent in that order. With the rows of the test
    /// below, every two rules adjacent in the whole order share a row, so a
    /// check moved earlier or later changes which of the two some row reports.
    #[test]
    fn the_method_name_arity_types_and_return_are_checked_in_that_order() {
        let mut last_is_u64 = vec![AbiType::U32; 33];
        last_is_u64[32] = AbiType::U64;
        assert_each_is_refused_for_the_earlier_rule(vec![
            (
                "__reserved",
                named_params(vec![AbiType::U32; 33]),
                AbiReturn::Scalar(AbiType::U32),
                StellarAbiError::ReservedExportName {
                    export: "__reserved".to_string(),
                },
            ),
            (
                "wide",
                named_params(last_is_u64),
                AbiReturn::Scalar(AbiType::U32),
                StellarAbiError::TooManyParameters {
                    export: "wide".to_string(),
                    count: 33,
                },
            ),
            (
                "f",
                named_params(vec![AbiType::U64]),
                AbiReturn::Scalar(AbiType::U64),
                StellarAbiError::UnsupportedParameter {
                    export: "f".to_string(),
                    position: 1,
                    ty: "u64".to_string(),
                },
            ),
        ]);
    }

    /// Descriptors that break two rules at once, each refused for the earlier
    /// rule: the method name, the parameter count, every parameter's type, the
    /// return, every parameter's name — `_` before length — and the module
    /// signature last. Each row here pairs a parameter-name rule with one of
    /// the rules around it, the signature with each of the two, so neither name
    /// rule can move past it; the test above pairs the four that run before
    /// them.
    #[test]
    fn the_refusal_order_is_name_arity_types_return_parameter_names_then_signature() {
        let too_wide = "n".repeat(31);
        assert_each_is_refused_for_the_earlier_rule(vec![
            (
                "__reserved",
                vec![AbiParam::unnamed(AbiType::U32)],
                AbiReturn::Scalar(AbiType::U32),
                StellarAbiError::ReservedExportName {
                    export: "__reserved".to_string(),
                },
            ),
            (
                "wide",
                vec![AbiParam::unnamed(AbiType::U32); 33],
                AbiReturn::Scalar(AbiType::U32),
                StellarAbiError::TooManyParameters {
                    export: "wide".to_string(),
                    count: 33,
                },
            ),
            (
                "f",
                vec![
                    AbiParam::unnamed(AbiType::U32),
                    AbiParam::named("amount", AbiType::U64),
                ],
                AbiReturn::Scalar(AbiType::U32),
                StellarAbiError::UnsupportedParameter {
                    export: "f".to_string(),
                    position: 2,
                    ty: "u64".to_string(),
                },
            ),
            (
                "f",
                vec![AbiParam::unnamed(AbiType::U32)],
                AbiReturn::Scalar(AbiType::U64),
                StellarAbiError::UnsupportedReturn {
                    export: "f".to_string(),
                    ty: "u64".to_string(),
                },
            ),
            (
                "f",
                vec![
                    AbiParam::unnamed(AbiType::U32),
                    AbiParam::named(too_wide.clone(), AbiType::U32),
                ],
                AbiReturn::Scalar(AbiType::U32),
                StellarAbiError::UnnamedParameter {
                    export: "f".to_string(),
                    position: 1,
                },
            ),
            (
                "f",
                vec![
                    AbiParam::named(too_wide.clone(), AbiType::U32),
                    AbiParam::unnamed(AbiType::U32),
                ],
                AbiReturn::Scalar(AbiType::U32),
                StellarAbiError::UnnamedParameter {
                    export: "f".to_string(),
                    position: 2,
                },
            ),
            (
                "f",
                vec![AbiParam::unnamed(AbiType::U32)],
                AbiReturn::Unit,
                StellarAbiError::UnnamedParameter {
                    export: "f".to_string(),
                    position: 1,
                },
            ),
            (
                "f",
                vec![AbiParam::named(too_wide.clone(), AbiType::U32)],
                AbiReturn::Unit,
                StellarAbiError::ParameterNameTooLong {
                    export: "f".to_string(),
                    position: 1,
                    name: too_wide,
                    len: 31,
                },
            ),
        ]);
    }

    /// The overflow-guard record is a list of raw function indices this crate
    /// does not decode. Appending the wrappers past every existing function is
    /// what lets it come through untouched — and keeps it true, since a wrapper
    /// holds no guardable arithmetic.
    #[test]
    fn the_overflow_guard_record_is_byte_identical() {
        let mut fixture = add_fixture();
        fixture.checked = Some(vec![1, 1, 0]);
        let input = fixture.build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");

        assert_eq!(
            section_body(&input, CUSTOM_SECTION_ID, "inference.checked"),
            section_body(&output, CUSTOM_SECTION_ID, "inference.checked")
        );
    }

    /// Three functions, one of them exported and not the last: the wrapper goes
    /// after all three, every existing body keeps its bytes and its index, and
    /// the wrapper calls the function the export named.
    #[test]
    fn wrappers_are_appended_so_no_existing_function_index_moves() {
        let fixture = Fixture {
            types: vec![(vec![ValType::I32], vec![ValType::I32])],
            functions: vec![0, 0, 0],
            bodies: vec![
                vec![Instruction::I32Const(1), Instruction::End],
                vec![Instruction::I32Const(2), Instruction::End],
                vec![Instruction::I32Const(3), Instruction::End],
            ],
            exports: vec![("middle", ExportKind::Func, 1)],
            ..Fixture::default()
        };
        let input = fixture.build();
        let output = rewrite(
            &input,
            &[signature(
                "middle",
                vec![AbiType::U32],
                AbiReturn::Scalar(AbiType::U32),
            )],
            STELLAR_ENV_PROTOCOL,
        )
        .expect("rewrites");

        let before = bodies_of(&input);
        let after = bodies_of(&output);
        assert_eq!(after.len(), 4);
        assert_eq!(before, after[..3]);
        assert_eq!(exports_of(&output)[0].2, 3);
        assert!(
            after[3].windows(2).any(|window| window == [0x10, 0x01]),
            "the wrapper calls function 1, the function the export named"
        );
    }

    /// Two methods of the same arity share one type entry, and a wrapper type
    /// the input already carries is reused rather than duplicated.
    #[test]
    fn wrapper_types_are_deduplicated_against_each_other_and_the_input() {
        let fixture = Fixture {
            types: vec![
                (vec![ValType::I32], vec![ValType::I32]),
                (vec![ValType::I64], vec![ValType::I64]),
            ],
            functions: vec![0, 0],
            bodies: vec![
                vec![Instruction::LocalGet(0), Instruction::End],
                vec![Instruction::LocalGet(0), Instruction::End],
            ],
            exports: vec![("one", ExportKind::Func, 0), ("two", ExportKind::Func, 1)],
            ..Fixture::default()
        };
        let input = fixture.build();
        let output = rewrite(
            &input,
            &[
                signature("one", vec![AbiType::U32], AbiReturn::Scalar(AbiType::U32)),
                signature("two", vec![AbiType::I32], AbiReturn::Scalar(AbiType::I32)),
            ],
            STELLAR_ENV_PROTOCOL,
        )
        .expect("rewrites");

        assert_eq!(
            types_of(&output).len(),
            2,
            "the input's `(i64) -> i64` is the wrapper type both methods need"
        );
        let function_types = function_types_of(&output);
        assert_eq!(function_types[2], 1);
        assert_eq!(function_types[3], 1);
    }

    /// A wrapper arity the input does not carry is appended, once, however many
    /// methods share it.
    #[test]
    fn one_type_entry_is_appended_per_new_arity() {
        let fixture = Fixture {
            types: vec![
                (vec![ValType::I32], vec![ValType::I32]),
                (vec![ValType::I32], Vec::new()),
            ],
            functions: vec![0, 1],
            bodies: vec![
                vec![Instruction::LocalGet(0), Instruction::End],
                vec![Instruction::End],
            ],
            exports: vec![("one", ExportKind::Func, 0), ("two", ExportKind::Func, 1)],
            ..Fixture::default()
        };
        let output = rewrite(
            &fixture.build(),
            &[
                signature("one", vec![AbiType::U32], AbiReturn::Scalar(AbiType::U32)),
                signature("two", vec![AbiType::Bool], AbiReturn::Unit),
            ],
            STELLAR_ENV_PROTOCOL,
        )
        .expect("rewrites");

        let types = types_of(&output);
        assert_eq!(types.len(), 3, "both methods share one appended arity");
        assert_eq!(
            types[2],
            (
                vec![wasmparser::ValType::I64],
                vec![wasmparser::ValType::I64]
            )
        );
        assert_eq!(function_types_of(&output), vec![0, 1, 2, 2]);
    }

    /// A descriptor that does not describe the function it names is a defect
    /// upstream of this pass. Caught here it names the export; left to the
    /// validator it would surface as a stack-height complaint at a byte offset.
    #[test]
    fn a_descriptor_that_disagrees_with_the_module_is_refused() {
        let fixture = Fixture {
            types: vec![(vec![ValType::I32], vec![ValType::I32])],
            functions: vec![0],
            bodies: vec![vec![Instruction::LocalGet(0), Instruction::End]],
            exports: vec![("one", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        assert_eq!(
            rewrite(
                &fixture.build(),
                &[signature("one", vec![AbiType::U32], AbiReturn::Unit)],
                STELLAR_ENV_PROTOCOL,
            ),
            Err(StellarAbiError::DescriptorDisagreesWithModule {
                export: "one".to_string(),
                expected: "(i32)".to_string(),
                found: "(i32) -> i32".to_string(),
            })
        );
    }

    /// The name section gains one entry per wrapper and keeps everything else,
    /// including the subsections this pass does not decode.
    #[test]
    fn the_name_section_gains_one_entry_per_wrapper_and_keeps_the_rest() {
        let mut fixture = add_fixture();
        fixture.function_names = vec![(0, "add")];
        let input = fixture.build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");

        let before = section_body(&input, CUSTOM_SECTION_ID, "name");
        let after = section_body(&output, CUSTOM_SECTION_ID, "name");
        assert!(after.len() > before.len());
        assert_eq!(&after[..12], &before[..12], "the module-name subsection");
        assert!(
            after.windows(7).any(|window| window == b"add$val"),
            "the wrapper is named after the method it carries"
        );

        let names = function_names_of(&output);
        assert_eq!(names, vec![(0, "add".to_string()), (1, "add$val".to_string())]);
    }

    /// A module with no name section does not grow one.
    #[test]
    fn a_module_without_a_name_section_does_not_gain_one() {
        let input = add_fixture().build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");
        assert!(
            !sections(&output)
                .iter()
                .any(|(_, name, _)| name == "name")
        );
    }

    fn function_names_of(wasm: &[u8]) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            let payload = payload.expect("the module parses");
            let Payload::CustomSection(reader) = &payload else {
                continue;
            };
            if reader.name() != "name" {
                continue;
            }
            let names = wasmparser::NameSectionReader::new(wasmparser::BinaryReader::new(
                reader.data(),
                reader.data_offset(),
            ));
            for subsection in names {
                if let wasmparser::Name::Function(map) = subsection.expect("a subsection decodes") {
                    for naming in map {
                        let naming = naming.expect("a naming decodes");
                        out.push((naming.index, naming.name.to_string()));
                    }
                }
            }
        }
        out
    }

    /// The subsection ids of the module's name section, in the order they are
    /// encoded. They must ascend, and nothing validates a custom section, so a
    /// mis-ordered one ships without complaint.
    fn name_subsection_ids(wasm: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            let payload = payload.expect("the module parses");
            let Payload::CustomSection(reader) = &payload else {
                continue;
            };
            if reader.name() != NAME_SECTION_NAME {
                continue;
            }
            let data = reader.data();
            let mut cursor = 0;
            while cursor < data.len() {
                out.push(data[cursor]);
                cursor += 1;
                let size = read_u32_leb(data, &mut cursor).expect("a subsection size");
                cursor += size as usize;
            }
        }
        out
    }

    /// A name section carrying no function names at all: the rebuild has to
    /// synthesize the subsection rather than extend one.
    #[test]
    fn a_name_section_without_function_names_gains_the_subsection() {
        let mut fixture = add_fixture();
        fixture.module_name = true;
        let input = fixture.build();
        assert_eq!(
            name_subsection_ids(&input),
            vec![0],
            "the fixture carries a module name and nothing else"
        );

        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");
        assert_eq!(name_subsection_ids(&output), vec![0, FUNCTION_NAMES_SUBSECTION]);
        assert_eq!(function_names_of(&output), vec![(1, "add$val".to_string())]);
    }

    /// And the synthesized subsection goes before the local names, whose id is
    /// higher. Out of order it is still a custom section, so nothing downstream
    /// would reject it; a disassembler would simply stop reading names.
    #[test]
    fn a_synthesized_function_name_subsection_precedes_the_local_names() {
        let mut fixture = add_fixture();
        fixture.local_names = vec![(0, vec![(0, "left"), (1, "right")])];
        let input = fixture.build();
        assert_eq!(name_subsection_ids(&input), vec![0, 2]);

        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");
        assert_eq!(
            name_subsection_ids(&output),
            vec![0, FUNCTION_NAMES_SUBSECTION, 2],
            "the function names go between the module name and the local names"
        );
        assert_eq!(function_names_of(&output), vec![(1, "add$val".to_string())]);
    }

    /// WebAssembly permits repeated custom sections and no validator rejects a
    /// second name section, so a module carrying two is a real input. This pass
    /// rebuilds exactly one function-name map, so it refuses rather than
    /// emitting the rebuilt body at both positions.
    #[test]
    fn a_module_with_two_name_sections_is_refused() {
        let mut fixture = add_fixture();
        fixture.function_names = vec![(0, "add")];
        let mut input = fixture.build();
        let second = {
            let mut module = Module::new();
            let mut names = NameSection::new();
            names.module("second");
            module.section(&names);
            module.finish()
        };
        input.extend_from_slice(&second[8..]);

        assert_eq!(
            check_wasm1(&input),
            Ok(()),
            "the validator accepts two name sections, which is why this pass has to refuse them"
        );
        assert_eq!(
            rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL),
            Err(StellarAbiError::MultipleNameSections)
        );
    }

    /// A module big enough that a section count and a function index need two
    /// LEB128 bytes. Every other fixture here has at most three functions, so
    /// the re-encoded counts and the appended name entries are otherwise only
    /// ever written as single bytes, while a real artifact routinely carries
    /// more than 127 functions. 127 is the size whose counts widen as the
    /// wrapper is added; 128 is the size whose wrapper index does.
    fn wide_fixture(count: u32) -> Fixture {
        let last = count - 1;
        Fixture {
            types: vec![(Vec::new(), Vec::new())],
            functions: vec![0; count as usize],
            bodies: vec![vec![Instruction::End]; count as usize],
            exports: vec![("last", ExportKind::Func, last)],
            function_names: vec![(0, "first"), (last, "last")],
            ..Fixture::default()
        }
    }

    #[test]
    fn a_module_past_the_leb128_boundary_keeps_its_bodies_and_names_its_wrapper() {
        for count in [127u32, 128] {
            let input = wide_fixture(count).build();
            let output = rewrite(
                &input,
                &[signature("last", Vec::new(), AbiReturn::Unit)],
                STELLAR_ENV_PROTOCOL,
            )
            .expect("rewrites");

            let before = bodies_of(&input);
            let after = bodies_of(&output);
            assert_eq!(after.len(), count as usize + 1);
            assert_eq!(before, after[..count as usize], "{count} bodies come through");

            assert_eq!(
                exports_of(&output)[0],
                ("last".to_string(), ExternalKind::Func, count),
                "the export names the wrapper, which sits past every existing function"
            );
            assert_eq!(
                function_names_of(&output).last().cloned(),
                Some((count, "last$val".to_string())),
                "the wrapper's name entry reads back at the wrapper's own index"
            );

            for id in [FUNCTION_SECTION_ID, CODE_SECTION_ID] {
                let body = section_body(&output, id, "");
                let mut offset = 0;
                assert_eq!(read_u32_leb(&body, &mut offset).expect("a count"), count + 1);
                assert_eq!(offset, 2, "section {id}'s re-encoded count is two bytes");
            }
        }
    }

    /// The import base is zero for every module that gets past the parse, since
    /// an import is refused there — so neither place that measures a function
    /// index against it is reachable through `rewrite`, and both are pinned
    /// here instead. WebAssembly puts imports first in the function index
    /// space, so a lookup that ignored the base would read the wrong function
    /// the day this target admits host imports, and would still validate
    /// whenever the arities happened to line up.
    #[test]
    fn a_signature_lookup_is_measured_from_the_import_base() {
        let module = ParsedModule {
            pieces: Vec::new(),
            types: Entries::default(),
            functions: Entries::default(),
            code: Entries::default(),
            name_section: None,
            imported_functions: 2,
            exports: Vec::new(),
            type_shapes: vec![
                (vec![wasmparser::ValType::I32], vec![wasmparser::ValType::I32]),
                (Vec::new(), Vec::new()),
            ],
            function_type_indices: vec![1, 0],
        };

        assert_eq!(
            module.signature_of(0),
            None,
            "an imported function's type is not in the function section"
        );
        assert_eq!(module.signature_of(1), None);
        assert_eq!(module.signature_of(2), Some(&(Vec::new(), Vec::new())));
        assert_eq!(
            module.signature_of(3),
            Some(&(vec![wasmparser::ValType::I32], vec![wasmparser::ValType::I32]))
        );
        assert_eq!(module.signature_of(4), None);
    }

    #[test]
    fn a_wrapper_index_is_measured_from_the_import_base() {
        let module = ParsedModule {
            pieces: Vec::new(),
            types: Entries {
                bytes: &[],
                count: 1,
            },
            functions: Entries {
                bytes: &[],
                count: 1,
            },
            code: Entries::default(),
            name_section: None,
            imported_functions: 2,
            exports: vec![ExportEntry {
                name: "tick",
                kind: ExportKind::Func,
                index: 2,
            }],
            type_shapes: vec![(Vec::new(), Vec::new())],
            function_type_indices: vec![0],
        };

        let wrappers = plan_wrappers(&module, &[signature("tick", Vec::new(), AbiReturn::Unit)])
            .expect("plans");
        assert_eq!(wrappers[0].inner, 2, "the export names the module's own index");
        assert_eq!(
            wrappers[0].index, 3,
            "the wrapper lands past the imports and the defined functions alike"
        );
    }

    /// The protocol is public API, and one below the first protocol Soroban
    /// shipped in produces a well-formed metadata section no host has ever
    /// accepted: it fails at upload, long after the compiler has said yes.
    #[test]
    fn a_protocol_older_than_soroban_is_refused() {
        let input = add_fixture().build();
        for protocol in [0, STELLAR_ENV_PROTOCOL - 1] {
            assert_eq!(
                rewrite(&input, &add_descriptor(), protocol),
                Err(StellarAbiError::ProtocolPredatesSoroban { protocol })
            );
        }
        assert!(rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).is_ok());
        assert!(
            rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL + 8).is_ok(),
            "a protocol above the floor chooses which networks accept the artifact, and is not \
             this pass's decision"
        );
    }

    #[test]
    fn the_rewritten_module_validates_at_webassembly_one() {
        let input = add_fixture().build();
        let output = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");
        assert_eq!(check_wasm1(&output), Ok(()));
    }

    /// The published entry point, on the instruction family a caller most often
    /// meets it with: one sign-extension instruction is the whole difference
    /// between the two modules, and it is what an ordinary `rustc`
    /// `wasm32-unknown-unknown` build emits by default.
    #[test]
    fn a_sign_extension_instruction_is_not_webassembly_one() {
        let mvp = Fixture {
            types: vec![(vec![ValType::I32], vec![ValType::I32])],
            functions: vec![0],
            bodies: vec![vec![Instruction::LocalGet(0), Instruction::End]],
            exports: vec![("narrow", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        assert_eq!(check_wasm1(&mvp.build()), Ok(()));

        let extended = Fixture {
            bodies: vec![vec![
                Instruction::LocalGet(0),
                Instruction::I32Extend8S,
                Instruction::End,
            ]],
            ..mvp
        };
        let reason = check_wasm1(&extended.build()).expect_err("post-1.0 instruction");
        assert!(
            reason.contains("sign extension"),
            "the refusal must name the feature, got: {reason}"
        );
    }

    /// Thirty-two parameters is the limit and is accepted; the refusal is
    /// pinned beside it so that neither side of the boundary can move alone.
    #[test]
    fn thirty_two_parameters_are_accepted_and_thirty_three_are_not() {
        let build = |count: usize| {
            let fixture = Fixture {
                types: vec![(vec![ValType::I32; count], vec![ValType::I32])],
                functions: vec![0],
                bodies: vec![vec![Instruction::I32Const(0), Instruction::End]],
                exports: vec![("wide", ExportKind::Func, 0)],
                ..Fixture::default()
            };
            rewrite(
                &fixture.build(),
                &[signature(
                    "wide",
                    vec![AbiType::U32; count],
                    AbiReturn::Scalar(AbiType::U32),
                )],
                STELLAR_ENV_PROTOCOL,
            )
        };

        assert!(build(MAX_VAL_PARAMETERS).is_ok());
        assert_eq!(
            build(MAX_VAL_PARAMETERS + 1),
            Err(StellarAbiError::TooManyParameters {
                export: "wide".to_string(),
                count: 33,
            })
        );
    }

    /// The negative matrix. Each row is a module the rewrite must refuse, and
    /// each refusal names the export and the element that caused it.
    #[test]
    fn an_i64_or_u64_parameter_is_refused() {
        for (ty, rendered) in [(AbiType::I64, "i64"), (AbiType::U64, "u64")] {
            let fixture = Fixture {
                types: vec![(vec![ValType::I64], vec![ValType::I32])],
                functions: vec![0],
                bodies: vec![vec![Instruction::I32Const(0), Instruction::End]],
                exports: vec![("wide", ExportKind::Func, 0)],
                ..Fixture::default()
            };
            assert_eq!(
                rewrite(
                    &fixture.build(),
                    &[signature("wide", vec![ty], AbiReturn::Scalar(AbiType::U32))],
                    STELLAR_ENV_PROTOCOL,
                ),
                Err(StellarAbiError::UnsupportedParameter {
                    export: "wide".to_string(),
                    position: 1,
                    ty: rendered.to_string(),
                })
            );
        }
    }

    #[test]
    fn an_i64_or_u64_return_is_refused() {
        for (ty, rendered) in [(AbiType::I64, "i64"), (AbiType::U64, "u64")] {
            let fixture = Fixture {
                types: vec![(Vec::new(), vec![ValType::I64])],
                functions: vec![0],
                bodies: vec![vec![Instruction::I64Const(0), Instruction::End]],
                exports: vec![("wide", ExportKind::Func, 0)],
                ..Fixture::default()
            };
            assert_eq!(
                rewrite(
                    &fixture.build(),
                    &[signature("wide", Vec::new(), AbiReturn::Scalar(ty))],
                    STELLAR_ENV_PROTOCOL,
                ),
                Err(StellarAbiError::UnsupportedReturn {
                    export: "wide".to_string(),
                    ty: rendered.to_string(),
                })
            );
        }
    }

    #[test]
    fn a_compound_return_is_refused_by_its_own_variant() {
        let fixture = Fixture {
            types: vec![(vec![ValType::I32], Vec::new())],
            functions: vec![0],
            bodies: vec![vec![Instruction::End]],
            exports: vec![("corners", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        assert_eq!(
            rewrite(
                &fixture.build(),
                &[signature(
                    "corners",
                    Vec::new(),
                    AbiReturn::Sret(AbiType::Array {
                        elem: Box::new(AbiType::I32),
                        len: 4,
                    }),
                )],
                STELLAR_ENV_PROTOCOL,
            ),
            Err(StellarAbiError::CompoundReturn {
                export: "corners".to_string(),
                ty: "[i32; 4]".to_string(),
            })
        );
    }

    #[test]
    fn a_compound_or_narrow_parameter_is_refused() {
        let cases = [
            (
                AbiType::Struct {
                    name: "Point".to_string(),
                },
                "Point",
            ),
            (
                AbiType::Array {
                    elem: Box::new(AbiType::U32),
                    len: 3,
                },
                "[u32; 3]",
            ),
            (
                AbiType::Enum {
                    name: "Colour".to_string(),
                },
                "Colour",
            ),
            (AbiType::U8, "u8"),
            (AbiType::I8, "i8"),
            (AbiType::U16, "u16"),
            (AbiType::I16, "i16"),
        ];
        for (ty, rendered) in cases {
            let fixture = Fixture {
                types: vec![(vec![ValType::I32], vec![ValType::I32])],
                functions: vec![0],
                bodies: vec![vec![Instruction::I32Const(0), Instruction::End]],
                exports: vec![("takes", ExportKind::Func, 0)],
                ..Fixture::default()
            };
            assert_eq!(
                rewrite(
                    &fixture.build(),
                    &[signature("takes", vec![ty], AbiReturn::Scalar(AbiType::U32))],
                    STELLAR_ENV_PROTOCOL,
                ),
                Err(StellarAbiError::UnsupportedParameter {
                    export: "takes".to_string(),
                    position: 1,
                    ty: rendered.to_string(),
                })
            );
        }
    }

    /// The position is the one the source declared, not the one the check
    /// happened to reach first — and it is one-based, so it reads as the
    /// source-level gate's does. The rendered sentence is asserted too: the
    /// number a user compares between two logs is in the prose, not in the
    /// field.
    #[test]
    fn an_inadmissible_parameter_is_named_by_its_declared_position() {
        let fixture = Fixture {
            types: vec![(
                vec![ValType::I32, ValType::I32, ValType::I64],
                vec![ValType::I32],
            )],
            functions: vec![0],
            bodies: vec![vec![Instruction::I32Const(0), Instruction::End]],
            exports: vec![("third", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        let refusal = rewrite(
            &fixture.build(),
            &[signature(
                "third",
                vec![AbiType::U32, AbiType::Bool, AbiType::U64],
                AbiReturn::Scalar(AbiType::U32),
            )],
            STELLAR_ENV_PROTOCOL,
        );
        assert_eq!(
            refusal,
            Err(StellarAbiError::UnsupportedParameter {
                export: "third".to_string(),
                position: 3,
                ty: "u64".to_string(),
            })
        );
        assert!(
            refusal.unwrap_err().to_string().contains("at parameter 3"),
            "the third declared parameter must be named as the third"
        );
    }

    fn named_export_refusal(name: &'static str) -> Result<Vec<u8>, StellarAbiError> {
        let fixture = Fixture {
            types: vec![(Vec::new(), Vec::new())],
            functions: vec![0],
            bodies: vec![vec![Instruction::End]],
            exports: vec![(name, ExportKind::Func, 0)],
            ..Fixture::default()
        };
        rewrite(
            &fixture.build(),
            &[signature(name, Vec::new(), AbiReturn::Unit)],
            STELLAR_ENV_PROTOCOL,
        )
    }

    #[test]
    fn an_empty_export_name_is_refused() {
        assert_eq!(
            named_export_refusal(""),
            Err(StellarAbiError::EmptyExportName)
        );
    }

    /// Exactly 32 bytes is accepted — measured, on both sides — and 33 is not.
    #[test]
    fn an_export_name_over_thirty_two_bytes_is_refused() {
        assert!(named_export_refusal("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_ok());
        assert_eq!(
            named_export_refusal("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Err(StellarAbiError::ExportNameTooLong {
                export: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                len: 33,
            })
        );
    }

    #[test]
    fn a_reserved_export_name_is_refused() {
        assert_eq!(
            named_export_refusal("__reserved"),
            Err(StellarAbiError::ReservedExportName {
                export: "__reserved".to_string(),
            })
        );
    }

    /// A single leading underscore is not the reserved prefix, and is a legal
    /// `Symbol` character.
    #[test]
    fn one_leading_underscore_is_not_reserved() {
        assert!(named_export_refusal("_private").is_ok());
    }

    #[test]
    fn an_export_name_outside_the_symbol_alphabet_is_refused() {
        assert_eq!(
            named_export_refusal("has-hyphen"),
            Err(StellarAbiError::ExportNameNotASymbol {
                export: "has-hyphen".to_string(),
                offending: '-',
            })
        );
        assert_eq!(
            named_export_refusal("dotted.name"),
            Err(StellarAbiError::ExportNameNotASymbol {
                export: "dotted.name".to_string(),
                offending: '.',
            })
        );
    }

    #[test]
    fn a_module_exporting_no_function_is_refused() {
        let fixture = Fixture {
            types: vec![(Vec::new(), Vec::new())],
            functions: vec![0],
            bodies: vec![vec![Instruction::End]],
            exports: Vec::new(),
            memory: true,
            ..Fixture::default()
        };
        assert_eq!(
            rewrite(&fixture.build(), &[], STELLAR_ENV_PROTOCOL),
            Err(StellarAbiError::NoExportedFunctions)
        );
    }

    #[test]
    fn a_surviving_import_is_refused() {
        let fixture = Fixture {
            types: vec![(Vec::new(), Vec::new())],
            functions: vec![0],
            bodies: vec![vec![Instruction::End]],
            exports: vec![("tick", ExportKind::Func, 1)],
            imports: vec![("env", "log", 0)],
            ..Fixture::default()
        };
        assert_eq!(
            rewrite(
                &fixture.build(),
                &[signature("tick", Vec::new(), AbiReturn::Unit)],
                STELLAR_ENV_PROTOCOL,
            ),
            Err(StellarAbiError::ImportsUnsupported {
                module: "env".to_string(),
                name: "log".to_string(),
            })
        );
    }

    #[test]
    fn a_start_section_is_refused() {
        let fixture = Fixture {
            types: vec![(Vec::new(), Vec::new())],
            functions: vec![0],
            bodies: vec![vec![Instruction::End]],
            exports: vec![("tick", ExportKind::Func, 0)],
            start: Some(0),
            ..Fixture::default()
        };
        assert_eq!(
            rewrite(
                &fixture.build(),
                &[signature("tick", Vec::new(), AbiReturn::Unit)],
                STELLAR_ENV_PROTOCOL,
            ),
            Err(StellarAbiError::StartSectionPresent { function: 0 })
        );
    }

    /// An exported function the descriptor is silent about would ship as a
    /// method that is reachable and always broken, so the mismatch is refused
    /// in both directions.
    #[test]
    fn an_export_the_descriptor_does_not_describe_is_refused() {
        let fixture = Fixture {
            types: vec![(Vec::new(), Vec::new())],
            functions: vec![0, 0],
            bodies: vec![vec![Instruction::End], vec![Instruction::End]],
            exports: vec![("tick", ExportKind::Func, 0), ("tock", ExportKind::Func, 1)],
            ..Fixture::default()
        };
        assert_eq!(
            rewrite(
                &fixture.build(),
                &[signature("tick", Vec::new(), AbiReturn::Unit)],
                STELLAR_ENV_PROTOCOL,
            ),
            Err(StellarAbiError::UnknownExport {
                export: "tock".to_string(),
            })
        );
    }

    #[test]
    fn a_described_export_the_module_does_not_carry_is_refused() {
        let fixture = Fixture {
            types: vec![(Vec::new(), Vec::new())],
            functions: vec![0],
            bodies: vec![vec![Instruction::End]],
            exports: vec![("tick", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        assert_eq!(
            rewrite(
                &fixture.build(),
                &[
                    signature("tick", Vec::new(), AbiReturn::Unit),
                    signature("tock", Vec::new(), AbiReturn::Unit),
                ],
                STELLAR_ENV_PROTOCOL,
            ),
            Err(StellarAbiError::MissingExport {
                export: "tock".to_string(),
            })
        );
    }

    /// Rewriting a contract again would wrap the wrappers, so the second pass
    /// is refused rather than performed — for the first of the three sections
    /// the parse meets, which is the spec section.
    #[test]
    fn a_module_that_is_already_a_contract_is_refused() {
        let input = add_fixture().build();
        let once = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL).expect("rewrites");
        assert_eq!(
            rewrite(&once, &add_descriptor(), STELLAR_ENV_PROTOCOL),
            Err(StellarAbiError::AlreadyAContract {
                section: "contractspecv0".to_string(),
            })
        );
    }

    /// Each of the three sections is refused on its own, and the refusal names
    /// the one it found. A second `contractspecv0` is the case with teeth: a
    /// reader that takes the first one it meets, as `soroban-spec`'s does, would
    /// invoke a module carrying a stale spec ahead of the rewrite's against the
    /// stale one.
    #[test]
    fn an_input_carrying_any_contract_section_is_refused_naming_it() {
        for section in ["contractspecv0", "contractmetav0", "contractenvmetav0"] {
            let mut input = add_fixture().build();
            let carried = {
                let mut module = Module::new();
                module.section(&CustomSection {
                    name: section.into(),
                    data: [0u8, 0, 0, 0].as_slice().into(),
                });
                module.finish()
            };
            input.extend_from_slice(&carried[8..]);

            let refusal = rewrite(&input, &add_descriptor(), STELLAR_ENV_PROTOCOL);
            assert_eq!(
                refusal,
                Err(StellarAbiError::AlreadyAContract {
                    section: section.to_string(),
                })
            );
            assert!(
                refusal
                    .unwrap_err()
                    .to_string()
                    .contains(&format!("`{section}`")),
                "the message must name the section"
            );
        }
    }

    #[test]
    fn bytes_that_are_not_a_module_are_refused() {
        let error = rewrite(b"not wasm at all", &add_descriptor(), STELLAR_ENV_PROTOCOL)
            .expect_err("refuses");
        assert!(matches!(error, StellarAbiError::InputNotWasm1 { .. }));
    }

    /// A module outside WebAssembly 1.0 is refused before anything is spliced,
    /// because a Soroban host will not run it either.
    #[test]
    fn a_module_using_a_post_one_point_zero_feature_is_refused() {
        let fixture = Fixture {
            types: vec![(vec![ValType::I32], vec![ValType::I32])],
            functions: vec![0],
            bodies: vec![vec![
                Instruction::LocalGet(0),
                Instruction::I32Extend8S,
                Instruction::End,
            ]],
            exports: vec![("widen", ExportKind::Func, 0)],
            ..Fixture::default()
        };
        let error = rewrite(
            &fixture.build(),
            &[signature(
                "widen",
                vec![AbiType::I32],
                AbiReturn::Scalar(AbiType::I32),
            )],
            STELLAR_ENV_PROTOCOL,
        )
        .expect_err("refuses");
        assert!(matches!(error, StellarAbiError::InputNotWasm1 { .. }));
    }

    /// Every message names the export it is about, so that a reader holding the
    /// source knows which declaration to change.
    ///
    /// The name is one no message could produce on its own. A one-letter name
    /// would be found in `module`, `method`, `memory` or `at most`, and the
    /// assertion would hold with every interpolation deleted; the backticks are
    /// asserted with it, so the name has to be rendered as a name.
    #[test]
    fn every_export_refusal_names_its_export() {
        const EXPORT: &str = "qqxz";
        let refusals = [
            StellarAbiError::UnknownExport {
                export: EXPORT.to_string(),
            },
            StellarAbiError::MissingExport {
                export: EXPORT.to_string(),
            },
            StellarAbiError::ExportNameTooLong {
                export: EXPORT.to_string(),
                len: 33,
            },
            StellarAbiError::ReservedExportName {
                export: EXPORT.to_string(),
            },
            StellarAbiError::ExportNameNotASymbol {
                export: EXPORT.to_string(),
                offending: '-',
            },
            StellarAbiError::TooManyParameters {
                export: EXPORT.to_string(),
                count: 33,
            },
            StellarAbiError::UnsupportedParameter {
                export: EXPORT.to_string(),
                position: 1,
                ty: "u64".to_string(),
            },
            StellarAbiError::UnsupportedReturn {
                export: EXPORT.to_string(),
                ty: "u64".to_string(),
            },
            StellarAbiError::CompoundReturn {
                export: EXPORT.to_string(),
                ty: "Point".to_string(),
            },
            StellarAbiError::DescriptorDisagreesWithModule {
                export: EXPORT.to_string(),
                expected: "(i32)".to_string(),
                found: "(i32) -> i32".to_string(),
            },
            StellarAbiError::UnnamedParameter {
                export: EXPORT.to_string(),
                position: 1,
            },
            StellarAbiError::ParameterNameTooLong {
                export: EXPORT.to_string(),
                position: 1,
                name: "n".repeat(31),
                len: 31,
            },
        ];
        for refusal in refusals {
            let message = refusal.to_string();
            assert!(message.contains(&format!("`{EXPORT}`")), "{message}");
        }
    }

    /// Five messages in full. A `#[error]` literal split across source lines
    /// keeps its indentation unless every break carries a continuation, and a
    /// substring assertion cannot see the runs of spaces that leaves behind —
    /// so the rendered text is compared whole.
    #[test]
    fn a_refusal_renders_as_one_line_of_prose() {
        assert_eq!(
            StellarAbiError::UnsupportedParameter {
                export: "transfer".to_string(),
                position: 1,
                ty: "u64".to_string(),
            }
            .to_string(),
            "the export `transfer` takes `u64` at parameter 1; a contract method parameter must \
             be `u32`, `i32` or `bool`"
        );
        assert_eq!(
            StellarAbiError::ExportNameTooLong {
                export: "x".to_string(),
                len: 33,
            }
            .to_string(),
            "the export `x` is 33 bytes long; a contract method name is at most 32 bytes, and a \
             longer one cannot be named by any caller"
        );
        assert_eq!(
            StellarAbiError::UnnamedParameter {
                export: "transfer".to_string(),
                position: 2,
            }
            .to_string(),
            "the export `transfer` leaves parameter 2 unnamed, written `_`; a contract method's \
             parameters are named, because `stellar contract invoke` passes each one as \
             `--<name>` and the contract spec section records that name, so name the parameter"
        );
        assert_eq!(
            StellarAbiError::ParameterNameTooLong {
                export: "transfer".to_string(),
                position: 1,
                name: "n".repeat(31),
                len: 31,
            }
            .to_string(),
            format!(
                "the export `transfer` names parameter 1 `{}`, which is 31 bytes long; a \
                 contract method's parameter name is at most 30 bytes, the width of the contract \
                 spec section's input-name field, so shorten it",
                "n".repeat(31)
            )
        );
        assert_eq!(
            StellarAbiError::AlreadyAContract {
                section: "contractmetav0".to_string(),
            }
            .to_string(),
            "the module already carries a `contractmetav0` section, which this pass writes, so \
             it has already been made a contract"
        );
    }
}
