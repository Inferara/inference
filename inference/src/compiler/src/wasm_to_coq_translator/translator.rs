use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};

#[derive(Debug)]
struct WasmParser {
    opcodes: HashMap<u8, &'static str>,
}

#[derive(Debug)]
struct FunctionTable {
    functions: Vec<u32>,
}

#[derive(Debug)]
struct CodeSection {
    codes: Vec<Code>,
}

#[derive(Debug)]
struct Code {
    locals: Vec<u8>,
    body: Vec<String>,
}

#[derive(Debug)]
struct Module {
    types: Vec<Type>,
    imports: Vec<Import>,
    function_table: Option<FunctionTable>,
    code_section: Option<CodeSection>,
}

#[derive(Debug)]
struct Type {
    params: Vec<u8>,
    results: Vec<u8>,
}

#[derive(Debug)]
struct Import {
    module: String,
    name: String,
    kind: u8,
}

impl WasmParser {
    fn new() -> Self {
        let mut opcodes = HashMap::new();
        // Add more opcodes as needed
        opcodes.insert(0x00, "unreachable");
        opcodes.insert(0x01, "nop");
        opcodes.insert(0x02, "block");
        opcodes.insert(0x03, "loop");
        opcodes.insert(0x04, "if");
        opcodes.insert(0x05, "else");
        opcodes.insert(0x0b, "end");
        opcodes.insert(0x0c, "br");
        opcodes.insert(0x0d, "br_if");
        opcodes.insert(0x0e, "br_table");
        opcodes.insert(0x0f, "return");
        opcodes.insert(0x10, "call");
        opcodes.insert(0x11, "call_indirect");
        opcodes.insert(0x1a, "drop");
        opcodes.insert(0x1b, "select");
        // Add more opcodes as necessary

        WasmParser { opcodes }
    }

    fn parse(&self, bytes: &[u8]) -> Vec<String> {
        let mut result = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if let Some(opcode) = self.opcodes.get(&bytes[i]) {
                result.push(opcode.to_string());
                i += 1;
            } else {
                result.push(format!("unknown({:#x})", bytes[i]));
                i += 1;
            }
        }
        result
    }

    fn parse_function_table(&self, bytes: &[u8], offset: &mut usize) -> FunctionTable {
        let mut functions = Vec::new();
        let count = self.parse_leb128(bytes, offset);
        for _ in 0..count {
            let func_index = self.parse_leb128(bytes, offset);
            functions.push(func_index);
        }
        FunctionTable { functions }
    }

    fn parse_code_section(&self, bytes: &[u8], offset: &mut usize) -> CodeSection {
        let mut codes = Vec::new();
        let count = self.parse_leb128(bytes, offset);
        for _ in 0..count {
            let code_size = self.parse_leb128(bytes, offset);
            let code_end = *offset + code_size as usize;
            let locals_count = self.parse_leb128(bytes, offset);
            let mut locals = Vec::new();
            for _ in 0..locals_count {
                let local_count = self.parse_leb128(bytes, offset);
                let local_type = bytes[*offset];
                *offset += 1;
                for _ in 0..local_count {
                    locals.push(local_type);
                }
            }
            let mut body = Vec::new();
            while *offset < code_end {
                if let Some(opcode) = self.opcodes.get(&bytes[*offset]) {
                    body.push(opcode.to_string());
                    *offset += 1;
                } else {
                    body.push(format!("unknown({:#x})", bytes[*offset]));
                    *offset += 1;
                }
            }
            codes.push(Code { locals, body });
        }
        CodeSection { codes }
    }

    fn parse_type_section(&self, bytes: &[u8], offset: &mut usize) -> Vec<Type> {
        let mut types = Vec::new();
        let count = self.parse_leb128(bytes, offset);
        for _ in 0..count {
            let form = bytes[*offset];
            *offset += 1;
            let param_count = self.parse_leb128(bytes, offset);
            let mut params = Vec::new();
            for _ in 0..param_count {
                params.push(bytes[*offset]);
                *offset += 1;
            }
            let result_count = self.parse_leb128(bytes, offset);
            let mut results = Vec::new();
            for _ in 0..result_count {
                results.push(bytes[*offset]);
                *offset += 1;
            }
            types.push(Type { params, results });
        }
        types
    }

    fn parse_import_section(&self, bytes: &[u8], offset: &mut usize) -> Vec<Import> {
        let mut imports = Vec::new();
        let count = self.parse_leb128(bytes, offset);
        for _ in 0..count {
            let module_len = self.parse_leb128(bytes, offset) as usize;
            let module = String::from_utf8(bytes[*offset..*offset + module_len].to_vec()).unwrap();
            *offset += module_len;
            let name_len = self.parse_leb128(bytes, offset) as usize;
            let name = String::from_utf8(bytes[*offset..*offset + name_len].to_vec()).unwrap();
            *offset += name_len;
            let kind = bytes[*offset];
            *offset += 1;
            imports.push(Import { module, name, kind });
        }
        imports
    }

    fn parse_leb128(&self, bytes: &[u8], offset: &mut usize) -> u32 {
        let mut result = 0;
        let mut shift = 0;
        loop {
            let byte = bytes[*offset];
            *offset += 1;
            result |= ((byte & 0x7F) as u32) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        result
    }
}

fn main() -> io::Result<()> {
    let mut file = File::open("example.wasm")?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let parser = WasmParser::new();
    let mut offset = 0;

    // Read the WASM header (magic number and version)
    let magic_number = &buffer[offset..offset + 4];
    offset += 4;
    let version = &buffer[offset..offset + 4];
    offset += 4;

    let mut module = Module {
        types: Vec::new(),
        imports: Vec::new(),
        function_table: None,
        code_section: None,
    };

    // Parse sections
    while offset < buffer.len() {
        let section_id = buffer[offset];
        offset += 1;
        let section_size = parser.parse_leb128(&buffer, &mut offset);
        let section_end = offset + section_size as usize;

        match section_id {
            1 => {
                // Type section
                module.types = parser.parse_type_section(&buffer, &mut offset);
                println!("Type Section: {:?}", module.types);
            }
            2 => {
                // Import section
                module.imports = parser.parse_import_section(&buffer, &mut offset);
                println!("Import Section: {:?}", module.imports);
            }
            3 => {
                // Function section
                module.function_table = Some(parser.parse_function_table(&buffer, &mut offset));
                println!("Function Table: {:?}", module.function_table);
            }
            10 => {
                // Code section
                module.code_section = Some(parser.parse_code_section(&buffer, &mut offset));
                println!("Code Section: {:?}", module.code_section);
            }
            _ => {
                // Skip other sections
                offset = section_end;
            }
        }
    }

    println!("Parsed Module: {:?}", module);

    Ok(())
}
