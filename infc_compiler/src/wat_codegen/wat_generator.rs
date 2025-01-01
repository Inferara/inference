#![warn(clippy::pedantic)]

use crate::ast::types::{
    BinaryExpression, Block, Definition, Expression, FunctionDefinition, OperatorKind, SourceFile,
    Statement, Type,
};

#[must_use]
#[allow(clippy::single_match)]
pub fn generate_for_source_file(source_file: &SourceFile) -> String {
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

#[allow(clippy::single_match)]
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

    let function_export = generate_function_export(function);
    let function_parameters = generate_function_parameters(function);
    let function_result = generate_function_result(function);

    result.push_str(&format!(
        "{spaces}(func {function_export} {function_parameters} {function_result}\n",
    ));

    result.push_str(generate_for_block(&function.body, indent + 1).as_str());

    if function.is_void() {
        result.push_str("i32.const 0");
    }
    result.push_str(")\n");
    result
}

fn generate_function_export(function: &FunctionDefinition) -> String {
    format!("(export \"{}\")", function.name())
}

fn generate_function_parameters(function: &FunctionDefinition) -> String {
    let mut result = String::new();
    if let Some(parameters) = &function.parameters {
        for parameter in parameters {
            result.push_str(&format!(
                "(param ${} {}) ",
                parameter.name(),
                generate_for_type(&parameter.type_, 0)
            ));
        }
    }
    if !result.is_empty() {
        result.pop();
    }
    result
}

fn generate_function_result(function: &FunctionDefinition) -> String {
    if let Some(returns) = &function.returns {
        format!("(result {})", generate_for_type(returns, 0))
    } else {
        "(result i32)".to_string()
    }
}

fn generate_for_block(block: &Block, indent: u32) -> String {
    let mut result = String::new();
    for statement in &block.statements {
        match statement {
            Statement::Return(return_statement) => {
                result.push_str(&generate_for_expression(
                    &return_statement.expression,
                    indent,
                ));
            }
            Statement::Expression(expression) => {
                result.push_str(&generate_for_expression(&expression.expression, indent));
            }
            _ => {}
        }
    }
    result
}

fn generate_for_binary_expression(bin_expr: &BinaryExpression, indent: u32) -> String {
    let mut result = String::new();
    let left = generate_for_expression(&bin_expr.left, indent);
    let right = generate_for_expression(&bin_expr.right, indent);
    let operator = generate_for_bin_expr_operator(&bin_expr.operator, indent);
    let indentation = generate_indentation(indent);
    result.push_str(format!("{indentation}{left}\n").as_str());
    result.push_str(format!("{indentation}{right}\n").as_str());
    result.push_str(format!("{indentation}{operator}\n").as_str());
    result
}

fn generate_for_expression(expr: &Expression, indent: u32) -> String {
    let indentation = generate_indentation(indent);
    match expr {
        Expression::Binary(bin_expr) => generate_for_binary_expression(bin_expr, indent),
        Expression::Identifier(identifier) => {
            format!("{}local.get ${}", indentation, identifier.name.clone())
        }
        _ => String::new(),
    }
}

fn generate_for_bin_expr_operator(operator: &OperatorKind, indent: u32) -> String {
    let indentation = generate_indentation(indent);
    match operator {
        OperatorKind::Add => format!("{indentation}i32.add"),
        _ => String::new(),
    }
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
    use wat_generator::generate_indentation;

    use super::*;
    use crate::ast::*;
    use crate::wat_codegen::*;

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

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_simple_add_function() {
        //"fn add(a: i32, b: i32) -> i32 { return a + b; }"
        let function = FunctionDefinition {
            location: Location {
                start: Position { row: 0, column: 0 },
                end: Position { row: 0, column: 0 },
            },
            name: Identifier {
                location: Location {
                    start: Position { row: 0, column: 3 },
                    end: Position { row: 0, column: 6 },
                },
                name: "add".to_string(),
            },
            parameters: Some(vec![
                Parameter {
                    location: Location {
                        start: Position { row: 0, column: 7 },
                        end: Position { row: 0, column: 10 },
                    },
                    name: Identifier {
                        location: Location {
                            start: Position { row: 0, column: 7 },
                            end: Position { row: 0, column: 8 },
                        },
                        name: "a".to_string(),
                    },
                    type_: Type::Simple(SimpleType {
                        location: Location {
                            start: Position { row: 0, column: 11 },
                            end: Position { row: 0, column: 14 },
                        },
                        name: "i32".to_string(),
                    }),
                },
                Parameter {
                    location: Location {
                        start: Position { row: 0, column: 15 },
                        end: Position { row: 0, column: 18 },
                    },
                    name: Identifier {
                        location: Location {
                            start: Position { row: 0, column: 15 },
                            end: Position { row: 0, column: 16 },
                        },
                        name: "b".to_string(),
                    },
                    type_: Type::Simple(SimpleType {
                        location: Location {
                            start: Position { row: 0, column: 19 },
                            end: Position { row: 0, column: 22 },
                        },
                        name: "i32".to_string(),
                    }),
                },
            ]),
            returns: Some(Type::Simple(SimpleType {
                location: Location {
                    start: Position { row: 0, column: 27 },
                    end: Position { row: 0, column: 30 },
                },
                name: "i32".to_string(),
            })),
            body: Block {
                location: Location {
                    start: Position { row: 0, column: 32 },
                    end: Position { row: 0, column: 39 },
                },
                statements: vec![Statement::Return(ReturnStatement {
                    location: Location {
                        start: Position { row: 0, column: 32 },
                        end: Position { row: 0, column: 39 },
                    },
                    expression: Expression::Binary(Box::new(BinaryExpression {
                        location: Location {
                            start: Position { row: 0, column: 39 },
                            end: Position { row: 0, column: 39 },
                        },
                        operator: OperatorKind::Add,
                        left: Box::new(Expression::Identifier(Identifier {
                            location: Location {
                                start: Position { row: 0, column: 39 },
                                end: Position { row: 0, column: 40 },
                            },
                            name: "a".to_string(),
                        })),
                        right: Box::new(Expression::Identifier(Identifier {
                            location: Location {
                                start: Position { row: 0, column: 43 },
                                end: Position { row: 0, column: 44 },
                            },
                            name: "b".to_string(),
                        })),
                    })),
                })],
            },
        };

        let wat = generate_for_function_definition(&function, 0);
        let expected = "(func (export \"add\") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
)";
        assert_eq!(wat.trim(), expected);
    }
}
