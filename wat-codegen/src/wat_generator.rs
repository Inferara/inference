#![warn(clippy::pedantic)]

use inference_ast::types::{
    BinaryExpression, BlockType, Definition, Expression, FunctionDefinition, Literal, OperatorKind,
    SourceFile, SpecDefinition, Statement, Type, VariableDefinitionStatement,
};

fn r_brace() -> String {
    String::from(")")
}

fn s_module() -> String {
    String::from("(module")
}

fn s_func() -> String {
    String::from("(func")
}

fn s_forall() -> String {
    String::from("(forall")
}

fn s_exists() -> String {
    String::from("(exists")
}

fn s_unique() -> String {
    String::from("(unique")
}

fn s_assume() -> String {
    String::from("(assume")
}

#[must_use]
pub fn generate_string_for_source_file(source_file: &SourceFile) -> String {
    generate_for_source_file(source_file).join(" ")
}

#[must_use]
#[allow(clippy::single_match)]
pub fn generate_for_source_file(source_file: &SourceFile) -> Vec<String> {
    let mut result = Vec::new();
    for definition in &source_file.definitions {
        match definition {
            Definition::Spec(spec) => {
                result.extend(generate_for_spec(spec));
            }
            Definition::Function(function) => {
                result.push(s_module());
                result.extend(generate_for_function_definition(function));
                result.push(r_brace());
            }
            _ => {}
        }
    }
    result
}

#[allow(clippy::single_match)]
fn generate_for_spec(spec: &SpecDefinition) -> Vec<String> {
    let mut result = Vec::new();
    result.push(s_module());
    for definition in &spec.definitions {
        match definition {
            Definition::Function(function) => {
                result.extend(generate_for_function_definition(function));
            }
            _ => {}
        }
    }
    result.push(r_brace());
    result
}

fn generate_for_function_definition(function: &FunctionDefinition) -> Vec<String> {
    let mut result = Vec::new();
    result.push(s_func());
    result.push(format!("(export \"{}\")", function.name()));
    result.extend(generate_function_parameters(function));

    if let Some(returns) = &function.returns {
        result.push(format!("(result {})", generate_for_type(returns)));
    }

    let mut block = generate_for_block(&function.body);
    let mut locals = Vec::new();
    let mut i = 0;
    while i < block.len() {
        if block[i].starts_with("(local ") {
            locals.push(block.remove(i));
        } else {
            i += 1;
        }
    }
    result.extend(locals);
    result.extend(block);
    result.push(r_brace());
    result
}

fn generate_function_parameters(function: &FunctionDefinition) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(parameters) = &function.parameters {
        for parameter in parameters {
            result.push(format!(
                "(param ${} {})",
                parameter.name(),
                generate_for_type(&parameter.type_)
            ));
        }
    }
    result
}

fn generate_for_block(block_type: &BlockType) -> Vec<String> {
    let mut result = Vec::new();
    match block_type {
        BlockType::Block(block) => {
            result.extend(generate_for_statements(&block.statements));
        }
        BlockType::Assume(assume) => {
            result.push(s_assume());
            result.extend(generate_for_statements(&assume.statements));
            result.push(r_brace());
        }
        BlockType::Forall(forall) => {
            result.push(s_forall());
            result.extend(generate_for_statements(&forall.statements));
            result.push(r_brace());
        }
        BlockType::Exists(exist) => {
            result.push(s_exists());
            result.extend(generate_for_statements(&exist.statements));
            result.push(r_brace());
        }
        BlockType::Unique(unique) => {
            result.push(s_unique());
            result.extend(generate_for_statements(&unique.statements));
            result.push(r_brace());
        }
    }
    result
}

fn generate_for_statements(statements: &Vec<Statement>) -> Vec<String> {
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
                result.extend(generate_for_block(block));
            }
            _ => result.push(format!("{statement:?} statement is not supported yet")),
        }
    }
    result
}

fn generate_for_binary_expression(bin_expr: &BinaryExpression) -> Vec<String> {
    let mut result = Vec::new();
    result.extend(generate_for_expression(&bin_expr.left));
    result.extend(generate_for_expression(&bin_expr.right));
    result.push(generate_for_bin_expr_operator(&bin_expr.operator));
    result
}

fn generate_for_expression(expr: &Expression) -> Vec<String> {
    let mut result = Vec::new();
    match expr {
        Expression::Binary(bin_expr) => {
            result.extend(generate_for_binary_expression(bin_expr));
        }
        Expression::Identifier(identifier) => {
            result.push(format!("local.get ${}", identifier.name.clone()));
        }
        Expression::Literal(literal) => result.push(generate_for_literal(literal)),
        Expression::Uzumaki(_) => result.push(String::from("i32.uzumaki")),
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
        result.extend(generate_for_expression(value));
        result.push(format!("local.set ${variable_name}"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use inference_ast::types::*;

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
}
