//! Typed index types for arena-allocated AST nodes.
//!
//! Each category of AST node has its own index type alias to prevent mixing up
//! expression indices with statement indices at compile time. All indices
//! are `Copy` + 4 bytes, matching the footprint of a plain `u32`.

use crate::la_arena::{Idx, RawIdx};
use crate::nodes::{BlockData, DefData, ExprData, Ident, SourceFileData, StmtData, TypeData};

pub type SourceFileId = Idx<SourceFileData>;
pub type DefId = Idx<DefData>;
pub type StmtId = Idx<StmtData>;
pub type ExprId = Idx<ExprData>;
pub type TypeId = Idx<TypeData>;
pub type BlockId = Idx<BlockData>;
pub type IdentId = Idx<Ident>;

/// Convenience: create a typed index from a raw u32 (for tests and iteration).
#[must_use]
pub fn idx_from_u32<T>(raw: u32) -> Idx<T> {
    Idx::from_raw(RawIdx::from_u32(raw))
}

/// A type-erased node identifier that can refer to any arena category.
///
/// Used for type annotation storage where heterogeneous node references
/// are needed.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum NodeId {
    SourceFile(SourceFileId),
    Def(DefId),
    Stmt(StmtId),
    Expr(ExprId),
    Type(TypeId),
    Block(BlockId),
    Ident(IdentId),
}
