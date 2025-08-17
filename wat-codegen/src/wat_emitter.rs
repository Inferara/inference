#![warn(clippy::pedantic)]

use inference_ast::types::{
    AssertStatement, BinaryExpression, BlockType, Definition, Expression, FunctionCallExpression,
    FunctionDefinition, Literal, MemberAccessExpression, OperatorKind, SourceFile, SpecDefinition,
    Statement, Type, VariableDefinitionStatement,
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

#[derive(Debug, Clone, Default)]
pub struct WatEmitter {
    source_files: Vec<SourceFile>,
    // functions_to_emit: HashMap<u32, Vec<String>>,
}

impl WatEmitter {
    pub fn add_source_file(&mut self, source_file: SourceFile) {
        self.source_files.push(source_file);
    }

    #[allow(clippy::missing_panics_doc)]
    #[must_use = "This function returns the generated WAT code as a string"]
    pub fn emit(&self) -> String {
        if self.source_files.is_empty() {
            return String::new();
        }
        self.emit_for_source_file(self.source_files.first().unwrap())
            .join("\n")
    }

    #[allow(clippy::single_match)]
    fn emit_for_source_file(&self, source_file: &SourceFile) -> Vec<String> {
        let mut result = Vec::new();
        result.push(s_module());
        for definition in &source_file.definitions {
            match definition {
                Definition::Spec(spec) => {
                    result.extend(self.emit_for_spec(spec));
                }
                Definition::Function(function) => {
                    result.extend(self.emit_for_function_definition(function));
                }
                _ => {}
            }
        }
        result.push(r_brace());
        result
    }

    #[allow(clippy::single_match)]
    fn emit_for_spec(&self, spec: &SpecDefinition) -> Vec<String> {
        let mut result = Vec::new();
        result.push(s_module());
        for definition in &spec.definitions {
            match definition {
                Definition::Function(function) => {
                    result.extend(self.emit_for_function_definition(function));
                }
                _ => {}
            }
        }
        result.push(r_brace());
        result
    }

    fn emit_for_function_definition(&self, function: &FunctionDefinition) -> Vec<String> {
        let mut result = Vec::new();
        result.push(format!("{} ${}", s_func(), function.name()));
        // result.push(format!("(export \"{}\")", function.name()));
        result.extend(WatEmitter::emit_function_parameters(function));

        if let Some(returns) = &function.returns {
            result.push(format!("(result {})", WatEmitter::emit_for_type(returns)));
        }

        let mut block = self.emit_for_block(&function.body);
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
        result.push(format!(
            "(export \"{}\" (func ${}))",
            function.name(),
            function.name()
        ));
        result
    }

    fn emit_function_parameters(function: &FunctionDefinition) -> Vec<String> {
        let mut result = Vec::new();
        if let Some(parameters) = &function.parameters {
            for parameter in parameters {
                result.push(format!(
                    "(param ${} {})",
                    parameter.name(),
                    WatEmitter::emit_for_type(&parameter.type_)
                ));
            }
        }
        result
    }

    fn emit_for_block(&self, block_type: &BlockType) -> Vec<String> {
        let mut result = Vec::new();
        match block_type {
            BlockType::Block(block) => {
                result.extend(self.emit_for_statements(&block.statements));
            }
            BlockType::Assume(assume) => {
                result.push(s_assume());
                result.extend(self.emit_for_statements(&assume.statements));
                result.push(r_brace());
            }
            BlockType::Forall(forall) => {
                result.push(s_forall());
                result.extend(self.emit_for_statements(&forall.statements));
                result.push(r_brace());
            }
            BlockType::Exists(exist) => {
                result.push(s_exists());
                result.extend(self.emit_for_statements(&exist.statements));
                result.push(r_brace());
            }
            BlockType::Unique(unique) => {
                result.push(s_unique());
                result.extend(self.emit_for_statements(&unique.statements));
                result.push(r_brace());
            }
        }
        result
    }

    fn emit_for_statements(&self, statements: &Vec<Statement>) -> Vec<String> {
        let mut result = Vec::new();
        for statement in statements {
            match statement {
                Statement::Assert(assert) => {
                    result.extend(self.emit_for_assert_statement(assert));
                }
                Statement::Return(return_statement) => {
                    result.extend(self.emit_for_expression(&return_statement.expression));
                }
                Statement::Expression(expression) => {
                    result.extend(self.emit_for_expression(&expression.expression));
                }
                Statement::VariableDefinition(variable_definition) => {
                    result.extend(self.emit_for_variable_definition(variable_definition));
                }
                Statement::Block(block) => {
                    result.extend(self.emit_for_block(block));
                }
                _ => result.push(format!("{statement:?} statement is not supported yet")),
            }
        }
        result
    }

    fn emit_for_binary_expression(&self, bin_expr: &BinaryExpression) -> Vec<String> {
        let mut result = Vec::new();
        if let Expression::Identifier(identifier) = &bin_expr.left {
            result.push(format!("local.get ${}", identifier.name));
        } else {
            result.extend(self.emit_for_expression(&bin_expr.left));
        }

        if let Expression::Identifier(identifier) = &bin_expr.right {
            result.push(format!("local.get ${}", identifier.name));
        } else {
            result.extend(self.emit_for_expression(&bin_expr.right));
        }
        result.push(WatEmitter::emit_for_bin_expr_operator(&bin_expr.operator));
        result
    }

    fn emit_for_function_call(&self, function_call: &FunctionCallExpression) -> Vec<String> {
        let mut result = Vec::new();
        if let Some(arguments) = &function_call.arguments {
            //TODO: check order
            for (_, arg_expr) in arguments {
                match arg_expr {
                    Expression::Identifier(identifier) => {
                        result.push(format!("local.get ${}", identifier.name));
                    }
                    _ => {
                        result.extend(self.emit_for_expression(arg_expr));
                    }
                }
            }
        }
        let function = self.emit_for_expression(&function_call.function).concat();
        result.push(format!("call ${function:?}"));
        result
    }

    fn emit_for_member_access(&self, member_access: &MemberAccessExpression) -> Vec<String> {
        let mut result = Vec::new();
        result.extend(self.emit_for_expression(&member_access.expression));
        result.push(format!("get ${}", member_access.name.name));
        result
    }

    fn emit_for_expression(&self, expr: &Expression) -> Vec<String> {
        let mut result = Vec::new();
        match expr {
            Expression::Binary(bin_expr) => {
                result.extend(self.emit_for_binary_expression(bin_expr));
            }
            Expression::Identifier(identifier) => {
                result.push(identifier.name.clone());
            }
            Expression::Literal(literal) => {
                result.push(WatEmitter::emit_for_literal(literal));
            }
            Expression::FunctionCall(function_call) => {
                result.extend(self.emit_for_function_call(function_call));
            }
            Expression::MemberAccess(member_access) => {
                result.extend(self.emit_for_member_access(member_access));
            }
            Expression::Uzumaki(_) => result.push(String::from("i32.uzumaki")),
            _ => result.push(format!("{expr:?} expression type is not supported yet")),
        }
        result
    }

    fn emit_for_bin_expr_operator(operator: &OperatorKind) -> String {
        match operator {
            OperatorKind::Add => String::from("i32.add"),
            OperatorKind::Sub => String::from("i32.sub"),
            OperatorKind::Mul => String::from("i32.mul"),
            OperatorKind::Eq => String::from("i32.eq"),
            OperatorKind::Le => String::from("i32.le_u"),
            _ => format!("{operator:?} operator is not supported yet"),
        }
    }

    fn emit_for_variable_definition(
        &self,
        variable_definition: &VariableDefinitionStatement,
    ) -> Vec<String> {
        let mut result = Vec::new();
        let variable_name = variable_definition.name();
        let variable_type = WatEmitter::emit_for_type(&variable_definition.type_);
        result.push(format!("(local ${variable_name} {variable_type})"));
        if let Some(value) = &variable_definition.value {
            match value {
                Expression::Identifier(identifier) => {
                    result.push(format!("local.get ${}", identifier.name));
                }
                _ => {
                    result.extend(self.emit_for_expression(value));
                }
            }
            result.push(format!("local.set ${variable_name}"));
        }
        result
    }

    fn emit_for_type(type_: &Type) -> String {
        match type_ {
            Type::Simple(simple) => simple.name.clone(),
            Type::Identifier(identifier) => identifier.name.clone(),
            _ => format!("{type_:?} type is not supported yet"),
        }
    }

    #[allow(clippy::single_match_else)]
    fn emit_for_literal(literal: &Literal) -> String {
        match literal {
            Literal::Number(number) => {
                let literal_value = &number.value;
                let type_ = WatEmitter::emit_for_type(&number.type_);
                format!("{type_}.const {literal_value}")
            }
            _ => format!("{literal:?} literal is not supported yet"),
        }
    }

    // fn emit_local_get_identifier_or_expression(&self, expression: &Expression) -> Vec<String> {
    //     if let Expression::Identifier(expr_identifier) = expression {
    //         vec![format!("local.get ${}", expr_identifier.name)]
    //     } else {
    //         self.emit_for_expression(expression)
    //     }
    // }

    fn emit_for_assert_statement(&self, assert: &AssertStatement) -> Vec<String> {
        match &assert.expression {
            Expression::Binary(bin_expr) => {
                let mut result = Vec::new();

                if let Expression::Identifier(identifier) = &bin_expr.left {
                    result.push(format!("local.get ${}", identifier.name));
                } else {
                    result.extend(self.emit_for_expression(&bin_expr.left));
                    let variable_name = format!("_assert_{}_left", assert.id);
                    let variable_type = "i32"; //TODO we cannot work with other types yet
                    result.push(format!("(local ${variable_name} {variable_type})"));
                    result.push(format!("local.set ${variable_name}"));
                    result.push(format!("local.get ${variable_name}"));
                }

                if let Expression::Identifier(identifier) = &bin_expr.right {
                    result.push(format!("local.get ${}", identifier.name));
                } else {
                    result.extend(self.emit_for_expression(&bin_expr.left));
                    let variable_name = format!("_assert_{}_right", assert.id);
                    let variable_type = "i32"; //TODO we cannot work with other types yet
                    result.push(format!("(local ${variable_name} {variable_type})"));
                    result.push(format!("local.set ${variable_name}"));
                    result.push(format!("local.get ${variable_name}"));
                }

                result.push(WatEmitter::emit_for_bin_expr_operator(&bin_expr.operator));
                result.push("if".to_string());
                result.push(";;".to_string());
                result.push("else".to_string());
                result.push("unreachable".to_string());
                result.push("end".to_string());
                result
            }
            _ => vec![format!(
                "{assert:?} assert statement with non binary expression is not supported yet"
            )],
        }
    }
}
