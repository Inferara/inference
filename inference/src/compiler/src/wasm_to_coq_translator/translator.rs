use uuid::Uuid;
use wasmparser::{
    AbstractHeapType, CompositeInnerType, Data, DataKind, Element, ElementKind, Export,
    FunctionBody, Global, HeapType, Import, MemoryType, OperatorsReader, RecGroup, RefType, Table,
    TypeRef, ValType,
};

#[derive(Debug)]
pub(crate) enum WasmModuleParseError {
    UnsupportedOperation(String),
}

impl WasmModuleParseError {
    fn add_string_to_reported_error(
        info: &String,
        error: WasmModuleParseError,
    ) -> WasmModuleParseError {
        let WasmModuleParseError::UnsupportedOperation(error_message) = error;
        let ret_err = format!("{info}\n\t{error_message}").to_string();
        WasmModuleParseError::UnsupportedOperation(ret_err)
    }
}

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

    fn add_module_name_to_reported_error(
        &self,
        error: WasmModuleParseError,
    ) -> WasmModuleParseError {
        let module_name = &self.mod_name;
        WasmModuleParseError::add_string_to_reported_error(
            &format!("\tModule name: {module_name}"),
            error,
        )
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn translate(&self) -> Result<String, WasmModuleParseError> {
        let mut coq = String::new();
        coq.push_str("Require Import String List BinInt BinNat.\n");
        coq.push_str("From Exetasis Require Import WasmStructure.\n");
        coq.push_str("Require Import Coq.Init.Byte.\n");
        let mut created_imports = Vec::new();
        for import in &self.imports {
            let (definition_name, res) = translate_import(import);
            coq.push_str(res.as_str());
            created_imports.push(definition_name);
        }
        let mut created_exports = Vec::new();
        for export in &self.exports {
            let (name, res) = translate_export(export);
            coq.push_str(res.as_str());
            created_exports.push(name);
        }
        let mut created_tables = Vec::new();
        for table in &self.tables {
            let (name, res) = translate_table(table);
            coq.push_str(res.as_str());
            created_tables.push(name);
        }
        let mut created_memory_types = Vec::new();
        for memory_type in &self.memory_types {
            let (name, res) = translate_memory_type(memory_type);
            coq.push_str(res.as_str());
            created_memory_types.push(name);
        }
        let mut created_globals = Vec::new();
        for global in &self.globals {
            match translate_global(global) {
                Ok((name, res)) => {
                    coq.push_str(res.as_str());
                    created_globals.push(name);
                }
                Err(e) => {
                    return Err(self.add_module_name_to_reported_error(e));
                }
            }
        }
        let mut created_data_segments = Vec::new();
        for data in &self.data {
            match translate_data_segment(data) {
                Ok((name, res)) => {
                    coq.push_str(res.as_str());
                    created_data_segments.push(name);
                }
                Err(e) => {
                    return Err(self.add_module_name_to_reported_error(e));
                }
            }
        }
        let mut created_elements = Vec::new();
        for element in &self.elements {
            match translate_element(element) {
                Ok((name, res)) => {
                    coq.push_str(res.as_str());
                    created_elements.push(name);
                }
                Err(e) => {
                    return Err(self.add_module_name_to_reported_error(e));
                }
            }
        }

        let mut created_function_types = Vec::new();
        for rec_group in &self.function_types {
            let (name, res) = translate_rec_group(rec_group);
            coq.push_str(res.as_str());
            created_function_types.push(name);
        }

        let created_functions =
            match translate_functions(&self.function_type_indexes, &self.function_bodies) {
                Ok((names, res)) => {
                    coq.push_str(res.as_str());
                    names
                }
                Err(e) => {
                    return Err(self.add_module_name_to_reported_error(e));
                }
            };

        let module_name = &self.mod_name;
        coq.push_str(format!("Definition {module_name} : WasmModule :=\n").as_str());
        coq.push_str("{|\n");

        let mut types = String::new();
        for ty in created_function_types {
            types.push_str(format!("{ty} :: ").as_str());
        }
        types.push_str("nil;\n");
        coq.push_str(format!("m_types := {types}").as_str());

        let mut funcs = String::new();
        for func in created_functions {
            funcs.push_str(format!("{func} :: ").as_str());
        }
        funcs.push_str("nil;\n");
        coq.push_str(format!("m_funcs := {funcs}").as_str());

        let mut tables = String::new();
        for table in created_tables {
            tables.push_str(format!("{table} :: ").as_str());
        }
        tables.push_str("nil;\n");
        coq.push_str(format!("m_tables := {tables}").as_str());

        let mut mems = String::new();
        for mem in created_memory_types {
            mems.push_str(format!("{mem} :: ").as_str());
        }
        mems.push_str("nil;\n");
        coq.push_str(format!("m_mems := {mems}").as_str());

        let mut globals = String::new();
        for global in created_globals {
            globals.push_str(format!("{global} :: ").as_str());
        }
        globals.push_str("nil;\n");
        coq.push_str(format!("m_globals := {globals}").as_str());

        let mut elems = String::new();
        for elem in created_elements {
            elems.push_str(format!("{elem} :: ").as_str());
        }
        elems.push_str("nil;\n");
        coq.push_str(format!("m_elems := {elems}").as_str());

        let mut datas = String::new();
        for data in created_data_segments {
            datas.push_str(format!("{data} :: ").as_str());
        }

        datas.push_str("nil;\n");
        coq.push_str(format!("m_datas := {datas}").as_str());

        if let Some(start_function) = self.start_function {
            coq.push_str(format!("m_start := Some({start_function});\n").as_str());
        } else {
            coq.push_str("m_start := None;\n");
        }

        let mut imports = String::new();
        for import in created_imports {
            imports.push_str(format!("{import} :: ").as_str());
        }
        imports.push_str("nil;\n");
        coq.push_str(format!("m_imports := {imports}").as_str());

        let mut exports = String::new();
        for export in created_exports {
            exports.push_str(format!("{export} :: ").as_str());
        }
        exports.push_str("nil;\n");
        coq.push_str(format!("m_exports := {exports}").as_str());

        coq.push_str("|}.");
        Ok(coq)
    }
}

