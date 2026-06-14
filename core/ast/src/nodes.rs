//! AST node type definitions for the Inference compiler.
//!
//! This module defines the complete AST type hierarchy using typed arena indices
//! instead of `Arc<T>` pointers. Every node is stored in a typed `Vec` inside
//! `AstArena` and referenced by a lightweight `Copy` index (`ExprId`, `StmtId`, etc.).
//!
//! # Layout
//!
//! Each arena category has a wrapper struct that holds `location` + `kind`:
//!
//! ```text
//! ExprData { location: Location, kind: Expr }
//! StmtData { location: Location, kind: Stmt }
//! DefData  { location: Location, kind: Def  }
//! TypeData { location: Location, kind: TypeNode }
//! ```
//!
//! Blocks and identifiers are simpler and store their data inline.

use core::fmt;
use std::fmt::{Display, Formatter};

use crate::ids::{BlockId, DefId, ExprId, IdentId, StmtId, TypeId};

// ---------------------------------------------------------------------------
// Location
// ---------------------------------------------------------------------------

/// Source location information for AST nodes.
///
/// Stores byte offsets and line/column positions.
/// Source text should be retrieved from the `SourceFile` using the offset range.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Location {
    pub offset_start: u32,
    pub offset_end: u32,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl Location {
    #[must_use]
    pub fn new(
        offset_start: u32,
        offset_end: u32,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
    ) -> Self {
        Self {
            offset_start,
            offset_end,
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.start_line, self.start_column)
    }
}

/// The diagnostic file label for a source file's module path, or `None` for the
/// entry file (the one file with an empty module path).
///
/// In a multi-file program every diagnostic must name the file it belongs to,
/// because source locations are per-file-local in the merged arena: a bare
/// `line:col` from an imported file would otherwise be misread as the entry
/// file the user invoked. The label is the `::`-joined module path (e.g.
/// `lib::geom`), matching the spelling used elsewhere for the same file. The
/// entry file returns `None`: it is the file the user named, so naming it adds
/// only noise, and a single-file program's diagnostics stay a bare `line:col`.
///
/// This is the single source of truth for the spelling; every diagnostic channel
/// renders file identity through it so the channels stay consistent.
#[must_use]
pub fn file_label(module_path: &[String]) -> Option<String> {
    if module_path.is_empty() {
        None
    } else {
        Some(module_path.join("::"))
    }
}

// ---------------------------------------------------------------------------
// Shared enums (unchanged)
// ---------------------------------------------------------------------------

/// Visibility modifier for definitions.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum Visibility {
    #[default]
    Private,
    Public,
}

/// Unary operator kinds for prefix expressions.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum UnaryOperatorKind {
    /// Logical negation: `!expr`
    Not,
    /// Numeric negation: `-expr`
    Neg,
    /// Bitwise NOT: `~expr`
    BitNot,
}

/// Simple type kinds for primitive built-in types.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SimpleTypeKind {
    Unit,
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl SimpleTypeKind {
    /// Returns the canonical lowercase source-code representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            SimpleTypeKind::Unit => "unit",
            SimpleTypeKind::Bool => "bool",
            SimpleTypeKind::I8 => "i8",
            SimpleTypeKind::I16 => "i16",
            SimpleTypeKind::I32 => "i32",
            SimpleTypeKind::I64 => "i64",
            SimpleTypeKind::U8 => "u8",
            SimpleTypeKind::U16 => "u16",
            SimpleTypeKind::U32 => "u32",
            SimpleTypeKind::U64 => "u64",
        }
    }
}

/// Binary operator kinds for expressions.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum OperatorKind {
    Pow,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

// ---------------------------------------------------------------------------
// Wrapper structs (stored in arena Vecs)
// ---------------------------------------------------------------------------

/// Expression wrapper: `location` + `kind`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExprData {
    pub location: Location,
    pub kind: Expr,
}

/// Statement wrapper: `location` + `kind`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StmtData {
    pub location: Location,
    pub kind: Stmt,
}

/// Definition wrapper: `location` + `kind`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DefData {
    pub location: Location,
    pub kind: Def,
}

