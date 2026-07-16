//! Jump-to-definition: resolves the identifier at a position to its declaration.
//!
//! Definitions may live in an imported file, so a [`NavigationTarget`] carries
//! the target file's real path and ranges in that file's own coordinates
//! (offsets are per-file-local in the merged arena).

use std::path::PathBuf;

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, IdentId, NodeId, SourceFileId, StmtId, TypeId};
use inference_ast::nodes::{ArgKind, Def, Expr, Location, Stmt, TypeNode};
use inference_ide_db::{FileAnalysis, NodeHit, TextRange};
use inference_type_checker::type_info::TypeInfoKind;

use crate::syntax::{
    def_is_public, def_name_ident, enclosing_function, find_def_by_name, find_method,
    imported_module_paths, in_scope_locals, is_call_callee, resolve_qualified_module, text_range,
};

/// A place a definition lives: the file's path, the whole declaration's range,
/// and the narrower range of just its name (what an editor highlights).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationTarget {
    pub path: PathBuf,
    pub full_range: TextRange,
    pub focus_range: TextRange,
}

/// Resolves the definition of the identifier at byte `offset` in the entry file.
///
/// Returns the target(s), or `None` when the offset is not on a resolvable
/// identifier. A single definition is the norm; the `Vec` leaves room for a
/// future ambiguous case without changing the signature.
#[must_use]
pub(crate) fn goto_definition(file: &FileAnalysis, offset: u32) -> Option<Vec<NavigationTarget>> {
    let entry = file.source_file_id(&[])?;
    let hit = file.hit_test(entry, offset)?;
    let NodeId::Ident(ident) = hit.node else {
        return None;
    };
    let grandparent = hit.ancestors.iter().rev().nth(1).copied();
    let target = match hit.ancestors.last().copied()? {
        NodeId::Expr(expr) => goto_in_expr(file, entry, &hit, expr, ident, grandparent, offset)?,
        NodeId::Type(type_id) => goto_in_type(file, entry, type_id, ident)?,
        NodeId::Def(def) => goto_in_def(file, entry, def, ident)?,
        NodeId::Stmt(stmt) => goto_in_stmt(file, entry, stmt, ident)?,
        _ => return None,
    };
    Some(vec![target])
}

fn path_of(file: &FileAnalysis, sfid: SourceFileId) -> Option<PathBuf> {
    let module_path = file.arena().source_file_module_path(sfid)?;
    Some(file.file(module_path)?.path().to_path_buf())
}

fn nav_for_def(file: &FileAnalysis, sfid: SourceFileId, def: DefId) -> Option<NavigationTarget> {
    let arena = file.arena();
    Some(NavigationTarget {
        path: path_of(file, sfid)?,
        full_range: text_range(arena[def].location),
        focus_range: text_range(arena[def_name_ident(arena, def)].location),
    })
}

fn nav_at_ident(
    file: &FileAnalysis,
    sfid: SourceFileId,
    full: Location,
    focus: IdentId,
) -> Option<NavigationTarget> {
    Some(NavigationTarget {
        path: path_of(file, sfid)?,
        full_range: text_range(full),
        focus_range: text_range(file.arena()[focus].location),
    })
}

fn goto_in_expr(
    file: &FileAnalysis,
    entry: SourceFileId,
    hit: &NodeHit,
    expr: ExprId,
    ident: IdentId,
    grandparent: Option<NodeId>,
    offset: u32,
) -> Option<NavigationTarget> {
    let arena = file.arena();
    match &arena[expr].kind {
        Expr::Identifier(_) => {
            if is_call_callee(arena, grandparent, expr) {
                return goto_call(file, expr);
            }
            let name = arena.ident_name(ident);
            if let Some(local) = resolve_local(file, hit, name, offset) {
                return nav_at_ident(file, entry, arena[local].location, local);
            }
            goto_value_def(file, entry, name)
        }
        // A member access is either a method call (its access expression is the
        // function of an enclosing call) or a plain field read.
        Expr::MemberAccess { expr: receiver, .. } => {
            if is_call_callee(arena, grandparent, expr) {
                goto_call(file, expr)
            } else {
                goto_field(file, *receiver, ident)
            }
        }
        // A `Type::member` is a `::`-qualified/associated call when it is a
        // call's function, an enum variant when its access type is an enum, and
        // otherwise a module-qualified item (a constant such as `lib::MAX`).
        Expr::TypeMemberAccess { expr: base, .. } => {
            if is_call_callee(arena, grandparent, expr) {
                goto_call(file, expr)
            } else {
                goto_variant(file, expr, ident).or_else(|| {
                    let qualifier = access_qualifier_segments(arena, *base)?;
                    goto_module_member(file, entry, &qualifier, ident)
                })
            }
        }
        Expr::StructLiteral { name, .. } => {
            if *name == ident {
                let (sfid, def) = resolve_type_def(file, entry, arena.ident_name(ident))?;
                return nav_for_def(file, sfid, def);
            }
            let struct_name = arena.ident_name(*name);
            let (sfid, struct_def) = resolve_type_def(file, entry, struct_name)?;
            goto_field_in_struct(file, sfid, struct_def, arena.ident_name(ident))
        }
        _ => None,
    }
}

