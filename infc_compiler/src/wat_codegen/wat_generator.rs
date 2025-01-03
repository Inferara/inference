#![warn(clippy::pedantic)]

use crate::ast::types::{
    BinaryExpression, BlockType, Definition, Expression, FunctionDefinition, Literal, OperatorKind,
    SourceFile, Statement, Type, VariableDefinitionStatement,
};

#[must_use]
#[allow(clippy::single_match)]
pub fn generate_for_source_file(source_file: &SourceFile) -> String {
    let mut result = String::new();
    result.push_str("(module\n");
    for definition in &source_file.definitions {
        match definition {
            Definition::Spec(spec) => {
                result.push_str(&format!("{}\n", generate_for_spec(spec, 1)));
            }
            Definition::Function(function) => {
                result.push_str(&format!(
                    "{}\n",
                    generate_for_function_definition(function, 1)
                ));
            }
            _ => {}
        }
    }
    result.push(')');
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
    result.push(')');
    result
}

pub(crate) fn generate_for_function_definition(
    function: &FunctionDefinition,
    indent: u32,
) -> String {
    let indentation = generate_indentation(indent);
    let mut result = String::new();

    let function_export = generate_function_export(function);
    let function_parameters = generate_function_parameters(function);
    let function_result = generate_function_result(function);

    result.push_str(&format!(
        "{indentation}(func {function_export} {function_parameters} {function_result}\n",
    ));

    result.push_str(generate_for_block(&function.body, indent + 1).as_str());
    result.push_str(format!("{indentation})").as_str());
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
                generate_for_type(&parameter.type_)
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
        format!("(result {})", generate_for_type(returns))
    } else {
        String::new()
    }
}

fn generate_for_block(block_type: &BlockType, indent: u32) -> String {
    let mut result = String::new();
    let indentation = generate_indentation(indent);
    let indentation_next = generate_indentation(indent + 1);
    match block_type {
        BlockType::Block(block) => {
            for stmt in &generate_for_statements(&block.statements, indent) {
                result.push_str(format!("{indentation}{stmt}\n").as_str());
            }
        }
        BlockType::Assume(assume) => {
            result.push_str(format!("{indentation}(assume\n").as_str());
            for stmt in &generate_for_statements(&assume.statements, indent + 1) {
                result.push_str(format!("{indentation_next}{stmt}\n").as_str());
            }
            result.push_str(format!("{indentation})\n").as_str());
        }
        BlockType::Forall(forall) => {
            result.push_str(format!("{indentation}(forall\n").as_str());
            for stmt in &generate_for_statements(&forall.statements, indent + 1) {
                result.push_str(format!("{indentation_next}{stmt}\n").as_str());
            }
            result.push_str(format!("{indentation})\n").as_str());
        }
        BlockType::Exists(exist) => {
            result.push_str(format!("{indentation}(exists\n").as_str());
            for stmt in &generate_for_statements(&exist.statements, indent + 1) {
                result.push_str(format!("{indentation_next}{stmt}\n").as_str());
            }
            result.push_str(format!("{indentation})\n").as_str());
        }
        BlockType::Unique(unique) => {
            result.push_str(format!("{indentation}(unique\n").as_str());
            for stmt in &generate_for_statements(&unique.statements, indent + 1) {
                result.push_str(format!("{indentation_next}{stmt}\n").as_str());
            }
            result.push_str(format!("{indentation})\n").as_str());
        }
    }
    result
}

fn generate_for_statements(statements: &Vec<Statement>, indent: u32) -> Vec<String> {
    let mut result = Vec::new();
    for statement in statements {
        match statement {
            Statement::Return(return_statement) => {
                for op in generate_for_expression(&return_statement.expression) {
                    result.push(op);
                }
            }
            Statement::Expression(expression) => {
                for op in generate_for_expression(&expression.expression) {
                    result.push(op);
                }
            }
            Statement::VariableDefinition(variable_definition) => {
                for op in generate_for_variable_definition(variable_definition) {
                    result.push(op);
                }
            }
            Statement::Block(block) => {
                result.push(generate_for_block(block, indent + 1));
            }
            _ => result.push(format!("{statement:?} statement is not supported yet")),
        }
    }
    result
}

fn generate_for_binary_expression(bin_expr: &BinaryExpression) -> Vec<String> {
    let mut result = Vec::new();
    let left = generate_for_expression(&bin_expr.left);
    let right = generate_for_expression(&bin_expr.right);
    let operator = generate_for_bin_expr_operator(&bin_expr.operator);
    for op in left {
        result.push(op);
    }
    for op in right {
        result.push(op);
    }
    result.push(operator);
    result
}

fn generate_for_expression(expr: &Expression) -> Vec<String> {
    let mut result = Vec::new();
    match expr {
        Expression::Binary(bin_expr) => {
            for op in generate_for_binary_expression(bin_expr) {
                result.push(op);
            }
        }
        Expression::Identifier(identifier) => {
            result.push(format!("local.get ${}", identifier.name.clone()));
        }
        Expression::Literal(literal) => result.push(generate_for_literal(literal)),
        _ => result.push(format!("{expr:?} expression type is not supported yet")),
    }
    result
}

fn generate_for_bin_expr_operator(operator: &OperatorKind) -> String {
    match operator {
        OperatorKind::Add => String::from("i32.add"),
        OperatorKind::Sub => String::from("i32.sub"),
        OperatorKind::Mul => String::from("i32.mul"),
        _ => format!("{operator:?} operator is not supported yet"),
    }
}

fn generate_for_variable_definition(
    variable_definition: &VariableDefinitionStatement,
) -> Vec<String> {
    let mut result = Vec::new();
    let variable_name = variable_definition.name();
    let variable_type = generate_for_type(&variable_definition.type_);
    result.push(format!("(local ${variable_name} {variable_type})"));
    if let Some(value) = &variable_definition.value {
        let exprs = generate_for_expression(value);
        let expr = exprs.first().unwrap();
        result.push(format!("(local.set ${variable_name} {expr})"));
    }
    result
}

fn generate_for_type(type_: &Type) -> String {
    match type_ {
        Type::Simple(simple) => simple.name.clone(),
        Type::Identifier(identifier) => identifier.name.clone(),
        _ => format!("{type_:?} type is not supported yet"),
    }
}

#[allow(clippy::single_match_else)]
fn generate_for_literal(literal: &Literal) -> String {
    match literal {
        Literal::Number(number) => {
            let literal_value = &number.value;
            let type_ = generate_for_type(&number.type_);
            format!("{type_}.const {literal_value}")
        }
        _ => format!("{literal:?} literal is not supported yet"),
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
            assert_eq!(generate_for_type(&type_), t);
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
            body: BlockType::Block(Block {
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
            }),
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