fn translate_import(import: &Import) -> (String, String) {
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
    (definition_name, res)
}

fn translate_export(export: &Export) -> (String, String) {
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
    (name.to_owned(), res)
}

fn translate_table(table: &Table) -> (String, String) {
    let mut res = String::new();
    let ty = table.ty;
    let mut name = String::new();
    if ty.element_type == RefType::FUNCREF {
        let id = get_id();
        name = format!("Table{id}");

        let max = match ty.maximum {
            Some(max) => max.to_string(),
            None => "None".to_string(),
        };

        res.push_str(format!("Definition {name} : WasmTableType :=\n").as_str());
        res.push_str("{|\n");
        res.push_str(
            format!("tt_limits := {{| l_min := 4; l_max := Some({max}%N) |}};\n").as_str(),
        );
        res.push_str("tt_reftype := rt_func\n");
        res.push_str("|}.\n");
    }
    res.push('\n');
    (name, res)
}

fn translate_memory_type(memory_type: &MemoryType) -> (String, String) {
    let mut res = String::new();
    let id = get_id();
    let name = format!("MemType{id}");

    let max = match memory_type.maximum {
        Some(max) => max.to_string(),
        None => "None".to_string(),
    };

    res.push_str(format!("Definition {name} : WasmMemoryType :=\n").as_str());
    res.push_str("{|\n");
    res.push_str(format!("l_min := 4; l_max := {max}\n").as_str());
    res.push_str("|}.\n");
    res.push('\n');
    (name, res)
}