/// The declaration of a local (param or `let`) named `name` visible at `offset`,
/// resolved syntactically: params first, then the in-scope `let` binding whose
/// name matches. Only bindings whose enclosing block encloses the use site are
/// considered (via [`in_scope_locals`]), so a binding in an already-closed
/// sibling block does not resolve here. Inference forbids shadowing, so a name
/// resolves to at most one declaration; the nearest before the use is returned.
fn resolve_local(file: &FileAnalysis, hit: &NodeHit, name: &str, offset: u32) -> Option<IdentId> {
    let arena = file.arena();
    let function = enclosing_function(arena, hit)?;
    if let Def::Function { args, .. } = &arena[function].kind {
        for arg in args {
            if let ArgKind::Named { name: param, .. } = &arg.kind
                && arena.ident_name(*param) == name
            {
                return Some(*param);
            }
        }
    }

    in_scope_locals(arena, hit, offset)
        .into_iter()
        .filter_map(|stmt| match &arena[stmt].kind {
            Stmt::VarDef { name: declared, .. } if arena.ident_name(*declared) == name => {
                Some(*declared)
            }
            _ => None,
        })
        .max_by_key(|&declared| arena[declared].location.offset_end)
}

/// The definition a call resolves to (free function or method), via the checker's
/// recorded call target — so a cross-file or re-exported call lands in the right
/// file.
fn goto_call(file: &FileAnalysis, callee: ExprId) -> Option<NavigationTarget> {
    let arena = file.arena();
    let target = file.typed_context().call_target(callee)?;
    let sfid = file.source_file_id(&target.module_path)?;
    let def = match &target.receiver_struct {
        Some(struct_name) => {
            let struct_def = find_def_by_name(arena, sfid, struct_name)?;
            find_method(arena, struct_def, &target.name)?
        }
        None => find_def_by_name(arena, sfid, &target.name)?,
    };
    nav_for_def(file, sfid, def)
}

/// A top-level definition (function or constant) used as a value: same file
/// first, then a `pub` definition of a directly-imported module.
fn goto_value_def(
    file: &FileAnalysis,
    entry: SourceFileId,
    name: &str,
) -> Option<NavigationTarget> {
    let arena = file.arena();
    if let Some(def) = find_def_by_name(arena, entry, name) {
        return nav_for_def(file, entry, def);
    }
    for module_path in imported_module_paths(arena, entry) {
        let Some(sfid) = file.source_file_id(&module_path) else {
            continue;
        };
        if let Some(def) = find_def_by_name(arena, sfid, name)
            && def_is_public(arena, def)
        {
            return nav_for_def(file, sfid, def);
        }
    }
    None
}

/// The field declaration named like `field` on the struct value `receiver`.
fn goto_field(file: &FileAnalysis, receiver: ExprId, field: IdentId) -> Option<NavigationTarget> {
    let arena = file.arena();
    let ctx = file.typed_context();
    let type_info = ctx.get_node_typeinfo(NodeId::Expr(receiver))?;
    let TypeInfoKind::Struct(bare, key) = &type_info.kind else {
        return None;
    };
    let module_path = ctx.module_path_of_struct_key(key)?;
    let sfid = file.source_file_id(&module_path)?;
    let struct_def = find_def_by_name(arena, sfid, bare)?;
    goto_field_in_struct(file, sfid, struct_def, arena.ident_name(field))
}

fn goto_field_in_struct(
    file: &FileAnalysis,
    sfid: SourceFileId,
    struct_def: DefId,
    field_name: &str,
) -> Option<NavigationTarget> {
    let arena = file.arena();
    let Def::Struct { fields, .. } = &arena[struct_def].kind else {
        return None;
    };
    let field = fields
        .iter()
        .find(|field| arena.ident_name(field.name) == field_name)?;
    nav_at_ident(file, sfid, arena[field.name].location, field.name)
}

