use uuid::Uuid;
use wasmparser::{
    Data, DataKind, Element, ElementKind, Export, FunctionBody, Global, Import, MemoryType,
    OperatorsReader, RefType, Table, TypeRef, ValType,
};

pub(crate) struct WasmParseData<'a> {
    pub(crate) imports: Vec<Import<'a>>,
    pub(crate) exports: Vec<Export<'a>>,
    pub(crate) tables: Vec<Table<'a>>,
    pub(crate) memory_types: Vec<MemoryType>,
    pub(crate) globals: Vec<Global<'a>>,
    pub(crate) data: Vec<Data<'a>>,
    pub(crate) elements: Vec<Element<'a>>,
    pub(crate) function_type_indexes: Vec<u32>,
    pub(crate) function_bodies: Vec<FunctionBody<'a>>,
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
            elements: Vec::new(),
            function_type_indexes: Vec::new(),
            function_bodies: Vec::new(),
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
        for element in &self.elements {
            coq.push_str(translate_element(element).as_str());
        }
        coq.push_str(
            translate_functions(&self.function_type_indexes, &self.function_bodies).as_str(),
        );
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
    res.push_str(format!("e_desc := {kind} {index} |").as_str());
    res.push_str("}.\n");
    res.push('\n');
    res
}

fn translate_table(table: &Table) -> String {
    let mut res = String::new();
    let ty = table.ty;
    if ty.element_type == RefType::FUNCREF {
        let id = get_id();

        let max = match ty.maximum {
            Some(max) => max.to_string(),
            None => "None".to_string(),
        };

        res.push_str(format!("Definition Table{id} : WasmTableType :=\n").as_str());
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

    res.push_str(format!("Definition MemType{id} : WasmMemoryType :=\n").as_str());
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

    res.push_str(format!("Definition Global{id} : WasmGlobalType :=\n").as_str());
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

    res.push_str(format!("Definition DataSegment{id} : WasmDataSegment :=\n").as_str());
    res.push_str("{|\n");

    let mode = match &data.kind {
        DataKind::Active {
            memory_index,
            offset_expr,
        } => {
            let expression = translate_operators_reader(offset_expr.get_operators_reader());
            format!("dms_active {memory_index} ({expression})")
        }
        DataKind::Passive => "dsm_passive".to_string(),
    };
    res.push_str(format!("ds_mode := {mode};\n").as_str());

    res.push_str("|}.\n");
    res.push('\n');
    res
}

#[allow(clippy::too_many_lines)]
fn translate_operators_reader(operators_reader: OperatorsReader) -> String {
    let mut res = String::new();

    for operator in operators_reader {
        let mut skip_extend_list_operator = false;
        if operator.is_ok() {
            let op = operator.unwrap();
            match op {
                wasmparser::Operator::Nop => res.push_str("(ci_nop)"),
                wasmparser::Operator::Unreachable => res.push_str("(ci_unreachable)"),
                wasmparser::Operator::Block { blockty }
                | wasmparser::Operator::Loop { blockty }
                | wasmparser::Operator::If { blockty } => {
                    let instruction = match op {
                        wasmparser::Operator::Block { .. } => "ci_block",
                        wasmparser::Operator::Loop { .. } => "ci_loop",
                        wasmparser::Operator::If { .. } => "ci_if",
                        _ => "",
                    };
                    match blockty {
                        wasmparser::BlockType::Empty => {
                            res.push_str(format!("(({instruction} (").as_str());
                        }
                        wasmparser::BlockType::Type(valtype) => match valtype {
                            ValType::I32 => {
                                res.push_str(format!("(({instruction} bt_val nt_i32 (").as_str());
                            }
                            ValType::I64 => {
                                res.push_str(format!("(({instruction} bt_val nt_i64 (").as_str());
                            }
                            ValType::F32 => {
                                res.push_str(format!("(({instruction} bt_val nt_f32 (").as_str());
                            }
                            ValType::F64 => {
                                res.push_str(format!("(({instruction} bt_val nt_f64 (").as_str());
                            }
                            ValType::V128 => {
                                res.push_str(format!("(({instruction} vt_vec vt_v128 (").as_str());
                            }
                            ValType::Ref(ref_type) => match ref_type {
                                RefType::FUNCREF => res
                                    .push_str(format!("(({instruction} vt_ref rt_func (").as_str()),
                                RefType::EXTERNREF => res.push_str(
                                    format!("(({instruction} vt_ref rt_extern (").as_str(),
                                ),
                                _ => res.push_str(format!("(({instruction} vt_ref (").as_str()),
                            },
                        },
                        wasmparser::BlockType::FuncType(index) => {
                            res.push_str(format!("((ci_block bt_idx {index} (").as_str());
                        }
                    }
                }
                wasmparser::Operator::Else | wasmparser::Operator::End => {
                    res.push_str("nil\n");
                    continue;
                }
                wasmparser::Operator::Br { relative_depth } => {
                    res.push_str(format!("ci_br {relative_depth})\n").as_str());
                }
                wasmparser::Operator::BrIf { relative_depth } => {
                    res.push_str(format!("ci_br_if {relative_depth})\n").as_str());
                }
                wasmparser::Operator::BrTable { targets } => {
                    res.push_str("ci_br_table");
                    if !targets.is_empty() {
                        res.push('(');
                        for target in targets.targets() {
                            let id = target.unwrap();
                            res.push_str(format!("{id}").as_str());
                            res.push_str(" :: ");
                        }
                        res.pop();
                        res.push(')');
                    }
                    let default = targets.default();
                    res.push_str(format!("{default})\n").as_str());
                }
                wasmparser::Operator::Return => res.push_str("ci_return\n"),
                wasmparser::Operator::Call { function_index } => {
                    res.push_str(format!("ci_call {function_index})\n").as_str());
                }
                wasmparser::Operator::CallIndirect {
                    type_index,
                    table_index,
                } => {
                    res.push_str(format!("ci_call_indirect ({table_index} {type_index})").as_str());
                }
                wasmparser::Operator::I32Load { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i32_load ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I64Load { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i64_load ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I32Store { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i32_store ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I64Store { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i64_store ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I32Load8U { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i32_load8_u ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I64Load8U { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i64_load8_u ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I32Load8S { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i32_load8_s ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I64Load8S { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i64_load8_s ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I32Load16U { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i32_load16_u ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I64Load16U { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i64_load16_u ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I32Load16S { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i32_load16_s ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I64Load16S { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i64_load16_s ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I64Load32U { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i64_load32_u ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I64Load32S { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i64_load32_s ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I32Store8 { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i32_store8 ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I64Store8 { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i64_store8 ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I32Store16 { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i32_store16 ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::I64Store16 { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("mi_i64_store16 ({offset}, {align}))\n").as_str());
                }
                wasmparser::Operator::MemorySize { mem }
                | wasmparser::Operator::MemoryGrow { mem }
                | wasmparser::Operator::MemoryFill { mem } => {
                    res.push_str(format!("{mem}").as_str());
                }
                wasmparser::Operator::MemoryCopy { dst_mem, src_mem } => {
                    res.push_str(format!("{dst_mem} {src_mem}\n").as_str());
                }
                wasmparser::Operator::MemoryInit { data_index, .. } => {
                    res.push_str(format!("mi_memory_init ({data_index})\n").as_str());
                }
                wasmparser::Operator::DataDrop { data_index } => {
                    res.push_str(format!("mi_data_drop ({data_index})\n").as_str());
                }
                wasmparser::Operator::I32Const { value } => {
                    res.push_str(format!("i_numeric (ni_i32_const {value})\n").as_str());
                }
                wasmparser::Operator::I64Const { value } => {
                    res.push_str(format!("i_numeric (ni_i64_const {value})\n").as_str());
                }
                wasmparser::Operator::I32Clz => res.push_str("i_numeric ni_i32_clz\n"),
                wasmparser::Operator::I32Ctz => res.push_str("i_numeric ni_i32_ctz\n"),
                wasmparser::Operator::I32Popcnt => res.push_str("i_numeric ni_i32_popcnt\n"),
                wasmparser::Operator::I32Add => res.push_str("i_numeric ni_i32_add\n"),
                wasmparser::Operator::I32Sub => res.push_str("i_numeric ni_i32_sub\n"),
                wasmparser::Operator::I32Mul => res.push_str("i_numeric ni_i32_mul\n"),
                wasmparser::Operator::I32DivS => res.push_str("i_numeric ni_i32_div_s\n"),
                wasmparser::Operator::I32DivU => res.push_str("i_numeric ni_i32_div_u\n"),
                wasmparser::Operator::I32RemS => res.push_str("i_numeric ni_i32_rem_s\n"),
                wasmparser::Operator::I32RemU => res.push_str("i_numeric ni_i32_rem_u\n"),
                wasmparser::Operator::I32And => res.push_str("i_numeric ni_i32_and\n"),
                wasmparser::Operator::I32Or => res.push_str("i_numeric ni_i32_or\n"),
                wasmparser::Operator::I32Xor => res.push_str("i_numeric ni_i32_xor\n"),
                wasmparser::Operator::I32Shl => res.push_str("i_numeric ni_i32_shl\n"),
                wasmparser::Operator::I32ShrS => res.push_str("i_numeric ni_i32_shr_s\n"),
                wasmparser::Operator::I32ShrU => res.push_str("i_numeric ni_i32_shr_u\n"),
                wasmparser::Operator::I32Rotl => res.push_str("i_numeric ni_i32_rotl\n"),
                wasmparser::Operator::I32Rotr => res.push_str("i_numeric ni_i32_rotr\n"),
                wasmparser::Operator::I32Eqz => res.push_str("i_numeric ni_i32_eqz\n"),
                wasmparser::Operator::I32Eq => res.push_str("i_numeric ni_i32_eq\n"),
                wasmparser::Operator::I32Ne => res.push_str("i_numeric ni_i32_ne\n"),
                wasmparser::Operator::I32LtS => res.push_str("i_numeric ni_i32_lt_s\n"),
                wasmparser::Operator::I32LtU => res.push_str("i_numeric ni_i32_lt_u\n"),
                wasmparser::Operator::I32LeS => res.push_str("i_numeric ni_i32_le_s\n"),
                wasmparser::Operator::I32LeU => res.push_str("i_numeric ni_i32_le_u\n"),
                wasmparser::Operator::I32GtS => res.push_str("i_numeric ni_i32_gt_s\n"),
                wasmparser::Operator::I32GtU => res.push_str("i_numeric ni_i32_gt_u\n"),
                wasmparser::Operator::I32GeS => res.push_str("i_numeric ni_i32_ge_s\n"),
                wasmparser::Operator::I32GeU => res.push_str("i_numeric ni_i32_ge_u\n"),
                wasmparser::Operator::I32Extend8S => res.push_str("i_numeric ni_i32_extend8_s\n"),
                wasmparser::Operator::I32Extend16S => res.push_str("i_numeric ni_i32_extend16_s\n"),
                wasmparser::Operator::I32WrapI64 => res.push_str("i_numeric ni_i32_wrap_i64\n"),
                wasmparser::Operator::I64Clz => res.push_str("i_numeric ni_i64_clz\n"),
                wasmparser::Operator::I64Ctz => res.push_str("i_numeric ni_i64_ctz\n"),
                wasmparser::Operator::I64Popcnt => res.push_str("i_numeric ni_i64_popcnt\n"),
                wasmparser::Operator::I64Add => res.push_str("i_numeric ni_i64_add\n"),
                wasmparser::Operator::I64Sub => res.push_str("i_numeric ni_i64_sub\n"),
                wasmparser::Operator::I64Mul => res.push_str("i_numeric ni_i64_mul\n"),
                wasmparser::Operator::I64DivS => res.push_str("i_numeric ni_i64_div_s\n"),
                wasmparser::Operator::I64DivU => res.push_str("i_numeric ni_i64_div_u\n"),
                wasmparser::Operator::I64RemS => res.push_str("i_numeric ni_i64_rem_s\n"),
                wasmparser::Operator::I64RemU => res.push_str("i_numeric ni_i64_rem_u\n"),
                wasmparser::Operator::I64And => res.push_str("i_numeric ni_i64_and\n"),
                wasmparser::Operator::I64Or => res.push_str("i_numeric ni_i64_or\n"),
                wasmparser::Operator::I64Xor => res.push_str("i_numeric ni_i64_xor\n"),
                wasmparser::Operator::I64Shl => res.push_str("i_numeric ni_i64_shl\n"),
                wasmparser::Operator::I64ShrS => res.push_str("i_numeric ni_i64_shr_s\n"),
                wasmparser::Operator::I64ShrU => res.push_str("i_numeric ni_i64_shr_u\n"),
                wasmparser::Operator::I64Rotl => res.push_str("i_numeric ni_i64_rotl\n"),
                wasmparser::Operator::I64Rotr => res.push_str("i_numeric ni_i64_rotr\n"),
                wasmparser::Operator::I64Eqz => res.push_str("i_numeric ni_i64_eqz\n"),
                wasmparser::Operator::I64Eq => res.push_str("i_numeric ni_i64_eq\n"),
                wasmparser::Operator::I64Ne => res.push_str("i_numeric ni_i64_ne\n"),
                wasmparser::Operator::I64LtS => res.push_str("i_numeric ni_i64_lt_s\n"),
                wasmparser::Operator::I64LtU => res.push_str("i_numeric ni_i64_lt_u\n"),
                wasmparser::Operator::I64LeS => res.push_str("i_numeric ni_i64_le_s\n"),
                wasmparser::Operator::I64LeU => res.push_str("i_numeric ni_i64_le_u\n"),
                wasmparser::Operator::I64GtS => res.push_str("i_numeric ni_i64_gt_s\n"),
                wasmparser::Operator::I64GtU => res.push_str("i_numeric ni_i64_gt_u\n"),
                wasmparser::Operator::I64GeS => res.push_str("i_numeric ni_i64_ge_s\n"),
                wasmparser::Operator::I64GeU => res.push_str("i_numeric ni_i64_ge_u\n"),
                wasmparser::Operator::I64Extend8S => res.push_str("i_numeric ni_i64_extend8_s\n"),
                wasmparser::Operator::I64Extend16S => res.push_str("i_numeric ni_i64_extend16_s\n"),
                wasmparser::Operator::I64Extend32S => res.push_str("i_numeric ni_i64_extend32_s\n"),
                wasmparser::Operator::I64ExtendI32S => {
                    res.push_str("i_numeric ni_i64_extend_i32_s\n");
                }
                wasmparser::Operator::I64ExtendI32U => {
                    res.push_str("i_numeric ni_i64_extend_i32_u\n");
                }
                _ => {
                    skip_extend_list_operator = true;
                }
            }

            if skip_extend_list_operator {
                continue;
            }
            res.push_str(":: \n");
        }
    }
    res
}

fn translate_element(element: &Element) -> String {
    let mut res = String::new();
    let id = get_id();

    res.push_str(format!("Definition ElementSegment{id} : WasmElementSegment :=\n").as_str());
    res.push_str("{|\n");
    match &element.kind {
        ElementKind::Active {
            table_index,
            offset_expr,
        } => {
            let expression = translate_operators_reader(offset_expr.get_operators_reader());
            let index = table_index.unwrap_or(0);
            res.push_str(format!("es_mode := esm_active ({index} {expression};\n").as_str());
        }
        ElementKind::Passive => {
            res.push_str("es_mode := esm_passive;\n");
        }
        ElementKind::Declared => {
            res.push_str("es_mode := esm_declarative;\n");
        }
    }

    match &element.items {
        wasmparser::ElementItems::Expressions(ref_type, expr) => {
            match *ref_type {
                RefType::FUNCREF => {
                    res.push_str("es_type := rt_func;\n");
                }
                RefType::EXTERNREF => {
                    res.push_str("es_type := rt_extern;\n");
                }
                _ => {}
            }
            let mut expression_translated = String::new();
            for e in expr.clone() {
                let expression = translate_operators_reader(e.unwrap().get_operators_reader());
                expression_translated.push_str(expression.as_str());
            }

            res.push_str(format!("es_init := ({expression_translated});\n").as_str());
        }
        wasmparser::ElementItems::Functions(_) => {
            res.push_str("es_type := rt_func;\n");
        }
    }

    res
}

fn translate_functions(function_type_indexes: &[u32], function_bodies: &[FunctionBody]) -> String {
    let mut res = String::new();
    for (index, function_body) in function_bodies.iter().enumerate() {
        let id = get_id();
        let type_index = function_type_indexes[index];

        let body = translate_operators_reader(function_body.get_operators_reader().unwrap());

        res.push_str(format!("Definition Function{id} : WasmFunction :=\n").as_str());
        res.push_str("{|\n");
        res.push_str(format!("f_typeidx := {type_index};\n").as_str());
        let mut locals = String::new();
        if let Ok(locals_reader) = function_body.get_locals_reader() {
            for local in locals_reader {
                let (count, val_type) = local.unwrap();
                let val_type = match val_type {
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
                locals.push_str(format!("({count}, {val_type}) :: ").as_str());
            }
        }
        res.push_str(format!("f_locals := {type_index};\n").as_str());
        res.push_str(format!("f_body := ({body});\n").as_str());
        res.push_str("|}.\n");
        res.push('\n');
    }
    res
}

fn get_id() -> String {
    let uuid = Uuid::new_v4().to_string();
    let mut parts = uuid.split('-');
    parts.next().unwrap().to_string()
}
