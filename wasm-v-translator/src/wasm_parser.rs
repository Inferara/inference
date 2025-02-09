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
use std::io::Read;

use crate::translator::WasmParseData;

pub fn translate_bytes(mod_name: &str, bytes: &[u8]) -> anyhow::Result<String> {
    let mut data = Vec::new();
    let mut reader = std::io::Cursor::new(bytes);
    reader.read_to_end(&mut data).unwrap();
    match parse(mod_name.to_string(), &data) {
        Ok(parse_data) => parse_data.translate(),
        Err(e) => Err(anyhow::anyhow!(e.to_string())),
    }
}

#[allow(clippy::match_same_arms)]
fn parse(mod_name: String, data: &[u8]) -> anyhow::Result<WasmParseData> {
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
                println!("Custom section name: {}", custom_section.name());
                match custom_section.as_known() {
                    inf_wasmparser::KnownCustom::Name(name_section) => {
                        for name in name_section {
                            let name = name?;
                            match name {
                                inf_wasmparser::Name::Module { name, .. } => {
                                    println!("Module name: {}", name);
                                }
                                inf_wasmparser::Name::Function(func_names) => {
                                    for func_name in func_names {
                                        let func_name = func_name?;
                                        println!(
                                            "Function name: {} at index {}",
                                            func_name.name, func_name.index
                                        );
                                    }
                                }
                                inf_wasmparser::Name::Local(locals) => {
                                    for local in locals {
                                        let local = local?;
                                        let index = local.index;
                                        for local_names in local.names {
                                            let local_names = local_names?;
                                            println!(
                                                "Local name: {} at index {} in function {}",
                                                local_names.name, local_names.index, index
                                            );
                                        }
                                    }
                                }
                                // inf_wasmparser::Name::Label(labels) => {
                                //     for (index, label_names) in labels {
                                //         for (label_index, name) in label_names {
                                //             println!(
                                //                 "Label name: {} at index {} in function {}",
                                //                 name, label_index, index
                                //             );
                                //         }
                                //     }
                                // }
                                // inf_wasmparser::Name::Type(types) => {
                                //     for (index, name) in types {
                                //         println!("Type name: {} at index {}", name, index);
                                //     }
                                // }
                                // inf_wasmparser::Name::Table(tables) => {
                                //     for (index, name) in tables {
                                //         println!("Table name: {} at index {}", name, index);
                                //     }
                                // }
                                // inf_wasmparser::Name::Memory(memories) => {
                                //     for (index, name) in memories {
                                //         println!("Memory name: {} at index {}", name, index);
                                //     }
                                // }
                                // inf_wasmparser::Name::Global(globals) => {
                                //     for (index, name) in globals {
                                //         println!("Global name: {} at index {}", name, index);
                                //     }
                                // }
                                // inf_wasmparser::Name::Element(elements) => {
                                //     for (index, name) in elements {
                                //         println!("Element name: {} at index {}", name, index);
                                //     }
                                // }
                                // inf_wasmparser::Name::Data(data) => {
                                //     for (index, name) in data {
                                //         println!("Data name: {} at index {}", name, index);
                                //     }
                                // }
                                // inf_wasmparser::Name::Field(fields) => {
                                //     for (index, field_names) in fields {
                                //         for (field_index, name) in field_names {
                                //             println!(
                                //                 "Field name {} at index {} in function {}",
                                //                 name, field_index, index
                                //             );
                                //         }
                                //     }
                                // }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
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
