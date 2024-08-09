use anyhow::Result;
use std::io::Read;
use wasmparser::{
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

use crate::wasm_to_coq_translator::translator::WasmParseData;

pub fn translate_bytes(mod_name: String, bytes: &[u8]) -> String {
    let mut data = Vec::new();
    let mut reader = std::io::Cursor::new(bytes);
    reader.read_to_end(&mut data).unwrap();
    let parse_data = parse(mod_name, &data).unwrap();
    parse_data.translate()
}

#[allow(clippy::match_same_arms)]
fn parse(mod_name: String, data: &[u8]) -> Result<WasmParseData> {
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
            TypeSection(_) => {
                /*
                    this section is missed because of its relation to the
                    gc functionality, proposed [here](https://github.com/torch2424/wasm-by-example/blob/master/examples/reading-and-writing-audio/demo/assemblyscript/README.md)
                */
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
            TagSection(_) => { /* ignore, see the TypeSection arm */ }
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
            StartSection { .. } => {
                //TODO handle it properly
                println!("Start section");
            }
            ElementSection(elements) => {
                for element in elements {
                    wasm_parse_data.elements.push(element?);
                }
            }
            DataCountSection { count, .. } => {
                println!("Data Count Section: {}", count);
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
            CodeSectionStart { .. } => {
                println!("Code section starts");
            }
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

            CustomSection(_) => { /* ... */ }

            // most likely you'd return an error here
            UnknownSection { .. } => { /* ... */ }

            // Once we've reached the end of a parser we either resume
            // at the parent parser or the payload iterator is at its
            // end and we're done.
            End(_) => {}
        }
    }
    Ok(wasm_parse_data)
}
