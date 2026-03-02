//! Vec-based typed arena storage for the AST.
//!
//! `AstArena` stores each node category in its own `Vec`, indexed by the
//! corresponding typed ID (`ExprId`, `StmtId`, etc.). This provides:
//!
//! - O(1) index-based lookup (vs. hash map in the old design)
//! - Type safety: you cannot accidentally index expressions with a statement ID
//! - `Send + Sync`: no `RefCell`, no interior mutability
//! - Cache-friendly sequential storage

use crate::ids::*;
use crate::nodes::*;
use rustc_hash::FxHashMap;

/// Central storage for all AST nodes.
///
/// Each node category has its own `Vec`. Typed IDs (`ExprId`, `StmtId`, etc.)
/// index into the corresponding `Vec`. Parent/child maps use `NodeId` for
/// heterogeneous traversal.
#[derive(Default, Clone)]
pub struct AstArena {
    pub(crate) source_files: Vec<SourceFileData>,
    pub(crate) defs: Vec<DefData>,
    pub(crate) stmts: Vec<StmtData>,
    pub(crate) exprs: Vec<ExprData>,
    pub(crate) types: Vec<TypeData>,
    pub(crate) blocks: Vec<BlockData>,
    pub(crate) idents: Vec<Ident>,
    pub(crate) parent_map: FxHashMap<NodeId, NodeId>,
    pub(crate) children_map: FxHashMap<NodeId, Vec<NodeId>>,
}

// Compile-time assertion: AstArena is Send + Sync.
const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    fn check() {
        assert_send_sync::<AstArena>();
    }
};

// ---------------------------------------------------------------------------
// Index impls
// ---------------------------------------------------------------------------

impl std::ops::Index<SourceFileId> for AstArena {
    type Output = SourceFileData;
    fn index(&self, id: SourceFileId) -> &SourceFileData {
        &self.source_files[id.index()]
    }
}

impl std::ops::Index<DefId> for AstArena {
    type Output = DefData;
    fn index(&self, id: DefId) -> &DefData {
        &self.defs[id.index()]
    }
}

impl std::ops::Index<StmtId> for AstArena {
    type Output = StmtData;
    fn index(&self, id: StmtId) -> &StmtData {
        &self.stmts[id.index()]
    }
}

impl std::ops::Index<ExprId> for AstArena {
    type Output = ExprData;
    fn index(&self, id: ExprId) -> &ExprData {
        &self.exprs[id.index()]
    }
}

impl std::ops::Index<TypeId> for AstArena {
    type Output = TypeData;
    fn index(&self, id: TypeId) -> &TypeData {
        &self.types[id.index()]
    }
}

impl std::ops::Index<BlockId> for AstArena {
    type Output = BlockData;
    fn index(&self, id: BlockId) -> &BlockData {
        &self.blocks[id.index()]
    }
}

impl std::ops::Index<IdentId> for AstArena {
    type Output = Ident;
    fn index(&self, id: IdentId) -> &Ident {
        &self.idents[id.index()]
    }
}

// ---------------------------------------------------------------------------
// Allocators
// ---------------------------------------------------------------------------

