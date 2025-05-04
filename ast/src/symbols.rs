use crate::{
    arena::Arena,
    types::{
        AstNode, Definition, Expression, FunctionType, Location, SimpleType, SourceFile, Type,
    },
};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolType {
    Global(String),
    Inner(String),
    Generic(String),
    Primitive(String),
    Unit,
}

pub enum SymbolScope {}

pub struct Symbol {
    name: String,
    location: Location,
    ty: Type,
    expr: Expression,
}

#[derive(Clone, Default)]
pub struct SymbolTable {
    /// mapping from identifier names to their types
    map: HashMap<String, Type>,
}

impl SymbolTable {
    pub fn build(source_files: &Vec<SourceFile>, types: &Vec<SymbolType>, arena: &Arena) -> Self {
        // Initialize the table by recording top-level definitions
        let mut table = SymbolTable {
            map: HashMap::new(),
        };
        for sf in source_files {
            for def in &sf.definitions {
                match def {
                    Definition::Constant(c) => {
                        let name = &c.name.name;
                        table.map.insert(name.clone(), c.ty.clone());
                    }
                    Definition::Function(f) => {
                        let name = &f.name.name;
                        // build function type
                        let param_types = f
                            .arguments
                            .as_ref()
                            .map(|args| args.iter().map(|p| p.ty.clone()).collect());
                        let return_ty = f.returns.clone().unwrap_or_else(|| {
                            Type::Simple(Rc::new(SimpleType::new(
                                0,
                                f.location.clone(),
                                "Unit".to_string(),
                            )))
                        });
                        let func_ty = Type::Function(Rc::new(FunctionType::new(
                            0,
                            f.location.clone(),
                            param_types,
                            Some(return_ty),
                        )));
                        table.map.insert(name.clone(), func_ty);
                    }
                    _ => {}
                }
            }
        }
        table
    }

    /// Look up an identifier's type in the symbol table
    pub fn lookup(&self, name: &str) -> Option<Type> {
        self.map.get(name).cloned()
    }
    /// Insert a new symbol into the table
    pub fn insert(&mut self, name: String, ty: Type) {
        self.map.insert(name, ty);
    }
}
