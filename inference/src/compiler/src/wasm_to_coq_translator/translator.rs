use anyhow::Result;
use std::io::Read;
use uuid::Uuid;
use wasmparser::{
    Export, Import, MemoryType, Parser,
    Payload::{
        CodeSectionEntry, CodeSectionStart, ComponentAliasSection, ComponentCanonicalSection,
        ComponentExportSection, ComponentImportSection, ComponentInstanceSection, ComponentSection,
        ComponentStartSection, ComponentTypeSection, CoreTypeSection, CustomSection,
        DataCountSection, DataSection, ElementSection, End, ExportSection, FunctionSection,
        GlobalSection, ImportSection, InstanceSection, MemorySection, ModuleSection, StartSection,
        TableSection, TagSection, TypeSection, UnknownSection, Version,
    },
    RefType, Table, TypeRef,
};

struct WasmParseData<'a> {
    imports: Vec<Import<'a>>,
    exports: Vec<Export<'a>>,
    tables: Vec<Table<'a>>,
    memory_types: Vec<MemoryType>,
}

impl WasmParseData<'_> {
    fn translate(&self) -> String {
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
        coq.push('\n');
        coq
    }
}

pub fn translate_bytes(bytes: &[u8]) -> String {
    let mut data = Vec::new();
    let mut reader = std::io::Cursor::new(bytes);
    reader.read_to_end(&mut data).unwrap();
    let parse_data = parse(&data).unwrap();
    parse_data.translate()
}

fn parse(data: &[u8]) -> Result<WasmParseData> {
    let parser = Parser::new(0);
    let mut wasm_parse_data = WasmParseData {
        imports: Vec::new(),
        exports: Vec::new(),
        tables: Vec::new(),
        memory_types: Vec::new(),
    };

    for payload in parser.parse_all(&data) {
        match payload? {
            // Sections for WebAssembly modules
            Version { .. } => {}
            TypeSection(_) => {}
            ImportSection(imports_section) => {
                for import in imports_section {
                    wasm_parse_data.imports.push(import?);
                }
            }
            FunctionSection(functions) => {
                println!("Function Section:");
                for func in functions {
                    let f = func?;
                    println!(" - {f:?}");
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
            TagSection(tags) => {
                println!("Tag Section:");
                for tag in tags {
                    println!(" - {tag:?}");
                }
            }
            GlobalSection(globals) => {
                println!("Global Section:");
                for global in globals {
                    println!(" - {global:?}");
                }
            }
            ExportSection(export_sections) => {
                for export in export_sections {
                    wasm_parse_data.exports.push(export?);
                }
            }
            StartSection { .. } => { /* ... */ }
            ElementSection(elements) => {
                for element in elements {
                    if element.is_ok() {
                        let elem = element.unwrap();
                        let items = elem.items;
                        let kind = elem.kind;
                    }
                }
            }
            DataCountSection { count, .. } => {
                println!("Data Count Section: {}", count);
            }
            DataSection(data) => {
                println!("Data Section:");
                for datum in data {
                    println!(" - {:?}", datum?);
                }
            }

            // Here we know how many functions we'll be receiving as
            // `CodeSectionEntry`, so we can prepare for that, and
            // afterwards we can parse and handle each function
            // individually.
            CodeSectionStart { .. } => { /* ... */ }
            CodeSectionEntry(body) => {
                // here we can iterate over `body` to parse the function
                // and its locals
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

            CustomSection(_) => { /* ... */ }

            // most likely you'd return an error here
            UnknownSection { id, .. } => { /* ... */ }

            // Once we've reached the end of a parser we either resume
            // at the parent parser or the payload iterator is at its
            // end and we're done.
            End(_) => {}
        }
    }
    Ok(wasm_parse_data)
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
    let id = {
        let uuid = Uuid::new_v4().to_string();
        let mut parts = uuid.split('-');
        parts.next().unwrap().to_string()
    };

    let max = match memory_type.maximum {
        Some(max) => max.to_string(),
        None => "None".to_string(),
    };

    res.push_str(format!("Definition {id}_mem : WasmMemoryType :=\n").as_str());
    res.push_str("{|\n");
    res.push_str(format!("l_min := 4; l_max := {max}\n").as_str());
    res.push_str("|}.\n");
    res.push('\n');
    res
}