/// The variant declaration named like `variant` on the enum value `access`
/// (`Enum::Variant`).
fn goto_variant(file: &FileAnalysis, access: ExprId, variant: IdentId) -> Option<NavigationTarget> {
    let arena = file.arena();
    let ctx = file.typed_context();
    let type_info = ctx.get_node_typeinfo(NodeId::Expr(access))?;
    let TypeInfoKind::Enum(bare, key) = &type_info.kind else {
        return None;
    };
    let info = ctx.lookup_enum(key)?;
    let sfid = file.source_file_id(&ctx.module_path_of_scope(info.definition_scope_id))?;
    let enum_def = find_def_by_name(arena, sfid, bare)?;
    let Def::Enum { variants, .. } = &arena[enum_def].kind else {
        return None;
    };
    let target = variants
        .iter()
        .copied()
        .find(|&candidate| arena.ident_name(candidate) == arena.ident_name(variant))?;
    nav_at_ident(file, sfid, arena[target].location, target)
}

/// Resolves a bare type name (struct or enum) to its defining file and def.
fn resolve_type_def(
    file: &FileAnalysis,
    entry: SourceFileId,
    name: &str,
) -> Option<(SourceFileId, DefId)> {
    let arena = file.arena();
    let ctx = file.typed_context();
    if let Some(module_path) = ctx.struct_module_path(name, &[])
        && let Some(sfid) = file.source_file_id(&module_path)
        && let Some(def) = find_def_by_name(arena, sfid, name)
    {
        return Some((sfid, def));
    }
    if let Some(key) = ctx.canonical_enum_key(name, &[])
        && let Some(info) = ctx.lookup_enum(&key)
    {
        let module_path = ctx.module_path_of_scope(info.definition_scope_id);
        if let Some(sfid) = file.source_file_id(&module_path)
            && let Some(def) = find_def_by_name(arena, sfid, name)
        {
            return Some((sfid, def));
        }
    }
    let def = find_def_by_name(arena, entry, name)?;
    Some((entry, def))
}

/// Resolves a type reference at `ident` to its definition. A bare (`Custom`) type
/// resolves by name in the entry scope; a `::`-qualified type (`lib::T`) resolves
/// through its qualifier segments to the module that defines it — the qualifier
/// is never dropped, so an imported type is reachable by its qualified spelling.
fn goto_in_type(
    file: &FileAnalysis,
    entry: SourceFileId,
    type_id: TypeId,
    ident: IdentId,
) -> Option<NavigationTarget> {
    let arena = file.arena();
    match &arena[type_id].kind {
        TypeNode::Qualified { qualifier, name } if *name == ident => {
            goto_module_member(file, entry, qualifier, *name)
        }
        TypeNode::QualifiedName { qualifier, name } if *name == ident => {
            goto_module_member(file, entry, std::slice::from_ref(qualifier), *name)
        }
        _ => {
            let (sfid, def) = resolve_type_def(file, entry, arena.ident_name(ident))?;
            nav_for_def(file, sfid, def)
        }
    }
}

/// The definition named `leaf` in the module named by a reference's `::`-qualifier
/// segments — the target of a qualified spelling such as `lib::T` (type position)
/// or `lib::MAX` (value position) that names an imported item without a selective
/// `use`. A cross-module target must be `pub`; an entry-file target need not be.
fn goto_module_member(
    file: &FileAnalysis,
    entry: SourceFileId,
    qualifier: &[IdentId],
    leaf: IdentId,
) -> Option<NavigationTarget> {
    let arena = file.arena();
    let sfid = resolve_qualified_module(file, entry, qualifier)?;
    let def = find_def_by_name(arena, sfid, arena.ident_name(leaf))?;
    if sfid != entry && !def_is_public(arena, def) {
        return None;
    }
    nav_for_def(file, sfid, def)
}

/// The `::`-qualifier segments carried by the base of a `TypeMemberAccess`: the
/// `lib` of `lib::MAX`, or the `[a, b]` of `a::b::MAX`. `None` when the base is
/// not a plain qualifier chain of identifiers.
fn access_qualifier_segments(arena: &AstArena, base: ExprId) -> Option<Vec<IdentId>> {
    match &arena[base].kind {
        Expr::Identifier(id) => Some(vec![*id]),
        Expr::TypeMemberAccess { expr, name } => {
            let mut segments = access_qualifier_segments(arena, *expr)?;
            segments.push(*name);
            Some(segments)
        }
        _ => None,
    }
}