fn translate_global(global: &Global) -> Result<(String, String), WasmModuleParseError> {
    let mut res = String::new();
    let id = get_id();
    let name = format!("Global{id}");

    let ty = global.ty;
    let mutability = ty.mutable;

    res.push_str(format!("Definition {name} : WasmGlobal :=\n").as_str());
    res.push_str("{|\n");
    res.push_str("g_type := {|\n");
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

    res.push_str(format!("gt_valtype := {val_type}\n").as_str());
    res.push_str("|};\n");

    match translate_operators_reader(global.init_expr.get_operators_reader()) {
        Ok(expression) => {
            res.push_str(format!("g_init := {expression}\n").as_str());
        }
        Err(e) => {
            return Err(WasmModuleParseError::add_string_to_reported_error(
                &String::from("Failed to translate global init expression"),
                e,
            ));
        }
    }

    res.push_str("|}.\n");
    res.push('\n');
    Ok((name, res))
}

fn translate_data_segment(data: &Data) -> Result<(String, String), WasmModuleParseError> {
    let mut res = String::new();
    let id = get_id();
    let name = format!("DataSegment{id}");

    res.push_str(format!("Definition {name} : WasmDataSegment :=\n").as_str());
    res.push_str("{|\n");

    let mut bytes_list = String::new();

    for byte in data.data {
        if *byte < 0x10 {
            bytes_list.push_str(format!("x0{byte:x}").as_str());
        } else {
            bytes_list.push_str(&format!("{byte:#2x?}")[1..]);
        }
        bytes_list.push_str(" :: ");
    }
    bytes_list.push_str("nil");

    res.push_str(format!("ds_init := {bytes_list};\n").as_str());

    let mode = match &data.kind {
        DataKind::Active {
            memory_index,
            offset_expr,
        } => match translate_operators_reader(offset_expr.get_operators_reader()) {
            Ok(expression) => {
                format!("dsm_active {memory_index} ({expression})")
            }
            Err(e) => {
                return Err(WasmModuleParseError::add_string_to_reported_error(
                    &String::from("Failed to translate data segment offset expression"),
                    e,
                ));
            }
        },
        DataKind::Passive => "dsm_passive".to_string(),
    };
    res.push_str(format!("ds_mode := {mode};\n").as_str());

    res.push_str("|}.\n");
    res.push('\n');
    Ok((name, res))
}

