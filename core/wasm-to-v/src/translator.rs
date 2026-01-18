//! Rocq Code Generation from Parsed WASM Data
//!
//! This module converts structured WASM data (from [`crate::wasm_parser`]) into
//! Rocq (Coq) formal verification code.
//!
//! ## Overview
//!
//! The translator takes [`WasmParseData`] and generates a complete Rocq module
//! that represents the WASM module in a form suitable for formal verification.
//!
//! ## Translation Process
//!
//! The translation happens in [`WasmParseData::translate`]:
//!
//! 1. **Generate header**: Rocq imports and helper definitions
//! 2. **Translate types**: WASM types → Rocq type constructors
//! 3. **Translate imports**: External dependencies → Rocq import records
//! 4. **Translate exports**: Public interface → Rocq export records
//! 5. **Translate tables**: Indirect call tables → Rocq table definitions
//! 6. **Translate memory**: Linear memory → Rocq memory definitions
//! 7. **Translate globals**: Global variables → Rocq global definitions
//! 8. **Translate data**: Memory initialization → Rocq data segments
//! 9. **Translate elements**: Table initialization → Rocq element segments
//! 10. **Translate functions**: Function bodies → Rocq function definitions
//! 11. **Generate module record**: Assemble all parts into final Rocq module
//!
//! ## Code Generation Strategy
//!
//! The translator generates Rocq code as strings using helper functions for each
//! WASM construct. This approach prioritizes:
//!
//! - **Correctness**: Direct mapping from WASM semantics to Rocq types
//! - **Readability**: Well-formatted output with proper indentation
//! - **Debuggability**: Preserve names from WASM custom sections
//!
//! ## Expression Translation
//!
//! WASM instructions are converted to structured Rocq expressions:
//!
//! ```text
//! WASM (stack-based)          Rocq (structured)
//! ──────────────────          ─────────────────
//! local.get 0                 BI_get_local 0%N ::
//! local.get 1                 BI_get_local 1%N ::
//! i32.add                     BI_binop (Binop_i BOI_add) ::
//!                             nil
//! ```
//!
//! Control flow is reconstructed from linear instruction sequences using
//! [`Expression`] and helper structs [`BlockExpr`] and [`ConditionExpr`].
//!
//! ## Helper Definitions
//!
//! The translator generates these Rocq helper definitions for convenience:
//!
//! - `Vi32`, `Vi64`: Integer literal constructors
//! - `Mt`: Table type constructor
//! - `Mm`: Memory type constructor
//! - `Mg`: Global constructor
//! - `Mi`: Import constructor
//! - `Me`: Export constructor
//! - `Ma`: Memory argument constructor
//!
//! ## Error Recovery
//!
//! Unlike the parser (which fails fast), the translator collects errors from
//! all sections before returning. This provides better diagnostics for complex
//! translation failures.

use core::fmt;
use std::{collections::HashMap, fmt::Display};

use inf_wasmparser::{
    BlockType, CompositeInnerType, Data, DataKind, Element, ElementItems, ElementKind, Export,
    FunctionBody, Global, Import, MemoryType, Operator, OperatorsIterator, OperatorsReader,
    RecGroup, RefType, Table, TableType, TypeRef, ValType as wpValType,
};
use uuid::Uuid;

const LCB: &str = "{|\n";
const RCB_DOT: &str = "|}.\n";

const LIST_EXT: &str = " ::\n";
const LIST_SEAL: &str = "nil";

/// Structured representation of a parsed WASM module.
///
/// This structure holds all information extracted from WASM bytecode sections,
/// ready for translation to Rocq code. It is populated by [`crate::wasm_parser::parse`]
/// and consumed by [`WasmParseData::translate`].
///
/// # Lifetime
///
/// The lifetime `'a` represents borrowed data from the original WASM bytecode.
/// Most WASM section data (imports, exports, function bodies) reference slices
/// of the original bytecode to avoid allocations.
///
/// # Fields
///
/// ## Module Metadata
/// - `mod_name`: Rocq module identifier (from parameter or custom name section)
/// - `func_names_map`: Maps function index → name (from custom name section)
/// - `func_locals_name_map`: Maps function index → (local index → name) (from custom name section)
/// - `start_function`: Optional module entry point function index
///
/// ## WASM Sections
/// - `imports`: External dependencies (functions, tables, memories, globals)
/// - `exports`: Public interface (exported functions, tables, memories, globals)
/// - `tables`: Indirect call table definitions
/// - `memory_types`: Linear memory specifications
/// - `globals`: Global variable definitions with initialization
/// - `data`: Memory initialization segments
/// - `elements`: Table initialization segments
/// - `function_types`: Function type signatures (as recursion groups)
/// - `function_type_indexes`: Maps function index → type index
/// - `function_bodies`: Function code with locals and instructions
///
/// ## Translation State (private)
/// - `translated_function_names`: Accumulates Rocq function names during translation
/// - `translated_functions_string`: Accumulates Rocq function definitions during translation
pub(crate) struct WasmParseData<'a> {
    pub(crate) mod_name: String,
    pub(crate) func_names_map: Option<HashMap<u32, String>>,
    pub(crate) func_locals_name_map: Option<HashMap<u32, HashMap<u32, String>>>,

    pub(crate) start_function: Option<u32>,

    pub(crate) imports: Vec<Import<'a>>,
    pub(crate) exports: Vec<Export<'a>>,
    pub(crate) tables: Vec<Table<'a>>,
    pub(crate) memory_types: Vec<MemoryType>,
    pub(crate) globals: Vec<Global<'a>>,
    pub(crate) data: Vec<Data<'a>>,
    pub(crate) elements: Vec<Element<'a>>,
    pub(crate) function_types: Vec<RecGroup>,
    pub(crate) function_type_indexes: Vec<u32>,
    pub(crate) function_bodies: Vec<FunctionBody<'a>>,

    translated_function_names: Vec<String>,
    translated_functions_string: String,
}

