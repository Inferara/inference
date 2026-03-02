//! Typed index types for arena-allocated AST nodes.
//!
//! Each category of AST node has its own index type to prevent mixing up
//! expression indices with statement indices at compile time. All indices
//! are `Copy` + 4 bytes, matching the footprint of a plain `u32`.

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub struct $name(u32);

        impl $name {
            #[inline]
            pub fn from_raw(raw: u32) -> Self { Self(raw) }

            #[inline]
            pub fn raw(self) -> u32 { self.0 }

            #[inline]
            pub(crate) fn index(self) -> usize { self.0 as usize }
        }
    };
}

define_id!(
    /// Index into `AstArena::source_files`.
    SourceFileId
);
define_id!(
    /// Index into `AstArena::defs`.
    DefId
);
define_id!(
    /// Index into `AstArena::stmts`.
    StmtId
);
define_id!(
    /// Index into `AstArena::exprs`.
    ExprId
);
define_id!(
    /// Index into `AstArena::types`.
    TypeId
);
define_id!(
    /// Index into `AstArena::blocks`.
    BlockId
);
define_id!(
    /// Index into `AstArena::idents`.
    IdentId
);

/// A type-erased node identifier that can refer to any arena category.
///
/// Used for the parent/children maps where heterogeneous node references
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
