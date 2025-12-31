use crate::{
    arena::Arena,
    nodes::{AstNode, Definition, Expression, SourceFile, Statement},
    type_info::TypeInfo,
};

#[derive(Clone, Default)]
pub struct Ast {
    pub source_files: Vec<SourceFile>,
    arena: Arena,
}

impl Ast {
    #[must_use]
    pub fn new(source_files: Vec<SourceFile>, arena: Arena) -> Self {
        Self {
            source_files,
            arena,
        }
    }

    pub fn filter_nodes<T: Fn(&AstNode) -> bool>(&self, fn_predicate: T) -> Vec<AstNode> {
        self.arena
            .nodes
            .values()
            .filter(|node| fn_predicate(node))
            .cloned()
            .collect()
    }

    pub fn infer_expression_types(&self) {
        //FIXME: very hacky way to infer Uzumaki expression types in return statements
        for function_def_node in
            self.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Function(_))))
        {
            let AstNode::Definition(Definition::Function(function_def)) = function_def_node else {
                unreachable!()
            };
            if function_def.is_void() {
                continue;
            }
            if let Some(Statement::Return(last_stmt)) = function_def.body.statements().last() {
                if !matches!(*last_stmt.expression.borrow(), Expression::Uzumaki(_)) {
                    continue;
                }

                match &*last_stmt.expression.borrow() {
                    Expression::Uzumaki(expr) => {
                        if expr.type_info.borrow().is_some() {
                            continue;
                        }
                        if let Some(return_type) = &function_def.returns {
                            expr.type_info.replace(Some(TypeInfo::new(return_type)));
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}
