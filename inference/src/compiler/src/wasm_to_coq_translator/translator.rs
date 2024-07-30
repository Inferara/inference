use anyhow::Result;
use std::{fmt::format, io::Read};
use wasmparser::{
    Export, Import, Parser,
    Payload::{
        CodeSectionEntry, CodeSectionStart, ComponentAliasSection, ComponentCanonicalSection,
        ComponentExportSection, ComponentImportSection, ComponentInstanceSection, ComponentSection,
        ComponentStartSection, ComponentTypeSection, CoreTypeSection, CustomSection,
        DataCountSection, DataSection, ElementSection, End, ExportSection, FunctionSection,
        GlobalSection, ImportSection, InstanceSection, MemorySection, ModuleSection, StartSection,
        TableSection, TagSection, TypeSection, UnknownSection, Version,
    },
    TypeRef,
};

pub fn translate_bytes(bytes: &[u8]) -> String {
    let mut data = Vec::new();
    let mut reader = std::io::Cursor::new(bytes);
    reader.read_to_end(&mut data).unwrap();
    parse(data).unwrap()
}

fn parse(data: Vec<u8>) -> Result<String> {
    let parser = Parser::new(0);
    let mut imports: Vec<Import> = Vec::new();
    let mut exports: Vec<Export> = Vec::new();

    for payload in parser.parse_all(&data) {
        match payload? {
            // Sections for WebAssembly modules
            Version { .. } => {}
            TypeSection(_) => {}
            ImportSection(imports_section) => {
                for import in imports_section {
                    imports.push(import?);
                }
            }
            FunctionSection(functions) => {
                for func in functions {
                    println!(" - {func:?}");
                }
            }
            TableSection(tables) => {
                println!("Table Section:");
                for table in tables {
                    println!(" - {table:?}");
                }
            }
            MemorySection(memories) => {
                for memory in memories {
                    println!(" - {memory:?}");
                }
            }
            TagSection(tags) => {
                for tag in tags {
                    println!(" - {tag:?}");
                }
            }
            GlobalSection(globals) => {
                for global in globals {
                    println!(" - {global:?}");
                }
            }
            ExportSection(export_sections) => {
                for export in export_sections {
                    exports.push(export?);
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

    let mut coq = String::new();
    coq.push_str("Require Import String List BinInt BinNat.\n");
    coq.push_str("From Exetasis Require Import WasmStructure.\n");
    for import in imports {
        coq.push_str(translate_import_section(&import).as_str());
    }
    for export in exports {
        coq.push_str(translate_export_section(&export).as_str());
    }
    coq.push('\n');
    Ok(coq)
}

fn translate_import_section(import: &Import) -> String {
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
        TypeRef::Global(_) => String::from("id_global"),
        TypeRef::Memory(_) => String::from("id_mem"),
        TypeRef::Table(_) => String::from("id_table"),
        TypeRef::Tag(_) => String::from("id_tag"),
    };
    res.push_str(format!("i_desc := {kind} |").as_str());
    res.push_str("}.\n");
    res.push('\n');
    res
}

fn translate_export_section(export: &Export) -> String {
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