/// Type node wrapper: `location` + `kind`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TypeData {
    pub location: Location,
    pub kind: TypeNode,
}

/// A block of statements with a kind (regular, forall, exists, assume, unique).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BlockData {
    pub location: Location,
    pub block_kind: BlockKind,
    pub stmts: Vec<StmtId>,
}

/// An identifier (variable name, type name, etc.).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ident {
    pub location: Location,
    pub name: String,
}

/// Root AST node representing a parsed source file.
///
/// In a multi-file program every file carries a `module_path`: the segments of
/// its location relative to the source root (the entry file's directory),
/// e.g. `src/lib/arith.inf` ⇒ `["lib", "arith"]`. These segments are the file's
/// canonical namespace name, used to qualify its symbols across the rest of the
/// pipeline.
///
/// The **entry file is the one and only file with an empty `module_path`**.
/// Identity is positional, never by filename: an imported file literally named
/// `main.inf` still receives its real path segments, so the entry is unspoofable
/// by name. Single-file programs (including the string-based `parse`) have one
/// file, which is the entry, so its `module_path` is empty.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceFileData {
    pub location: Location,
    pub source: String,
    pub defs: Vec<DefId>,
    pub directives: Vec<Directive>,
    /// Source-root-relative path segments naming this file's namespace; empty
    /// for the entry file. See the type-level docs for the entry invariant.
    pub module_path: Vec<String>,
}

