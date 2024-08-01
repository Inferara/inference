use uuid::Uuid;
use wasmparser::{
    ConstExpr, Data, DataKind, Export, Global, Import, MemoryType, RefType, Table, TypeRef, ValType,
};

pub(crate) struct WasmParseData<'a> {
    pub(crate) imports: Vec<Import<'a>>,
    pub(crate) exports: Vec<Export<'a>>,
    pub(crate) tables: Vec<Table<'a>>,
    pub(crate) memory_types: Vec<MemoryType>,
    pub(crate) globals: Vec<Global<'a>>,
    pub(crate) data: Vec<Data<'a>>,
}

impl WasmParseData<'_> {
    pub(crate) fn new<'a>() -> WasmParseData<'a> {
        WasmParseData {
            imports: Vec::new(),
            exports: Vec::new(),
            tables: Vec::new(),
            memory_types: Vec::new(),
            globals: Vec::new(),
            data: Vec::new(),
        }
    }

    pub(crate) fn translate(&self) -> String {
        let mut coq = String::new();
        coq.push_str("Require Import String List BinInt BinNat.\n");
        coq.push_str("From Exetasis Require Import WasmStructure.\n");
        for import in &self.imports {
            coq.push_str(translate_import(import).as_str());
        }
        for export in &self.exports {
            coq.push_str(translate_export(export).as_str());
        }
        for table in &self.tables {
            coq.push_str(translate_table(table).as_str());
        }
        for memory_type in &self.memory_types {
            coq.push_str(translate_memory_type(memory_type).as_str());
        }
        for global in &self.globals {
            coq.push_str(translate_global(global).as_str());
        }
        for data in &self.data {
            coq.push_str(translate_data(data).as_str());
        }
        coq.push('\n');
        coq
    }
}

fn translate_import(import: &Import) -> String {
    let mut res = String::new();
    let name = String::from(import.name);
    let module = String::from(import.module);
    let definition_name = module.clone() + &name.clone().remove(0).to_uppercase().to_string();
    res.push_str(format!("Definition {definition_name} : WasmImport :=\n").as_str());
    res.push_str("{|\n");
    res.push_str(format!("i_module := \"{name}\";\n").as_str());
    res.push_str(format!("i_name := \"{module}\";\n").as_str());
    let kind = match import.ty {
        TypeRef::Func(index) => format!("id_func {index}"),
        TypeRef::Global(_) => String::from("id_global"), //TODO
        TypeRef::Memory(_) => String::from("id_mem"),    //TODO
        TypeRef::Table(_) => String::from("id_table"),   //TODO
        TypeRef::Tag(_) => String::from("id_tag"),
    };
    res.push_str(format!("i_desc := {kind} |").as_str());
    res.push_str("}.\n");
    res.push('\n');
    res
}

fn translate_export(export: &Export) -> String {
    let mut res = String::new();
    let name = export.name;
    res.push_str(format!("Definition {name} : WasmExport :=\n").as_str());
    res.push_str("{|\n");
    res.push_str(format!("e_name := \"{name}\";\n").as_str());
    let kind = match export.kind {
        wasmparser::ExternalKind::Func => "ed_func",
        wasmparser::ExternalKind::Table => "ed_table",
        wasmparser::ExternalKind::Memory => "ed_mem",
        wasmparser::ExternalKind::Global => "ed_global",
        wasmparser::ExternalKind::Tag => "ed_tag",
    };
    let index = export.index;
    res.push_str(format!("e_kind := {kind} {index} |").as_str());
    res.push_str("}.\n");
    res.push('\n');
    res
}

fn translate_table(table: &Table) -> String {
    let mut res = String::new();
    let ty = table.ty;
    if ty.element_type == RefType::FUNCREF {
        let id = {
            let uuid = Uuid::new_v4().to_string();
            let mut parts = uuid.split('-');
            parts.next().unwrap().to_string()
        };

        let max = match ty.maximum {
            Some(max) => max.to_string(),
            None => "None".to_string(),
        };

        res.push_str(format!("Definition {id}_table : WasmTableType :=\n").as_str());
        res.push_str("{|\n");
        res.push_str(format!("tt_limits := {{| l_min := 4; l_max := {max} |}};\n").as_str());
        res.push_str("tt_reftype := rt_func\n");
        res.push_str("|}.\n");
    }
    res.push('\n');
    res
}

