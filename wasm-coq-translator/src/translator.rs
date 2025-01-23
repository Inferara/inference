use core::fmt;
use std::{fmt::Display, iter::Peekable};

use anyhow::bail;
use uuid::Uuid;
use wasmparser::{
    AbstractHeapType, BlockType, CompositeInnerType, Data, DataKind, Element, ElementItems,
    ElementKind, Export, FuncType, FunctionBody, Global, HeapType, Import, MemoryType, Operator,
    OperatorsIterator, OperatorsReader, RecGroup, RefType, Table, TableType, TypeRef,
    ValType as wpValType,
};

const LCB: &str = "{|\n";
const RCB_DOT: &str = "|}.\n";
const LRB: char = '(';
const RRB: char = ')';

const LIST_EXT: &str = " :: ";
const LIST_SEAL: &str = "nil)";

pub(crate) struct WasmParseData<'a> {
    mod_name: String,

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
}

impl WasmParseData<'_> {
    pub(crate) fn new<'a>(mod_name: String) -> WasmParseData<'a> {
        WasmParseData {
            mod_name,
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
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn translate(&self) -> anyhow::Result<String /* WasmModuleParseError*/> {
        let mut res = String::new();
        res.push_str("Require Import List.\n");
        res.push_str("Require Import String.\n");
        res.push_str("Require Import BinNat.\n");
        res.push_str("Require Import ZArith.\n");
        res.push_str("From Wasm Require Import numerics.\n");
        res.push_str("From Wasm Require Import datatypes.\n");

        res.push_str("\n\n");

        let mut translated_imports = String::new();
        let mut errors = Vec::new();
        for import in &self.imports {
            match translate_module_import(import) {
                Ok(translated_import) => {
                    translated_imports.push_str(translated_import.as_str());
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        let mut created_exports = String::new();
        for export in &self.exports {
            created_exports.push(LRB);
            match translate_export_module(export) {
                Ok(translated_export) => {
                    created_exports.push_str(translated_export.as_str());
                    created_exports.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
            created_exports.push_str(LIST_SEAL);
        }
        let mut created_tables = String::new();
        for table in &self.tables {
            created_tables.push(LRB);
            match translate_table_type(table) {
                Ok(translated_table_type) => {
                    created_tables.push_str(translated_table_type.as_str());
                    created_tables.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
            created_tables.push_str(LIST_SEAL);
        }
        let mut created_memory_types = String::new();
        for memory_type in &self.memory_types {
            created_memory_types.push(LRB);
            match translate_memory_type(memory_type) {
                Ok(translated_memory) => {
                    created_memory_types.push_str(translated_memory.as_str());
                    created_memory_types.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
            created_memory_types.push_str(LIST_SEAL);
        }
        let mut created_globals = String::new();
        for global in &self.globals {
            created_globals.push(LRB);
            match translate_global(global) {
                Ok(translated_global) => {
                    created_globals.push_str(translated_global.as_str());
                    created_globals.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
            created_globals.push_str(LIST_SEAL);
        }
        let mut created_data_segments = String::new();
        for data in &self.data {
            created_data_segments.push(LRB);
            match translate_data(data) {
                Ok(translated_data) => {
                    created_data_segments.push_str(translated_data.as_str());
                    created_data_segments.push_str(LIST_EXT);
                }
                Err(e) => errors.push(e),
            }
            created_data_segments.push_str(LIST_SEAL);
        }
        let mut created_elements = String::new();
        for element in &self.elements {
            created_elements.push(LRB);
            match translate_element(element) {
                Ok(translated_element) => {
                    created_elements.push_str(translated_element.as_str());
                    created_elements.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
            created_elements.push_str(LIST_SEAL);
        }

        let mut created_function_types = String::new();
        for rec_group in &self.function_types {
            created_function_types.push(LRB);
            match translate_function_type(rec_group) {
                Ok(translated_function_type) => {
                    created_function_types.push_str(translated_function_type.as_str());
                    created_function_types.push_str(LIST_EXT);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
            created_function_types.push_str(LIST_SEAL);
        }

        let mut created_functions = String::new();
        match translate_functions(&self.function_type_indexes, &self.function_bodies) {
            Ok(translated_function) => {
                res.push_str(translated_function.as_str());
                created_functions.push_str(translated_function.as_str());
            }
            Err(e) => {
                errors.push(e);
            }
        };

        //Record module
        let module_name = &self.mod_name;
        res.push_str(format!("Definition {module_name} : module :=\n").as_str());
        res.push_str(LCB);
        res.push_str(format!("mod_types := Tf {created_function_types};").as_str());
        res.push_str(format!("mod_funcs := Tf {created_functions};").as_str());
        res.push_str(format!("mod_tables := Tf {created_tables};").as_str());
        res.push_str(format!("mod_mems := Tf {created_memory_types};").as_str());
        res.push_str(format!("mod_globals := Tf {created_globals};").as_str());
        res.push_str(format!("mod_elems := Tf {created_elements};").as_str());
        res.push_str(format!("mod_datas := Tf {created_data_segments};").as_str());
        if let Some(start_function) = self.start_function {
            res.push_str(format!("mod_start := Some({start_function});\n").as_str());
        } else {
            res.push_str("mod_start := None;\n");
        }
        res.push_str(format!("mod_imports := Tf {translated_imports};").as_str());
        res.push_str(format!("mod_exports := Tf {created_exports};").as_str());
        res.push_str(RCB_DOT);
        res.push_str(".\n");
        Ok(res)
    }
}

//Inductive reference_type
fn translate_ref_type(ref_type: &RefType) -> anyhow::Result<String> {
    if *ref_type == RefType::FUNCREF {
        Ok(String::from("T_funcref"))
    } else if *ref_type == RefType::EXTERNREF {
        Ok(String::from("T_externref"))
    } else {
        Err(anyhow::anyhow!("Unsupported reference type {:?}", ref_type))
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
    let definition_name =
        imp_module.clone() + &imp_name.clone().remove(0).to_uppercase().to_string();
    let imp_desc = translate_module_import_desc(import)?;
    let mut res = String::new();
    res.push_str(format!("Definition {definition_name} : module_import :=\n").as_str());
    res.push_str(LCB);
    res.push_str(format!("imp_module := \"{imp_module}\";\n").as_str());
    res.push_str(format!("imp_name := \"{imp_name}\";\n").as_str());
    res.push_str(format!("imp_desc := {imp_desc}\n").as_str());
    res.push_str(RCB_DOT);
    res.push_str(".\n");
    Ok(res)
}

//Inductive module_import_desc
fn translate_module_import_desc(import: &Import) -> anyhow::Result<String> {
    let res = match import.ty {
        TypeRef::Func(index) => format!("MID_func {index}"),
        TypeRef::Global(global_type) => {
            let tg_mut = translate_mutability(global_type.mutable);
            let tg_t = translate_value_type(&global_type.content_type)?;
            format!("MID_global tg_mut := {tg_mut}; tg_t := {tg_t}")
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
    let lim_min = table_type.initial.to_string();
    let lim_max = match table_type.maximum {
        Some(max) => max.to_string(),
        None => "None".to_string(),
    };
    let ref_type = translate_ref_type(&table_type.element_type)?;
    Ok(format!("{LCB} tt_limits := {LCB} lim_min := {lim_min}; lim_max := {lim_max} {RCB_DOT}; tt_elem_type := {ref_type} {RCB_DOT}"))
}

//Record limits
fn translate_memory_type_limits(memory_type: &MemoryType) -> anyhow::Result<String> {
    let lim_min = memory_type.initial.to_string();
    let lim_max = match memory_type.maximum {
        Some(max) => max.to_string(),
        None => "None".to_string(),
    };
    Ok(format!(
        "{LCB} l_min := {lim_min}; l_max := {lim_max} {RCB_DOT}"
    ))
}

//Inductive translate_export_module
fn translate_export_module(export: &Export) -> anyhow::Result<String> {
    let mut res = String::new();
    let modexp_name = export.name;
    let modexp_desc = translate_module_export_desc(export)?;
    res.push_str(format!("Definition {modexp_name} : module_export :=\n").as_str());
    res.push_str(LCB);
    res.push_str(format!("modexp_name := \"{modexp_name}\";\n").as_str());
    res.push_str(format!("modexp_desc := {modexp_desc}\n").as_str());
    res.push_str(RCB_DOT);
    res.push_str(".\n");
    Ok(res)
}

//Inductive module_export_desc
fn translate_module_export_desc(export: &Export) -> anyhow::Result<String> {
    let res = match export.kind {
        wasmparser::ExternalKind::Func => format!("MED_func {}", export.index),
        wasmparser::ExternalKind::Table => format!("MED_table {}", export.index),
        wasmparser::ExternalKind::Memory => format!("MED_mem {}", export.index),
        wasmparser::ExternalKind::Global => format!("MED_global {}", export.index),
        wasmparser::ExternalKind::Tag => return Err(anyhow::anyhow!("Tag is not supported")),
    };
    Ok(res)
}

//Record table_type
fn translate_table_type(table: &Table) -> anyhow::Result<String> {
    let mut res = String::new();
    let tt_limits = translate_table_type_limits(&table.ty)?;
    let tt_elem_type = translate_ref_type(&table.ty.element_type)?;
    let id = get_id();
    res.push_str(format!("Definition tt_{id} : table_type :=\n").as_str());
    res.push_str(LCB);
    res.push_str(format!("tt_limits := {tt_limits};\n").as_str());
    res.push_str(format!("tt_elem_type := {tt_elem_type}\n").as_str());
    res.push_str(RCB_DOT);
    res.push_str(".\n");
    Ok(res)
}

//Definition memory_type
fn translate_memory_type(memory_type: &MemoryType) -> anyhow::Result<String> {
    let mut res = String::new();
    let id = get_id();
    let limits = translate_memory_type_limits(memory_type)?;
    res.push_str(format!("Definition mem_{id} : memory_type :=\n").as_str());
    res.push_str(LCB);
    res.push_str(format!("limits := {limits}\n").as_str());
    res.push_str(RCB_DOT);
    res.push_str(".\n");
    Ok(res)
}

//Record global_type
fn translate_global(global: &Global) -> anyhow::Result<String> {
    let mut res = String::new();
    let id = get_id();
    let tg_mut = translate_mutability(global.ty.mutable);
    let tg_t = translate_value_type(&global.ty.content_type)?;
    res.push_str(format!("Definition global_{id} : global_type :=\n").as_str());
    res.push_str(LCB);
    res.push_str(format!("tg_mut := {tg_mut};\n").as_str());
    res.push_str(format!("tg_t := {tg_t}\n").as_str());
    res.push_str(RCB_DOT);
    res.push_str(".\n");
    Ok(res)
}

//Inductive module_datamode
fn translate_module_datamode(data: &Data) -> anyhow::Result<String> {
    let res = match &data.kind {
        DataKind::Active {
            memory_index,
            offset_expr,
        } => {
            let expression = translate_expr(&mut offset_expr.get_operators_reader())?;
            format!("MD_active {memory_index} ({expression})")
        }
        DataKind::Passive => "MD_passive".to_string(),
    };
    Ok(res)
}

enum ExpressionPart<'a> {
    Operator(Operator<'a>),
    Expression(Expression<'a>),
}

struct Expression<'a> {
    parts: Vec<ExpressionPart<'a>>,
}

impl Expression<'_> {
    fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    fn print_with_offset(&self, tabs_count: usize) -> String {
        let mut res = String::new();
        let prefix = "  ".repeat(tabs_count);
        for part in &self.parts {
            res.push_str(&prefix);
            match part {
                ExpressionPart::Operator(op) => {
                    let translated_op = translate_basic_operator(op).unwrap();
                    res.push_str(&translated_op);
                }
                ExpressionPart::Expression(expr) => {
                    res.push_str(" (\n");
                    res.push_str(&expr.print_with_offset(tabs_count + 1));
                    res.push_str(")\n");
                }
            }
            res.push_str(LIST_EXT);
            res.push('\n');
        }
        res.push_str(&"  ".repeat(tabs_count));
        res.push_str("nil");
        res
    }
}

impl Display for Expression<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.print_with_offset(2))
    }
}

fn translate_expression<'a>(
    operators_reader: &mut Peekable<OperatorsIterator<'a>>,
) -> anyhow::Result<Expression<'a>> {
    let mut result = Expression { parts: Vec::new() };
    while let Some(next_operator) = operators_reader.peek().cloned() {
        let next_operator = next_operator.as_ref().unwrap();
        match next_operator {
            wasmparser::Operator::Block { .. }
            | wasmparser::Operator::Loop { .. }
            | wasmparser::Operator::If { .. } => {
                operators_reader.next();
                let inner_expression = translate_expression(operators_reader)?;
                if !inner_expression.is_empty() {
                    result
                        .parts
                        .push(ExpressionPart::Expression(inner_expression));
                }
            }
            wasmparser::Operator::Else => {
                result
                    .parts
                    .push(ExpressionPart::Operator(next_operator.to_owned()));
                operators_reader.next();
                let inner_expression = translate_expression(operators_reader)?;
                result
                    .parts
                    .push(ExpressionPart::Expression(inner_expression));
            }
            wasmparser::Operator::End => {
                operators_reader.next();
                break;
            }
            _ => {
                operators_reader.next();
                result
                    .parts
                    .push(ExpressionPart::Operator(next_operator.to_owned()))
            }
        }
    }
    Ok(result)
}

//Definition expr
fn translate_expr(operators_reader: &mut OperatorsReader) -> anyhow::Result<String> {
    let mut peekable_operators_reader = operators_reader.clone().into_iter().peekable();
    let expression = translate_expression(&mut peekable_operators_reader)?;
    Ok(expression.to_string())
}

fn translate_block_type(block_type: &BlockType) -> anyhow::Result<String> {
    let res = match block_type {
        BlockType::Empty => String::new(),
        BlockType::FuncType(index) => format!("BT_id {index}"),
        BlockType::Type(valtype) => {
            let valtype = translate_value_type(valtype)?;
            format!("BT_valtype {valtype}")
        }
    };
    Ok(res)
}

//Record memarg
fn translate_memarg(memarg: &wasmparser::MemArg) -> anyhow::Result<String> {
    let mut res = String::new();
    let id = get_id();
    let memarg_offset = memarg.offset.to_string();
    let memarg_align = memarg.align.to_string();
    res.push_str(format!("Definition memarg_{id} : memarg :=\n").as_str());
    res.push_str(LCB);
    res.push_str(format!("memarg_offset := {memarg_offset};\n").as_str());
    res.push_str(format!("memarg_align := {memarg_align}\n").as_str());
    res.push_str(RCB_DOT);
    res.push_str(".\n");
    Ok(res)
}

//Record module_element
fn translate_element(element: &Element) -> anyhow::Result<String> {
    let mut res = String::new();
    let id = get_id();
    let module_elemmode = match &element.kind {
        ElementKind::Active {
            table_index,
            offset_expr,
        } => {
            let tableidx = table_index.unwrap_or_default();
            let mut expr = String::new();
            for operator in offset_expr.get_operators_reader() {
                let op = operator?;
                let translated_op = translate_basic_operator(&op)?;
                expr.push_str(translated_op.as_str());
                expr.push_str("::");
            }
            expr.push_str("nil");
            format!("ME_active {tableidx} ({expr})")
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
                let mut expr = String::new();
                for operator in expr_reader.get_operators_reader() {
                    let op = operator?;
                    let translated_op = translate_basic_operator(&op)?;
                    expr.push_str(translated_op.as_str());
                    expr.push_str("::");
                }
                expr.push_str("nil");
                expr_list.push_str(expr.as_str());
                expr_list.push_str("::");
            }
            format!("ME_expressions ({expr_list})")
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
    res.push_str(format!("Definition element_{id} : module_element :=\n").as_str());
    res.push_str(LCB);
    res.push_str(format!("modelem_type := {modelem_type};\n").as_str());
    res.push_str(format!("modelem_init := {modelem_init};\n").as_str());
    res.push_str(format!("module_elemmode := {module_elemmode};\n").as_str());
    res.push_str(RCB_DOT);
    res.push_str(".\n");
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
    let id = get_id();
    for ty in rec_group.types() {
        match &ty.composite_type.inner {
            CompositeInnerType::Func(ft) => {
                let mut params_str = String::new();
                for param in ft.params() {
                    let val_type = translate_value_type(param)?;
                    params_str.push_str(format!("{val_type} :: ").as_str());
                }
                params_str.push_str("nil;\n");

                let mut results_str = String::new();
                for result in ft.results() {
                    let val_type = translate_value_type(result)?;
                    results_str.push_str(format!("{val_type} :: ").as_str());
                }
                results_str.push_str("nil;\n");

                res.push_str(format!("Definition ft_{id} : function_type :=\n").as_str());
                res.push_str(LCB);
                res.push_str(format!("ft_params := {params_str};").as_str());
                res.push_str(format!("ft_results := {results_str}").as_str());
                res.push_str(RCB_DOT);
                res.push_str(".\n");
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

//Record module_func
fn translate_functions(
    function_type_indexes: &[u32],
    function_bodies: &[FunctionBody],
) -> anyhow::Result<String> {
    let mut res = String::new();
    for (index, function_body) in function_bodies.iter().enumerate() {
        let id = get_id();
        let modfunc_type = *function_type_indexes.get(index).unwrap_or(&0);

        let mut modfunc_locals = String::new();
        if let Ok(locals_reader) = function_body.get_locals_reader() {
            for local in locals_reader {
                let (_, val_type) = local.unwrap();
                let val_type = translate_value_type(&val_type)?;
                modfunc_locals.push_str(format!("{val_type} :: ").as_str());
            }
        }
        modfunc_locals.push_str("nil");

        let modfunc_body = translate_expr(&mut function_body.get_operators_reader()?)?;

        res.push_str(format!("Definition func_{id} : module_func :=\n").as_str());
        res.push_str(LCB);
        res.push_str(format!("modfunc_type := {modfunc_type}%N;\n").as_str());
        res.push_str(format!("modfunc_locals := {modfunc_locals};\n").as_str());
        res.push_str(format!("modfunc_body :=\n{modfunc_body};\n").as_str());
        res.push_str(RCB_DOT);
        res.push('\n');
    }
    Ok(res)
}

//Inductive basic_instruction
fn translate_basic_operator(operator: &Operator) -> anyhow::Result<String> {
    let operator = match operator {
        wasmparser::Operator::Nop => "BI_nop".to_string(),
        wasmparser::Operator::Unreachable => "BI_unreachable".to_string(),
        wasmparser::Operator::Block { blockty } => {
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
        Operator::Else => String::new(),
        Operator::End => String::new(),
        Operator::Br { relative_depth } => format!("BI_br {relative_depth}"),
        Operator::BrIf { relative_depth } => format!("BI_br_if {relative_depth}"),
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
        Operator::LocalGet { local_index } => format!("BI_local_get {local_index}%N"),
        Operator::LocalSet { local_index } => format!("BI_local_set {local_index}%N"),
        Operator::LocalTee { local_index } => format!("BI_local_tee {local_index}%N"),
        Operator::GlobalGet { global_index } => format!("BI_global_get {global_index}%N"),
        Operator::GlobalSet { global_index } => format!("BI_global_set {global_index}%N"),
        Operator::I32Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 None {memarg}")
        }
        Operator::I64Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 None {memarg}")
        }
        Operator::F32Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_f32 None {memarg}")
        }
        Operator::F64Load { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_f64 None {memarg}")
        }
        Operator::I32Load8S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i8, SX_S)) {memarg}")
        }
        Operator::I32Load8U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i8, SX_U)) {memarg}")
        }
        Operator::I32Load16S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i16, SX_S)) {memarg}")
        }
        Operator::I32Load16U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i32 (Some (Tp_i16, SX_U)) {memarg}")
        }
        Operator::I64Load8S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i8, SX_S)) {memarg}")
        }
        Operator::I64Load8U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i8, SX_U)) {memarg}")
        }
        Operator::I64Load16S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i16, SX_S)) {memarg}")
        }
        Operator::I64Load16U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i16, SX_U)) {memarg}")
        }
        Operator::I64Load32S { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i32, SX_S)) {memarg}")
        }
        Operator::I64Load32U { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_load T_i64 (Some (Tp_i32, SX_U)) {memarg}")
        }
        Operator::I32Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i32 None {memarg}")
        }
        Operator::I64Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 None {memarg}")
        }
        Operator::F32Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_f32 None {memarg}")
        }
        Operator::F64Store { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_f64 None {memarg}")
        }
        Operator::I32Store8 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i32 (Some Tp_i8) {memarg}")
        }
        Operator::I32Store16 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i32 (Some Tp_i16) {memarg}")
        }
        Operator::I64Store8 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 (Some Tp_i8) {memarg}")
        }
        Operator::I64Store16 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 (Some Tp_i16) {memarg}")
        }
        Operator::I64Store32 { memarg } => {
            let memarg = translate_memarg(memarg)?;
            format!("BI_store T_i64 (Some Tp_i32) {memarg}")
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
        Operator::I32Const { value } => format!("BI_const_num (VAL_int32 {value})"),
        Operator::I64Const { value } => format!("BI_const_num (VAL_int64 {value})"),
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
        Operator::F32Eq => "BI_relop T_f32 (relop_f ROI_eq)".to_string(),
        Operator::F32Ne => "BI_relop T_f32 (relop_f ROI_ne)".to_string(),
        Operator::F32Lt => "BI_relop T_f32 (relop_f ROI_lt)".to_string(),
        Operator::F32Gt => "BI_relop T_f32 (relop_f ROI_gt)".to_string(),
        Operator::F32Le => "BI_relop T_f32 (relop_f ROI_le)".to_string(),
        Operator::F32Ge => "BI_relop T_f32 (relop_f ROI_ge)".to_string(),
        Operator::F64Eq => "BI_relop T_f64 (relop_f ROI_eq)".to_string(),
        Operator::F64Ne => "BI_relop T_f64 (relop_f ROI_ne)".to_string(),
        Operator::F64Lt => "BI_relop T_f64 (relop_f ROI_lt)".to_string(),
        Operator::F64Gt => "BI_relop T_f64 (relop_f ROI_gt)".to_string(),
        Operator::F64Le => "BI_relop T_f64 (relop_f ROI_le)".to_string(),
        Operator::F64Ge => "BI_relop T_f64 (relop_f ROI_ge)".to_string(),
        Operator::I32Clz => "BI_unop T_i32 (Unop_i UOI_clz)".to_string(),
        Operator::I32Ctz => "BI_unop T_i32 (Unop_i UOI_ctz)".to_string(),
        Operator::I32Popcnt => "BI_unop T_i32 (Unop_i UOI_popcnt)".to_string(),
        Operator::I32Add => "BI_binop T_i32 (binop_i BOI_add)".to_string(),
        Operator::I32Sub => "BI_binop T_i32 (binop_i BOI_sub)".to_string(),
        Operator::I32Mul => "BI_binop T_i32 (binop_i BOI_mul)".to_string(),
        Operator::I32DivS => "BI_binop T_i32 (binop_i (BOI_div SX_S))".to_string(),
        Operator::I32DivU => "BI_binop T_i32 (binop_i (BOI_div SX_U))".to_string(),
        Operator::I32RemS => "BI_binop T_i32 (binop_i (BOI_rem SX_S))".to_string(),
        Operator::I32RemU => "BI_binop T_i32 (binop_i (BOI_rem SX_U))".to_string(),
        Operator::I32And => "BI_binop T_i32 (binop_i BOI_and)".to_string(),
        Operator::I32Or => "BI_binop T_i32 (binop_i BOI_or)".to_string(),
        Operator::I32Xor => "BI_binop T_i32 (binop_i BOI_xor)".to_string(),
        Operator::I32Shl => "BI_binop T_i32 (binop_i BOI_shl)".to_string(),
        Operator::I32ShrS => "BI_binop T_i32 (binop_i (BOI_shr SX_S))".to_string(),
        Operator::I32ShrU => "BI_binop T_i32 (binop_i (BOI_shr SX_U))".to_string(),
        Operator::I32Rotl => "BI_binop T_i32 (binop_i BOI_rotl)".to_string(),
        Operator::I32Rotr => "BI_binop T_i32 (binop_i BOI_rotr)".to_string(),
        Operator::I64Clz => "BI_unop T_i64 (Unop_i UOI_clz)".to_string(),
        Operator::I64Ctz => "BI_unop T_i64 (Unop_i UOI_ctz)".to_string(),
        Operator::I64Popcnt => "BI_unop T_i64 (Unop_i UOI_popcnt)".to_string(),
        Operator::I64Add => "BI_binop T_i64 (binop_i BOI_add)".to_string(),
        Operator::I64Sub => "BI_binop T_i64 (binop_i BOI_sub)".to_string(),
        Operator::I64Mul => "BI_binop T_i64 (binop_i BOI_mul)".to_string(),
        Operator::I64DivS => "BI_binop T_i64 (binop_i (BOI_div SX_S))".to_string(),
        Operator::I64DivU => "BI_binop T_i64 (binop_i (BOI_div SX_U))".to_string(),
        Operator::I64RemS => "BI_binop T_i64 (binop_i (BOI_rem SX_S))".to_string(),
        Operator::I64RemU => "BI_binop T_i64 (binop_i (BOI_rem SX_U))".to_string(),
        Operator::I64And => "BI_binop T_i64 (binop_i BOI_and)".to_string(),
        Operator::I64Or => "BI_binop T_i64 (binop_i BOI_or)".to_string(),
        Operator::I64Xor => "BI_binop T_i64 (binop_i BOI_xor)".to_string(),
        Operator::I64Shl => "BI_binop T_i64 (binop_i BOI_shl)".to_string(),
        Operator::I64ShrS => "BI_binop T_i64 (binop_i (BOI_shr SX_S))".to_string(),
        Operator::I64ShrU => "BI_binop T_i64 (binop_i (BOI_shr SX_U))".to_string(),
        Operator::I64Rotl => "BI_binop T_i64 (binop_i BOI_rotl)".to_string(),
        Operator::I64Rotr => "BI_binop T_i64 (binop_i BOI_rotr)".to_string(),
        Operator::F32Abs => "BI_unop T_f32 (Unop_f UOF_abs)".to_string(),
        Operator::F32Neg => "BI_unop T_f32 (Unop_f UOF_neg)".to_string(),
        Operator::F32Ceil => "BI_unop T_f32 (Unop_f UOF_ceil)".to_string(),
        Operator::F32Floor => "BI_unop T_f32 (Unop_f UOF_floor)".to_string(),
        Operator::F32Trunc => "BI_unop T_f32 (Unop_f UOF_trunc)".to_string(),
        Operator::F32Nearest => "BI_unop T_f32 (Unop_f UOF_nearest)".to_string(),
        Operator::F32Sqrt => "BI_unop T_f32 (Unop_f UOF_sqrt)".to_string(),
        Operator::F32Add => "BI_binop T_f32 (binop_f BOF_add)".to_string(),
        Operator::F32Sub => "BI_binop T_f32 (binop_f BOF_sub)".to_string(),
        Operator::F32Mul => "BI_binop T_f32 (binop_f BOF_mul)".to_string(),
        Operator::F32Div => "BI_binop T_f32 (binop_f BOF_div)".to_string(),
        Operator::F32Min => "BI_binop T_f32 (binop_f BOF_min)".to_string(),
        Operator::F32Max => "BI_binop T_f32 (binop_f BOF_max)".to_string(),
        Operator::F32Copysign => "BI_binop T_f32 (binop_f BOF_copysign)".to_string(),
        Operator::F64Abs => "BI_unop T_f64 (Unop_f UOF_abs)".to_string(),
        Operator::F64Neg => "BI_unop T_f64 (Unop_f UOF_neg)".to_string(),
        Operator::F64Ceil => "BI_unop T_f64 (Unop_f UOF_ceil)".to_string(),
        Operator::F64Floor => "BI_unop T_f64 (Unop_f UOF_floor)".to_string(),
        Operator::F64Trunc => "BI_unop T_f64 (Unop_f UOF_trunc)".to_string(),
        Operator::F64Nearest => "BI_unop T_f64 (Unop_f UOF_nearest)".to_string(),
        Operator::F64Sqrt => "BI_unop T_f64 (Unop_f UOF_sqrt)".to_string(),
        Operator::F64Add => "BI_binop T_f64 (binop_f BOF_add)".to_string(),
        Operator::F64Sub => "BI_binop T_f64 (binop_f BOF_sub)".to_string(),
        Operator::F64Mul => "BI_binop T_f64 (binop_f BOF_mul)".to_string(),
        Operator::F64Div => "BI_binop T_f64 (binop_f BOF_div)".to_string(),
        Operator::F64Min => "BI_binop T_f64 (binop_f BOF_min)".to_string(),
        Operator::F64Max => "BI_binop T_f64 (binop_f BOF_max)".to_string(),
        Operator::F64Copysign => "BI_binop T_f64 (binop_f BOF_copysign)".to_string(),
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
        Operator::StructNew { struct_type_index } => todo!(),
        Operator::StructNewDefault { struct_type_index } => todo!(),
        Operator::StructGet {
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructGetS {
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructGetU {
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::StructSet {
            struct_type_index,
            field_index,
        } => todo!(),
        Operator::ArrayNew { array_type_index } => todo!(),
        Operator::ArrayNewDefault { array_type_index } => todo!(),
        Operator::ArrayNewFixed {
            array_type_index,
            array_size,
        } => todo!(),
        Operator::ArrayNewData {
            array_type_index,
            array_data_index,
        } => todo!(),
        Operator::ArrayNewElem {
            array_type_index,
            array_elem_index,
        } => todo!(),
        Operator::ArrayGet { array_type_index } => todo!(),
        Operator::ArrayGetS { array_type_index } => todo!(),
        Operator::ArrayGetU { array_type_index } => todo!(),
        Operator::ArraySet { array_type_index } => todo!(),
        Operator::ArrayLen => todo!(),
        Operator::ArrayFill { array_type_index } => todo!(),
        Operator::ArrayCopy {
            array_type_index_dst,
            array_type_index_src,
        } => todo!(),
        Operator::ArrayInitData {
            array_type_index,
            array_data_index,
        } => todo!(),
        Operator::ArrayInitElem {
            array_type_index,
            array_elem_index,
        } => todo!(),
        Operator::RefTestNonNull { hty } => todo!(),
        Operator::RefTestNullable { hty } => todo!(),
        Operator::RefCastNonNull { hty } => todo!(),
        Operator::RefCastNullable { hty } => todo!(),
        Operator::BrOnCast {
            relative_depth,
            from_ref_type,
            to_ref_type,
        } => todo!(),
        Operator::BrOnCastFail {
            relative_depth,
            from_ref_type,
            to_ref_type,
        } => todo!(),
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
        Operator::TableInit { elem_index, table } => todo!(),
        Operator::ElemDrop { elem_index } => todo!(),
        Operator::TableCopy {
            dst_table,
            src_table,
        } => todo!(),
        Operator::TypedSelect { ty } => todo!(),
        Operator::RefNull { hty } => todo!(),
        Operator::RefIsNull => "BI_ref_is_null".to_string(),
        Operator::RefFunc { function_index } => format!("BI_ref_func {function_index}"),
        Operator::TableFill { table } => format!("BI_table_fill {table}"),
        Operator::TableGet { table } => format!("BI_table_get {table}"),
        Operator::TableSet { table } => format!("BI_table_set {table}"),
        Operator::TableGrow { table } => format!("BI_table_grow {table}"),
        Operator::TableSize { table } => format!("BI_table_size {table}"),
        Operator::ReturnCall { function_index } => todo!(),
        Operator::ReturnCallIndirect {
            type_index,
            table_index,
        } => todo!(),
        Operator::MemoryDiscard { mem } => todo!(),
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
                "Atomic instruction {:?} are not supported",
                operator
            ))
        }
        Operator::V128Load { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i16, SX_U)) {memarg}")
        }
        Operator::V128Load8x8S { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i8, SX_S)) {memarg}")
        }
        Operator::V128Load8x8U { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i8, SX_U)) {memarg}")
        }
        Operator::V128Load16x4S { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i16, SX_S)) {memarg}")
        }
        Operator::V128Load16x4U { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i16, SX_U)) {memarg}")
        }
        Operator::V128Load32x2S { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i32, SX_S)) {memarg}")
        }
        Operator::V128Load32x2U { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_packed T_i64 (Some (Tp_i32, SX_U)) {memarg}")
        }
        Operator::V128Load8Splat { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_splat Twv_8 {memarg}")
        }
        Operator::V128Load16Splat { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_splat Twv_16 {memarg}")
        }
        Operator::V128Load32Splat { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_splat Twv_32 {memarg}")
        }
        Operator::V128Load64Splat { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_splat Twv_64 {memarg}")
        }
        Operator::V128Load32Zero { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_zero Tztv_32 {memarg}")
        }
        Operator::V128Load64Zero { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_load_vec LVA_zero Tztv_64 {memarg}")
        }
        Operator::V128Store { memarg } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_store_vec_lane Twv_64 {memarg} 0")
        }
        Operator::V128Load8Lane { memarg, lane } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_store_vec_lane Twv_8 {memarg} {lane}")
        }
        Operator::V128Load16Lane { memarg, lane } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_store_vec_lane Twv_16 {memarg} {lane}")
        }
        Operator::V128Load32Lane { memarg, lane } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_store_vec_lane Twv_32 {memarg} {lane}")
        }
        Operator::V128Load64Lane { memarg, lane } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_store_vec_lane Twv_64 {memarg} {lane}")
        }
        Operator::V128Store8Lane { memarg, lane } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_store_vec_lane Twv_8 {memarg} {lane}")
        }
        Operator::V128Store16Lane { memarg, lane } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_store_vec_lane Twv_16 {memarg} {lane}")
        }
        Operator::V128Store32Lane { memarg, lane } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_store_vec_lane Twv_32 {memarg} {lane}")
        }
        Operator::V128Store64Lane { memarg, lane } => {
            let memarg = translate_memarg(&memarg)?;
            format!("BI_store_vec_lane Twv_64 {memarg} {lane}")
        }
        Operator::V128Const { value } => {
            let value = value.i128();
            format!("BI_const_vec {value}")
        }
        Operator::I8x16Shuffle { lanes } => todo!(),
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
        Operator::TryTable { try_table } => todo!(),
        Operator::Throw { tag_index } => todo!(),
        Operator::ThrowRef => todo!(),
        Operator::Try { blockty } => todo!(),
        Operator::Catch { tag_index } => todo!(),
        Operator::Rethrow { relative_depth } => todo!(),
        Operator::Delegate { relative_depth } => todo!(),
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
                "Atomic instruction {:?} are not supported",
                operator
            ))
        }
        Operator::RefI31Shared => todo!(),
        Operator::CallRef { type_index } => todo!(),
        Operator::ReturnCallRef { type_index } => todo!(),
        Operator::RefAsNonNull => todo!(),
        Operator::BrOnNull { relative_depth } => todo!(),
        Operator::BrOnNonNull { relative_depth } => todo!(),
        Operator::ContNew { cont_type_index } => todo!(),
        Operator::ContBind {
            argument_index,
            result_index,
        } => todo!(),
        Operator::Suspend { tag_index } => todo!(),
        Operator::Resume {
            cont_type_index,
            resume_table,
        } => todo!(),
        Operator::ResumeThrow {
            cont_type_index,
            tag_index,
            resume_table,
        } => todo!(),
        Operator::Switch {
            cont_type_index,
            tag_index,
        } => todo!(),
        Operator::I64Add128 => todo!(),
        Operator::I64Sub128 => todo!(),
        Operator::I64MulWideS => todo!(),
        Operator::I64MulWideU => todo!(),
        _ => return Err(anyhow::anyhow!("Operator {:?} not recognized", operator)),
    };
    Ok(operator.to_string())
}

//Record module_data
fn translate_data(data: &Data) -> anyhow::Result<String> {
    let mut res = String::new();
    let id = get_id();
    let moddata_mode = translate_module_datamode(data)?;
    let mut moddata_init = String::new();
    for byte in data.data {
        if *byte < 0x10 {
            moddata_init.push_str(format!("x0{byte:x}").as_str());
        } else {
            moddata_init.push_str(&format!("{byte:#2x?}")[1..]);
        }
        moddata_init.push_str(" :: ");
    }
    moddata_init.push_str("nil");
    res.push_str(format!("Definition moddata_{id} : module_data :=\n").as_str());
    res.push_str(LCB);
    res.push_str(format!("moddata_init := {moddata_init};\n").as_str());
    res.push_str(format!("moddata_mode := {moddata_mode};\n").as_str());
    res.push_str(RCB_DOT);
    res.push_str(".\n");
    Ok(res)
}

fn get_id() -> String {
    let uuid = Uuid::new_v4().to_string();
    let mut parts = uuid.split('-');
    parts.next().unwrap().to_string()
}
