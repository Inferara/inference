//! WASM Bytecode Parser
//!
//! This module provides the parsing phase (Phase 1) of WASM to Rocq translation.
//! It streams through WASM bytecode sections and builds a structured representation
//! suitable for Rocq code generation.
//!
//! ## Overview
//!
//! The parser uses [`inf-wasmparser`] (a fork of `wasmparser` with non-deterministic
//! instruction support) to incrementally parse WASM sections. This streaming approach
//! processes bytecode without loading the entire module into memory, enabling efficient
//! handling of large WASM files.
//!
//! ## Entry Point
//!
//! The main entry point is [`translate_bytes`], which orchestrates the complete
//! translation pipeline:
//!
//! 1. **Parse Phase**: Call [`parse`] to stream through WASM sections
//! 2. **Build Structure**: Populate [`WasmParseData`] with extracted information
//! 3. **Translate Phase**: Call [`WasmParseData::translate`] to generate Rocq code
//!
//! ## Parsing Strategy
//!
//! The parser makes a single forward pass through the WASM module, processing
//! sections in WebAssembly specification order:
//!
//! ```text
//! Version → Type → Import → Function → Table → Memory → Global →
//! Export → Start → Element → DataCount → Data → Code → Custom
//! ```
//!
//! Each section handler:
//! 1. Receives a section iterator from `inf-wasmparser`
//! 2. Iterates through section entries
//! 3. Pushes parsed data into the corresponding `WasmParseData` field
//!
//! ### Zero-Copy Parsing
//!
//! The parser uses borrowed data (`&[u8]`) throughout to minimize allocations.
//! Most WASM section data references slices of the original bytecode, avoiding
//! unnecessary copies.
//!
//! ## Custom Name Section
//!
//! The parser extracts debug information from the custom "name" section:
//!
//! - **Module name**: Overrides the default module name parameter
//! - **Function names**: Maps function indices to human-readable identifiers
//! - **Local names**: Maps (function index, local index) to variable names
//!
//! This information dramatically improves readability of generated Rocq code by
//! preserving original source-level names.
//!
//! ## Component Model Sections
//!
//! A component binary is refused at its version header: it shares the core
//! preamble's first four bytes and differs only in the layer field, so accepting
//! one produced an empty core module — complete with its validity theorem —
//! describing content that was never read. The component-model section arms are
//! refused too, so the property is local to the match rather than inherited from
//! the version check.
//!
//! ## Structural Consistency
//!
//! No [`inf_wasmparser::Validator`] is constructed here, deliberately: the
//! rejection fixtures in this crate's tests are stack-invalid on purpose and must
//! still translate. What *is* checked is the structure a binary asserts about
//! itself, since a contradiction there yields a `.v` describing a different
//! program rather than a rejected one:
//!
//! - the core version, and that the binary is a module rather than a component;
//! - no repeated core section, and none out of the order the format fixes (see
//!   [`section_rank`] — the ids are not monotonic in that order);
//! - the data count section against the data section it describes;
//! - the function section against the code section, so no body's type is
//!   fabricated from a default.
//!
//! Value-level validation — that indices point at things that exist — is *not*
//! performed. A caller outside the linker pipeline owns it.
//!
//! ## Error Handling
//!
//! Both phases fail closed. Every rejection is a typed [`WasmToVError`] carrying
//! a prefix this crate owns, so a caller can classify it and a test can grep it
//! without depending on the parser's wording. No input reaches a panic, an
//! unbounded allocation, or a silently truncated translation.

use inf_wasmparser::{
    Parser,
    Payload::{
        CodeSectionEntry, CodeSectionStart, ComponentAliasSection, ComponentCanonicalSection,
        ComponentExportSection, ComponentImportSection, ComponentInstanceSection, ComponentSection,
        ComponentStartSection, ComponentTypeSection, CoreTypeSection, CustomSection,
        DataCountSection, DataSection, ElementSection, End, ExportSection, FunctionSection,
        GlobalSection, ImportSection, InstanceSection, MemorySection, ModuleSection, StartSection,
        TableSection, TagSection, TypeSection, UnknownSection, Version,
    },
};
use inference_hassert::HSpecMap;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;

use crate::errors::WasmToVError;
use crate::gallina::neutralize_comment_delimiters;
use crate::rocq_names::{
    sanitize_rocq_identifier, validate_module_name_available, validate_rocq_identifier,
};
use crate::translator::WasmParseData;

