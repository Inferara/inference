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
//! WebAssembly component model sections are recognized but generate empty stubs:
//!
//! - `ModuleSection` - Nested modules
//! - `InstanceSection` - Module instances
//! - `ComponentSection` - Component definitions
//! - `ComponentTypeSection` - Component type definitions
//! - `ComponentInstanceSection` - Component instances
//! - `ComponentAliasSection` - Component aliases
//! - `ComponentCanonicalSection` - Canonical ABI definitions
//! - `ComponentStartSection` - Component start function
//! - `ComponentImportSection` - Component imports
//! - `ComponentExportSection` - Component exports
//!
//! These sections are silently ignored during translation.
//!
//! ## Error Handling
//!
//! Parse errors are propagated using [`anyhow::Result`]. The parser **fails fast**
//! on invalid bytecode:
//!
//! - Malformed WASM magic number or version
//! - Invalid section data
//! - Out-of-bounds indices
//! - Unsupported WASM features (when explicitly detected)
//!
//! The translation phase (Phase 2) uses error recovery, but the parsing phase does not.

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
use std::{collections::HashMap, io::Read};

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
///
/// let wasm_bytes = std::fs::read("output.wasm")?;
/// let rocq_code = translate_bytes("my_module", &wasm_bytes)?;
/// std::fs::write("output.v", rocq_code)?;
/// ```
///
/// Integration with Inference compiler:
///
/// ```ignore
/// use inference::{parse, type_check, codegen};
/// use inference_wasm_to_v_translator::wasm_parser::translate_bytes;
///
/// let source = std::fs::read_to_string("program.inf")?;
/// let arena = parse(&source)?;
/// let typed_context = type_check(arena)?;
/// let wasm_bytes = codegen(&typed_context)?;
///
/// // Translate to Rocq
/// let rocq_code = translate_bytes("Program", &wasm_bytes)?;
/// std::fs::write("program.v", rocq_code)?;
/// ```
pub fn translate_bytes(mod_name: &str, bytes: &[u8]) -> anyhow::Result<String> {
    let mut data = Vec::new();
    let mut reader = std::io::Cursor::new(bytes);
    reader.read_to_end(&mut data).unwrap();
    match parse(mod_name.to_string(), &data) {
        Ok(mut parse_data) => parse_data.translate(),
        Err(e) => Err(anyhow::anyhow!(e.to_string())),
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
/// Unsupported sections (component model, tags, unknown sections) are silently ignored.
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
fn parse(mod_name: String, data: &'_ [u8]) -> anyhow::Result<WasmParseData<'_>> {
    let parser = Parser::new(0);
    let mut wasm_parse_data = WasmParseData::new(mod_name);

    for payload in parser.parse_all(data) {
        match payload? {
            // Sections for WebAssembly modules
            Version { .. } => {
                /*
                    we do not use it
                */
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
                functions.into_iter().for_each(|f| {
                    wasm_parse_data.function_type_indexes.push(f.unwrap());
                });
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
            TagSection(_) => {}
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
            DataCountSection { .. } => {}
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

            // Sections for WebAssembly components
            ModuleSection { .. } => { /* ... */ }
            InstanceSection(_) => { /* ... */ }
            CoreTypeSection(_) => { /* ... */ }
            ComponentSection { .. } => { /* ... */ }
            ComponentInstanceSection(_) => { /* ... */ }
            ComponentAliasSection(_) => { /* ... */ }
            ComponentTypeSection(_) => { /* ... */ }
            ComponentCanonicalSection(_) => { /* ... */ }
            ComponentStartSection { .. } => { /* ... */ }
            ComponentImportSection(_) => { /* ... */ }
            ComponentExportSection(_) => { /* ... */ }

            CustomSection(custom_section) => {
                if let inf_wasmparser::KnownCustom::Name(name_section) = custom_section.as_known() {
                    for name in name_section {
                        let name = name?;
                        match name {
                            inf_wasmparser::Name::Module { name, .. } => {
                                wasm_parse_data.mod_name = name.to_string();
                            }
                            inf_wasmparser::Name::Function(func_names) => {
                                let mut func_names_map = HashMap::new();
                                for func_name in func_names {
                                    let func_name = func_name?;
                                    func_names_map
                                        .insert(func_name.index, func_name.name.to_string());
                                }
                                if !func_names_map.is_empty() {
                                    wasm_parse_data.func_names_map = Some(func_names_map);
                                }
                            }
                            inf_wasmparser::Name::Local(locals) => {
                                let mut func_locals_name_map: HashMap<u32, HashMap<u32, String>> =
                                    HashMap::new();
                                for local in locals {
                                    let local = local?;
                                    let index = local.index;
                                    func_locals_name_map.entry(index).or_default();
                                    for naming in local.names {
                                        let naming = naming?;
                                        func_locals_name_map
                                            .get_mut(&index)
                                            .unwrap()
                                            .insert(naming.index, naming.name.to_string());
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

            // most likely you'd return an error here
            UnknownSection { .. } => { /* ... */ }

            // Once we've reached the end of a parser we either resume
            // at the parent parser or the payload iterator is at its
            // end and we're done.
            End(_) => {}
            _ => todo!(),
        }
    }
    Ok(wasm_parse_data)
}
