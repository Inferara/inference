use crate::nodes::{BlockType, Definition, Expression, Statement, Type};

impl Definition {
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Definition::Spec(spec) => spec.name.name(),
            Definition::Struct(struct_def) => struct_def.name.name(),
            Definition::Enum(enum_def) => enum_def.name.name(),
            Definition::Constant(const_def) => const_def.name.name(),
            Definition::Function(func_def) => func_def.name.name(),
            Definition::ExternalFunction(ext_func_def) => ext_func_def.name.name(),
            Definition::Type(type_def) => type_def.name.name(),
        }
    }
}

impl BlockType {
    #[must_use]
    pub fn statements(&self) -> Vec<Statement> {
        match self {
            BlockType::Block(block)
            | BlockType::Forall(block)
            | BlockType::Assume(block)
            | BlockType::Exists(block)
            | BlockType::Unique(block) => block.statements.clone(),
        }
    }
    #[must_use]
    pub fn is_non_det(&self) -> bool {
        match self {
            BlockType::Block(block) => block
                .statements
                .iter()
                .any(super::nodes::Statement::is_non_det),
            _ => true,
        }
    }
    #[must_use]
    pub fn is_void(&self) -> bool {
        let fn_find_ret_stmt = |statements: &Vec<Statement>| -> bool {
            for stmt in statements {
                match stmt {
                    Statement::Return(_) => return true,
                    Statement::Block(block_type) => {
                        if block_type.is_void() {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            false
        };
        !fn_find_ret_stmt(&self.statements())
    }
}

impl Statement {
    #[must_use]
    pub fn is_non_det(&self) -> bool {
        match self {
            Statement::Block(block_type) => !matches!(block_type, BlockType::Block(_)),
            Statement::Expression(expr_stmt) => expr_stmt.is_non_det(),
            Statement::Return(ret_stmt) => ret_stmt.expression.borrow().is_non_det(),
            Statement::Loop(loop_stmt) => loop_stmt
                .condition
                .borrow()
                .as_ref()
                .is_some_and(super::nodes::Expression::is_non_det),
            Statement::If(if_stmt) => {
                if_stmt.condition.borrow().is_non_det()
                    || if_stmt.if_arm.is_non_det()
                    || if_stmt
                        .else_arm
                        .as_ref()
                        .is_some_and(super::nodes::BlockType::is_non_det)
            }
            Statement::VariableDefinition(var_def) => var_def
                .value
                .as_ref()
                .is_some_and(|value| value.borrow().is_non_det()),
            _ => false,
        }
    }
}

impl Expression {
    #[must_use]
    pub fn is_non_det(&self) -> bool {
        matches!(self, Expression::Uzumaki(_))
    }
}

impl Type {
    pub(crate) fn is_unit_type(&self) -> bool {
        match self {
            Type::Simple(simple_type) => simple_type.name == "unit", //FIXME string comparison
            _ => false,
        }
    }
}
