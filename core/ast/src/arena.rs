//! Arena-based typed storage for the AST.
//!
//! `AstArena` stores each node category in its own `Arena<T>` from the vendored
//! la-arena crate, indexed by the corresponding typed ID (`ExprId`, `StmtId`,
//! etc.). This provides:
//!
//! - O(1) index-based lookup
//! - Type safety: you cannot accidentally index expressions with a statement ID
//! - `Send + Sync`: no `RefCell`, no interior mutability
//! - Cache-friendly sequential storage

use crate::ids::{BlockId, DefId, ExprId, IdentId, NodeId, SourceFileId, StmtId, TypeId};
use crate::la_arena::Arena;
use crate::nodes::{
    BlockData, Def, DefData, ExprData, Ident, Location, SourceFileData, StmtData, TypeData,
};

/// Central storage for all AST nodes.
///
/// Each node category has its own `Arena<T>`. Typed IDs (`ExprId`, `StmtId`, etc.)
/// index into the corresponding `Arena`.
#[derive(Default, Clone)]
pub struct AstArena {
    pub source_files: Arena<SourceFileData>,
    pub defs: Arena<DefData>,
    pub stmts: Arena<StmtData>,
    pub exprs: Arena<ExprData>,
    pub types: Arena<TypeData>,
    pub blocks: Arena<BlockData>,
    pub idents: Arena<Ident>,
}

// Compile-time assertion: AstArena is Send + Sync.
const _: () = {
    const fn assert_send<T: Send>() {}
    const fn assert_sync<T: Sync>() {}
    assert_send::<AstArena>();
    assert_sync::<AstArena>();
};

// ---------------------------------------------------------------------------
// Index impls — forward to inner Arena<T>
// ---------------------------------------------------------------------------

impl std::ops::Index<SourceFileId> for AstArena {
    type Output = SourceFileData;
    fn index(&self, id: SourceFileId) -> &SourceFileData {
        &self.source_files[id]
    }
}

impl std::ops::Index<DefId> for AstArena {
    type Output = DefData;
    fn index(&self, id: DefId) -> &DefData {
        &self.defs[id]
    }
}

impl std::ops::Index<StmtId> for AstArena {
    type Output = StmtData;
    fn index(&self, id: StmtId) -> &StmtData {
        &self.stmts[id]
    }
}

impl std::ops::Index<ExprId> for AstArena {
    type Output = ExprData;
    fn index(&self, id: ExprId) -> &ExprData {
        &self.exprs[id]
    }
}

impl std::ops::Index<TypeId> for AstArena {
    type Output = TypeData;
    fn index(&self, id: TypeId) -> &TypeData {
        &self.types[id]
    }
}

impl std::ops::Index<BlockId> for AstArena {
    type Output = BlockData;
    fn index(&self, id: BlockId) -> &BlockData {
        &self.blocks[id]
    }
}

impl std::ops::Index<IdentId> for AstArena {
    type Output = Ident;
    fn index(&self, id: IdentId) -> &Ident {
        &self.idents[id]
    }
}

// ---------------------------------------------------------------------------
// Source text retrieval
// ---------------------------------------------------------------------------

impl AstArena {
    /// Returns the source location of any node.
    #[must_use = "returns the source location of the node"]
    pub fn node_location(&self, node_id: NodeId) -> Location {
        match node_id {
            NodeId::SourceFile(id) => self.source_files[id].location,
            NodeId::Def(id) => self.defs[id].location,
            NodeId::Stmt(id) => self.stmts[id].location,
            NodeId::Expr(id) => self.exprs[id].location,
            NodeId::Type(id) => self.types[id].location,
            NodeId::Block(id) => self.blocks[id].location,
            NodeId::Ident(id) => self.idents[id].location,
        }
    }

    /// Finds which source file contains the given definition.
    ///
    /// Searches all source files' def lists, including nested defs inside
    /// structs, specs, and modules.
    #[must_use = "returns the source file containing the given definition"]
    pub fn find_source_file_for_def(&self, target: DefId) -> Option<SourceFileId> {
        for (sf_id, sf) in self.source_files.iter() {
            if self.def_in_list(target, &sf.defs) {
                return Some(sf_id);
            }
        }
        None
    }