/// Translates WebAssembly bytecode into Rocq (Coq) formal verification code.
///
/// This is the main entry point for WASM to Rocq translation. It performs a complete
/// translation in two phases:
///
/// 1. **Parse Phase**: Streams through WASM sections to build [`WasmParseData`]
/// 2. **Translate Phase**: Converts structured data into Rocq code strings
///
/// # Parameters
///
/// - `mod_name`: The Rocq module name for generated definitions (may be overridden by WASM custom name section)
/// - `bytes`: Raw WASM bytecode to translate
///
/// # Returns
///
/// Returns a `String` containing complete Rocq code including:
/// - Required Rocq imports
/// - Helper definitions
/// - Type translations
/// - Function definitions
/// - Module record with all WASM sections
///
/// # Errors
///
/// Returns an error if:
/// - WASM bytecode is malformed or invalid
/// - Required WASM sections are missing
/// - Unsupported WASM features are encountered (e.g., tag section, unknown reference types)
/// - Translation of specific instructions fails
///
/// # Examples
///
/// Basic usage:
///
/// ```ignore
/// use inference_wasm_to_v_translator::wasm_parser::translate_bytes;
/// use rustc_hash::FxHashMap;
///
/// let wasm_bytes = std::fs::read("output.wasm")?;
/// let rocq_code = translate_bytes("my_module", &wasm_bytes, &FxHashMap::default())?;
/// std::fs::write("output.v", rocq_code)?;
/// ```
///
/// Integration with Inference compiler:
///
/// ```ignore
/// use inference::{parse, type_check, codegen};
/// use inference_wasm_to_v_translator::wasm_parser::translate_bytes;
/// use rustc_hash::FxHashMap;
///
/// let source = std::fs::read_to_string("program.inf")?;
/// let arena = parse(&source)?;
/// let typed_context = type_check(arena)?;
/// let codegen_output = codegen(&typed_context)?;
///
/// // Translate to Rocq
/// let rocq_code = translate_bytes("Program", codegen_output.wasm(), &FxHashMap::default())?;
/// std::fs::write("program.v", rocq_code)?;
/// ```
pub fn translate_bytes(
    mod_name: &str,
    bytes: &[u8],
    spec_funcs_by_spec: &FxHashMap<String, Vec<u32>>,
    hspecs_by_spec: &HSpecMap,
) -> anyhow::Result<String> {
    // API-boundary validation: every name we accept here is checked once,
    // up front. Names that come from the embedded `inference.spec_funcs`
    // section are validated separately at the decode boundary
    // (`decode_spec_funcs_section`), so the per-spec loop inside
    // `WasmParseData::translate` is no longer needed.
    validate_rocq_identifier(mod_name)?;
    // Syntactic legality is not availability: the emitted preamble already
    // occupies eight top-level `Definition` names, and the module record claims
    // this one.
    validate_module_name_available(mod_name)?;
    for spec_name in spec_funcs_by_spec.keys() {
        validate_rocq_identifier(spec_name)?;
    }

    match parse(
        mod_name.to_string(),
        bytes,
        spec_funcs_by_spec.clone(),
        hspecs_by_spec.clone(),
    ) {
        Ok(mut parse_data) => parse_data.translate(),
        Err(e) => {
            if e.downcast_ref::<WasmToVError>().is_some() {
                Err(e)
            } else {
                Err(anyhow::anyhow!(WasmToVError::WasmParse(e.to_string())))
            }
        }
    }
}

