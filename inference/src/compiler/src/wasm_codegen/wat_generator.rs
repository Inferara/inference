#![warn(clippy::pedantic)]
use crate::ast::types::{Definition, FunctionDefinition, SourceFile, Type};

pub(crate) fn generate_for_source_file(source_file: &SourceFile) -> String {
    let mut result = String::new();
    for definition in &source_file.definitions {
        match definition {
            Definition::Spec(spec) => {
                result.push_str(&format!("{}\n", generate_for_spec(spec, 0)));
            }
            _ => {}
        }
    }
    result
}

fn generate_for_spec(spec: &crate::ast::types::SpecDefinition, indent: u32) -> String {
    let mut result = String::new();
    let spaces = generate_indentation(indent);
    result.push_str(&format!("{spaces}(module\n"));
    for definition in &spec.definitions {
        match definition {
            Definition::Function(function) => {
                result.push_str(&format!(
                    "{}\n",
                    generate_for_function_definition(function, indent + 1)
                ));
            }
            _ => {}
        }
    }
    result.push_str(")\n");
    result
}

pub(crate) fn generate_for_function_definition(
    function: &FunctionDefinition,
    indent: u32,
) -> String {
    let spaces = generate_indentation(indent);
    let mut result = String::new();
    let parameters_len = function.parameters.as_ref().map_or(0, std::vec::Vec::len);
    result.push_str(&format!(
        "{}(func (export {}) (param {})\n",
        spaces, function.name.name, parameters_len
    ));
    if function.parameters.is_some() {
        for parameter in function.parameters.as_ref().unwrap() {
            result.push_str(&format!(
                "{}(param ${} {})\n",
                spaces,
                parameter.name.name,
                generate_for_type(&parameter.type_, 0)
            ));
        }
    }
    if function.returns.is_some() {
        result.push_str(&format!(
            "{}(result {})\n",
            spaces,
            generate_for_type(function.returns.as_ref().unwrap(), 0)
        ));
        result.push_str(&format!(
            "{}(local ${} {})\n",
            spaces,
            function.name.name,
            generate_for_type(function.returns.as_ref().unwrap(), 0)
        ));
    } else {
        result.push_str(&format!("{}(local ${} i32)\n", spaces, function.name.name));
    }

    result.push_str(&format!("{spaces}(nop)\n",));
    result.push_str(")\n");
    result
}

fn generate_for_type(type_: &Type, _: u32) -> String {
    match type_ {
        Type::Simple(simple) => simple.name.clone(),
        Type::Identifier(identifier) => identifier.name.clone(),
        _ => String::new(),
    }
}

fn generate_indentation(indent: u32) -> String {
    " ".repeat((indent * 2) as usize)
}

#[cfg(test)]
mod tests {
    use types::*;
    use wat_generator::generate_for_function_definition;
    use wat_generator::generate_indentation;

    use super::*;
    use crate::ast::*;
    use crate::wasm_codegen::*;

    #[test]
    fn test_generate_indentation() {
        assert_eq!(generate_indentation(0), "");
        assert_eq!(generate_indentation(1), "  ");
        assert_eq!(generate_indentation(2), "    ");
        assert_eq!(generate_indentation(3), "      ");
    }

    #[test]
    fn test_generate_for_type_simple() {
        for t in ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"] {
            let type_ = Type::Simple(SimpleType {
                location: Location::default(),
                name: t.to_string(),
            });
            assert_eq!(generate_for_type(&type_, 0), t);
        }
    }
}