    fn def_in_list(&self, target: DefId, defs: &[DefId]) -> bool {
        for &def_id in defs {
            if def_id == target {
                return true;
            }
            match &self[def_id].kind {
                Def::Struct { methods, .. } => {
                    if self.def_in_list(target, methods) {
                        return true;
                    }
                }
                Def::Spec { defs, .. }
                | Def::Module {
                    defs: Some(defs), ..
                } => {
                    if self.def_in_list(target, defs) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Finds which source file a node belongs to.
    ///
    /// For `SourceFile` nodes this is trivial. For `Def` nodes it delegates to
    /// `find_source_file_for_def`. For other nodes it falls back to byte-offset
    /// matching.
    #[must_use = "returns the source file containing the given node"]
    pub fn find_source_file_for_node(&self, node_id: NodeId) -> Option<SourceFileId> {
        match node_id {
            NodeId::SourceFile(id) => Some(id),
            NodeId::Def(def_id) => self.find_source_file_for_def(def_id),
            _ => {
                let location = self.node_location(node_id);
                self.find_source_file_by_offset(location)
            }
        }
    }

    fn find_source_file_by_offset(&self, location: Location) -> Option<SourceFileId> {
        let end = location.offset_end as usize;
        for (sf_id, sf) in self.source_files.iter() {
            if end <= sf.source.len() {
                return Some(sf_id);
            }
        }
        None
    }

    /// Returns the source text of a node by slicing its source file.
    ///
    /// Returns `None` if the source file cannot be determined or the byte
    /// offsets fall outside the source text.
    #[must_use]
    pub fn get_node_source(&self, node_id: NodeId) -> Option<&str> {
        let location = self.node_location(node_id);
        let start = location.offset_start as usize;
        let end = location.offset_end as usize;
        if start > end {
            return None;
        }
        let sf_id = self.find_source_file_for_node(node_id)?;
        self[sf_id].source.get(start..end)
    }
}

// ---------------------------------------------------------------------------
// Query methods
// ---------------------------------------------------------------------------

impl AstArena {
    /// Returns all source file data entries.
    #[must_use]
    pub fn source_files(&self) -> impl ExactSizeIterator<Item = &SourceFileData> + '_ {
        self.source_files.values()
    }

    /// Iterates over all source file IDs.
    pub fn source_file_ids(&self) -> impl Iterator<Item = SourceFileId> + '_ {
        self.source_files.iter().map(|(id, _)| id)
    }

    /// Returns all definition IDs that are functions across all source files.
    #[must_use]
    pub fn function_def_ids(&self) -> Vec<DefId> {
        let mut result = Vec::new();
        for sf in self.source_files.values() {
            for &def_id in &sf.defs {
                if matches!(self[def_id].kind, Def::Function { .. }) {
                    result.push(def_id);
                }
            }
        }
        result
    }

    /// Returns the name string of a definition (function, struct, etc.).
    #[must_use = "returns the name of the definition"]
    pub fn def_name(&self, def_id: DefId) -> &str {
        let name_id = match &self[def_id].kind {
            Def::Function { name, .. }
            | Def::ExternFunction { name, .. }
            | Def::Struct { name, .. }
            | Def::Enum { name, .. }
            | Def::Spec { name, .. }
            | Def::Constant { name, .. }
            | Def::TypeAlias { name, .. }
            | Def::Module { name, .. } => *name,
        };
        &self[name_id].name
    }

    /// Returns the name string of an identifier.
    #[must_use = "returns the name of the identifier"]
    pub fn ident_name(&self, id: IdentId) -> &str {
        &self[id].name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::{BlockKind, Expr, Stmt, Visibility};

    #[test]
    fn alloc_and_index_expr() {
        let mut arena = AstArena::default();
        let id = arena.exprs.alloc(ExprData {
            location: Location::default(),
            kind: Expr::NumberLiteral {
                value: "42".to_string(),
            },
        });
        assert_eq!(id.into_raw().into_u32(), 0);
        assert!(matches!(arena[id].kind, Expr::NumberLiteral { .. }));
    }

    #[test]
    fn alloc_and_index_ident() {
        let mut arena = AstArena::default();
        let id = arena.idents.alloc(Ident {
            location: Location::default(),
            name: "foo".to_string(),
        });
        assert_eq!(arena[id].name, "foo");
    }

    #[test]
    fn send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AstArena>();
    }

    #[test]
    fn node_location_returns_location() {
        let mut arena = AstArena::default();
        let loc = Location::new(10, 20, 1, 10, 1, 20);
        let id = arena.exprs.alloc(ExprData {
            location: loc,
            kind: Expr::NumberLiteral {
                value: "42".to_string(),
            },
        });
        assert_eq!(arena.node_location(NodeId::Expr(id)), loc);
    }

    #[test]
    fn find_source_file_for_def_finds_top_level() {
        let mut arena = AstArena::default();
        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: "foo".to_string(),
        });
        let body = arena.blocks.alloc(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![],
        });
        let def_id = arena.defs.alloc(DefData {
            location: Location::default(),
            kind: Def::Function {
                name,
                vis: Visibility::default(),
                type_params: vec![],
                args: vec![],
                returns: None,
                body,
            },
        });
        let sf_id = arena.source_files.alloc(SourceFileData {
            location: Location::default(),
            source: String::new(),
            defs: vec![def_id],
            directives: vec![],
        });
        assert_eq!(arena.find_source_file_for_def(def_id), Some(sf_id));
    }

    #[test]
    fn find_source_file_for_def_finds_nested_method() {
        let mut arena = AstArena::default();
        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: "m".to_string(),
        });
        let body = arena.blocks.alloc(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![],
        });
        let method = arena.defs.alloc(DefData {
            location: Location::default(),
            kind: Def::Function {
                name,
                vis: Visibility::default(),
                type_params: vec![],
                args: vec![],
                returns: None,
                body,
            },
        });
        let struct_name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: "S".to_string(),
        });
        let struct_def = arena.defs.alloc(DefData {
            location: Location::default(),
            kind: Def::Struct {
                name: struct_name,
                vis: Visibility::default(),
                fields: vec![],
                methods: vec![method],
            },
        });
        let sf_id = arena.source_files.alloc(SourceFileData {
            location: Location::default(),
            source: String::new(),
            defs: vec![struct_def],
            directives: vec![],
        });
        assert_eq!(arena.find_source_file_for_def(method), Some(sf_id));
    }

    #[test]
    fn get_node_source_returns_source_text() {
        let mut arena = AstArena::default();
        let source = "fn foo() {}".to_string();
        let loc = Location::new(0, 11, 1, 0, 1, 11);
        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: "foo".to_string(),
        });
        let body = arena.blocks.alloc(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![],
        });
        let def_id = arena.defs.alloc(DefData {
            location: loc,
            kind: Def::Function {
                name,
                vis: Visibility::default(),
                type_params: vec![],
                args: vec![],
                returns: None,
                body,
            },
        });
        arena.source_files.alloc(SourceFileData {
            location: Location::new(0, 11, 1, 0, 1, 11),
            source,
            defs: vec![def_id],
            directives: vec![],
        });
        assert_eq!(
            arena.get_node_source(NodeId::Def(def_id)),
            Some("fn foo() {}")
        );
    }

    #[test]
    fn get_node_source_returns_none_for_invalid_offsets() {
        let mut arena = AstArena::default();
        let loc = Location::new(100, 200, 1, 0, 1, 0);
        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: "x".to_string(),
        });
        let body = arena.blocks.alloc(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![],
        });
        let def_id = arena.defs.alloc(DefData {
            location: loc,
            kind: Def::Function {
                name,
                vis: Visibility::default(),
                type_params: vec![],
                args: vec![],
                returns: None,
                body,
            },
        });
        arena.source_files.alloc(SourceFileData {
            location: Location::default(),
            source: "short".to_string(),
            defs: vec![def_id],
            directives: vec![],
        });
        assert_eq!(arena.get_node_source(NodeId::Def(def_id)), None);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn get_node_source_fallback_without_parent_chain() {
        let mut arena = AstArena::default();
        let source = "fn foo() { return 42; }".to_string();
        let sf_loc = Location::new(0, source.len() as u32, 1, 0, 1, source.len() as u32);

        let name = arena.idents.alloc(Ident {
            location: Location::default(),
            name: "foo".to_string(),
        });
        let lit_loc = Location::new(18, 20, 1, 18, 1, 20);
        let lit = arena.exprs.alloc(ExprData {
            location: lit_loc,
            kind: Expr::NumberLiteral {
                value: "42".to_string(),
            },
        });
        let ret_stmt = arena.stmts.alloc(StmtData {
            location: Location::default(),
            kind: Stmt::Return { expr: lit },
        });
        let block = arena.blocks.alloc(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![ret_stmt],
        });
        let def_id = arena.defs.alloc(DefData {
            location: sf_loc,
            kind: Def::Function {
                name,
                vis: Visibility::default(),
                type_params: vec![],
                args: vec![],
                returns: None,
                body: block,
            },
        });
        arena.source_files.alloc(SourceFileData {
            location: sf_loc,
            source,
            defs: vec![def_id],
            directives: vec![],
        });

        assert_eq!(arena.get_node_source(NodeId::Expr(lit)), Some("42"));
    }
}