fn translate_memory_type(memory_type: &MemoryType) -> String {
    let mut res = String::new();
    let id = get_id();

    let max = match memory_type.maximum {
        Some(max) => max.to_string(),
        None => "None".to_string(),
    };

    res.push_str(format!("Definition {id}MemType : WasmMemoryType :=\n").as_str());
    res.push_str("{|\n");
    res.push_str(format!("l_min := 4; l_max := {max}\n").as_str());
    res.push_str("|}.\n");
    res.push('\n');
    res
}

fn translate_global(global: &Global) -> String {
    let mut res = String::new();
    let id = get_id();

    let ty = global.ty;
    let mutability = ty.mutable;

    res.push_str(format!("Definition {id}Global : WasmGlobalType :=\n").as_str());
    res.push_str("{|\n");
    res.push_str(format!("gt_mut := {mutability};\n").as_str());

    let val_type = match ty.content_type {
        ValType::I32 => "vt_num nt_i32",
        ValType::I64 => "vt_num nt_i64",
        ValType::F32 => "vt_num nt_f32",
        ValType::F64 => "vt_num nt_f64",
        ValType::V128 => "vt_vec vt_v128",
        ValType::Ref(ref_type) => match ref_type {
            RefType::FUNCREF => "vt_ref rt_func",
            RefType::EXTERNREF => "vt_ref rt_extern",
            _ => "vt_ref _",
        },
    };

    res.push_str(format!("gt_valtype := {val_type};\n").as_str());
    res.push_str("|}.\n");
    res.push('\n');
    res
}

fn translate_data(data: &Data) -> String {
    let mut res = String::new();
    let id = get_id();

    res.push_str(format!("Definition {id}DataSegment : WasmDataSegment :=\n").as_str());
    res.push_str("{|\n");

    let mode = match &data.kind {
        DataKind::Active {
            memory_index,
            offset_expr,
        } => {
            let expression = translate_wasm_expression(offset_expr);
            format!("dms_active {memory_index} {expression}")
        }
        DataKind::Passive => "dsm_passive".to_string(),
    };
    res.push_str(format!("ds_mode: {mode};\n").as_str());

    res.push_str("|}.\n");
    res.push('\n');
    res
}

fn translate_wasm_expression(expression: &ConstExpr) -> String {
    let mut res = String::new();
    let mut is_in_block = 0;

    for operator in expression.get_operators_reader() {
        if operator.is_ok() {
            is_in_block = match operator.clone().unwrap() {
                wasmparser::Operator::Block { .. } => is_in_block + 1,
                wasmparser::Operator::End => is_in_block - 1,
                _ => is_in_block,
            };

            if is_in_block > 0 {}

            match operator.unwrap() {
                wasmparser::Operator::Nop => res.push_str("(ci_nop)"),
                wasmparser::Operator::Unreachable => res.push_str("(ci_unreachable)"),
                wasmparser::Operator::Block { blockty } => match blockty {
                    wasmparser::BlockType::Empty => res.push_str("((ci_block "),
                    wasmparser::BlockType::Type(valtype) => match valtype {
                        ValType::I32 => res.push_str("((ci_block bt_val nt_i32 "),
                        ValType::I64 => res.push_str("((ci_block bt_val nt_i64 "),
                        ValType::F32 => res.push_str("((ci_block bt_val nt_f32 "),
                        ValType::F64 => res.push_str("((ci_block bt_val nt_f64 "),
                        ValType::V128 => res.push_str("((ci_block vt_vec vt_v128 "),
                        ValType::Ref(ref_type) => match ref_type {
                            RefType::FUNCREF => res.push_str("((ci_block vt_ref rt_func "),
                            RefType::EXTERNREF => res.push_str("((ci_block vt_ref rt_extern "),
                            _ => res.push_str("((ci_block vt_ref "),
                        },
                    },
                    wasmparser::BlockType::FuncType(index) => {
                        res.push_str(format!("((ci_block bt_idx {index} ").as_str());
                    }
                },
                _ => {}
            }

            if is_in_block == 0 {
                res.push_str(" :: \n");
            }
        }
    }
    res.pop();
    res
}

fn get_id() -> String {
    let uuid = Uuid::new_v4().to_string();
    let mut parts = uuid.split('-');
    parts.next().unwrap().to_string()
}
