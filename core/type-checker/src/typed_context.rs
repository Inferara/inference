use std::rc::Rc;

use crate::{
    symbol_table::SymbolTable,
    type_info::{NumberTypeKindNumberType, TypeInfo, TypeInfoKind},
};
use inference_ast::{arena::Arena, nodes::SourceFile};
use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct TypedContext {
    pub(crate) symbol_table: SymbolTable,
    node_types: FxHashMap<u32, TypeInfo>,
    arena: Arena,
}

impl TypedContext {
    pub(crate) fn new(arena: Arena) -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            node_types: FxHashMap::default(),
            arena,
        }
    }

    pub fn source_files(&self) -> Vec<Rc<SourceFile>> {
        self.arena.source_files()
    }

    pub fn is_node_i32(&self, node_id: u32) -> bool {
        self.is_node_type(node_id, |kind| {
            matches!(kind, TypeInfoKind::Number(NumberTypeKindNumberType::I32))
        })
    }

    pub fn is_node_i64(&self, node_id: u32) -> bool {
        self.is_node_type(node_id, |kind| {
            matches!(kind, TypeInfoKind::Number(NumberTypeKindNumberType::I64))
        })
    }

    pub fn get_node_typeinfo(&self, node_id: u32) -> Option<TypeInfo> {
        self.node_types.get(&node_id).cloned()
    }

    pub(crate) fn set_node_typeinfo(&mut self, node_id: u32, type_info: TypeInfo) {
        self.node_types.insert(node_id, type_info);
    }

    fn is_node_type<T>(&self, node_id: u32, type_checker: T) -> bool
    where
        T: Fn(&TypeInfoKind) -> bool,
    {
        if let Some(type_info) = self.get_node_typeinfo(node_id) {
            type_checker(&type_info.kind)
        } else {
            false
        }
    }

    // pub fn infer_expression_types(&self) {
    //     //FIXME: very hacky way to infer Uzumaki expression types in return statements
    //     for function_def_node in
    //         self.filter_nodes(|node| matches!(node, AstNode::Definition(Definition::Function(_))))
    //     {
    //         let AstNode::Definition(Definition::Function(function_def)) = function_def_node else {
    //             unreachable!()
    //         };
    //         if function_def.is_void() {
    //             continue;
    //         }
    //         if let Some(Statement::Return(last_stmt)) = function_def.body.statements().last() {
    //             if !matches!(*last_stmt.expression.borrow(), Expression::Uzumaki(_)) {
    //                 continue;
    //             }

    //             match &*last_stmt.expression.borrow() {
    //                 Expression::Uzumaki(expr) => {
    //                     if expr.type_info.borrow().is_some() {
    //                         continue;
    //                     }
    //                     if let Some(return_type) = &function_def.returns {
    //                         expr.type_info.replace(Some(TypeInfo::new(return_type)));
    //                     }
    //                 }
    //                 _ => unreachable!(),
    //             }
    //         }
    //     }
    // }
}