impl SourceFileData {
    /// Whether this is the program entry file (the one file with no module
    /// path). Exactly one file in an arena satisfies this.
    #[must_use]
    pub fn is_entry(&self) -> bool {
        self.module_path.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Block kind
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockKind {
    Regular,
    Forall,
    Exists,
    Assume,
    Unique,
}

// ---------------------------------------------------------------------------
// Directives
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Directive {
    Use(UseDirective),
}

/// A logical, platform-independent module reference.
///
/// Carries the identifier-path `segments` of a `from` clause (e.g. `crypto::sha256`
/// lowers to `["crypto", "sha256"]`). It is deliberately *not* a filesystem path:
/// the driver maps it to a concrete `.wasm` file at resolve time, so source stays
/// portable across operating systems (no `./`, no OS separators).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ModuleRef {
    pub segments: Vec<IdentId>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UseDirective {
    pub location: Location,
    /// Visibility of the import. `Public` (`pub use …`) re-exports the imported
    /// namespace or items from the importing file; `Private` (the default) keeps
    /// the import local to that file.
    pub vis: Visibility,
    pub imported_types: Vec<IdentId>,
    pub segments: Vec<IdentId>,
    /// Whether the directive carried a `{ … }` item list. An item import always
    /// sets this; a brace-free file import (`use a::b;`) does not. It lets the
    /// type checker tell an empty item list (`use a::b::{};`) — which is
    /// parseable but meaningless — apart from a file import, since both leave
    /// `imported_types` empty.
    pub braced: bool,
    /// Logical module reference of a `from` clause, if present. The string-literal
    /// path form (`from "./sort.wasm"`) was removed in favour of this portable form.
    pub from: Option<ModuleRef>,
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Def {
    Function {
        name: IdentId,
        vis: Visibility,
        type_params: Vec<IdentId>,
        args: Vec<ArgData>,
        returns: Option<TypeId>,
        body: BlockId,
    },
    ExternFunction {
        name: IdentId,
        vis: Visibility,
        args: Vec<ArgData>,
        returns: Option<TypeId>,
    },
    Struct {
        name: IdentId,
        vis: Visibility,
        fields: Vec<Field>,
        methods: Vec<DefId>,
    },
    Enum {
        name: IdentId,
        vis: Visibility,
        variants: Vec<IdentId>,
    },
    Spec {
        name: IdentId,
        vis: Visibility,
        defs: Vec<DefId>,
    },
    Constant {
        name: IdentId,
        vis: Visibility,
        ty: TypeId,
        value: ExprId,
    },
    TypeAlias {
        name: IdentId,
        vis: Visibility,
        ty: TypeId,
    },
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Stmt {
    Block(BlockId),
    Expr(ExprId),
    Assign {
        left: ExprId,
        right: ExprId,
    },
    Return {
        expr: ExprId,
    },
    Loop {
        condition: Option<ExprId>,
        body: BlockId,
    },
    Break,
    If {
        condition: ExprId,
        then_block: BlockId,
        else_block: Option<BlockId>,
    },
    VarDef {
        name: IdentId,
        ty: TypeId,
        value: Option<ExprId>,
        is_mut: bool,
    },
    TypeDef {
        name: IdentId,
        ty: TypeId,
    },
    Assert {
        expr: ExprId,
    },
    ConstDef(DefId),
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Expr {
    Binary {
        left: ExprId,
        right: ExprId,
        op: OperatorKind,
    },
    PrefixUnary {
        expr: ExprId,
        op: UnaryOperatorKind,
    },
    Parenthesized {
        expr: ExprId,
    },
    FunctionCall {
        function: ExprId,
        type_params: Vec<IdentId>,
        args: Vec<(Option<IdentId>, ExprId)>,
    },
    ArrayIndexAccess {
        array: ExprId,
        index: ExprId,
    },
    MemberAccess {
        expr: ExprId,
        name: IdentId,
    },
    TypeMemberAccess {
        expr: ExprId,
        name: IdentId,
    },
    StructLiteral {
        name: IdentId,
        fields: Vec<(IdentId, ExprId)>,
    },
    Identifier(IdentId),
    NumberLiteral {
        value: String,
    },
    BoolLiteral {
        value: bool,
    },
    StringLiteral {
        value: String,
    },
    ArrayLiteral {
        elements: Vec<ExprId>,
    },
    UnitLiteral,
    Uzumaki,
    /// A type in expression position (e.g., type annotations stored as expressions).
    Type(TypeId),
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TypeNode {
    Simple(SimpleTypeKind),
    Array {
        element: TypeId,
        size: ExprId,
    },
    Generic {
        base: IdentId,
        params: Vec<IdentId>,
    },
    Function {
        params: Vec<TypeId>,
        ret: Option<TypeId>,
    },
    QualifiedName {
        qualifier: IdentId,
        name: IdentId,
    },
    /// A `::`-qualified type reference such as `geo::Level` or
    /// `lib::geom::Point`. `qualifier` holds the leading namespace segments in
    /// source order (`["lib", "geom"]`); `name` is the leaf type. The qualifier
    /// is always non-empty — a bare type lowers to [`TypeNode::Custom`] instead.
    /// Carrying every segment (not just one) is what lets a multi-hop path
    /// resolve through the file-namespace chain rather than dropping its prefix.
    Qualified {
        qualifier: Vec<IdentId>,
        name: IdentId,
    },
    Custom(IdentId),
}

// ---------------------------------------------------------------------------
// Inline helper structs (not arena-allocated)
// ---------------------------------------------------------------------------

/// A function/method argument definition.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ArgData {
    pub location: Location,
    pub kind: ArgKind,
}

/// The kind of a function argument.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ArgKind {
    /// Named argument: `name: Type` or `mut name: Type`
    Named {
        name: IdentId,
        ty: TypeId,
        is_mut: bool,
    },
    /// Self reference: `self` or `mut self`
    SelfRef { is_mut: bool },
    /// Ignored argument: `_: Type`
    Ignored { ty: TypeId },
    /// Type-only argument (positional type)
    TypeOnly(TypeId),
}

/// A struct field definition.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Field {
    pub name: IdentId,
    pub ty: TypeId,
}

// ---------------------------------------------------------------------------
// Convenience impls
// ---------------------------------------------------------------------------

impl TypeNode {
    /// Returns `true` if this type is the unit type `()`.
    #[must_use]
    pub fn is_unit_type(&self) -> bool {
        matches!(self, TypeNode::Simple(SimpleTypeKind::Unit))
    }

    /// The `::`-joined source path of a qualified type reference
    /// (`lib::geom::Point`), or `None` for any non-qualified type.
    ///
    /// Centralizes the segment ordering — leading namespace qualifiers, then the
    /// leaf name — so every consumer (type-info construction, validation,
    /// dependency collection, code generation) reads the same canonical string
    /// rather than re-implementing the chain/join.
    #[must_use = "the path is the return value"]
    pub fn qualified_path(&self, arena: &crate::arena::AstArena) -> Option<String> {
        match self {
            TypeNode::Qualified { qualifier, name } => Some(
                qualifier
                    .iter()
                    .chain(std::iter::once(name))
                    .map(|id| arena[*id].name.as_str())
                    .collect::<Vec<_>>()
                    .join("::"),
            ),
            TypeNode::QualifiedName { qualifier, name } => {
                Some(format!("{}::{}", arena[*qualifier].name, arena[*name].name))
            }
            _ => None,
        }
    }

    /// The `::`-separated segments of a qualified type reference as owned strings
    /// (`["lib", "geom", "Point"]`), or `None` for any non-qualified type.
    ///
    /// The sibling of [`Self::qualified_path`] for callers that resolve the path
    /// segment by segment rather than as a single string.
    #[must_use = "the segments are the return value"]
    pub fn qualified_segments(&self, arena: &crate::arena::AstArena) -> Option<Vec<String>> {
        match self {
            TypeNode::Qualified { qualifier, name } => Some(
                qualifier
                    .iter()
                    .chain(std::iter::once(name))
                    .map(|id| arena[*id].name.clone())
                    .collect(),
            ),
            TypeNode::QualifiedName { qualifier, name } => {
                Some(vec![arena[*qualifier].name.clone(), arena[*name].name.clone()])
            }
            _ => None,
        }
    }
}

impl BlockKind {
    /// Returns `true` for non-deterministic block kinds (forall, exists, assume, unique).
    #[must_use]
    pub fn is_non_det(&self) -> bool {
        !matches!(self, BlockKind::Regular)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::AstArena;

    fn ident(arena: &mut AstArena, name: &str) -> IdentId {
        arena.idents.alloc(Ident {
            location: Location::default(),
            name: name.to_string(),
        })
    }

    #[test]
    fn file_label_is_none_for_entry_file() {
        assert_eq!(file_label(&[]), None);
    }

    #[test]
    fn file_label_joins_module_path_with_double_colon() {
        assert_eq!(
            file_label(&["lib".to_string(), "geom".to_string()]),
            Some("lib::geom".to_string())
        );
        assert_eq!(file_label(&["math".to_string()]), Some("math".to_string()));
    }

    #[test]
    fn file_label_joins_deep_module_path() {
        assert_eq!(
            file_label(&["a".to_string(), "b".to_string(), "c".to_string()]),
            Some("a::b::c".to_string())
        );
    }

    #[test]
    fn qualified_path_joins_all_segments() {
        let mut arena = AstArena::default();
        let lib = ident(&mut arena, "lib");
        let geom = ident(&mut arena, "geom");
        let point = ident(&mut arena, "Point");
        let node = TypeNode::Qualified {
            qualifier: vec![lib, geom],
            name: point,
        };
        assert_eq!(node.qualified_path(&arena).as_deref(), Some("lib::geom::Point"));
        assert_eq!(
            node.qualified_segments(&arena),
            Some(vec!["lib".to_string(), "geom".to_string(), "Point".to_string()])
        );
    }

    #[test]
    fn qualified_path_single_qualifier() {
        let mut arena = AstArena::default();
        let geo = ident(&mut arena, "geo");
        let level = ident(&mut arena, "Level");
        let node = TypeNode::Qualified {
            qualifier: vec![geo],
            name: level,
        };
        assert_eq!(node.qualified_path(&arena).as_deref(), Some("geo::Level"));
    }

    #[test]
    fn qualified_path_none_for_non_qualified() {
        let mut arena = AstArena::default();
        let custom = ident(&mut arena, "Point");
        assert!(TypeNode::Custom(custom).qualified_path(&arena).is_none());
        assert!(
            TypeNode::Simple(SimpleTypeKind::I32)
                .qualified_segments(&arena)
                .is_none()
        );
    }
}