/// Parses WebAssembly bytecode into structured [`WasmParseData`].
///
/// This function makes a single forward pass through the WASM module,
/// processing each section and populating the corresponding fields in
/// [`WasmParseData`].
///
/// # Section Processing
///
/// The parser handles these WASM sections:
///
/// - **Type Section**: Function type signatures stored as `RecGroup` entries
/// - **Import Section**: External function, table, memory, and global imports
/// - **Function Section**: Maps function indices to their type indices
/// - **Table Section**: Indirect call table definitions
/// - **Memory Section**: Linear memory definitions with size limits
/// - **Global Section**: Global variable definitions with initialization expressions
/// - **Export Section**: Exported functions, tables, memories, and globals
/// - **Start Section**: Optional module entry point function
/// - **Element Section**: Table element initialization
/// - **Data Section**: Memory initialization data
/// - **Code Section**: Function bodies with local variables and instructions
/// - **Custom Section**: Name mappings for functions and local variables (debug info)
///
/// Sections outside the core module format — component-model sections, the tag
/// section, and any unrecognised id — are rejected rather than ignored: a `.v`
/// emitted alongside content this translator did not read would describe only
/// the part of the module it happened to understand.
///
/// # Parameters
///
/// - `mod_name`: Default module name (may be overridden by custom name section)
/// - `data`: Raw WASM bytecode slice
///
/// # Returns
///
/// Returns [`WasmParseData`] containing all parsed information ready for translation.
///
/// # Errors
///
/// Returns an error if WASM bytecode is malformed or contains invalid section data.
#[allow(clippy::match_same_arms)]
fn parse(
    mod_name: String,
    data: &'_ [u8],
    spec_funcs_by_spec: FxHashMap<String, Vec<u32>>,
    hspecs_by_spec: HSpecMap,
) -> anyhow::Result<WasmParseData<'_>> {
    let parser = Parser::new(0);
    let explicit_non_empty = !spec_funcs_by_spec.is_empty();
    let explicit_hspecs_non_empty = !hspecs_by_spec.is_empty();
    let mut wasm_parse_data = WasmParseData::new(mod_name, spec_funcs_by_spec, hspecs_by_spec);
    let mut embedded_spec_funcs: Option<FxHashMap<String, Vec<u32>>> = None;
    let mut embedded_hspecs: Option<HSpecMap> = None;
    let mut seen_name_section = false;
    let mut seen_sections: FxHashSet<u8> = FxHashSet::default();
    let mut last_rank = 0u8;
    let mut declared_data_count: Option<u32> = None;

    for payload in parser.parse_all(data) {
        let payload = payload?;

        // Cross-section structure, checked before the payload is consumed.
        // Custom sections (id 0) are exempt from both rules: they may legally
        // repeat and may appear anywhere, which is why the `name`,
        // `inference.spec_funcs` and `inference.hspecs` decoders carry their own
        // per-name duplicate guards instead.
        if let Some((id, _)) = payload.as_section()
            && id != 0
        {
            if !seen_sections.insert(id) {
                return Err(anyhow::anyhow!(WasmToVError::WasmParse(format!(
                    "duplicate WASM section id {id}"
                ))));
            }
            let rank = section_rank(id);
            if rank < last_rank {
                return Err(anyhow::anyhow!(WasmToVError::WasmParse(format!(
                    "WASM section id {id} appears out of order"
                ))));
            }
            last_rank = rank;
        }

        match payload {
            // Sections for WebAssembly modules
            // A component binary shares the core preamble's first four bytes and
            // differs only in the layer field, so without this check an 8-byte
            // component header parsed as an empty core module and produced a
            // complete `ValidModule` theorem about a program whose content was
            // never read. This arm also runs for each nested core module's own
            // header, so it must not assume it fires once.
            Version { num, encoding, .. } => {
                if encoding != inf_wasmparser::Encoding::Module {
                    return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                        description: "WebAssembly component binary (the proof contract \
                                      models core modules only)"
                            .into(),
                    }));
                }
                if num != 1 {
                    return Err(anyhow::anyhow!(WasmToVError::WasmParse(format!(
                        "unsupported core WASM version {num} (expected 1)"
                    ))));
                }
            }
            TypeSection(type_section) => {
                for ty in type_section {
                    wasm_parse_data.function_types.push(ty?);
                }
            }
            ImportSection(imports_section) => {
                for import in imports_section {
                    wasm_parse_data.imports.push(import?);
                }
            }
            FunctionSection(functions) => {
                // Read like every sibling section arm: a malformed entry is
                // propagated, not unwrapped. The prefix is this crate's own, so
                // a rejection test greps it rather than the parser's wording.
                for f in functions {
                    let type_index = f.map_err(|e| {
                        anyhow::anyhow!(WasmToVError::WasmParse(format!("function section: {e}")))
                    })?;
                    wasm_parse_data.function_type_indexes.push(type_index);
                }
            }
            TableSection(tables_section) => {
                for table in tables_section {
                    wasm_parse_data.tables.push(table?);
                }
            }
            MemorySection(memories) => {
                for memory in memories {
                    wasm_parse_data.memory_types.push(memory?);
                }
            }
            // A module declaring tags uses exception handling, which the proof
            // contract does not model. Ignoring the section emitted a `.v` for a
            // module without them, while a tag reaching an import or an export
            // is already refused — the same construct cannot be both.
            TagSection(_) => {
                return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                    description: "tag section (exception-handling proposal)".into(),
                }));
            }
            GlobalSection(globals) => {
                for global in globals {
                    wasm_parse_data.globals.push(global?);
                }
            }
            ExportSection(export_sections) => {
                for export in export_sections {
                    wasm_parse_data.exports.push(export?);
                }
            }
            StartSection { func, .. } => {
                wasm_parse_data.start_function = Some(func);
            }
            ElementSection(elements) => {
                for element in elements {
                    wasm_parse_data.elements.push(element?);
                }
            }
            // Recorded rather than ignored: the count is a claim about the data
            // section, reconciled against it once the module has been read.
            DataCountSection { count, .. } => {
                declared_data_count = Some(count);
            }
            DataSection(data) => {
                for datum in data {
                    wasm_parse_data.data.push(datum?);
                }
            }

            // Here we know how many functions we'll be receiving as
            // `CodeSectionEntry`, so we can prepare for that, and
            // afterwards we can parse and handle each function
            // individually.
            CodeSectionStart { .. } => {}
            CodeSectionEntry(body) => {
                wasm_parse_data.function_bodies.push(body);
            }

            // Component-model sections. The version guard above already refuses
            // a component binary, so these are unreachable through the current
            // entry points; they are refused rather than ignored so the property
            // stays local to this match instead of being derived from an arm
            // three hundred lines away. Ignoring them once meant a nested core
            // module's own type, function and code payloads were folded into the
            // *parent* record.
            ModuleSection { .. }
            | InstanceSection(_)
            | CoreTypeSection(_)
            | ComponentSection { .. }
            | ComponentInstanceSection(_)
            | ComponentAliasSection(_)
            | ComponentTypeSection(_)
            | ComponentCanonicalSection(_)
            | ComponentStartSection { .. }
            | ComponentImportSection(_)
            | ComponentExportSection(_) => {
                return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                    description: "component-model section (the proof contract models \
                                  core modules only)"
                        .into(),
                }));
            }

            CustomSection(custom_section) => {
                if custom_section.name() == crate::SPEC_FUNCS_SECTION_NAME {
                    if embedded_spec_funcs.is_some() {
                        return Err(anyhow::anyhow!(WasmToVError::WasmParse(format!(
                            "duplicate `{}` custom section",
                            crate::SPEC_FUNCS_SECTION_NAME
                        ))));
                    }
                    embedded_spec_funcs = Some(decode_spec_funcs_section(custom_section.data())?);
                } else if custom_section.name() == inference_hassert::HSPECS_SECTION_NAME {
                    if embedded_hspecs.is_some() {
                        return Err(anyhow::anyhow!(WasmToVError::WasmParse(format!(
                            "duplicate `{}` custom section",
                            inference_hassert::HSPECS_SECTION_NAME
                        ))));
                    }
                    embedded_hspecs = Some(
                        inference_hassert::decode(custom_section.data()).map_err(|e| {
                            anyhow::anyhow!(WasmToVError::WasmParse(format!(
                                "{} section: {e}",
                                inference_hassert::HSPECS_SECTION_NAME
                            )))
                        })?,
                    );
                } else if let inf_wasmparser::KnownCustom::Name(name_section) =
                    custom_section.as_known()
                {
                    if seen_name_section {
                        return Err(anyhow::anyhow!(WasmToVError::WasmParse(
                            "duplicate WASM `name` custom section".into(),
                        )));
                    }
                    seen_name_section = true;
                    for name in name_section {
                        let name = name?;
                        match name {
                            inf_wasmparser::Name::Module { name, .. } => {
                                wasm_parse_data.mod_name = name.to_string();
                                // The embedded `name` section bypasses the
                                // CLI-side validation. Re-run validation so
                                // a hand-crafted binary cannot smuggle an
                                // invalid identifier — or a name the emitted
                                // preamble already occupies — into Rocq
                                // emission.
                                validate_rocq_identifier(&wasm_parse_data.mod_name)?;
                                validate_module_name_available(&wasm_parse_data.mod_name)?;
                            }
                            inf_wasmparser::Name::Function(func_names) => {
                                let mut func_names_map = HashMap::new();
                                let mut raw_func_names_map = HashMap::new();
                                for func_name in func_names {
                                    let func_name = func_name?;
                                    // Function names are emitted verbatim as
                                    // `Definition <name>`, so they must be
                                    // legal Rocq identifiers. WASM names are
                                    // not (Inference emits `Struct.method`; an
                                    // adversarial external may use a Coq
                                    // keyword). Sanitize at the decode boundary
                                    // so no illegal identifier reaches Gallina;
                                    // the translator de-duplicates the result.
                                    func_names_map.insert(
                                        func_name.index,
                                        sanitize_rocq_identifier(func_name.name),
                                    );
                                    // Retain the RAW, unsanitized name-section
                                    // string too: an `inference.hspecs`
                                    // obligation references a callee by exactly
                                    // this symbol (`is_prime`, `Point.new`, or
                                    // a merged external's `mathlib.sum`), and
                                    // `T_app` resolution keys on it —
                                    // sanitization would break the match.
                                    raw_func_names_map
                                        .insert(func_name.index, func_name.name.to_string());
                                }
                                if !func_names_map.is_empty() {
                                    wasm_parse_data.func_names_map = Some(func_names_map);
                                    wasm_parse_data.raw_func_names_map = Some(raw_func_names_map);
                                }
                            }
                            inf_wasmparser::Name::Local(locals) => {
                                let mut func_locals_name_map: HashMap<u32, HashMap<u32, String>> =
                                    HashMap::new();
                                for local in locals {
                                    let local = local?;
                                    let index = local.index;
                                    func_locals_name_map.entry(index).or_default();
                                    // Neutralized here rather than at the three
                                    // emission sites: a local name is only ever
                                    // rendered inside a `(* … *)` comment, so no
                                    // consumer needs the raw form, and one
                                    // boundary cannot drift out of step with
                                    // itself the way three call sites can.
                                    for naming in local.names {
                                        let naming = naming?;
                                        func_locals_name_map.get_mut(&index).unwrap().insert(
                                            naming.index,
                                            neutralize_comment_delimiters(naming.name),
                                        );
                                    }
                                }
                                if !func_locals_name_map.is_empty() {
                                    wasm_parse_data.func_locals_name_map =
                                        Some(func_locals_name_map);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            // An unrecognised section id carries content this translator cannot
            // account for, so a `.v` emitted alongside it would describe only
            // the part of the module that happened to be understood.
            UnknownSection { id, .. } => {
                return Err(anyhow::anyhow!(WasmToVError::UnsupportedFeature {
                    description: format!("unknown WASM section id {id}"),
                }));
            }

            // Once we've reached the end of a parser we either resume
            // at the parent parser or the payload iterator is at its
            // end and we're done.
            End(_) => {}
            _ => {
                return Err(anyhow::anyhow!(WasmToVError::WasmParse(
                    "unexpected WASM payload variant in module".into(),
                )));
            }
        }
    }

    // Every defined function's type comes from its function-section entry. With
    // fewer entries than bodies the emitted `modfunc_type` was fabricated from a
    // default, giving the `.v` a function whose signature the binary never
    // stated; with more, a declared function has no body at all.
    if wasm_parse_data.function_type_indexes.len() != wasm_parse_data.function_bodies.len() {
        return Err(anyhow::anyhow!(WasmToVError::WasmParse(format!(
            "function section declares {} function(s) but the code section carries {} bod(ies)",
            wasm_parse_data.function_type_indexes.len(),
            wasm_parse_data.function_bodies.len(),
        ))));
    }

    // The data count section exists so `memory.init` and `data.drop` can be
    // validated before the data section is read; a count disagreeing with the
    // segments that follow makes those indices mean something other than what
    // the module says.
    if let Some(declared) = declared_data_count {
        let actual = u32::try_from(wasm_parse_data.data.len()).unwrap_or(u32::MAX);
        if declared != actual {
            return Err(anyhow::anyhow!(WasmToVError::WasmParse(format!(
                "data count section declares {declared} segment(s) but the data \
                 section carries {actual}"
            ))));
        }
    }

    if let Some(embedded) = embedded_spec_funcs {
        if explicit_non_empty {
            if wasm_parse_data.spec_funcs_by_spec != embedded {
                return Err(anyhow::anyhow!(WasmToVError::EmbeddedSpecMismatch {
                    explicit: wasm_parse_data.spec_funcs_by_spec.clone(),
                    embedded,
                }));
            }
        } else {
            wasm_parse_data.spec_funcs_by_spec = embedded;
        }
    }

    // Same explicit-vs-embedded reconciliation for `inference.hspecs` as for
    // `inference.spec_funcs` above: an explicit non-empty map must match the
    // embedded section exactly; an empty explicit map adopts the embedded one
    // (the post-link CLI path, where the linker rewrote the section); an
    // explicit map with no embedded section wins (a pre-link translation).
    if let Some(embedded) = embedded_hspecs {
        if explicit_hspecs_non_empty {
            if wasm_parse_data.hspecs_by_spec != embedded {
                return Err(anyhow::anyhow!(WasmToVError::EmbeddedHspecsMismatch {
                    explicit: wasm_parse_data.hspecs_by_spec.clone(),
                    embedded,
                }));
            }
        } else {
            wasm_parse_data.hspecs_by_spec = embedded;
        }
    }

    // Cross-invariant: every spec carrying obligations must be a spec the
    // `inference.spec_funcs` section knows about. It is a subset, not an
    // equality — a spec block may contain only methods (function indices but no
    // free-function obligations), so a spec name can appear in `spec_funcs`
    // with no matching `hspecs` entry, but never the reverse. A `.wasm` whose
    // two sections disagree here is a corrupt proof artifact.
    for spec_name in wasm_parse_data.hspecs_by_spec.keys() {
        if !wasm_parse_data.spec_funcs_by_spec.contains_key(spec_name) {
            return Err(anyhow::anyhow!(WasmToVError::HspecInconsistent(format!(
                "spec `{spec_name}` carries obligations but is absent from the \
                 `inference.spec_funcs` section"
            ))));
        }
    }

    Ok(wasm_parse_data)
}

/// Position of a section id in the order the WebAssembly binary format fixes.
///
/// The ids are deliberately not compared directly: they are *not* monotonic in
/// the required order. The data count section carries id 12 but must sit between
/// the element section (9) and the code section (10), so `id > previous_id`
/// would reject every module that uses bulk memory. The tag section (13)
/// likewise belongs between memory and global.
///
/// An id outside the core set ranks last; it cannot reach an ordering decision,
/// because an unknown section is refused outright before its content is read.
fn section_rank(id: u8) -> u8 {
    match id {
        1 => 1,    // type
        2 => 2,    // import
        3 => 3,    // function
        4 => 4,    // table
        5 => 5,    // memory
        13 => 6,   // tag
        6 => 7,    // global
        7 => 8,    // export
        8 => 9,    // start
        9 => 10,   // element
        12 => 11,  // data count
        10 => 12,  // code
        11 => 13,  // data
        _ => u8::MAX,
    }
}

/// Decodes the `inference.spec_funcs` custom section payload.
///
/// Schema (LEB128 u32 throughout):
/// ```text
/// version
/// count
/// repeat count times:
///   spec_name_len   spec_name_bytes (utf-8)
///   indices_count   repeat indices_count times: func_idx
/// ```
///
/// LEB128 reads and the length-prefixed UTF-8 spec-name read are delegated to
/// `inf_wasmparser::BinaryReader`, which already enforces canonical LEB128
/// encoding (overlong rejection, integer-too-large rejection) and UTF-8
/// validation. Errors are mapped to `WasmToVError::WasmParse` so the existing
/// downcast points in the CLI keep working.
fn decode_spec_funcs_section(data: &[u8]) -> anyhow::Result<FxHashMap<String, Vec<u32>>> {
    use inf_wasmparser::BinaryReader;

    let mut reader = BinaryReader::new(data, 0);

    let version = reader
        .read_var_u32()
        .map_err(|e| anyhow::anyhow!(WasmToVError::WasmParse(format!(
            "spec_funcs section: truncated LEB128 in version: {e}"
        ))))?;
    if version != crate::SPEC_FUNCS_SECTION_VERSION {
        return Err(anyhow::anyhow!(WasmToVError::WasmParse(format!(
            "unsupported inference.spec_funcs version {version} (expected {})",
            crate::SPEC_FUNCS_SECTION_VERSION
        ))));
    }

    let count = reader
        .read_var_u32()
        .map_err(|e| anyhow::anyhow!(WasmToVError::WasmParse(format!(
            "spec_funcs section: truncated LEB128 in count: {e}"
        ))))?;
    // A malformed binary advertising `count = 0xFFFFFFFF` could trigger a
    // multi-gigabyte allocation if we trusted it. Each pair consumes at least
    // two bytes (one for the name-length LEB128, one for the indices-count
    // LEB128); bound the count by half the remaining payload size before
    // allocating.
    if count as usize > reader.bytes_remaining() / 2 {
        return Err(anyhow::anyhow!(WasmToVError::WasmParse(
            "spec_funcs section: declared pair count exceeds remaining payload".into(),
        )));
    }

    let mut out: FxHashMap<String, Vec<u32>> = FxHashMap::default();
    for _ in 0..count {
        let name = reader
            .read_string()
            .map_err(|e| {
                // `read_string` reports "malformed UTF-8 encoding" for bad UTF-8
                // and "unexpected end-of-file" for truncation. Surface a stable
                // prefix that the existing downcast tests grep on.
                let msg = e.to_string();
                let prefix = if msg.contains("UTF-8") {
                    "spec_funcs section: invalid UTF-8 in spec name"
                } else {
                    "spec_funcs section: truncated LEB128 or name body"
                };
                anyhow::anyhow!(WasmToVError::WasmParse(format!("{prefix}: {msg}")))
            })?
            .to_string();
        // Cap individual spec names defensively. The encoder side enforces a
        // 255-character limit via `validate_rocq_identifier`'s `TooLong` rule,
        // but a hand-crafted payload could advertise a much longer name; cap
        // here so the per-name allocation stays bounded regardless of payload.
        if name.len() > MAX_SPEC_NAME_LEN {
            return Err(anyhow::anyhow!(WasmToVError::WasmParse(format!(
                "spec_funcs section: spec name length {} exceeds cap {MAX_SPEC_NAME_LEN}",
                name.len()
            ))));
        }
        // Validate at the decode boundary so a hand-crafted binary cannot
        // smuggle an invalid Rocq identifier (empty name, `__`, reserved
        // keyword, stdlib shadow) past `translate()`'s per-spec check.
        validate_rocq_identifier(&name)?;

        let idx_count = reader
            .read_var_u32()
            .map_err(|e| anyhow::anyhow!(WasmToVError::WasmParse(format!(
                "spec_funcs section: truncated LEB128 in indices count: {e}"
            ))))?;
        // Same defense as for `count` above: each index consumes at least one
        // payload byte, so `idx_count` cannot legitimately exceed what's left.
        if idx_count as usize > reader.bytes_remaining() {
            return Err(anyhow::anyhow!(WasmToVError::WasmParse(
                "spec_funcs section: declared index count exceeds remaining payload".into(),
            )));
        }
        let mut indices = Vec::with_capacity(idx_count as usize);
        for _ in 0..idx_count {
            indices.push(
                reader
                    .read_var_u32()
                    .map_err(|e| anyhow::anyhow!(WasmToVError::WasmParse(format!(
                        "spec_funcs section: truncated LEB128 in func index: {e}"
                    ))))?,
            );
        }
        out.insert(name, indices);
    }
    // Reject trailing bytes: every byte of the payload must be accounted for
    // by the (version, count, repeated (name, indices)) schema. A malformed
    // binary with extra bytes after the last entry is silently accepted
    // otherwise, weakening the canonical-encoding guarantee.
    if reader.bytes_remaining() != 0 {
        return Err(anyhow::anyhow!(WasmToVError::WasmParse(format!(
            "spec_funcs section: {} trailing byte(s) after last entry",
            reader.bytes_remaining()
        ))));
    }
    Ok(out)
}

/// Cap on the declared length of any single spec name embedded in the
/// `inference.spec_funcs` custom section. Matches the
/// [`crate::rocq_names::validate_rocq_identifier`] `TooLong` threshold so the
/// decode-time and encode-time limits are aligned.
const MAX_SPEC_NAME_LEN: usize = 255;