impl AstArena {
    #[allow(clippy::cast_possible_truncation)]
    pub fn alloc_source_file(&mut self, data: SourceFileData) -> SourceFileId {
        let id = SourceFileId::from_raw(self.source_files.len() as u32);
        self.source_files.push(data);
        id
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn alloc_def(&mut self, data: DefData) -> DefId {
        let id = DefId::from_raw(self.defs.len() as u32);
        self.defs.push(data);
        id
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn alloc_stmt(&mut self, data: StmtData) -> StmtId {
        let id = StmtId::from_raw(self.stmts.len() as u32);
        self.stmts.push(data);
        id
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn alloc_expr(&mut self, data: ExprData) -> ExprId {
        let id = ExprId::from_raw(self.exprs.len() as u32);
        self.exprs.push(data);
        id
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn alloc_type(&mut self, data: TypeData) -> TypeId {
        let id = TypeId::from_raw(self.types.len() as u32);
        self.types.push(data);
        id
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn alloc_block(&mut self, data: BlockData) -> BlockId {
        let id = BlockId::from_raw(self.blocks.len() as u32);
        self.blocks.push(data);
        id
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn alloc_ident(&mut self, data: Ident) -> IdentId {
        let id = IdentId::from_raw(self.idents.len() as u32);
        self.idents.push(data);
        id
    }
}

// ---------------------------------------------------------------------------
// Parent/child recording
// ---------------------------------------------------------------------------

impl AstArena {
    /// Records a parent-child relationship between two nodes.
    pub fn record_parent(&mut self, child: NodeId, parent: NodeId) {
        self.parent_map.insert(child, parent);
        self.children_map.entry(parent).or_default().push(child);
    }

    /// Returns the parent node ID, or `None` for root nodes.
    #[must_use]
    pub fn find_parent(&self, id: NodeId) -> Option<NodeId> {
        self.parent_map.get(&id).copied()
    }

    /// Returns the children of a node, or an empty slice if none.
    #[must_use]
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.children_map
            .get(&id)
            .map_or(&[], |v| v.as_slice())
    }
}

// ---------------------------------------------------------------------------
// Source text retrieval
// ---------------------------------------------------------------------------

impl AstArena {
    /// Returns the source location of any node, or `None` if the ID is out of range.
    #[must_use]
    pub fn node_location(&self, node_id: NodeId) -> Option<Location> {
        match node_id {
            NodeId::SourceFile(id) => self.source_files.get(id.index()).map(|n| n.location),
            NodeId::Def(id) => self.defs.get(id.index()).map(|n| n.location),
            NodeId::Stmt(id) => self.stmts.get(id.index()).map(|n| n.location),
            NodeId::Expr(id) => self.exprs.get(id.index()).map(|n| n.location),
            NodeId::Type(id) => self.types.get(id.index()).map(|n| n.location),
            NodeId::Block(id) => self.blocks.get(id.index()).map(|n| n.location),
            NodeId::Ident(id) => self.idents.get(id.index()).map(|n| n.location),
        }
    }

    /// Finds which source file contains the given definition.
    ///
    /// Searches all source files' def lists, including nested defs inside
    /// structs, specs, and modules.
    #[must_use]
    pub fn find_source_file_for_def(&self, target: DefId) -> Option<SourceFileId> {
        for (idx, sf) in self.source_files.iter().enumerate() {
            if self.def_in_list(target, &sf.defs) {
                #[allow(clippy::cast_possible_truncation)]
                return Some(SourceFileId::from_raw(idx as u32));
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
    /// `find_source_file_for_def`. For other nodes it walks the parent chain
    /// looking for a `Def` or `SourceFile`, falling back to byte-offset matching.
    #[must_use]
    pub fn find_source_file_for_node(&self, node_id: NodeId) -> Option<SourceFileId> {
        match node_id {
            NodeId::SourceFile(id) => Some(id),
            NodeId::Def(def_id) => self.find_source_file_for_def(def_id),
            _ => {
                let mut current = node_id;
                loop {
                    match current {
                        NodeId::SourceFile(id) => return Some(id),
                        NodeId::Def(def_id) => {
                            return self.find_source_file_for_def(def_id);
                        }
                        _ => match self.find_parent(current) {
                            Some(parent) => current = parent,
                            None => break,
                        },
                    }
                }
                let location = self.node_location(node_id)?;
                self.find_source_file_by_offset(location)
            }
        }
    }

    fn find_source_file_by_offset(&self, location: Location) -> Option<SourceFileId> {
        let end = location.offset_end as usize;
        #[allow(clippy::cast_possible_truncation)]
        for (idx, sf) in self.source_files.iter().enumerate() {
            if end <= sf.source.len() {
                return Some(SourceFileId::from_raw(idx as u32));
            }
        }
        None
    }

    /// Returns the source text of a node by slicing its source file.
    ///
    /// Returns `None` if the node ID is invalid, the source file cannot be
    /// determined, or the byte offsets fall outside the source text.
    #[must_use]
    pub fn get_node_source(&self, node_id: NodeId) -> Option<&str> {
        let location = self.node_location(node_id)?;
        let start = location.offset_start as usize;
        let end = location.offset_end as usize;
        if start > end {
            return None;
        }
        let sf_id = self.find_source_file_for_node(node_id)?;
        self.source_files[sf_id.index()].source.get(start..end)
    }
}

// ---------------------------------------------------------------------------
// Query methods
// ---------------------------------------------------------------------------

impl AstArena {
    /// Returns all source file data entries.
    #[must_use]
    pub fn source_files(&self) -> &[SourceFileData] {
        &self.source_files
    }

    /// Iterates over all source file IDs.
    pub fn source_file_ids(&self) -> impl Iterator<Item = SourceFileId> {
        #[allow(clippy::cast_possible_truncation)]
        (0..self.source_files.len() as u32).map(SourceFileId::from_raw)
    }

    /// Returns all definition IDs that are functions across all source files.
    #[must_use]
    pub fn function_def_ids(&self) -> Vec<DefId> {
        let mut result = Vec::new();
        for sf in &self.source_files {
            for &def_id in &sf.defs {
                if matches!(self.defs[def_id.index()].kind, Def::Function { .. }) {
                    result.push(def_id);
                }
            }
        }
        result
    }

    /// Returns the name string of a definition (function, struct, etc.).
    #[must_use]
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
    #[must_use]
    pub fn ident_name(&self, id: IdentId) -> &str {
        &self[id].name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_and_index_expr() {
        let mut arena = AstArena::default();
        let id = arena.alloc_expr(ExprData {
            location: Location::default(),
            kind: Expr::NumberLiteral {
                value: "42".to_string(),
            },
        });
        assert_eq!(id.raw(), 0);
        assert!(matches!(arena[id].kind, Expr::NumberLiteral { .. }));
    }

    #[test]
    fn alloc_and_index_ident() {
        let mut arena = AstArena::default();
        let id = arena.alloc_ident(Ident {
            location: Location::default(),
            name: "foo".to_string(),
        });
        assert_eq!(arena[id].name, "foo");
    }

    #[test]
    fn parent_child_recording() {
        let mut arena = AstArena::default();
        let parent = NodeId::Block(BlockId::from_raw(0));
        let child = NodeId::Stmt(StmtId::from_raw(0));
        arena.record_parent(child, parent);

        assert_eq!(arena.find_parent(child), Some(parent));
        assert_eq!(arena.children(parent), &[child]);
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
        let id = arena.alloc_expr(ExprData {
            location: loc,
            kind: Expr::NumberLiteral {
                value: "42".to_string(),
            },
        });
        assert_eq!(arena.node_location(NodeId::Expr(id)), Some(loc));
    }

    #[test]
    fn node_location_returns_none_for_invalid_id() {
        let arena = AstArena::default();
        assert_eq!(
            arena.node_location(NodeId::Expr(ExprId::from_raw(999))),
            None
        );
    }

    #[test]
    fn find_source_file_for_def_finds_top_level() {
        let mut arena = AstArena::default();
        let name = arena.alloc_ident(Ident {
            location: Location::default(),
            name: "foo".to_string(),
        });
        let body = arena.alloc_block(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![],
        });
        let def_id = arena.alloc_def(DefData {
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
        let sf_id = arena.alloc_source_file(SourceFileData {
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
        let name = arena.alloc_ident(Ident {
            location: Location::default(),
            name: "m".to_string(),
        });
        let body = arena.alloc_block(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![],
        });
        let method = arena.alloc_def(DefData {
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
        let struct_name = arena.alloc_ident(Ident {
            location: Location::default(),
            name: "S".to_string(),
        });
        let struct_def = arena.alloc_def(DefData {
            location: Location::default(),
            kind: Def::Struct {
                name: struct_name,
                vis: Visibility::default(),
                fields: vec![],
                methods: vec![method],
            },
        });
        let sf_id = arena.alloc_source_file(SourceFileData {
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
        let name = arena.alloc_ident(Ident {
            location: Location::default(),
            name: "foo".to_string(),
        });
        let body = arena.alloc_block(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![],
        });
        let def_id = arena.alloc_def(DefData {
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
        arena.alloc_source_file(SourceFileData {
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
        let name = arena.alloc_ident(Ident {
            location: Location::default(),
            name: "x".to_string(),
        });
        let body = arena.alloc_block(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![],
        });
        let def_id = arena.alloc_def(DefData {
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
        arena.alloc_source_file(SourceFileData {
            location: Location::default(),
            source: "short".to_string(),
            defs: vec![def_id],
            directives: vec![],
        });
        assert_eq!(arena.get_node_source(NodeId::Def(def_id)), None);
    }

    #[test]
    fn get_node_source_with_parent_chain() {
        let mut arena = AstArena::default();
        let source = "fn foo() { return 42; }".to_string();
        let sf_loc = Location::new(0, source.len() as u32, 1, 0, 1, source.len() as u32);

        let name = arena.alloc_ident(Ident {
            location: Location::default(),
            name: "foo".to_string(),
        });
        let lit_loc = Location::new(18, 20, 1, 18, 1, 20);
        let lit = arena.alloc_expr(ExprData {
            location: lit_loc,
            kind: Expr::NumberLiteral {
                value: "42".to_string(),
            },
        });
        let ret_loc = Location::new(11, 21, 1, 11, 1, 21);
        let ret_stmt = arena.alloc_stmt(StmtData {
            location: ret_loc,
            kind: Stmt::Return { expr: lit },
        });
        let block = arena.alloc_block(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![ret_stmt],
        });
        let def_id = arena.alloc_def(DefData {
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
        arena.alloc_source_file(SourceFileData {
            location: sf_loc,
            source,
            defs: vec![def_id],
            directives: vec![],
        });

        arena.record_parent(NodeId::Expr(lit), NodeId::Stmt(ret_stmt));
        arena.record_parent(NodeId::Stmt(ret_stmt), NodeId::Block(block));
        arena.record_parent(NodeId::Block(block), NodeId::Def(def_id));

        assert_eq!(arena.get_node_source(NodeId::Expr(lit)), Some("42"));
        assert_eq!(
            arena.get_node_source(NodeId::Stmt(ret_stmt)),
            Some("return 42;")
        );
    }

    #[test]
    fn get_node_source_fallback_without_parent_chain() {
        let mut arena = AstArena::default();
        let source = "fn foo() { return 42; }".to_string();
        let sf_loc = Location::new(0, source.len() as u32, 1, 0, 1, source.len() as u32);

        let name = arena.alloc_ident(Ident {
            location: Location::default(),
            name: "foo".to_string(),
        });
        let lit_loc = Location::new(18, 20, 1, 18, 1, 20);
        let lit = arena.alloc_expr(ExprData {
            location: lit_loc,
            kind: Expr::NumberLiteral {
                value: "42".to_string(),
            },
        });
        let ret_stmt = arena.alloc_stmt(StmtData {
            location: Location::default(),
            kind: Stmt::Return { expr: lit },
        });
        let block = arena.alloc_block(BlockData {
            location: Location::default(),
            block_kind: BlockKind::Regular,
            stmts: vec![ret_stmt],
        });
        let def_id = arena.alloc_def(DefData {
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
        arena.alloc_source_file(SourceFileData {
            location: sf_loc,
            source,
            defs: vec![def_id],
            directives: vec![],
        });

        assert_eq!(arena.get_node_source(NodeId::Expr(lit)), Some("42"));
    }
}