#[allow(clippy::too_many_lines)]
fn translate_operators_reader(
    operators_reader: OperatorsReader,
) -> Result<String, WasmModuleParseError> {
    let mut res = String::new();
    let mut blocks_stack: Vec<(bool, bool)> = Vec::new();
    let total_ops = operators_reader.clone().into_iter().count();
    let mut current_op = 0;

    for operator in operators_reader {
        current_op += 1;
        if let Ok(operator) = operator {
            let op = operator;
            match op {
                wasmparser::Operator::Nop => res.push_str("i_control ci_nop "),
                wasmparser::Operator::Unreachable => res.push_str("i_control ci_unreachable "),
                wasmparser::Operator::Block { blockty }
                | wasmparser::Operator::Loop { blockty }
                | wasmparser::Operator::If { blockty } => {
                    let instruction = match op {
                        wasmparser::Operator::Block { .. } => "i_control (ci_block",
                        wasmparser::Operator::Loop { .. } => "i_control (ci_loop",
                        wasmparser::Operator::If { .. } => "i_control (ci_if",
                        _ => "",
                    };
                    match blockty {
                        wasmparser::BlockType::Empty => {
                            res.push_str(format!("{instruction} (bt_val None) (").as_str());
                        }
                        wasmparser::BlockType::Type(valtype) => match valtype {
                            ValType::I32 => {
                                res.push_str(
                                    format!("{instruction} (bt_val (Some (vt_num nt_i32))) ( ")
                                        .as_str(),
                                );
                            }
                            ValType::I64 => {
                                res.push_str(
                                    format!("{instruction} (bt_val (Some (vt_num nt_i64))) ( ")
                                        .as_str(),
                                );
                            }
                            ValType::F32 => {
                                res.push_str(
                                    format!("{instruction} (bt_val (Some (vt_num nt_f32))) ( ")
                                        .as_str(),
                                );
                            }
                            ValType::F64 => {
                                res.push_str(
                                    format!("{instruction} (bt_val (Some (vt_num nt_f64))) ( ")
                                        .as_str(),
                                );
                            }
                            ValType::V128 => {
                                res.push_str(format!("{instruction} (vt_vec vt_v128 ").as_str());
                            }
                            ValType::Ref(ref_type) => match ref_type {
                                RefType::FUNCREF => {
                                    res.push_str(
                                        format!("i_reference {instruction} (vt_ref rt_func ")
                                            .as_str(),
                                    );
                                }
                                RefType::EXTERNREF => res.push_str(
                                    format!("i_reference {instruction} (vt_ref rt_extern ")
                                        .as_str(),
                                ),
                                _ => res.push_str(
                                    format!("i_reference {instruction} (vt_ref ").as_str(),
                                ),
                            },
                        },
                        wasmparser::BlockType::FuncType(index) => {
                            res.push_str(format!("{instruction} (bt_idx {index} ").as_str());
                        }
                    }

                    blocks_stack.push((instruction == "i_control (ci_if", false));
                    continue;
                }
                wasmparser::Operator::Else => {
                    let (is_if, is_else) = blocks_stack.pop().unwrap();
                    if is_if {
                        res.push_str("nil )( ");
                        blocks_stack.push((true, true));
                        continue;
                    }
                    blocks_stack.push((is_if, is_else));
                    continue;
                }
                wasmparser::Operator::End => {
                    if blocks_stack.is_empty() {
                        res.push_str("nil\n");
                        continue;
                    }

                    let (is_if, is_else) = blocks_stack.pop().unwrap();

                    if is_if {
                        if is_else {
                            res.push_str("nil))");
                        } else {
                            res.push_str("nil) nil)");
                        }
                    } else {
                        res.push_str("nil))\n");
                    }

                    if current_op < total_ops {
                        res.push_str(":: \n");
                    }
                    continue;
                }
                wasmparser::Operator::Br { relative_depth } => {
                    res.push_str(format!("i_control (ci_br {relative_depth})\n").as_str());
                }
                wasmparser::Operator::BrIf { relative_depth } => {
                    res.push_str(format!("i_control (ci_br_if {relative_depth})\n").as_str());
                }
                wasmparser::Operator::BrTable { targets } => {
                    res.push_str("i_control (ci_br_table");
                    if !targets.is_empty() {
                        res.push('(');
                        for target in targets.targets() {
                            let id = target.unwrap();
                            res.push_str(format!("{id}").as_str());
                            res.push_str(" :: ");
                        }
                        res.push_str("nil)");
                    }
                    let default = targets.default();
                    res.push_str(format!(" {default})\n").as_str());
                }
                wasmparser::Operator::Return => res.push_str("i_control ci_return\n"),
                wasmparser::Operator::Call { function_index } => {
                    res.push_str(format!("i_control (ci_call {function_index})\n").as_str());
                }
                wasmparser::Operator::CallIndirect {
                    type_index,
                    table_index,
                } => {
                    res.push_str(
                        format!("i_control (ci_call_indirect ({table_index} {type_index}))")
                            .as_str(),
                    );
                }
                wasmparser::Operator::I32Load { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i32_load {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I64Load { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i64_load {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I32Store { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i32_store {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I64Store { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i64_store {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I32Load8U { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i32_load8_u {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I64Load8U { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i64_load8_u {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I32Load8S { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i32_load8_s {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I64Load8S { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i64_load8_s {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I32Load16U { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(
                        format!("i_memory (mi_i32_load16_u {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str(),
                    );
                }
                wasmparser::Operator::I64Load16U { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(
                        format!("i_memory (mi_i64_load16_u {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str(),
                    );
                }
                wasmparser::Operator::I32Load16S { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(
                        format!("i_memory (mi_i32_load16_s {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str(),
                    );
                }
                wasmparser::Operator::I64Load16S { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(
                        format!("i_memory (mi_i64_load16_s {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str(),
                    );
                }
                wasmparser::Operator::I64Load32U { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(
                        format!("i_memory (mi_i64_load32_u {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str(),
                    );
                }
                wasmparser::Operator::I64Load32S { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(
                        format!("i_memory (mi_i64_load32_s {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str(),
                    );
                }
                wasmparser::Operator::I32Store8 { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i32_store8 {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I64Store8 { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i64_store8 {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I32Store16 { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i32_store16 {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::I64Store16 { memarg } => {
                    let offset = memarg.offset;
                    let align = memarg.align;
                    res.push_str(format!("i_memory (mi_i64_store16 {{| mi_offset := {offset}; mi_align := {align} |}})\n").as_str());
                }
                wasmparser::Operator::MemorySize { .. } => {
                    res.push_str("i_memory mi_memory_size\n");
                }
                wasmparser::Operator::MemoryGrow { .. } => {
                    res.push_str("i_memory mi_memory_grow\n");
                }
                wasmparser::Operator::MemoryFill { .. } => {
                    res.push_str("i_memory mi_memory_fill\n");
                }
                wasmparser::Operator::MemoryCopy { .. } => {
                    res.push_str("i_memory mi_memory_copy\n");
                }
                wasmparser::Operator::MemoryInit { data_index, .. } => {
                    res.push_str(format!("i_memory mi_memory_init ({data_index})\n").as_str());
                }
                wasmparser::Operator::DataDrop { data_index } => {
                    res.push_str(format!("i_memory mi_data_drop ({data_index})\n").as_str());
                }
                wasmparser::Operator::I32Const { value } => {
                    res.push_str(format!("i_numeric (ni_i32_const ({value}))\n").as_str());
                }
                wasmparser::Operator::I64Const { value } => {
                    res.push_str(format!("i_numeric (ni_i64_const ({value}))\n").as_str());
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
                wasmparser::Operator::LocalGet { local_index } => {
                    res.push_str(format!("i_variable (vi_local_get {local_index})\n").as_str());
                }
                wasmparser::Operator::LocalSet { local_index } => {
                    res.push_str(format!("i_variable (vi_local_set {local_index})\n").as_str());
                }
                wasmparser::Operator::LocalTee { local_index } => {
                    res.push_str(format!("i_variable (vi_local_tee {local_index})\n").as_str());
                }
                wasmparser::Operator::GlobalGet { global_index } => {
                    res.push_str(format!("i_variable (vi_global_get {global_index})\n").as_str());
                }
                wasmparser::Operator::GlobalSet { global_index } => {
                    res.push_str(format!("i_variable (vi_global_set {global_index})\n").as_str());
                }
                wasmparser::Operator::RefNull { hty } => {
                    match hty {
                        HeapType::Abstract { ty, .. } => match ty {
                            AbstractHeapType::Func => {
                                res.push_str("i_reference (ri_ref_null rt_func)\n");
                            }
                            AbstractHeapType::Extern => {
                                res.push_str("i_reference (ri__ref_null rt_extern)\n");
                            }
                            _ => {
                                return Err(WasmModuleParseError::UnsupportedOperation(
                                    format!("Failed to translate operator: {op:?}").to_string(),
                                ));
                            }
                        },
                        HeapType::Concrete(_) => {
                            return Err(WasmModuleParseError::UnsupportedOperation(
                                format!("Failed to translate operator: {op:?}").to_string(),
                            ));
                        }
                    }
                    res.push_str(format!("i_reference (ri_null {hty:?})\n").as_str());
                }
                wasmparser::Operator::RefIsNull => {
                    res.push_str("i_reference ri_ref_is_null\n");
                }
                wasmparser::Operator::RefFunc { function_index } => {
                    res.push_str(format!("i_reference (ri_ref_func {function_index})\n").as_str());
                }
                wasmparser::Operator::TableGet { table } => {
                    res.push_str(format!("i_table (ti_table_get {table})\n").as_str());
                }
                wasmparser::Operator::TableSet { table } => {
                    res.push_str(format!("i_table (ti_table_set {table})\n").as_str());
                }
                wasmparser::Operator::TableSize { table } => {
                    res.push_str(format!("i_table (ti_table_size {table})\n").as_str());
                }
                wasmparser::Operator::TableGrow { table } => {
                    res.push_str(format!("i_table (ti_table_grow {table})\n").as_str());
                }
                wasmparser::Operator::TableFill { table } => {
                    res.push_str(format!("i_table (ti_table_fill {table})\n").as_str());
                }
                wasmparser::Operator::TableCopy {
                    src_table,
                    dst_table,
                } => {
                    res.push_str(
                        format!("i_table (ti_table_copy {src_table} {dst_table})\n").as_str(),
                    );
                }
                wasmparser::Operator::TableInit { elem_index, table } => {
                    res.push_str(
                        format!("i_table (ti_table_init {elem_index} {table})\n").as_str(),
                    );
                }
                wasmparser::Operator::ElemDrop { elem_index } => {
                    res.push_str(format!("i_table (ti_elem_drop {elem_index})\n").as_str());
                }
                wasmparser::Operator::Drop => res.push_str("i_parametric (pi_drop)\n"),
                wasmparser::Operator::Select => res.push_str("i_parametric (pi_select)\n"),
                wasmparser::Operator::TypedSelect { ty } => match ty {
                    ValType::I32 => {
                        res.push_str("i_parametric (pi_select (vt_num nt_i32))\n");
                    }
                    ValType::I64 => {
                        res.push_str("i_parametric (pi_select (vt_num nt_i64))\n");
                    }
                    ValType::F32 => {
                        res.push_str("i_parametric (pi_select (vt_num nt_f32))\n");
                    }
                    ValType::F64 => {
                        res.push_str("i_parametric (pi_select (vt_num nt_f64))\n");
                    }
                    ValType::V128 => {
                        res.push_str("i_parametric (pi_select (vt_vec vt_v128))\n");
                    }
                    ValType::Ref(ref_type) => match ref_type {
                        RefType::FUNCREF => {
                            res.push_str("i_parametric (pi_select (vt_ref rt_func))\n");
                        }
                        RefType::EXTERNREF => {
                            res.push_str("i_parametric (pi_select (vt_ref rt_extern))\n");
                        }
                        _ => {
                            return Err(WasmModuleParseError::UnsupportedOperation(
                                format!("Failed to translate operator: {op:?}").to_string(),
                            ));
                        }
                    },
                },
                _ => {
                    return Err(WasmModuleParseError::UnsupportedOperation(
                        format!("Failed to translate operator: {op:?}").to_string(),
                    ));
                }
            }

            res.push_str(" :: \n");
        }
    }
    Ok(res)
}

fn translate_element(element: &Element) -> Result<(String, String), WasmModuleParseError> {
    let mut res = String::new();
    let id = get_id();
    let name = format!("ElementSegment{id}");

    res.push_str(format!("Definition {name} : WasmElementSegment :=\n").as_str());
    res.push_str("{|\n");

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
                match translate_operators_reader(e.unwrap().get_operators_reader()) {
                    Ok(expression) => {
                        expression_translated.push_str(expression.as_str());
                    }
                    Err(e) => {
                        return Err(WasmModuleParseError::add_string_to_reported_error(
                            &String::from("Failed to translate element segment expression"),
                            e,
                        ));
                    }
                }
            }

            res.push_str(format!("es_init := ({expression_translated});\n").as_str());
        }
        wasmparser::ElementItems::Functions(indexes) => {
            let mut index_val = String::new();
            for index in indexes.clone() {
                let index_unwrapped = index.unwrap();
                index_val.push_str(format!("{index_unwrapped}").as_str());
            }
            res.push_str("es_type := rt_func;\n");
            res.push_str(
                format!("es_init := (i_reference (ri_ref_func {index_val}) :: nil) :: nil;\n")
                    .as_str(),
            );
        }
    }

    match &element.kind {
        ElementKind::Active {
            table_index,
            offset_expr,
        } => match translate_operators_reader(offset_expr.get_operators_reader()) {
            Ok(expression) => {
                let index = table_index.unwrap_or(0);
                res.push_str(format!("es_mode := esm_active {index} ({expression})\n").as_str());
            }
            Err(e) => {
                return Err(WasmModuleParseError::add_string_to_reported_error(
                    &String::from("Failed to translate element segment offset expression"),
                    e,
                ));
            }
        },
        ElementKind::Passive => {
            res.push_str("es_mode := esm_passive\n");
        }
        ElementKind::Declared => {
            res.push_str("es_mode := esm_declarative\n");
        }
    }

    res.push_str("|}.\n\n");
    Ok((name, res))
}

fn translate_rec_group(rec_group: &RecGroup) -> (String, String) {
    let mut res = String::new();
    let id = get_id();
    let name = format!("FuncionType{id}");
    res.push_str(format!("Definition {name} : WasmFuncionType :=\n").as_str());
    res.push_str("{|\n");

    for ty in rec_group.types() {
        match &ty.composite_type.inner {
            CompositeInnerType::Func(ft) => {
                let mut params_str = String::new();
                for param in ft.params() {
                    let sp = stringify_val_type(*param);
                    params_str.push_str(format!("{sp} :: ").as_str());
                }
                params_str.push_str("nil;\n");
                res.push_str(format!("ft_params := {params_str}").as_str());

                let mut results_str = String::new();
                for result in ft.results() {
                    let sp = stringify_val_type(*result);
                    results_str.push_str(format!("{sp} :: ").as_str());
                }
                results_str.push_str("nil;\n");
                res.push_str(format!("ft_results := {results_str}").as_str());
            }
            CompositeInnerType::Array(_) | CompositeInnerType::Struct(_) => {
                //TODO
            }
        }
    }
    res.push_str("|}.\n\n");
    (name, res)
}

fn translate_functions(
    function_type_indexes: &[u32],
    function_bodies: &[FunctionBody],
) -> Result<(Vec<String>, String), WasmModuleParseError> {
    let mut res = String::new();
    let mut function_names = Vec::new();
    for (index, function_body) in function_bodies.iter().enumerate() {
        let id = get_id();
        let name = format!("Function{id}");
        let type_index = *function_type_indexes.get(index).unwrap_or(&0);

        res.push_str(format!("Definition {name} : WasmFunction :=\n").as_str());
        res.push_str("{|\n");
        res.push_str(format!("f_typeidx := {type_index};\n").as_str());
        let mut locals = String::new();
        if let Ok(locals_reader) = function_body.get_locals_reader() {
            for local in locals_reader {
                let (_, val_type) = local.unwrap();
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
                locals.push_str(format!("{val_type} :: ").as_str());
            }
        }
        locals.push_str("nil");
        res.push_str(format!("f_locals := {locals};\n").as_str());
        match translate_operators_reader(function_body.get_operators_reader().unwrap()) {
            Ok(expression) => {
                res.push_str(format!("f_body := {expression}").as_str());
            }
            Err(e) => {
                return Err(WasmModuleParseError::add_string_to_reported_error(
                    &String::from("Failed to translate function body"),
                    e,
                ));
            }
        }
        res.push_str("|}.\n");
        res.push('\n');
        function_names.push(name);
    }
    Ok((function_names, res))
}

fn get_id() -> String {
    let uuid = Uuid::new_v4().to_string();
    let mut parts = uuid.split('-');
    parts.next().unwrap().to_string()
}

fn stringify_val_type(val_type: ValType) -> String {
    match val_type {
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
    }
    .to_string()
}