fn goto_in_def(
    file: &FileAnalysis,
    entry: SourceFileId,
    def: DefId,
    ident: IdentId,
) -> Option<NavigationTarget> {
    let arena = file.arena();
    if def_name_ident(arena, def) == ident {
        return nav_for_def(file, entry, def);
    }
    match &arena[def].kind {
        Def::Function { args, .. } | Def::ExternFunction { args, .. } => {
            for arg in args {
                if let ArgKind::Named { name, .. } = &arg.kind
                    && *name == ident
                {
                    return nav_at_ident(file, entry, arena[*name].location, *name);
                }
            }
            None
        }
        Def::Struct { fields, .. } => {
            let field = fields.iter().find(|field| field.name == ident)?;
            nav_at_ident(file, entry, arena[field.name].location, field.name)
        }
        _ => None,
    }
}

fn goto_in_stmt(
    file: &FileAnalysis,
    entry: SourceFileId,
    stmt: StmtId,
    ident: IdentId,
) -> Option<NavigationTarget> {
    let arena = file.arena();
    match &arena[stmt].kind {
        Stmt::VarDef { name, .. } | Stmt::TypeDef { name, .. } if *name == ident => {
            nav_at_ident(file, entry, arena[stmt].location, *name)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_possible_truncation)]

    use crate::NavigationTarget;
    use crate::test_utils::{at, module_path, nth, single, with_lib};

    fn goto(source: &str, offset: u32) -> Option<Vec<NavigationTarget>> {
        let (mut host, path) = single(source);
        host.analysis().goto_definition(&path, offset)
    }

    fn one(source: &str, offset: u32) -> NavigationTarget {
        let mut targets = goto(source, offset).expect("a definition");
        assert_eq!(targets.len(), 1, "exactly one target");
        targets.remove(0)
    }

    const OBJ: &str = "struct P { x: i32; }\n\
fn helper() -> i32 { return 7; }\n\
fn use_it(p: P) -> i32 { let v: i32 = helper(); return v + p.x; }";

    #[test]
    fn goto_local_variable_reaches_its_let() {
        let target = one(OBJ, at(OBJ, "v + p.x"));
        assert_eq!(target.path, module_path("main"));
        assert_eq!(target.focus_range.start, at(OBJ, "v: i32"));
    }

    #[test]
    fn goto_parameter_reaches_its_declaration() {
        let target = one(OBJ, at(OBJ, "p.x"));
        assert_eq!(target.focus_range.start, at(OBJ, "p: P"));
    }

    #[test]
    fn goto_field_in_member_access_reaches_the_field() {
        let target = one(OBJ, at(OBJ, "p.x") + "p.".len() as u32);
        assert_eq!(target.focus_range.start, at(OBJ, "x: i32"));
    }

    #[test]
    fn goto_same_file_call_reaches_the_function() {
        let target = one(OBJ, at(OBJ, "helper();"));
        assert_eq!(target.focus_range.start, at(OBJ, "helper"));
    }

    #[test]
    fn goto_struct_name_in_type_position_reaches_the_struct() {
        let target = one(OBJ, at(OBJ, "p: P") + "p: ".len() as u32);
        assert_eq!(
            target.focus_range.start,
            at(OBJ, "struct P") + "struct ".len() as u32
        );
    }

    #[test]
    fn goto_method_call_via_receiver_reaches_the_method() {
        let source = "struct Q { y: i32; fn getq(self) -> i32 { return self.y; } }\n\
fn m(q: Q) -> i32 { return q.getq(); }";
        let target = one(source, at(source, "q.getq()") + "q.".len() as u32);
        assert_eq!(target.focus_range.start, at(source, "getq"));
    }

    // The return type is `i32`, not `R`, so `"R {"` occurs exactly twice — the
    // struct declaration (0) and the literal (1) — and `nth(_, 1)` reliably lands
    // on the literal rather than a return-type `R`.
    const LITERAL: &str = "struct R { z: i32; }\n\
fn mk() -> i32 { let r: R = R { z: 1 }; return r.z; }";

    #[test]
    fn goto_struct_literal_name_reaches_the_struct() {
        let target = one(LITERAL, nth(LITERAL, "R {", 1));
        assert_eq!(
            target.focus_range.start,
            at(LITERAL, "struct R") + "struct ".len() as u32
        );
    }

    #[test]
    fn goto_struct_literal_field_reaches_the_field() {
        let target = one(LITERAL, at(LITERAL, "z: 1"));
        assert_eq!(target.focus_range.start, at(LITERAL, "z: i32"));
    }

    #[test]
    fn goto_imported_constant_used_as_a_value() {
        let entry = "use lib::{MAX};\nfn main() -> i32 { return MAX; }";
        let lib = "pub const MAX: i32 = 99;";
        let (mut host, path) = with_lib(entry, lib);
        let mut targets = host
            .analysis()
            .goto_definition(&path, at(entry, "return MAX") + "return ".len() as u32)
            .expect("a definition for the imported constant");
        assert_eq!(targets.len(), 1, "exactly one target");
        let target = targets.remove(0);
        assert_eq!(target.path, module_path("lib"));
        assert_eq!(target.focus_range.start, at(lib, "MAX"));
    }

    #[test]
    fn goto_enum_variant_reaches_the_variant() {
        let source = "enum Color { Red, Green }\n\
fn f() -> Color { let c: Color = Color::Red; return c; }";
        let target = one(source, at(source, "Color::Red") + "Color::".len() as u32);
        assert_eq!(target.focus_range.start, at(source, "Red"));
    }

    #[test]
    fn goto_definition_name_reaches_itself() {
        let source = "fn helper() -> i32 { return 7; }";
        let target = one(source, at(source, "helper"));
        assert_eq!(target.focus_range.start, at(source, "helper"));
    }

    #[test]
    fn goto_unresolved_name_is_none() {
        let source = "fn f() -> i32 { return zzz; }";
        assert!(goto(source, at(source, "zzz")).is_none());
    }

    #[test]
    fn goto_local_in_a_closed_sibling_block_does_not_resolve() {
        // `inner` is declared inside the `if` block; at the use after the block
        // has closed it is out of scope (the checker reports it undeclared), so
        // goto must not teleport into the sibling block. Contrast with the
        // in-scope `let z`, whose block encloses the use.
        let source = "fn f(c: bool) -> i32 {\n\
if c {\n\
let inner: i32 = 1;\n\
assert(inner > 0);\n\
}\n\
let z: i32 = 0;\n\
return z + inner;\n\
}";
        assert!(
            goto(source, at(source, "inner;")).is_none(),
            "a use after the declaring block closed is out of scope"
        );
        // The sibling `z`, declared in the enclosing function body, still resolves.
        let target = one(source, at(source, "z + inner"));
        assert_eq!(target.focus_range.start, at(source, "z: i32"));
    }

    #[test]
    fn goto_qualified_type_reaches_the_imported_struct() {
        // `lib::T` is the qualified spelling of an imported type; goto must follow
        // the qualifier into lib.inf rather than dropping it and returning None.
        let entry = "use lib;\nfn main() -> i32 { let t: lib::T = lib::mk(); return t.v; }\n";
        let lib = "pub struct T { v: i32; }\npub fn mk() -> T { return T { v: 1 }; }\n";
        let (mut host, path) = with_lib(entry, lib);
        let target = host
            .analysis()
            .goto_definition(&path, at(entry, "lib::T") + "lib::".len() as u32)
            .expect("a definition for the qualified type")
            .remove(0);
        assert_eq!(target.path, module_path("lib"));
        assert_eq!(
            target.focus_range.start,
            at(lib, "struct T") + "struct ".len() as u32
        );
    }

    #[test]
    fn goto_qualified_constant_reaches_the_imported_constant() {
        // `lib::MAX` reads an imported constant through its qualified spelling; the
        // access is a `TypeMemberAccess` that is not an enum variant, so goto must
        // fall through to module-member resolution rather than returning None.
        let entry = "use lib;\nfn main() -> i32 { return lib::MAX; }\n";
        let lib = "pub const MAX: i32 = 99;\n";
        let (mut host, path) = with_lib(entry, lib);
        let target = host
            .analysis()
            .goto_definition(&path, at(entry, "lib::MAX") + "lib::".len() as u32)
            .expect("a definition for the qualified constant")
            .remove(0);
        assert_eq!(target.path, module_path("lib"));
        assert_eq!(target.focus_range.start, at(lib, "MAX"));
    }

    #[test]
    fn goto_cross_module_function_returns_the_imported_file() {
        let entry = "use lib;\nfn main() -> i32 { return lib::helper(); }";
        let lib = "pub fn helper() -> i32 { return 7; }";
        let (mut host, path) = with_lib(entry, lib);
        let mut targets = host
            .analysis()
            .goto_definition(&path, at(entry, "helper();"))
            .expect("a cross-module definition");
        assert_eq!(targets.len(), 1, "exactly one target");
        let target = targets.remove(0);
        assert_eq!(target.path, module_path("lib"));
        assert_eq!(target.focus_range.start, at(lib, "helper"));
    }
}