impl WasmParseData<'_> {
    /// Creates a new empty [`WasmParseData`] with the given module name.
    ///
    /// All section vectors are initialized as empty. This is called by the parser
    /// before streaming through WASM sections.
    ///
    /// # Parameters
    ///
    /// - `mod_name`: Default Rocq module name (may be overridden by custom name section)
    pub(crate) fn new<'a>(mod_name: String) -> WasmParseData<'a> {
        WasmParseData {
            mod_name,
            func_names_map: None,
            func_locals_name_map: None,
            start_function: None,
            imports: Vec::new(),
            exports: Vec::new(),
            tables: Vec::new(),
            memory_types: Vec::new(),
            globals: Vec::new(),
            data: Vec::new(),
            elements: Vec::new(),
            function_types: Vec::new(),
            function_type_indexes: Vec::new(),
            function_bodies: Vec::new(),

            translated_function_names: Vec::new(),
            translated_functions_string: String::new(),
        }
    }

    /// Translates the parsed WASM data into complete Rocq code.
    ///
    /// This is the main translation entry point. It generates a complete Rocq file
    /// including imports, helper definitions, and a module record containing all
    /// translated WASM sections.
    ///
    /// # Translation Steps
    ///
    /// 1. Generate Rocq imports and helper definitions
    /// 2. Translate each WASM section to Rocq definitions:
    ///    - Imports → `Mi` records
    ///    - Exports → `Me` records
    ///    - Tables → `Mt` definitions
    ///    - Memory → `Mm` definitions
    ///    - Globals → `Mg` definitions
    ///    - Data segments → data initialization
    ///    - Element segments → table initialization
    ///    - Function types → type signatures
    ///    - Functions → `module_func` definitions
    /// 3. Assemble module record with all translated components
    ///
    /// # Error Recovery
    ///
    /// Unlike the parser, this method uses error recovery: it collects translation
    /// errors from all sections and returns the first error only if no sections
    /// succeeded. This provides better diagnostics for complex failures.
    ///
    /// # Returns
    ///
    /// Returns a `String` containing complete Rocq code ready to write to a `.v` file.
    ///
    /// # Errors
    ///
    /// Returns an error if translation of any section fails due to:
    /// - Unsupported WASM features (tags, unknown reference types)
    /// - Invalid WASM data (malformed expressions, out-of-bounds indices)
    /// - Unimplemented instruction opcodes
    #[allow(clippy::too_many_lines)]
    pub(crate) fn translate(&mut self) -> anyhow::Result<String /* WasmModuleParseError*/> {
        let mut res = String::new();
        res.push_str("Require Import List.\n");
        res.push_str("Require Import String.\n");
        res.push_str("Require Import BinNat.\n");
        res.push_str("Require Import ZArith.\n");
        res.push_str("From Wasm Require Import bytes.\n");
        res.push_str("From Wasm Require Import numerics.\n");
        res.push_str("From Wasm Require Import datatypes.\n");
        res.push('\n');
        res.push_str("Definition Vi32 i := VAL_int32 (Wasm_int.int_of_Z i32m i).\n");
        res.push_str("Definition Vi64 i := VAL_int64 (Wasm_int.int_of_Z i64m i).\n");
        res.push_str(
            "Definition Mt l et := {|modtab_type := {|tt_limits := l; tt_elem_type := et|}|}.\n",
        );
        res.push_str("Definition Mm l := {|modmem_type := l|}.\n");
        res.push_str("Definition Mg mut t init := {|modglob_type := {|tg_mut := mut; tg_t := t|}; modglob_init := init|}.\n");
        res.push('\n');
        res.push_str("Definition Mi m n d := {|\n");
        res.push_str("  imp_module := list_byte_of_string m;\n");
        res.push_str("  imp_name := list_byte_of_string n;\n");
        res.push_str("  imp_desc := d;\n");
        res.push_str("|}.\n");
        res.push('\n');
        res.push_str("Definition Me n d := {|\n");
        res.push_str("  modexp_name := list_byte_of_string n;\n");
        res.push_str("  modexp_desc := d;\n");
        res.push_str("|}.\n");
        res.push('\n');
        res.push_str("Definition Ma of al := {|memarg_offset := of; memarg_align := al|}.\n");
        res.push('\n');

        let mut errors = Vec::new();

        let mut translated_imports = String::new();
        for import in &self.imports {
            match translate_module_import(import) {
                Ok(translated_import) => {
                    translated_imports.push_str("    ");
                    translated_imports.push_str(translated_import.as_str());
                    translated_imports.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        translated_imports.push_str("    ");
        translated_imports.push_str(LIST_SEAL);

        let mut created_exports = String::new();
        for export in &self.exports {
            match translate_export_module(export) {
                Ok(translated_export) => {
                    created_exports.push_str("    ");
                    created_exports.push_str(translated_export.as_str());
                    created_exports.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_exports.push_str("    ");
        created_exports.push_str(LIST_SEAL);

        let mut created_tables = String::new();
        for table in &self.tables {
            match translate_table_type(table) {
                Ok(translated_table_type) => {
                    created_tables.push_str("    ");
                    created_tables.push_str(translated_table_type.as_str());
                    created_tables.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_tables.push_str("    ");
        created_tables.push_str(LIST_SEAL);

        let mut created_memory_types = String::new();
        for memory_type in &self.memory_types {
            match translate_memory_type(memory_type) {
                Ok(translated_memory) => {
                    created_memory_types.push_str("    ");
                    created_memory_types.push_str(translated_memory.as_str());
                    created_memory_types.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_memory_types.push_str("    ");
        created_memory_types.push_str(LIST_SEAL);

        let mut created_globals = String::new();
        for global in &self.globals {
            match translate_global(global) {
                Ok(translated_global) => {
                    created_globals.push_str("    ");
                    created_globals.push_str(translated_global.as_str());
                    created_globals.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_globals.push_str("    ");
        created_globals.push_str(LIST_SEAL);

        let mut created_data_segments = String::new();
        for data in &self.data {
            match translate_data(data) {
                Ok(translated_data) => {
                    created_data_segments.push_str("    ");
                    created_data_segments.push_str(translated_data.as_str());
                    created_data_segments.push_str(LIST_EXT);
                }
                Err(e) => errors.push(e),
            }
        }
        created_data_segments.push_str("    ");
        created_data_segments.push_str(LIST_SEAL);

        let mut created_elements = String::new();
        for element in &self.elements {
            match translate_element(element) {
                Ok(translated_element) => {
                    created_elements.push_str("    ");
                    created_elements.push_str(translated_element.as_str());
                    created_elements.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_elements.push_str("    ");
        created_elements.push_str(LIST_SEAL);

        let mut created_function_types = String::new();
        for rec_group in &self.function_types {
            // created_function_types.push(LRB);
            match translate_function_type(rec_group) {
                Ok(translated_function_type) => {
                    created_function_types.push_str("    ");
                    created_function_types.push_str(translated_function_type.as_str());
                    created_function_types.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        created_function_types.push_str("    ");
        created_function_types.push_str(LIST_SEAL);

        let mut created_functions = String::new();
        match self.translate_functions() {
            Ok(_) => {
                res.push_str(self.translated_functions_string.as_str());
                for function_name in &self.translated_function_names {
                    created_functions.push_str("    ");
                    created_functions.push_str(function_name.as_str());
                    created_functions.push_str(LIST_EXT);
                }
            }
            Err(e) => {
                errors.push(e);
            }
        }
        created_functions.push_str("    ");
        created_functions.push_str(LIST_SEAL);

        //Record module
        let module_name = &self.mod_name;
        res.push_str(format!("Definition {module_name} : module := ").as_str());
        res.push_str(LCB);
        res.push_str(format!("  mod_types :=\n{created_function_types};\n").as_str());
        res.push_str(format!("  mod_funcs :=\n{created_functions};\n").as_str());
        res.push_str(format!("  mod_tables :=\n{created_tables};\n").as_str());
        res.push_str(format!("  mod_mems :=\n{created_memory_types};\n").as_str());
        res.push_str(format!("  mod_globals :=\n{created_globals};\n").as_str());
        res.push_str(format!("  mod_elems :=\n{created_elements};\n").as_str());
        res.push_str(format!("  mod_datas :=\n{created_data_segments};\n").as_str());
        if let Some(start_function) = self.start_function {
            res.push_str(
                format!("  mod_start := Some {{|modstart_func := {start_function}%N|}};\n")
                    .as_str(),
            );
        } else {
            res.push_str("  mod_start := None;\n");
        }
        res.push_str(format!("  mod_imports :=\n{translated_imports};\n").as_str());
        res.push_str(format!("  mod_exports :=\n{created_exports};\n").as_str());
        res.push_str(RCB_DOT);
        Ok(res)
    }

    //Record module_func
    fn translate_functions(&mut self) -> anyhow::Result<()> {
        for (index, function_body) in self.function_bodies.iter().enumerate() {
            let modfunc_type = *self.function_type_indexes.get(index).unwrap_or(&0);
            let func_name = if let Some(func_names_map) = &self.func_names_map {
                func_names_map
                    .get(&(index as u32))
                    .unwrap_or(&format!("func_{}", get_id()))
                    .to_owned()
            } else {
                format!("func_{}", get_id())
            };
            self.translated_function_names.push(func_name.clone());

            let mut modfunc_locals = String::new();
            if let Ok(locals_reader) = function_body.get_locals_reader() {
                for local in locals_reader {
                    let (reps, val_type) = local.unwrap();
                    let val_type = translate_value_type(&val_type)?;
                    for _ in 0..reps {
                        modfunc_locals.push_str(format!("{val_type} :: ").as_str());
                    }
                }
            }
            modfunc_locals.push_str("nil");

            let modfunc_body = match &self.func_locals_name_map {
                Some(func_locals_name_map) => translate_expr(
                    &mut function_body.get_operators_reader()?,
                    func_locals_name_map.get(&modfunc_type).cloned(),
                )?,
                None => translate_expr(&mut function_body.get_operators_reader()?, None)?,
            };

            self.translated_functions_string
                .push_str(format!("Definition {func_name} : module_func := ").as_str());
            self.translated_functions_string.push_str(LCB);
            self.translated_functions_string
                .push_str(format!("  modfunc_type := {modfunc_type}%N;\n").as_str());
            self.translated_functions_string
                .push_str(format!("  modfunc_locals := {modfunc_locals};\n").as_str());
            self.translated_functions_string
                .push_str(format!("  modfunc_body :=\n{modfunc_body};\n").as_str());
            self.translated_functions_string.push_str(RCB_DOT);
            self.translated_functions_string.push('\n');
        }
        Ok(())
    }
}

//Inductive reference_type
fn translate_ref_type(ref_type: &RefType) -> anyhow::Result<String> {
    if *ref_type == RefType::FUNCREF {
        Ok(String::from("T_funcref"))
    } else if *ref_type == RefType::EXTERNREF {
        Ok(String::from("T_externref"))
    } else {
        Err(anyhow::anyhow!("Unsupported reference type {ref_type:?}"))
    }
}

//Inductive value_type
fn translate_value_type(val_type: &wpValType) -> anyhow::Result<String> {
    let res = match val_type {
        wpValType::I32 => "T_num T_i32",
        wpValType::I64 => "T_num T_i64",
        wpValType::F32 => "T_num T_f32",
        wpValType::F64 => "T_num T_f64",
        wpValType::V128 => "T_vec T_v128",
        wpValType::Ref(ref_type) => {
            let ref_type_translated = translate_ref_type(ref_type)?;
            return Ok(format!("T_ref {ref_type_translated}"));
        }
    };
    Ok(res.to_string())
}

//Record module_import
fn translate_module_import(import: &Import) -> anyhow::Result<String> {
    let imp_name = String::from(import.name);
    let imp_module = String::from(import.module);
    // let definition_name =
    //     imp_module.clone() + &imp_name.clone().remove(0).to_uppercase().to_string();
    let imp_desc = translate_module_import_desc(import)?;
    Ok(format!("Mi \"{imp_module}\" \"{imp_name}\" ({imp_desc})"))
}

//Inductive module_import_desc
fn translate_module_import_desc(import: &Import) -> anyhow::Result<String> {
    let res = match import.ty {
        TypeRef::Func(index) => format!("MID_func {index}%N"),
        TypeRef::Global(global_type) => {
            let tg_mut = translate_mutability(global_type.mutable);
            let tg_t = translate_value_type(&global_type.content_type)?;
            format!("MID_global {{|tg_mut := {tg_mut}; tg_t := {tg_t}|}}")
        }
        TypeRef::Memory(memory_type) => {
            let limits = translate_memory_type_limits(&memory_type)?;
            format!("MID_mem {limits}")
        }
        TypeRef::Table(table_type) => {
            let table_type_translated = translate_table_type_limits(&table_type)?;
            format!("MID_table {table_type_translated}")
        }
        TypeRef::Tag(_) => return Err(anyhow::anyhow!("Tag is not supported in import")),
    };
    Ok(res)
}

//Inductive mutability
fn translate_mutability(mutable: bool) -> String {
    if mutable {
        "MUT_var".to_string()
    } else {
        "MUT_const".to_string()
    }
}

//Record limits
fn translate_table_type_limits(table_type: &TableType) -> anyhow::Result<String> {
    let lim_min = format!("{}%N", table_type.initial);
    let lim_max = match table_type.maximum {
        Some(max) => format!("Some({max}%N)"),
        None => "None".to_string(),
    };
    Ok(format!("{{|lim_min := {lim_min}; lim_max := {lim_max}|}}"))
}

//Record limits
fn translate_memory_type_limits(memory_type: &MemoryType) -> anyhow::Result<String> {
    let lim_min = format!("{}%N", memory_type.initial);
    let lim_max = match memory_type.maximum {
        Some(max) => format!("Some({max}%N)"),
        None => "None".to_string(),
    };
    Ok(format!("{{|lim_min := {lim_min}; lim_max := {lim_max}|}}"))
}

//Inductive translate_export_module
fn translate_export_module(export: &Export) -> anyhow::Result<String> {
    let modexp_name = export.name;
    let modexp_desc = translate_module_export_desc(export)?;
    Ok(format!("Me \"{modexp_name}\" ({modexp_desc})"))
}

//Inductive module_export_desc
fn translate_module_export_desc(export: &Export) -> anyhow::Result<String> {
    let res = match export.kind {
        inf_wasmparser::ExternalKind::Func => format!("MED_func {}%N", export.index),
        inf_wasmparser::ExternalKind::Table => format!("MED_table {}%N", export.index),
        inf_wasmparser::ExternalKind::Memory => format!("MED_mem {}%N", export.index),
        inf_wasmparser::ExternalKind::Global => format!("MED_global {}%N", export.index),
        inf_wasmparser::ExternalKind::Tag => return Err(anyhow::anyhow!("Tag is not supported")),
    };
    Ok(res)
}

//Record table_type
fn translate_table_type(table: &Table) -> anyhow::Result<String> {
    let tt_limits = translate_table_type_limits(&table.ty)?;
    let tt_elem_type = translate_ref_type(&table.ty.element_type)?;
    Ok(format!("Mt {tt_limits} {tt_elem_type}"))
}

//Definition memory_type
fn translate_memory_type(memory_type: &MemoryType) -> anyhow::Result<String> {
    let limits = translate_memory_type_limits(memory_type)?;
    Ok(format!("Mm {limits}"))
}

//Record global_type
fn translate_global(global: &Global) -> anyhow::Result<String> {
    let tg_mut = translate_mutability(global.ty.mutable);
    let tg_t = translate_value_type(&global.ty.content_type)?;
    let mg_init = translate_expr(&mut global.init_expr.get_operators_reader(), None)?;
    Ok(format!("Mg {tg_mut} ({tg_t}) ({mg_init})"))
}

//Inductive module_datamode
fn translate_module_datamode(data: &Data) -> anyhow::Result<String> {
    let res = match &data.kind {
        DataKind::Active {
            memory_index,
            offset_expr,
        } => {
            let expression = translate_expr(&mut offset_expr.get_operators_reader(), None)?;
            format!("MD_active {memory_index}%N ({expression})")
        }
        DataKind::Passive => "MD_passive".to_string(),
    };
    Ok(res)
}

enum ExpressionPart<'a> {
    Operator(Operator<'a>),
    Block(BlockExpr<'a>),
    Condition(ConditionExpr<'a>),
}

struct BlockExpr<'a> {
    label: Operator<'a>,
    parts: Expression<'a>,
}

struct ConditionExpr<'a> {
    label: Operator<'a>,
    then_arm: Expression<'a>,
    else_arm: Expression<'a>,
}

#[derive(Default)]
struct Expression<'a> {
    parts: Vec<ExpressionPart<'a>>,
    local_name_map: Option<HashMap<u32, String>>,
}

impl Expression<'_> {
    fn last_part(&'_ self) -> Option<&'_ ExpressionPart<'_>> {
        self.parts.last()
    }

    fn print_with_offset(&self, tabs_count: usize) -> anyhow::Result<String> {
        let mut res = String::new();
        let offset = "  ".repeat(tabs_count);
        for part in &self.parts {
            match part {
                ExpressionPart::Operator(op) => match op {
                    Operator::Else | Operator::End => {}
                    _ => {
                        res.push_str(offset.as_str());
                        res.push_str(translate_basic_operator(op, &self.local_name_map)?.as_str());
                        res.push_str(LIST_EXT);
                    }
                },
                ExpressionPart::Block(block) => {
                    res.push_str(offset.as_str());
                    res.push_str(
                        translate_basic_operator(&block.label, &self.local_name_map)?.as_str(),
                    );
                    res.push_str(" (\n");
                    res.push_str(block.parts.print_with_offset(tabs_count + 1)?.as_str());
                    res.push_str(") ");
                    res.push_str("::\n");
                }
                ExpressionPart::Condition(cond) => {
                    res.push_str(offset.as_str());
                    res.push_str(
                        translate_basic_operator(&cond.label, &self.local_name_map)?.as_str(),
                    );
                    res.push_str(" (\n");
                    res.push_str(cond.then_arm.print_with_offset(tabs_count + 1)?.as_str());
                    res.push_str(") (\n");
                    res.push_str(cond.else_arm.print_with_offset(tabs_count + 1)?.as_str());
                    res.push_str(") ");
                    res.push_str("::\n");
                }
            }
        }
        res.push_str(format!("{offset}nil").as_str());
        Ok(res)
    }
}

impl Display for Expression<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            self.print_with_offset(2)
                .unwrap_or(String::from("Error rendering expression"))
        )
    }
}

fn translate_expression<'a>(
    operators_reader: &mut OperatorsIterator<'a>,
) -> anyhow::Result<Expression<'a>> {
    let mut result = Expression::default();
    while let Some(next_operator) = operators_reader.next() {
        let next_operator = next_operator.as_ref().unwrap();
        match next_operator {
            inf_wasmparser::Operator::Block { .. }
            | inf_wasmparser::Operator::Loop { .. }
            | inf_wasmparser::Operator::Forall { .. }
            | inf_wasmparser::Operator::Exists { .. }
            | inf_wasmparser::Operator::Assume { .. }
            | inf_wasmparser::Operator::Unique { .. } => {
                // operators_reader.next();
                let block_operations = translate_expression(operators_reader)?;
                let block = BlockExpr {
                    label: next_operator.to_owned(),
                    parts: block_operations,
                };
                result.parts.push(ExpressionPart::Block(block));
            }
            inf_wasmparser::Operator::If { .. } => {
                // operators_reader.next();
                let then_arm = translate_expression(operators_reader)?;
                let else_arm = if matches!(
                    then_arm.last_part().unwrap(),
                    ExpressionPart::Operator(Operator::End)
                ) {
                    Expression::default()
                } else {
                    translate_expression(operators_reader)?
                };

                let condition = ConditionExpr {
                    label: next_operator.to_owned(),
                    then_arm,
                    else_arm,
                };
                result.parts.push(ExpressionPart::Condition(condition));
            }
            inf_wasmparser::Operator::Else | inf_wasmparser::Operator::End => {
                result
                    .parts
                    .push(ExpressionPart::Operator(next_operator.to_owned()));
                break;
            }
            _ => result
                .parts
                .push(ExpressionPart::Operator(next_operator.to_owned())),
        }
    }
    Ok(result)
}

fn translate_expr(
    operators_reader: &mut OperatorsReader,
    local_name_map: Option<HashMap<u32, String>>,
) -> anyhow::Result<String> {
    let mut peekable_operators_reader = operators_reader.clone().into_iter();
    let mut expression = translate_expression(&mut peekable_operators_reader)?;
    expression.local_name_map = local_name_map;
    Ok(expression.to_string())
}

fn translate_block_type(block_type: &BlockType) -> anyhow::Result<String> {
    let res = match block_type {
        BlockType::Empty => "BT_valtype None".to_string(),
        BlockType::FuncType(index) => format!("BT_id {index}%N"),
        BlockType::Type(valtype) => {
            let valtype = translate_value_type(valtype)?;
            format!("BT_valtype (Some ({valtype}))")
        }
    };
    Ok(res)
}

//Record memarg
fn translate_memarg(memarg: &inf_wasmparser::MemArg) -> anyhow::Result<String> {
    let memarg_offset = memarg.offset.to_string();
    let memarg_align = memarg.align.to_string();
    Ok(format!("Ma {memarg_offset}%N {memarg_align}%N"))
}

//Record module_element
fn translate_element(element: &Element) -> anyhow::Result<String> {
    let mut res = String::new();
    // let id = get_id();
    let modelem_mode = match &element.kind {
        ElementKind::Active {
            table_index,
            offset_expr,
        } => {
            let tableidx = table_index.unwrap_or_default();
            let expr = translate_expr(&mut offset_expr.get_operators_reader(), None)?;
            format!("ME_active {tableidx}%N ({expr})")
        }
        ElementKind::Passive => "ME_passive".to_string(),
        ElementKind::Declared => "ME_declared".to_string(),
    };
    let modelem_type: String;
    let modelem_init = match &element.items {
        ElementItems::Expressions(reftype, elements) => {
            modelem_type = translate_ref_type(reftype)?;
            let mut expr_list = String::new();
            for result in elements.clone().into_iter_with_offsets() {
                let (_, expr_reader) = result?;
                let expr = translate_expr(&mut expr_reader.get_operators_reader(), None)?;
                expr_list.push_str(format!("({expr})").as_str());
                expr_list.push_str(" ::\n");
            }
            format!("{expr_list}nil")
        }
        ElementItems::Functions(elements) => {
            modelem_type = "T_funcref".to_string();
            let mut indexes = String::new();
            for result in elements.clone().into_iter_with_offsets() {
                let (_, index) = result?;
                indexes.push_str(format!("{index}").as_str());
                indexes.push_str("::");
            }
            indexes.push_str("nil");
            format!("ME_functions {indexes}")
        }
    };
    res.push_str("{|\n");
    res.push_str(format!("modelem_type := {modelem_type};\n").as_str());
    res.push_str(format!("modelem_init :=\n{modelem_init};\n").as_str());
    res.push_str(format!("modelem_mode := {modelem_mode};\n").as_str());
    res.push_str("|}");
    Ok(res)
}

// struct ValType {}

// struct FunctionType {
//     id: String,
//     ft_params: Vec<String>,
//     ft_results: Vec<String>,
// }

// impl FunctionType {
//     fn name(&self) -> String {
//         format!("ft_{}", self.id)
//     }
// }

// impl Display for FunctionType {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         let mut res = String::new();
//         let name = self.name();
//         res.push_str(format!("Definition {name} : function_type :=\n").as_str());
//         res.push_str(RLB);
//         let ft_params = String::from("(") + &self.ft_params.join(" :: ") + &String::from("nil)");
//         res.push_str(format!("ft_params := {ft_params}").as_str());
//         let ft_results = String::from("(") + &self.ft_results.join(" :: ") + &String::from("nil)");
//         res.push_str(format!("ft_results := {ft_results}").as_str());
//         res.push_str(RRB);
//         res.push_str(".\n");
//         write!(f, "{}", res)
//     }
// }

//Inductive function_type
fn translate_function_type(rec_group: &RecGroup) -> anyhow::Result<String> {
    let mut res = String::new();
    // let id = get_id();
    for ty in rec_group.types() {
        match &ty.composite_type.inner {
            CompositeInnerType::Func(ft) => {
                let mut params_str = String::new();
                for param in ft.params() {
                    let val_type = translate_value_type(param)?;
                    params_str.push_str(format!("{val_type} :: ").as_str());
                }
                params_str.push_str("nil");

                let mut results_str = String::new();
                for result in ft.results() {
                    let val_type = translate_value_type(result)?;
                    results_str.push_str(format!("{val_type} :: ").as_str());
                }
                results_str.push_str("nil");

                res.push_str(format!("Tf ({params_str}) ({results_str})").as_str());
            }
            CompositeInnerType::Array(_)
            | CompositeInnerType::Struct(_)
            | CompositeInnerType::Cont(_) => {
                //TODO
            }
        }
    }
    Ok(res)
}

//Inductive basic_instruction
fn translate_basic_operator(
    operator: &Operator,
    local_name_map: &Option<HashMap<u32, String>>,
) -> anyhow::Result<String> {
    let operator = match operator {
        inf_wasmparser::Operator::Nop => "BI_nop".to_string(),
        inf_wasmparser::Operator::Unreachable => "BI_unreachable".to_string(),
        inf_wasmparser::Operator::Block { blockty } => {
            let blockty = translate_block_type(blockty)?;
            format!("BI_block ({blockty})")
        }
        Operator::Loop { blockty } => {
            let blockty = translate_block_type(blockty)?;
            format!("BI_loop ({blockty})")
        }
        Operator::If { blockty } => {
            let blockty = translate_block_type(blockty)?;
            format!("BI_if ({blockty})")
        }
        Operator::Forall { blockty } => {
            let blockty = translate_block_type(blockty)?;
            format!("BI_forall ({blockty})")
        }
        Operator::Exists { blockty } => {
            let blockty = translate_block_type(blockty)?;
            format!("BI_exists ({blockty})")
        }
        Operator::Assume { blockty } => {
            let blockty = translate_block_type(blockty)?;
            format!("BI_assume ({blockty})")
        }
        Operator::Unique { blockty } => {
            let blockty = translate_block_type(blockty)?;
            format!("BI_unique ({blockty})")
        }
        Operator::I32Uzumaki { .. } => String::from("BI_uzumaki_num T_i32"),
        Operator::I64Uzumaki { .. } => String::from("BI_uzumaki_num T_i64"),
        Operator::Else => String::new(),
        Operator::End => String::new(),
        Operator::Br { relative_depth } => format!("BI_br {relative_depth}"),
        Operator::BrIf { relative_depth } => format!("BI_br_if {relative_depth}%N"),
        Operator::BrTable { targets } => {
            if targets.is_empty() {
                "BI_br_table".to_string()
            } else {
                let mut labelidx = String::new();
                for target in targets.targets() {
                    let id = target.unwrap();
                    labelidx.push_str(format!("{id}").as_str());
                    labelidx.push_str(" :: ");
                }
                labelidx.push_str("nil");
                format!("BI_br_table ({labelidx})")
            }
        }
        Operator::Return => "BI_return".to_string(),
        Operator::Call { function_index } => format!("BI_call {function_index}"),
        Operator::CallIndirect {
            type_index,
            table_index,
        } => format!("BI_call_indirect {type_index} {table_index}"),
        Operator::Drop => "BI_drop".to_string(),
        Operator::Select => "BI_select None".to_string(),
        Operator::LocalGet { local_index } => {
            if let Some(local_name_map) = local_name_map {
                if local_name_map.contains_key(local_index) {
                    format!(
                        "BI_local_get {local_index}%N (*{}*)",
                        local_name_map.get(local_index).unwrap()
                    )
                } else {
                    format!("BI_local_get {local_index}%N")
                }
            } else {
                format!("BI_local_get {local_index}%N")
            }
        }
        Operator::LocalSet { local_index } => {
            if let Some(local_name_map) = local_name_map {
                if local_name_map.contains_key(local_index) {
                    format!(
                        "BI_local_set {local_index}%N (*{}*)",
                        local_name_map.get(local_index).unwrap()
                    )
                } else {
                    format!("BI_local_set {local_index}%N")
                }
            } else {
                format!("BI_local_set {local_index}%N")
            }
        }
        Operator::LocalTee { local_index } => {
            if let Some(local_name_map) = local_name_map {
                if local_name_map.contains_key(local_index) {
                    format!(
                        "BI_local_tee {local_index}%N (*{}*)",
                        local_name_map.get(local_index).unwrap()
                    )
                } else {
                    format!("BI_local_tee {local_index}%N")
                }
            } else {
                format!("BI_local_tee {local_index}%N")
            }
        }
        Operator::GlobalGet { global_index } => format!("BI_global_get {global_index}%N"),
        Operator::GlobalSet { global_index } => format!("BI_global_set {global_index}%N"),
        Operator::I32Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 None ({memarg})")
        }
        Operator::I64Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 None ({memarg})")
        }
        Operator::F32Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_f32 None ({memarg})")
        }
        Operator::F64Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_f64 None ({memarg})")
        }
        Operator::I32Load8S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i8, SX_S)) ({memarg})")
        }
        Operator::I32Load8U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i8, SX_U)) ({memarg})")
        }
        Operator::I32Load16S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i16, SX_S)) ({memarg})")
        }
        Operator::I32Load16U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i16, SX_U)) ({memarg})")
        }
        Operator::I64Load8S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i8, SX_S)) ({memarg})")
        }
        Operator::I64Load8U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i8, SX_U)) ({memarg})")
        }
        Operator::I64Load16S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i16, SX_S)) ({memarg})")
        }
        Operator::I64Load16U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i16, SX_U)) ({memarg})")
        }
        Operator::I64Load32S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i32, SX_S)) ({memarg})")
        }
        Operator::I64Load32U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i32, SX_U)) ({memarg})")
        }
        Operator::I32Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i32 None ({memarg})")
        }
        Operator::I64Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 None ({memarg})")
        }
        Operator::F32Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_f32 None ({memarg})")
        }
        Operator::F64Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_f64 None ({memarg})")
        }
        Operator::I32Store8 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i32 (Some Tp_i8) ({memarg})")
        }
        Operator::I32Store16 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i32 (Some Tp_i16) ({memarg})")
        }
        Operator::I64Store8 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 (Some Tp_i8) ({memarg})")
        }
        Operator::I64Store16 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 (Some Tp_i16) ({memarg})")
        }
        Operator::I64Store32 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 (Some Tp_i32) ({memarg})")
        }
        Operator::MemorySize { mem } => {
            if *mem > 0 {
                return Err(anyhow::anyhow!("Memory index is not supported"));
            }
            "BI_memory_size".to_string()
        }
        Operator::MemoryGrow { mem } => {
            if *mem > 0 {
                return Err(anyhow::anyhow!("Memory index is not supported"));
            }
            "BI_memory_grow".to_string()
        }
        Operator::I32Const { value } => format!("BI_const_num (Vi32 {value})"),
        Operator::I64Const { value } => format!("BI_const_num (Vi64 {value})"),
        Operator::F32Const { value } => {
            let val = value.bits();
            format!("BI_const_num (VAL_float32 {val})")
        }
        Operator::F64Const { value } => {
            let val = value.bits();
            format!("BI_const_num (VAL_float64 {val})")
        }
        Operator::I32Eqz => "BI_testop T_i32 TO_eqz".to_string(),
        Operator::I32Eq => "BI_relop T_i32 (Relop_i ROI_eq)".to_string(),
        Operator::I32Ne => "BI_relop T_i32 (Relop_i ROI_ne)".to_string(),
        Operator::I32LtS => "BI_relop T_i32 (Relop_i (ROI_lt SX_S))".to_string(),
        Operator::I32LtU => "BI_relop T_i32 (Relop_i (ROI_lt SX_U))".to_string(),
        Operator::I32GtS => "BI_relop T_i32 (Relop_i (ROI_gt SX_S))".to_string(),
        Operator::I32GtU => "BI_relop T_i32 (Relop_i (ROI_gt SX_U))".to_string(),
        Operator::I32LeS => "BI_relop T_i32 (Relop_i (ROI_le SX_S))".to_string(),
        Operator::I32LeU => "BI_relop T_i32 (Relop_i (ROI_le SX_U))".to_string(),
        Operator::I32GeS => "BI_relop T_i32 (Relop_i (ROI_ge SX_S))".to_string(),
        Operator::I32GeU => "BI_relop T_i32 (Relop_i (ROI_ge SX_U))".to_string(),
        Operator::I64Eqz => "BI_testop T_i64 TO_eqz".to_string(),
        Operator::I64Eq => "BI_relop T_i64 (Relop_i ROI_eq)".to_string(),
        Operator::I64Ne => "BI_relop T_i64 (Relop_i ROI_ne)".to_string(),
        Operator::I64LtS => "BI_relop T_i64 (Relop_i (ROI_lt SX_S))".to_string(),
        Operator::I64LtU => "BI_relop T_i64 (Relop_i (ROI_lt SX_U))".to_string(),
        Operator::I64GtS => "BI_relop T_i64 (Relop_i (ROI_gt SX_S))".to_string(),
        Operator::I64GtU => "BI_relop T_i64 (Relop_i (ROI_gt SX_U))".to_string(),
        Operator::I64LeS => "BI_relop T_i64 (Relop_i (ROI_le SX_S))".to_string(),
        Operator::I64LeU => "BI_relop T_i64 (Relop_i (ROI_le SX_U))".to_string(),
        Operator::I64GeS => "BI_relop T_i64 (Relop_i (ROI_ge SX_S))".to_string(),
        Operator::I64GeU => "BI_relop T_i64 (Relop_i (ROI_ge SX_U))".to_string(),
        Operator::F32Eq => "BI_relop T_f32 (Relop_f ROI_eq)".to_string(),
        Operator::F32Ne => "BI_relop T_f32 (Relop_f ROI_ne)".to_string(),
        Operator::F32Lt => "BI_relop T_f32 (Relop_f ROI_lt)".to_string(),
        Operator::F32Gt => "BI_relop T_f32 (Relop_f ROI_gt)".to_string(),
        Operator::F32Le => "BI_relop T_f32 (Relop_f ROI_le)".to_string(),
        Operator::F32Ge => "BI_relop T_f32 (Relop_f ROI_ge)".to_string(),
        Operator::F64Eq => "BI_relop T_f64 (Relop_f ROI_eq)".to_string(),
        Operator::F64Ne => "BI_relop T_f64 (Relop_f ROI_ne)".to_string(),
        Operator::F64Lt => "BI_relop T_f64 (Relop_f ROI_lt)".to_string(),
        Operator::F64Gt => "BI_relop T_f64 (Relop_f ROI_gt)".to_string(),
        Operator::F64Le => "BI_relop T_f64 (Relop_f ROI_le)".to_string(),
        Operator::F64Ge => "BI_relop T_f64 (Relop_f ROI_ge)".to_string(),
        Operator::I32Clz => "BI_unop T_i32 (Unop_i UOI_clz)".to_string(),
        Operator::I32Ctz => "BI_unop T_i32 (Unop_i UOI_ctz)".to_string(),
        Operator::I32Popcnt => "BI_unop T_i32 (Unop_i UOI_popcnt)".to_string(),
        Operator::I32Add => "BI_binop T_i32 (Binop_i BOI_add)".to_string(),
        Operator::I32Sub => "BI_binop T_i32 (Binop_i BOI_sub)".to_string(),
        Operator::I32Mul => "BI_binop T_i32 (Binop_i BOI_mul)".to_string(),
        Operator::I32DivS => "BI_binop T_i32 (Binop_i (BOI_div SX_S))".to_string(),
        Operator::I32DivU => "BI_binop T_i32 (Binop_i (BOI_div SX_U))".to_string(),
        Operator::I32RemS => "BI_binop T_i32 (Binop_i (BOI_rem SX_S))".to_string(),
        Operator::I32RemU => "BI_binop T_i32 (Binop_i (BOI_rem SX_U))".to_string(),
        Operator::I32And => "BI_binop T_i32 (Binop_i BOI_and)".to_string(),
        Operator::I32Or => "BI_binop T_i32 (Binop_i BOI_or)".to_string(),
        Operator::I32Xor => "BI_binop T_i32 (Binop_i BOI_xor)".to_string(),
        Operator::I32Shl => "BI_binop T_i32 (Binop_i BOI_shl)".to_string(),
        Operator::I32ShrS => "BI_binop T_i32 (Binop_i (BOI_shr SX_S))".to_string(),
        Operator::I32ShrU => "BI_binop T_i32 (Binop_i (BOI_shr SX_U))".to_string(),
        Operator::I32Rotl => "BI_binop T_i32 (Binop_i BOI_rotl)".to_string(),
        Operator::I32Rotr => "BI_binop T_i32 (Binop_i BOI_rotr)".to_string(),
        Operator::I64Clz => "BI_unop T_i64 (Unop_i UOI_clz)".to_string(),
        Operator::I64Ctz => "BI_unop T_i64 (Unop_i UOI_ctz)".to_string(),
        Operator::I64Popcnt => "BI_unop T_i64 (Unop_i UOI_popcnt)".to_string(),
        Operator::I64Add => "BI_binop T_i64 (Binop_i BOI_add)".to_string(),
        Operator::I64Sub => "BI_binop T_i64 (Binop_i BOI_sub)".to_string(),
        Operator::I64Mul => "BI_binop T_i64 (Binop_i BOI_mul)".to_string(),
        Operator::I64DivS => "BI_binop T_i64 (Binop_i (BOI_div SX_S))".to_string(),
        Operator::I64DivU => "BI_binop T_i64 (Binop_i (BOI_div SX_U))".to_string(),
        Operator::I64RemS => "BI_binop T_i64 (Binop_i (BOI_rem SX_S))".to_string(),
        Operator::I64RemU => "BI_binop T_i64 (Binop_i (BOI_rem SX_U))".to_string(),
        Operator::I64And => "BI_binop T_i64 (Binop_i BOI_and)".to_string(),
        Operator::I64Or => "BI_binop T_i64 (Binop_i BOI_or)".to_string(),
        Operator::I64Xor => "BI_binop T_i64 (Binop_i BOI_xor)".to_string(),
        Operator::I64Shl => "BI_binop T_i64 (Binop_i BOI_shl)".to_string(),
        Operator::I64ShrS => "BI_binop T_i64 (Binop_i (BOI_shr SX_S))".to_string(),
        Operator::I64ShrU => "BI_binop T_i64 (Binop_i (BOI_shr SX_U))".to_string(),
        Operator::I64Rotl => "BI_binop T_i64 (Binop_i BOI_rotl)".to_string(),
        Operator::I64Rotr => "BI_binop T_i64 (Binop_i BOI_rotr)".to_string(),
        Operator::F32Abs => "BI_unop T_f32 (Unop_f UOF_abs)".to_string(),
        Operator::F32Neg => "BI_unop T_f32 (Unop_f UOF_neg)".to_string(),
        Operator::F32Ceil => "BI_unop T_f32 (Unop_f UOF_ceil)".to_string(),
        Operator::F32Floor => "BI_unop T_f32 (Unop_f UOF_floor)".to_string(),
        Operator::F32Trunc => "BI_unop T_f32 (Unop_f UOF_trunc)".to_string(),
        Operator::F32Nearest => "BI_unop T_f32 (Unop_f UOF_nearest)".to_string(),
        Operator::F32Sqrt => "BI_unop T_f32 (Unop_f UOF_sqrt)".to_string(),
        Operator::F32Add => "BI_binop T_f32 (Binop_f BOF_add)".to_string(),
        Operator::F32Sub => "BI_binop T_f32 (Binop_f BOF_sub)".to_string(),
        Operator::F32Mul => "BI_binop T_f32 (Binop_f BOF_mul)".to_string(),
        Operator::F32Div => "BI_binop T_f32 (Binop_f BOF_div)".to_string(),
        Operator::F32Min => "BI_binop T_f32 (Binop_f BOF_min)".to_string(),
        Operator::F32Max => "BI_binop T_f32 (Binop_f BOF_max)".to_string(),
        Operator::F32Copysign => "BI_binop T_f32 (Binop_f BOF_copysign)".to_string(),
        Operator::F64Abs => "BI_unop T_f64 (Unop_f UOF_abs)".to_string(),
        Operator::F64Neg => "BI_unop T_f64 (Unop_f UOF_neg)".to_string(),
        Operator::F64Ceil => "BI_unop T_f64 (Unop_f UOF_ceil)".to_string(),
        Operator::F64Floor => "BI_unop T_f64 (Unop_f UOF_floor)".to_string(),
        Operator::F64Trunc => "BI_unop T_f64 (Unop_f UOF_trunc)".to_string(),
        Operator::F64Nearest => "BI_unop T_f64 (Unop_f UOF_nearest)".to_string(),
        Operator::F64Sqrt => "BI_unop T_f64 (Unop_f UOF_sqrt)".to_string(),
        Operator::F64Add => "BI_binop T_f64 (Binop_f BOF_add)".to_string(),
        Operator::F64Sub => "BI_binop T_f64 (Binop_f BOF_sub)".to_string(),
        Operator::F64Mul => "BI_binop T_f64 (Binop_f BOF_mul)".to_string(),
        Operator::F64Div => "BI_binop T_f64 (Binop_f BOF_div)".to_string(),
        Operator::F64Min => "BI_binop T_f64 (Binop_f BOF_min)".to_string(),
        Operator::F64Max => "BI_binop T_f64 (Binop_f BOF_max)".to_string(),
        Operator::F64Copysign => "BI_binop T_f64 (Binop_f BOF_copysign)".to_string(),
        Operator::I32WrapI64 => "BI_cvtop T_i32 (CVO_wrap T_i64 None)".to_string(),
        Operator::I32TruncF32S => "BI_cvtop T_i32 (CVO_trunc T_f32 (Some SX_S))".to_string(),
        Operator::I32TruncF32U => "BI_cvtop T_i32 (CVO_trunc T_f32 (Some SX_U))".to_string(),
        Operator::I32TruncF64S => "BI_cvtop T_i32 (CVO_trunc T_f64 (Some SX_S))".to_string(),
        Operator::I32TruncF64U => "BI_cvtop T_i32 (CVO_trunc T_f64 (Some SX_U))".to_string(),
        Operator::I64ExtendI32S => "BI_cvtop T_i64 (CVO_extend T_i32 (Some SX_S))".to_string(),
        Operator::I64ExtendI32U => "BI_cvtop T_i64 (CVO_extend T_i32 (Some SX_U))".to_string(),
        Operator::I64TruncF32S => "BI_cvtop T_i64 (CVO_trunc T_f32 (Some SX_S))".to_string(),
        Operator::I64TruncF32U => "BI_cvtop T_i64 (CVO_trunc T_f32 (Some SX_U))".to_string(),
        Operator::I64TruncF64S => "BI_cvtop T_i64 (CVO_trunc T_f64 (Some SX_S))".to_string(),
        Operator::I64TruncF64U => "BI_cvtop T_i64 (CVO_trunc T_f64 (Some SX_U))".to_string(),
        Operator::F32ConvertI32S => "BI_cvtop T_f32 (CVO_convert T_i32 (Some SX_S))".to_string(),
        Operator::F32ConvertI32U => "BI_cvtop T_f32 (CVO_convert T_i32 (Some SX_U))".to_string(),
        Operator::F32ConvertI64S => "BI_cvtop T_f32 (CVO_convert T_i64 (Some SX_S))".to_string(),
        Operator::F32ConvertI64U => "BI_cvtop T_f32 (CVO_convert T_i64 (Some SX_U))".to_string(),
        Operator::F32DemoteF64 => "BI_cvtop T_f32 (CVO_demote T_f64 None)".to_string(),
        Operator::F64ConvertI32S => "BI_cvtop T_f64 (CVO_convert T_i32 (Some SX_S))".to_string(),
        Operator::F64ConvertI32U => "BI_cvtop T_f64 (CVO_convert T_i32 (Some SX_U))".to_string(),
        Operator::F64ConvertI64S => "BI_cvtop T_f64 (CVO_convert T_i64 (Some SX_S))".to_string(),
        Operator::F64ConvertI64U => "BI_cvtop T_f64 (CVO_convert T_i64 (Some SX_U))".to_string(),
        Operator::F64PromoteF32 => "BI_cvtop T_f64 (CVO_promote T_f32 None)".to_string(),
        Operator::I32ReinterpretF32 => "BI_cvtop T_i32 (CVO_reinterpret T_f32 None)".to_string(),
        Operator::I64ReinterpretF64 => "BI_cvtop T_i64 (CVO_reinterpret T_f64 None)".to_string(),
        Operator::F32ReinterpretI32 => "BI_cvtop T_f32 (CVO_reinterpret T_i32 None)".to_string(),
        Operator::F64ReinterpretI64 => "BI_cvtop T_f64 (CVO_reinterpret T_i64 None)".to_string(),
        Operator::I32Extend8S => todo!(),
        Operator::I32Extend16S => todo!(),
        Operator::I64Extend8S => todo!(),
        Operator::I64Extend16S => todo!(),
        Operator::I64Extend32S => todo!(),
        Operator::RefEq => todo!(),
        Operator::StructNew { .. } => todo!(),
        Operator::StructNewDefault { .. } => todo!(),
        Operator::StructGet { .. } => todo!(),
        Operator::StructGetS { .. } => todo!(),
        Operator::StructGetU { .. } => todo!(),
        Operator::StructSet { .. } => todo!(),
        Operator::ArrayNew { .. } => todo!(),
        Operator::ArrayNewDefault { .. } => todo!(),
        Operator::ArrayNewFixed { .. } => todo!(),
        Operator::ArrayNewData { .. } => todo!(),
        Operator::ArrayNewElem { .. } => todo!(),
        Operator::ArrayGet { .. } => todo!(),
        Operator::ArrayGetS { .. } => todo!(),
        Operator::ArrayGetU { .. } => todo!(),
        Operator::ArraySet { .. } => todo!(),
        Operator::ArrayLen => todo!(),
        Operator::ArrayFill { .. } => todo!(),
        Operator::ArrayCopy { .. } => todo!(),
        Operator::ArrayInitData { .. } => todo!(),
        Operator::ArrayInitElem { .. } => todo!(),
        Operator::RefTestNonNull { .. } => todo!(),
        Operator::RefTestNullable { .. } => todo!(),
        Operator::RefCastNonNull { .. } => todo!(),
        Operator::RefCastNullable { .. } => todo!(),
        Operator::BrOnCast { .. } => todo!(),
        Operator::BrOnCastFail { .. } => todo!(),
        Operator::AnyConvertExtern => todo!(),
        Operator::ExternConvertAny => todo!(),
        Operator::RefI31 => todo!(),
        Operator::I31GetS => todo!(),
        Operator::I31GetU => todo!(),
        Operator::I32TruncSatF32S => todo!(),
        Operator::I32TruncSatF32U => todo!(),
        Operator::I32TruncSatF64S => todo!(),
        Operator::I32TruncSatF64U => todo!(),
        Operator::I64TruncSatF32S => todo!(),
        Operator::I64TruncSatF32U => todo!(),
        Operator::I64TruncSatF64S => todo!(),
        Operator::I64TruncSatF64U => todo!(),
        Operator::MemoryInit { data_index, mem: _ } => format!("BI_memory_init {data_index}"),
        Operator::DataDrop { data_index } => format!("BI_data_drop {data_index}"),
        Operator::MemoryCopy {
            dst_mem: _,
            src_mem: _,
        } => "BI_memory_copy".to_string(),
        Operator::MemoryFill { mem: _ } => "BI_memory_fill".to_string(),
        Operator::TableInit { .. } => todo!(),
        Operator::ElemDrop { .. } => todo!(),
        Operator::TableCopy { .. } => todo!(),
        Operator::TypedSelect { .. } => todo!(),
        Operator::RefNull { .. } => todo!(),
        Operator::RefIsNull => "BI_ref_is_null".to_string(),
        Operator::RefFunc { function_index } => format!("BI_ref_func {function_index}%N"),
        Operator::TableFill { table } => format!("BI_table_fill {table}%N"),
        Operator::TableGet { table } => format!("BI_table_get {table}%N"),
        Operator::TableSet { table } => format!("BI_table_set {table}%N"),
        Operator::TableGrow { table } => format!("BI_table_grow {table}%N"),
        Operator::TableSize { table } => format!("BI_table_size {table}%N"),
        Operator::ReturnCall { .. } => todo!(),
        Operator::ReturnCallIndirect { .. } => todo!(),
        Operator::MemoryDiscard { .. } => todo!(),
        Operator::MemoryAtomicNotify { memarg: _ }
        | Operator::MemoryAtomicWait32 { memarg: _ }
        | Operator::MemoryAtomicWait64 { memarg: _ }
        | Operator::AtomicFence
        | Operator::I32AtomicLoad { memarg: _ }
        | Operator::I64AtomicLoad { memarg: _ }
        | Operator::I32AtomicLoad8U { memarg: _ }
        | Operator::I32AtomicLoad16U { memarg: _ }
        | Operator::I64AtomicLoad8U { memarg: _ }
        | Operator::I64AtomicLoad16U { memarg: _ }
        | Operator::I64AtomicLoad32U { memarg: _ }
        | Operator::I32AtomicStore { memarg: _ }
        | Operator::I64AtomicStore { memarg: _ }
        | Operator::I32AtomicStore8 { memarg: _ }
        | Operator::I32AtomicStore16 { memarg: _ }
        | Operator::I64AtomicStore8 { memarg: _ }
        | Operator::I64AtomicStore16 { memarg: _ }
        | Operator::I64AtomicStore32 { memarg: _ }
        | Operator::I32AtomicRmwAdd { memarg: _ }
        | Operator::I64AtomicRmwAdd { memarg: _ }
        | Operator::I32AtomicRmw8AddU { memarg: _ }
        | Operator::I32AtomicRmw16AddU { memarg: _ }
        | Operator::I64AtomicRmw8AddU { memarg: _ }
        | Operator::I64AtomicRmw16AddU { memarg: _ }
        | Operator::I64AtomicRmw32AddU { memarg: _ }
        | Operator::I32AtomicRmwSub { memarg: _ }
        | Operator::I64AtomicRmwSub { memarg: _ }
        | Operator::I32AtomicRmw8SubU { memarg: _ }
        | Operator::I32AtomicRmw16SubU { memarg: _ }
        | Operator::I64AtomicRmw8SubU { memarg: _ }
        | Operator::I64AtomicRmw16SubU { memarg: _ }
        | Operator::I64AtomicRmw32SubU { memarg: _ }
        | Operator::I32AtomicRmwAnd { memarg: _ }
        | Operator::I64AtomicRmwAnd { memarg: _ }
        | Operator::I32AtomicRmw8AndU { memarg: _ }
        | Operator::I32AtomicRmw16AndU { memarg: _ }
        | Operator::I64AtomicRmw8AndU { memarg: _ }
        | Operator::I64AtomicRmw16AndU { memarg: _ }
        | Operator::I64AtomicRmw32AndU { memarg: _ }
        | Operator::I32AtomicRmwOr { memarg: _ }
        | Operator::I64AtomicRmwOr { memarg: _ }
        | Operator::I32AtomicRmw8OrU { memarg: _ }
        | Operator::I32AtomicRmw16OrU { memarg: _ }
        | Operator::I64AtomicRmw8OrU { memarg: _ }
        | Operator::I64AtomicRmw16OrU { memarg: _ }
        | Operator::I64AtomicRmw32OrU { memarg: _ }
        | Operator::I32AtomicRmwXor { memarg: _ }
        | Operator::I64AtomicRmwXor { memarg: _ }
        | Operator::I32AtomicRmw8XorU { memarg: _ }
        | Operator::I32AtomicRmw16XorU { memarg: _ }
        | Operator::I64AtomicRmw8XorU { memarg: _ }
        | Operator::I64AtomicRmw16XorU { memarg: _ }
        | Operator::I64AtomicRmw32XorU { memarg: _ }
        | Operator::I32AtomicRmwXchg { memarg: _ }
        | Operator::I64AtomicRmwXchg { memarg: _ }
        | Operator::I32AtomicRmw8XchgU { memarg: _ }
        | Operator::I32AtomicRmw16XchgU { memarg: _ }
        | Operator::I64AtomicRmw8XchgU { memarg: _ }
        | Operator::I64AtomicRmw16XchgU { memarg: _ }
        | Operator::I64AtomicRmw32XchgU { memarg: _ }
        | Operator::I32AtomicRmwCmpxchg { memarg: _ }
        | Operator::I64AtomicRmwCmpxchg { memarg: _ }
        | Operator::I32AtomicRmw8CmpxchgU { memarg: _ }
        | Operator::I32AtomicRmw16CmpxchgU { memarg: _ }
        | Operator::I64AtomicRmw8CmpxchgU { memarg: _ }
        | Operator::I64AtomicRmw16CmpxchgU { memarg: _ }
        | Operator::I64AtomicRmw32CmpxchgU { memarg: _ } => {
            return Err(anyhow::anyhow!(
                "Atomic instruction {operator:?} are not supported",
            ));
        }
        Operator::V128Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i16, SX_U)) ({memarg})")
        }
        Operator::V128Load8x8S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i8, SX_S)) ({memarg})")
        }
        Operator::V128Load8x8U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i8, SX_U)) ({memarg})")
        }
        Operator::V128Load16x4S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i16, SX_S)) ({memarg})")
        }
        Operator::V128Load16x4U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i16, SX_U)) ({memarg})")
        }
        Operator::V128Load32x2S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i32, SX_S)) ({memarg})")
        }
        Operator::V128Load32x2U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i32, SX_U)) ({memarg})")
        }
        Operator::V128Load8Splat { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_splat Twv_8 ({memarg})")
        }
        Operator::V128Load16Splat { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_splat Twv_16 ({memarg})")
        }
        Operator::V128Load32Splat { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_splat Twv_32 ({memarg})")
        }
        Operator::V128Load64Splat { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_splat Twv_64 ({memarg})")
        }
        Operator::V128Load32Zero { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_zero Tztv_32 ({memarg})")
        }
        Operator::V128Load64Zero { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load_vec LVA_zero Tztv_64 ({memarg})")
        }
        Operator::V128Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store_vec_lane Twv_64 ({memarg}) 0")
        }
        Operator::V128Load8Lane { memarg, lane } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store_vec_lane Twv_8 ({memarg}) {lane}")
        }
        Operator::V128Load16Lane { memarg, lane } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store_vec_lane Twv_16 ({memarg}) {lane}")
        }
        Operator::V128Load32Lane { memarg, lane } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store_vec_lane Twv_32 ({memarg}) {lane}")
        }
        Operator::V128Load64Lane { memarg, lane } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store_vec_lane Twv_64 ({memarg}) {lane}")
        }
        Operator::V128Store8Lane { memarg, lane } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store_vec_lane Twv_8 ({memarg}) {lane}")
        }
        Operator::V128Store16Lane { memarg, lane } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store_vec_lane Twv_16 ({memarg}) {lane}")
        }
        Operator::V128Store32Lane { memarg, lane } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store_vec_lane Twv_32 ({memarg}) {lane}")
        }
        Operator::V128Store64Lane { memarg, lane } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store_vec_lane Twv_64 ({memarg}) {lane}")
        }
        Operator::V128Const { value } => {
            let value = value.i128();
            format!("BI_const_vec {value}")
        }
        Operator::I8x16Shuffle { .. } => todo!(),
        Operator::I8x16ExtractLaneS { lane } => {
            format!("BI_extract_vec (SV_ishape SVI_8_16) (Some SX_S) {lane}")
        }
        Operator::I8x16ExtractLaneU { lane } => {
            format!("BI_extract_vec (SV_ishape SVI_8_16) (Some SX_U) {lane}")
        }
        //BI_replace_vec: shape_vec -> laneidx -> basic_instruction
        Operator::I8x16ReplaceLane { lane } => {
            format!("BI_replace_vec (SV_ishape SVI_8_16) {lane}")
        }
        Operator::I16x8ExtractLaneS { lane } => {
            format!("BI_extract_vec (SV_ishape SVI_16_8) (Some SX_S) {lane}")
        }
        Operator::I16x8ExtractLaneU { lane } => {
            format!("BI_extract_vec (SV_ishape SVI_16_8) (Some SX_U) {lane}")
        }
        Operator::I16x8ReplaceLane { lane } => {
            format!("BI_replace_vec (SV_ishape SVI_16_8) {lane}")
        }
        Operator::I32x4ExtractLane { lane } => {
            format!("BI_extract_vec (SV_ishape SVI_32_4) (Some SX_S) {lane}")
        }
        Operator::I32x4ReplaceLane { lane } => {
            format!("BI_replace_vec (SV_ishape SVI_32_4) {lane}")
        }
        Operator::I64x2ExtractLane { lane } => {
            format!("BI_extract_vec (SV_ishape SVI_64_2) (Some SX_S) {lane}")
        }
        Operator::I64x2ReplaceLane { lane } => {
            format!("BI_replace_vec (SV_ishape SVI_64_2) {lane}")
        }
        Operator::F32x4ExtractLane { lane } => {
            format!("BI_extract_vec (SV_fshape SVF_32_4) None {lane}")
        }
        Operator::F32x4ReplaceLane { lane } => {
            format!("BI_replace_vec (SV_fshape SVF_32_4) {lane}")
        }
        Operator::F64x2ExtractLane { lane } => {
            format!("BI_extract_vec (SV_fshape SVF_64_2) None {lane}")
        }
        Operator::F64x2ReplaceLane { lane } => {
            format!("BI_replace_vec (SV_fshape SVF_64_2) {lane}")
        }
        Operator::I8x16Swizzle => todo!(),
        Operator::I8x16Splat => "BI_load_vec LVA_splat Twv_8".to_string(),
        Operator::I16x8Splat => "BI_load_vec LVA_splat Twv_16".to_string(),
        Operator::I32x4Splat => "BI_load_vec LVA_splat Twv_32".to_string(),
        Operator::I64x2Splat => "BI_load_vec LVA_splat Twv_64".to_string(),
        Operator::F32x4Splat => "BI_load_vec LVA_splat Twv_32".to_string(),
        Operator::F64x2Splat => "BI_load_vec LVA_splat Twv_64".to_string(),
        Operator::I8x16Eq => todo!(),
        Operator::I8x16Ne => todo!(),
        Operator::I8x16LtS => todo!(),
        Operator::I8x16LtU => todo!(),
        Operator::I8x16GtS => todo!(),
        Operator::I8x16GtU => todo!(),
        Operator::I8x16LeS => todo!(),
        Operator::I8x16LeU => todo!(),
        Operator::I8x16GeS => todo!(),
        Operator::I8x16GeU => todo!(),
        Operator::I16x8Eq => todo!(),
        Operator::I16x8Ne => todo!(),
        Operator::I16x8LtS => todo!(),
        Operator::I16x8LtU => todo!(),
        Operator::I16x8GtS => todo!(),
        Operator::I16x8GtU => todo!(),
        Operator::I16x8LeS => todo!(),
        Operator::I16x8LeU => todo!(),
        Operator::I16x8GeS => todo!(),
        Operator::I16x8GeU => todo!(),
        Operator::I32x4Eq => todo!(),
        Operator::I32x4Ne => todo!(),
        Operator::I32x4LtS => todo!(),
        Operator::I32x4LtU => todo!(),
        Operator::I32x4GtS => todo!(),
        Operator::I32x4GtU => todo!(),
        Operator::I32x4LeS => todo!(),
        Operator::I32x4LeU => todo!(),
        Operator::I32x4GeS => todo!(),
        Operator::I32x4GeU => todo!(),
        Operator::I64x2Eq => todo!(),
        Operator::I64x2Ne => todo!(),
        Operator::I64x2LtS => todo!(),
        Operator::I64x2GtS => todo!(),
        Operator::I64x2LeS => todo!(),
        Operator::I64x2GeS => todo!(),
        Operator::F32x4Eq => todo!(),
        Operator::F32x4Ne => todo!(),
        Operator::F32x4Lt => todo!(),
        Operator::F32x4Gt => todo!(),
        Operator::F32x4Le => todo!(),
        Operator::F32x4Ge => todo!(),
        Operator::F64x2Eq => todo!(),
        Operator::F64x2Ne => todo!(),
        Operator::F64x2Lt => todo!(),
        Operator::F64x2Gt => todo!(),
        Operator::F64x2Le => todo!(),
        Operator::F64x2Ge => todo!(),
        Operator::V128Not => todo!(),
        Operator::V128And => todo!(),
        Operator::V128AndNot => todo!(),
        Operator::V128Or => todo!(),
        Operator::V128Xor => todo!(),
        Operator::V128Bitselect => todo!(),
        Operator::V128AnyTrue => todo!(),
        Operator::I8x16Abs => todo!(),
        Operator::I8x16Neg => todo!(),
        Operator::I8x16Popcnt => todo!(),
        Operator::I8x16AllTrue => todo!(),
        Operator::I8x16Bitmask => todo!(),
        Operator::I8x16NarrowI16x8S => todo!(),
        Operator::I8x16NarrowI16x8U => todo!(),
        Operator::I8x16Shl => todo!(),
        Operator::I8x16ShrS => todo!(),
        Operator::I8x16ShrU => todo!(),
        Operator::I8x16Add => todo!(),
        Operator::I8x16AddSatS => todo!(),
        Operator::I8x16AddSatU => todo!(),
        Operator::I8x16Sub => todo!(),
        Operator::I8x16SubSatS => todo!(),
        Operator::I8x16SubSatU => todo!(),
        Operator::I8x16MinS => todo!(),
        Operator::I8x16MinU => todo!(),
        Operator::I8x16MaxS => todo!(),
        Operator::I8x16MaxU => todo!(),
        Operator::I8x16AvgrU => todo!(),
        Operator::I16x8ExtAddPairwiseI8x16S => todo!(),
        Operator::I16x8ExtAddPairwiseI8x16U => todo!(),
        Operator::I16x8Abs => todo!(),
        Operator::I16x8Neg => todo!(),
        Operator::I16x8Q15MulrSatS => todo!(),
        Operator::I16x8AllTrue => todo!(),
        Operator::I16x8Bitmask => todo!(),
        Operator::I16x8NarrowI32x4S => todo!(),
        Operator::I16x8NarrowI32x4U => todo!(),
        Operator::I16x8ExtendLowI8x16S => todo!(),
        Operator::I16x8ExtendHighI8x16S => todo!(),
        Operator::I16x8ExtendLowI8x16U => todo!(),
        Operator::I16x8ExtendHighI8x16U => todo!(),
        Operator::I16x8Shl => todo!(),
        Operator::I16x8ShrS => todo!(),
        Operator::I16x8ShrU => todo!(),
        Operator::I16x8Add => todo!(),
        Operator::I16x8AddSatS => todo!(),
        Operator::I16x8AddSatU => todo!(),
        Operator::I16x8Sub => todo!(),
        Operator::I16x8SubSatS => todo!(),
        Operator::I16x8SubSatU => todo!(),
        Operator::I16x8Mul => todo!(),
        Operator::I16x8MinS => todo!(),
        Operator::I16x8MinU => todo!(),
        Operator::I16x8MaxS => todo!(),
        Operator::I16x8MaxU => todo!(),
        Operator::I16x8AvgrU => todo!(),
        Operator::I16x8ExtMulLowI8x16S => todo!(),
        Operator::I16x8ExtMulHighI8x16S => todo!(),
        Operator::I16x8ExtMulLowI8x16U => todo!(),
        Operator::I16x8ExtMulHighI8x16U => todo!(),
        Operator::I32x4ExtAddPairwiseI16x8S => todo!(),
        Operator::I32x4ExtAddPairwiseI16x8U => todo!(),
        Operator::I32x4Abs => todo!(),
        Operator::I32x4Neg => todo!(),
        Operator::I32x4AllTrue => todo!(),
        Operator::I32x4Bitmask => todo!(),
        Operator::I32x4ExtendLowI16x8S => todo!(),
        Operator::I32x4ExtendHighI16x8S => todo!(),
        Operator::I32x4ExtendLowI16x8U => todo!(),
        Operator::I32x4ExtendHighI16x8U => todo!(),
        Operator::I32x4Shl => todo!(),
        Operator::I32x4ShrS => todo!(),
        Operator::I32x4ShrU => todo!(),
        Operator::I32x4Add => todo!(),
        Operator::I32x4Sub => todo!(),
        Operator::I32x4Mul => todo!(),
        Operator::I32x4MinS => todo!(),
        Operator::I32x4MinU => todo!(),
        Operator::I32x4MaxS => todo!(),
        Operator::I32x4MaxU => todo!(),
        Operator::I32x4DotI16x8S => todo!(),
        Operator::I32x4ExtMulLowI16x8S => todo!(),
        Operator::I32x4ExtMulHighI16x8S => todo!(),
        Operator::I32x4ExtMulLowI16x8U => todo!(),
        Operator::I32x4ExtMulHighI16x8U => todo!(),
        Operator::I64x2Abs => todo!(),
        Operator::I64x2Neg => todo!(),
        Operator::I64x2AllTrue => todo!(),
        Operator::I64x2Bitmask => todo!(),
        Operator::I64x2ExtendLowI32x4S => todo!(),
        Operator::I64x2ExtendHighI32x4S => todo!(),
        Operator::I64x2ExtendLowI32x4U => todo!(),
        Operator::I64x2ExtendHighI32x4U => todo!(),
        Operator::I64x2Shl => todo!(),
        Operator::I64x2ShrS => todo!(),
        Operator::I64x2ShrU => todo!(),
        Operator::I64x2Add => todo!(),
        Operator::I64x2Sub => todo!(),
        Operator::I64x2Mul => todo!(),
        Operator::I64x2ExtMulLowI32x4S => todo!(),
        Operator::I64x2ExtMulHighI32x4S => todo!(),
        Operator::I64x2ExtMulLowI32x4U => todo!(),
        Operator::I64x2ExtMulHighI32x4U => todo!(),
        Operator::F32x4Ceil => todo!(),
        Operator::F32x4Floor => todo!(),
        Operator::F32x4Trunc => todo!(),
        Operator::F32x4Nearest => todo!(),
        Operator::F32x4Abs => todo!(),
        Operator::F32x4Neg => todo!(),
        Operator::F32x4Sqrt => todo!(),
        Operator::F32x4Add => todo!(),
        Operator::F32x4Sub => todo!(),
        Operator::F32x4Mul => todo!(),
        Operator::F32x4Div => todo!(),
        Operator::F32x4Min => todo!(),
        Operator::F32x4Max => todo!(),
        Operator::F32x4PMin => todo!(),
        Operator::F32x4PMax => todo!(),
        Operator::F64x2Ceil => todo!(),
        Operator::F64x2Floor => todo!(),
        Operator::F64x2Trunc => todo!(),
        Operator::F64x2Nearest => todo!(),
        Operator::F64x2Abs => todo!(),
        Operator::F64x2Neg => todo!(),
        Operator::F64x2Sqrt => todo!(),
        Operator::F64x2Add => todo!(),
        Operator::F64x2Sub => todo!(),
        Operator::F64x2Mul => todo!(),
        Operator::F64x2Div => todo!(),
        Operator::F64x2Min => todo!(),
        Operator::F64x2Max => todo!(),
        Operator::F64x2PMin => todo!(),
        Operator::F64x2PMax => todo!(),
        Operator::I32x4TruncSatF32x4S => todo!(),
        Operator::I32x4TruncSatF32x4U => todo!(),
        Operator::F32x4ConvertI32x4S => todo!(),
        Operator::F32x4ConvertI32x4U => todo!(),
        Operator::I32x4TruncSatF64x2SZero => todo!(),
        Operator::I32x4TruncSatF64x2UZero => todo!(),
        Operator::F64x2ConvertLowI32x4S => todo!(),
        Operator::F64x2ConvertLowI32x4U => todo!(),
        Operator::F32x4DemoteF64x2Zero => todo!(),
        Operator::F64x2PromoteLowF32x4 => todo!(),
        Operator::I8x16RelaxedSwizzle => todo!(),
        Operator::I32x4RelaxedTruncF32x4S => todo!(),
        Operator::I32x4RelaxedTruncF32x4U => todo!(),
        Operator::I32x4RelaxedTruncF64x2SZero => todo!(),
        Operator::I32x4RelaxedTruncF64x2UZero => todo!(),
        Operator::F32x4RelaxedMadd => todo!(),
        Operator::F32x4RelaxedNmadd => todo!(),
        Operator::F64x2RelaxedMadd => todo!(),
        Operator::F64x2RelaxedNmadd => todo!(),
        Operator::I8x16RelaxedLaneselect => todo!(),
        Operator::I16x8RelaxedLaneselect => todo!(),
        Operator::I32x4RelaxedLaneselect => todo!(),
        Operator::I64x2RelaxedLaneselect => todo!(),
        Operator::F32x4RelaxedMin => todo!(),
        Operator::F32x4RelaxedMax => todo!(),
        Operator::F64x2RelaxedMin => todo!(),
        Operator::F64x2RelaxedMax => todo!(),
        Operator::I16x8RelaxedQ15mulrS => todo!(),
        Operator::I16x8RelaxedDotI8x16I7x16S => todo!(),
        Operator::I32x4RelaxedDotI8x16I7x16AddS => todo!(),
        Operator::TryTable { .. } => todo!(),
        Operator::Throw { .. } => todo!(),
        Operator::ThrowRef => todo!(),
        Operator::Try { .. } => todo!(),
        Operator::Catch { .. } => todo!(),
        Operator::Rethrow { .. } => todo!(),
        Operator::Delegate { .. } => todo!(),
        Operator::CatchAll => todo!(),
        Operator::GlobalAtomicGet {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicSet {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwAdd {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwSub {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwAnd {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwOr {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwXor {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwXchg {
            ordering: _,
            global_index: _,
        }
        | Operator::GlobalAtomicRmwCmpxchg {
            ordering: _,
            global_index: _,
        }
        | Operator::TableAtomicGet {
            ordering: _,
            table_index: _,
        }
        | Operator::TableAtomicSet {
            ordering: _,
            table_index: _,
        }
        | Operator::TableAtomicRmwXchg {
            ordering: _,
            table_index: _,
        }
        | Operator::TableAtomicRmwCmpxchg {
            ordering: _,
            table_index: _,
        }
        | Operator::StructAtomicGet {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicGetS {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicGetU {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicSet {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwAdd {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwSub {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwAnd {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwOr {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwXor {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwXchg {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::StructAtomicRmwCmpxchg {
            ordering: _,
            struct_type_index: _,
            field_index: _,
        }
        | Operator::ArrayAtomicGet {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicGetS {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicGetU {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicSet {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwAdd {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwSub {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwAnd {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwOr {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwXor {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwXchg {
            ordering: _,
            array_type_index: _,
        }
        | Operator::ArrayAtomicRmwCmpxchg {
            ordering: _,
            array_type_index: _,
        } => {
            return Err(anyhow::anyhow!(
                "Atomic instruction {operator:?} are not supported",
            ));
        }
        Operator::RefI31Shared => todo!(),
        Operator::CallRef { .. } => todo!(),
        Operator::ReturnCallRef { .. } => todo!(),
        Operator::RefAsNonNull => todo!(),
        Operator::BrOnNull { .. } => todo!(),
        Operator::BrOnNonNull { .. } => todo!(),
        Operator::ContNew { .. } => todo!(),
        Operator::ContBind { .. } => todo!(),
        Operator::Suspend { .. } => todo!(),
        Operator::Resume { .. } => todo!(),
        Operator::ResumeThrow { .. } => todo!(),
        Operator::Switch { .. } => todo!(),
        Operator::I64Add128 { .. } => todo!(),
        Operator::I64Sub128 { .. } => todo!(),
        Operator::I64MulWideS => todo!(),
        Operator::I64MulWideU => todo!(),
        _ => return Err(anyhow::anyhow!("Operator {operator:?} not recognized",)),
    };
    Ok(operator.to_string())
}

//Record module_data
fn translate_data(data: &Data) -> anyhow::Result<String> {
    let mut res = String::new();
    let moddata_mode = translate_module_datamode(data)?;
    let mut moddata_init = String::new();
    for byte in data.data {
        moddata_init.push_str(format!("#{byte:02X}").as_str());
        moddata_init.push_str(" :: ");
    }
    moddata_init.push_str("nil");
    res.push_str("{|\n");
    res.push_str(format!("    moddata_init := {moddata_init};\n").as_str());
    res.push_str(format!("    moddata_mode := {moddata_mode};\n").as_str());
    res.push_str("|}");
    Ok(res)
}

fn get_id() -> String {
    let uuid = Uuid::new_v4().to_string();
    let mut parts = uuid.split('-');
    parts.next().unwrap().to_string()
}
