//! Type and documentation shown when hovering a position in a document.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, IdentId, NodeId, SourceFileId, TypeId};
use inference_ast::nodes::{ArgKind, Def, Directive, Expr, TypeNode};
use inference_ide_db::{FileAnalysis, NodeHit, TextRange};
use inference_type_checker::type_info::TypeInfo;
use inference_type_checker::typed_context::TypedContext;

use crate::nondet_docs::{UZUMAKI_HOVER, block_hover, block_keyword};
use crate::syntax::{
    def_is_public, def_name_ident, def_signature, find_def_by_name, find_free_def_by_name,
    find_method, is_call_callee, resolve_qualified_module, text_range,
};
use crate::type_render::render_type;

/// The information shown in a hover popover: markdown contents plus the source
/// range they describe (so the editor can underline the hovered token).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hover {
    pub contents_markdown: String,
    pub range: TextRange,
}

/// Computes the hover for byte `offset` in the entry file, or `None` when nothing
/// meaningful sits there (whitespace, or a token with no type or documentation).
#[must_use]
pub(crate) fn hover(file: &FileAnalysis, offset: u32) -> Option<Hover> {
    let arena = file.arena();
    let ctx = file.typed_context();
    let entry = file.source_file_id(&[])?;
    let hit = file.enclosing_hit(entry, offset)?;

    if let Some(hover) = nondet_keyword_hover(arena, &hit, offset) {
        return Some(hover);
    }
    if let NodeId::Expr(expr) = hit.node
        && matches!(arena[expr].kind, Expr::Uzumaki)
    {
        return Some(Hover {
            contents_markdown: UZUMAKI_HOVER.to_string(),
            range: text_range(arena[expr].location),
        });
    }
    if let NodeId::Ident(ident) = hit.node {
        return hover_ident(file, entry, &hit, ident);
    }
    if let NodeId::Type(ty) = hit.node {
        return Some(Hover {
            contents_markdown: code_block(&render_type(&TypeInfo::from_type_id(arena, ty))),
            range: text_range(arena[ty].location),
        });
    }
    if let NodeId::Expr(expr) = hit.node {
        let type_info = ctx.get_node_typeinfo(NodeId::Expr(expr))?;
        return Some(Hover {
            contents_markdown: code_block(&render_type(&type_info)),
            range: text_range(arena[expr].location),
        });
    }
    None
}

fn code_block(body: &str) -> String {
    format!("```inference\n{body}\n```")
}

/// A hover for a non-det block keyword, when `offset` falls inside it.
fn nondet_keyword_hover(arena: &AstArena, hit: &NodeHit, offset: u32) -> Option<Hover> {
    let NodeId::Block(block) = hit.node else {
        return None;
    };
    let data = &arena[block];
    let keyword = block_keyword(data.block_kind)?;
    let start = data.location.offset_start;
    let end = start.checked_add(u32::try_from(keyword.len()).ok()?)?;
    if offset < start || offset >= end {
        return None;
    }
    Some(Hover {
        contents_markdown: block_hover(data.block_kind)?.to_string(),
        range: TextRange { start, end },
    })
}

/// A hover for an identifier, dispatched on the role its parent gives it.
fn hover_ident(
    file: &FileAnalysis,
    entry: SourceFileId,
    hit: &NodeHit,
    ident: IdentId,
) -> Option<Hover> {
    let arena = file.arena();
    let ctx = file.typed_context();
    let range = text_range(arena[ident].location);
    let parent = hit.ancestors.last().copied();
    let grandparent = hit.ancestors.iter().rev().nth(1).copied();

    // An identifier with no ancestors is a `use`-directive segment or item (see
    // `hit_test`); every definition-tree identifier carries its owning node.
    let contents = match parent {
        Some(NodeId::Def(def)) => ident_in_def(arena, entry, def, ident)?,
        Some(NodeId::Expr(expr)) => ident_in_expr(file, entry, expr, ident, grandparent)?,
        Some(NodeId::Type(type_id)) => type_ident_signature(file, entry, type_id, ident),
        Some(NodeId::Stmt(stmt)) => ident_in_stmt(arena, ctx, stmt, ident)?,
        None => ident_in_directive(file, entry, ident)?,
        _ => return None,
    };
    Some(Hover {
        contents_markdown: contents,
        range,
    })
}

/// The def's own name (→ its signature), a declared type parameter or enum
/// variant (→ itself), or one of its params/fields (→ its type).
fn ident_in_def(
    arena: &AstArena,
    entry: SourceFileId,
    def: DefId,
    ident: IdentId,
) -> Option<String> {
    if def_name_ident(arena, def) == ident {
        return def_signature(arena, entry, def).map(|sig| code_block(&sig));
    }
    // A declared type parameter names itself; show it in its source spelling.
    if let Def::Function { type_params, .. } = &arena[def].kind
        && type_params.contains(&ident)
    {
        return Some(code_block(&format!("{}'", arena.ident_name(ident))));
    }
    match &arena[def].kind {
        Def::Function { args, .. } | Def::ExternFunction { args, .. } => {
            for arg in args {
                if let ArgKind::Named { name, ty, .. } = &arg.kind
                    && *name == ident
                {
                    let type_info = TypeInfo::from_type_id(arena, *ty);
                    return Some(named_type(arena.ident_name(ident), &type_info));
                }
            }
            None
        }
        Def::Struct { fields, .. } => {
            let field = fields.iter().find(|field| field.name == ident)?;
            let type_info = TypeInfo::from_type_id(arena, field.ty);
            Some(named_type(arena.ident_name(ident), &type_info))
        }
        // A variant declaration names itself; show it qualified by its enum.
        Def::Enum { name, variants, .. } => {
            let variant = variants.iter().copied().find(|&variant| variant == ident)?;
            Some(code_block(&format!(
                "{}::{}",
                arena.ident_name(*name),
                arena.ident_name(variant)
            )))
        }
        _ => None,
    }
}

/// An identifier appearing inside an expression: a value reference, a call
/// callee, a member name, or a struct-literal name or field.
fn ident_in_expr(
    file: &FileAnalysis,
    entry: SourceFileId,
    expr: ExprId,
    ident: IdentId,
    grandparent: Option<NodeId>,
) -> Option<String> {
    let arena = file.arena();
    let ctx = file.typed_context();
    match &arena[expr].kind {
        Expr::Identifier(_) => {
            if is_call_callee(arena, grandparent, expr) {
                return callee_signature(file, expr);
            }
            let type_info = ctx.get_node_typeinfo(NodeId::Expr(expr))?;
            Some(named_type(arena.ident_name(ident), &type_info))
        }
        Expr::MemberAccess { .. } | Expr::TypeMemberAccess { .. } => {
            // A method or `::`-qualified call resolves to its callee's signature;
            // a plain field or variant access shows the member's type.
            if is_call_callee(arena, grandparent, expr) {
                return callee_signature(file, expr);
            }
            let type_info = ctx.get_node_typeinfo(NodeId::Expr(expr))?;
            Some(named_type(arena.ident_name(ident), &type_info))
        }
        Expr::StructLiteral { name, .. } => {
            if *name == ident {
                return Some(type_name_signature(file, entry, ident));
            }
            let struct_name = arena.ident_name(*name);
            let info = ctx.lookup_struct_in(struct_name, &[])?;
            let field = info.get_field_info_by_name(arena.ident_name(ident))?;
            Some(named_type(arena.ident_name(ident), &field.type_info))
        }
        _ => None,
    }
}

/// The hover for a `use`-directive identifier: a path segment shows the module it
/// names, a braced item import shows the resolved definition's signature. A
/// segment or item that names no source module or `pub` definition has no hover.
fn ident_in_directive(
    file: &FileAnalysis,
    entry: SourceFileId,
    ident: IdentId,
) -> Option<String> {
    let arena = file.arena();
    for directive in &arena[entry].directives {
        let Directive::Use(use_directive) = directive;
        if let Some(index) = use_directive.segments.iter().position(|&s| s == ident) {
            let module_path: Vec<String> = use_directive.segments[..=index]
                .iter()
                .map(|&segment| arena.ident_name(segment).to_string())
                .collect();
            file.source_file_id(&module_path)?;
            return Some(code_block(&format!("module {}", module_path.join("::"))));
        }
        if use_directive.imported_types.contains(&ident) {
            let module_path: Vec<String> = use_directive
                .segments
                .iter()
                .map(|&segment| arena.ident_name(segment).to_string())
                .collect();
            let sfid = file.source_file_id(&module_path)?;
            let def = find_def_by_name(arena, sfid, arena.ident_name(ident))?;
            if !def_is_public(arena, def) {
                return None;
            }
            return def_signature(arena, sfid, def).map(|sig| code_block(&sig));
        }
    }
    None
}

/// A `let name: T` / `type name = …` binding: the declared name's type.
fn ident_in_stmt(
    arena: &AstArena,
    ctx: &TypedContext,
    stmt: inference_ast::ids::StmtId,
    ident: IdentId,
) -> Option<String> {
    use inference_ast::nodes::Stmt;
    match &arena[stmt].kind {
        Stmt::VarDef { name, ty, .. } if *name == ident => {
            let type_info = ctx
                .get_node_typeinfo(NodeId::Ident(ident))
                .unwrap_or_else(|| TypeInfo::from_type_id(arena, *ty));
            Some(named_type(arena.ident_name(ident), &type_info))
        }
        Stmt::TypeDef { name, ty } if *name == ident => Some(code_block(&format!(
            "type {} = {}",
            arena.ident_name(ident),
            render_type(&TypeInfo::from_type_id(arena, *ty))
        ))),
        _ => None,
    }
}

fn named_type(name: &str, type_info: &TypeInfo) -> String {
    code_block(&format!("{name}: {}", render_type(type_info)))
}

/// The signature of the function a call resolves to, in the callee's own file.
fn callee_signature(file: &FileAnalysis, callee: ExprId) -> Option<String> {
    let arena = file.arena();
    let ctx = file.typed_context();
    let target = ctx.call_target(callee)?;
    let sfid = file.source_file_id(&target.module_path)?;
    let def = match &target.receiver_struct {
        // A `Some` receiver is a method or associated function; `None` is a free
        // function, so its signature must come from a non-method def — never a
        // same-named method's `fn get(self) -> …` shown for a free `fn get()`.
        Some(struct_name) => {
            let struct_def = find_def_by_name(arena, sfid, struct_name)?;
            find_method(arena, struct_def, &target.name)?
        }
        None => find_free_def_by_name(arena, sfid, &target.name)?,
    };
    def_signature(arena, sfid, def).map(|sig| code_block(&sig))
}

/// The signature shown for a type-position identifier. A `::`-qualified type
/// (`lib::T`) resolves through its qualifier to the module that defines it —
/// exactly as goto does — so the qualifier is honored rather than dropped in
/// favor of a same-named local type; a bare type name falls through to
/// [`type_name_signature`].
fn type_ident_signature(
    file: &FileAnalysis,
    entry: SourceFileId,
    type_id: TypeId,
    ident: IdentId,
) -> String {
    let arena = file.arena();
    let qualifier = match &arena[type_id].kind {
        TypeNode::Qualified { qualifier, name } if *name == ident => Some(qualifier.clone()),
        TypeNode::QualifiedName { qualifier, name } if *name == ident => Some(vec![*qualifier]),
        _ => None,
    };
    if let Some(qualifier) = qualifier
        && let Some(signature) = qualified_member_signature(file, entry, &qualifier, ident)
    {
        return signature;
    }
    type_name_signature(file, entry, ident)
}

/// The signature of the definition named `leaf` in the module addressed by a
/// reference's `::`-qualifier segments (`lib::T`), or `None` when the qualifier
/// names no module or the target is not a visible `pub` definition. Mirrors
/// goto's `goto_module_member`.
fn qualified_member_signature(
    file: &FileAnalysis,
    entry: SourceFileId,
    qualifier: &[IdentId],
    leaf: IdentId,
) -> Option<String> {
    let arena = file.arena();
    let sfid = resolve_qualified_module(file, entry, qualifier)?;
    let def = find_def_by_name(arena, sfid, arena.ident_name(leaf))?;
    if sfid != entry && !def_is_public(arena, def) {
        return None;
    }
    def_signature(arena, sfid, def).map(|sig| code_block(&sig))
}

/// The signature of the struct/enum/type a bare type name refers to — its
/// defining file when cross-module, else the entry file — or the bare type
/// spelling when it names no such definition.
fn type_name_signature(file: &FileAnalysis, entry: SourceFileId, ident: IdentId) -> String {
    let arena = file.arena();
    let ctx = file.typed_context();
    let name = arena.ident_name(ident);

    // Resolve the type's defining file for a struct or an enum, so a cross-module
    // name shows its real signature (mirrors goto's `resolve_type_def`).
    let defining_file = ctx
        .struct_module_path(name, &[])
        .or_else(|| enum_module_path(ctx, name))
        .and_then(|module_path| file.source_file_id(&module_path))
        .unwrap_or(entry);

    if let Some(def) = find_def_by_name(arena, defining_file, name)
        && let Some(signature) = def_signature(arena, defining_file, def)
    {
        return code_block(&signature);
    }
    code_block(name)
}

/// The defining-file module path of the enum named `name` referenced from the
/// entry file, or `None` if `name` names no visible enum.
fn enum_module_path(ctx: &TypedContext, name: &str) -> Option<Vec<String>> {
    let key = ctx.canonical_enum_key(name, &[])?;
    let info = ctx.lookup_enum(&key)?;
    Some(ctx.module_path_of_scope(info.definition_scope_id))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_possible_truncation)]

    use crate::Hover;
    use crate::test_utils::{at, nth, single, with_lib};

    fn hover_at(source: &str, offset: u32) -> Option<Hover> {
        let (mut host, path) = single(source);
        host.analysis().hover(&path, offset)
    }

    const OBJ: &str = "struct P { x: i32; fn get(self) -> i32 { return self.x; } }\n\
fn helper() -> i32 { return 7; }\n\
fn use_it(p: P) -> i32 { let v: i32 = helper(); return v; }";

    #[test]
    fn hover_local_variable_shows_its_type() {
        let hover = hover_at(OBJ, at(OBJ, "return v") + "return ".len() as u32).expect("hover");
        assert_eq!(hover.contents_markdown, "```inference\nv: i32\n```");
    }

    #[test]
    fn hover_parameter_shows_its_type() {
        let hover = hover_at(OBJ, at(OBJ, "p: P")).expect("hover");
        assert_eq!(hover.contents_markdown, "```inference\np: P\n```");
    }

    #[test]
    fn hover_free_call_shows_the_signature() {
        let hover = hover_at(OBJ, at(OBJ, "helper();")).expect("hover");
        assert_eq!(
            hover.contents_markdown,
            "```inference\nfn helper() -> i32\n```"
        );
    }

    #[test]
    fn hover_field_access_shows_the_field_type() {
        let hover = hover_at(OBJ, at(OBJ, "self.x") + "self.".len() as u32).expect("hover");
        assert_eq!(hover.contents_markdown, "```inference\nx: i32\n```");
    }

    #[test]
    fn hover_struct_name_in_type_position_shows_its_signature() {
        let hover = hover_at(OBJ, at(OBJ, "p: P") + "p: ".len() as u32).expect("hover");
        assert!(
            hover.contents_markdown.contains("struct P"),
            "{}",
            hover.contents_markdown
        );
    }

    #[test]
    fn hover_each_nondet_keyword_returns_its_doc_and_keyword_range() {
        let cases = [
            ("forall", "fn f() { forall { assert(true); } }"),
            ("exists", "fn f() { exists { assert(true); } }"),
            ("unique", "fn f() { unique { assert(true); } }"),
            ("assume", "fn f() { assume { assert(true); } }"),
        ];
        for (keyword, source) in cases {
            let start = at(source, keyword);
            let hover = hover_at(source, start).unwrap_or_else(|| panic!("hover on {keyword}"));
            assert!(
                hover.contents_markdown.contains(&format!("`{keyword}`")),
                "doc for {keyword}: {}",
                hover.contents_markdown
            );
            assert_eq!(hover.range.start, start);
            assert_eq!(hover.range.end, start + keyword.len() as u32);
        }
    }

    #[test]
    fn hover_uzumaki_returns_its_doc() {
        let source = "fn f() { forall { let x: i32 = @; assert(x == x); } }";
        let hover = hover_at(source, at(source, "@")).expect("hover on @");
        assert!(
            hover.contents_markdown.contains("`@`"),
            "{}",
            hover.contents_markdown
        );
        assert_eq!(hover.range.start, at(source, "@"));
        assert_eq!(hover.range.end, at(source, "@") + 1);
    }

    #[test]
    fn hover_number_literal_shows_its_type() {
        let source = "fn f() -> i32 { return 42; }";
        let hover = hover_at(source, at(source, "42")).expect("a number literal has a type");
        assert_eq!(hover.contents_markdown, "```inference\ni32\n```");
    }

    #[test]
    fn hover_method_call_shows_the_method_signature() {
        let source = "struct Q { y: i32; fn getq(self) -> i32 { return self.y; } }\n\
fn u(q: Q) -> i32 { return q.getq(); }";
        let hover = hover_at(source, at(source, "q.getq()") + "q.".len() as u32).expect("hover");
        assert_eq!(
            hover.contents_markdown,
            "```inference\nfn getq(self) -> i32\n```"
        );
    }

    #[test]
    fn hover_struct_literal_name_and_field() {
        let source = "struct R { z: i32; }\n\
fn mk() -> i32 { let r: R = R { z: 1 }; return r.z; }";
        let name = hover_at(source, nth(source, "R {", 1)).expect("hover on literal name");
        assert!(
            name.contents_markdown.contains("struct R"),
            "{}",
            name.contents_markdown
        );
        let field = hover_at(source, at(source, "z: 1")).expect("hover on literal field");
        assert_eq!(field.contents_markdown, "```inference\nz: i32\n```");
    }

    #[test]
    fn hover_declaration_name_shows_its_type() {
        let source = "fn f() -> i32 { let count: i32 = 5; return count; }";
        let hover = hover_at(source, at(source, "count: i32")).expect("hover on let name");
        assert_eq!(hover.contents_markdown, "```inference\ncount: i32\n```");
    }

    #[test]
    fn hover_enum_type_name_shows_its_signature() {
        let source = "enum Color { Red, Green }\nfn f(c: Color) -> i32 { return 0; }";
        let hover = hover_at(source, at(source, "c: Color") + "c: ".len() as u32).expect("hover");
        assert!(
            hover.contents_markdown.contains("enum Color"),
            "{}",
            hover.contents_markdown
        );
    }

    #[test]
    fn hover_definition_name_shows_its_signature() {
        let source = "fn add(a: i32, b: i32) -> i32 { return a + b; }";
        let hover = hover_at(source, at(source, "add")).expect("hover on def name");
        assert_eq!(
            hover.contents_markdown,
            "```inference\nfn add(a: i32, b: i32) -> i32\n```"
        );
    }

    #[test]
    fn hover_field_of_a_cross_module_struct() {
        // The struct is defined in `lib`; hovering the field access in the entry
        // must still resolve its type through the imported file's definition.
        let entry = "use lib;\nfn use_pt(p: lib::Point) -> i32 { return p.x; }";
        let lib = "pub struct Point { pub x: i32; }";
        let (mut host, path) = with_lib(entry, lib);
        let hover = host
            .analysis()
            .hover(&path, at(entry, "p.x") + "p.".len() as u32)
            .expect("hover on cross-module field");
        assert_eq!(hover.contents_markdown, "```inference\nx: i32\n```");
    }

    #[test]
    fn hover_whitespace_between_definitions_is_none() {
        let source = "fn a() { return; }   fn b() { return; }";
        assert!(hover_at(source, at(source, "   ") + 1).is_none());
    }

    // --- caret at the exclusive end of an identifier (issue #244) ---

    #[test]
    fn hover_at_the_word_end_of_a_local_use_shows_its_type() {
        let source = "fn f() -> i32 { let value: i32 = 1; return value; }";
        let word_end = at(source, "value; }") + "value".len() as u32;
        let hover = hover_at(source, word_end).expect("hover at the word end");
        assert_eq!(hover.contents_markdown, "```inference\nvalue: i32\n```");
    }

    // --- hover on a `use` directive (issue #244) ---

    #[test]
    fn hover_plain_use_segment_names_the_module() {
        let entry = "use lib;\nfn main() -> i32 { return 0; }";
        let lib = "pub fn helper() -> i32 { return 7; }";
        let (mut host, path) = with_lib(entry, lib);
        let hover = host
            .analysis()
            .hover(&path, at(entry, "lib"))
            .expect("hover on the use segment");
        assert_eq!(hover.contents_markdown, "```inference\nmodule lib\n```");
    }

    #[test]
    fn hover_braced_item_import_shows_the_definition_signature() {
        let entry = "use lib::{helper};\nfn main() -> i32 { return 0; }";
        let lib = "pub fn helper() -> i32 { return 7; }";
        let (mut host, path) = with_lib(entry, lib);
        let hover = host
            .analysis()
            .hover(&path, at(entry, "helper"))
            .expect("hover on the braced item");
        assert_eq!(
            hover.contents_markdown,
            "```inference\npub fn helper() -> i32\n```"
        );
    }

    // --- declared type parameter and enum variant (issue #244) ---

    #[test]
    fn hover_type_parameter_declaration_shows_it() {
        let source = "fn id T'(x: i32) -> i32 { return x; }";
        let hover = hover_at(source, at(source, "T'")).expect("hover on the type parameter");
        assert_eq!(hover.contents_markdown, "```inference\nT'\n```");
    }

    #[test]
    fn hover_enum_variant_declaration_shows_it_qualified() {
        let source = "enum Color { Red, Green }\nfn f() -> i32 { return 0; }";
        let hover =
            hover_at(source, at(source, "Red")).expect("hover on the variant declaration");
        assert_eq!(hover.contents_markdown, "```inference\nColor::Red\n```");
    }

    // --- checker-agreeing resolution (issue #245) ---

    #[test]
    fn hover_free_call_over_a_same_named_method_shows_the_free_signature() {
        // A struct method and a free function share the name `get`. The call
        // `get()` is a free call, so hover must show the free `fn get() -> i32`,
        // not the method's `fn get(self) -> i32`.
        let source = "struct S { v: i32; fn get(self) -> i32 { return self.v; } }\n\
fn get() -> i32 { return 7; }\n\
fn caller() -> i32 { return get(); }";
        let hover = hover_at(source, at(source, "get();")).expect("hover on the free call");
        assert_eq!(
            hover.contents_markdown,
            "```inference\nfn get() -> i32\n```"
        );
    }

    #[test]
    fn hover_qualified_type_leaf_matches_the_goto_target() {
        // Hovering the `T` of `lib::T` must resolve through the qualifier into
        // lib.inf — the same file goto lands in — not a bare-name degradation or
        // a local same-named struct.
        let entry = "use lib;\nfn main() -> i32 { let t: lib::T = lib::mk(); return t.v; }\n";
        let lib = "pub struct T { v: i32; }\npub fn mk() -> T { return T { v: 1 }; }\n";
        let (mut host, path) = with_lib(entry, lib);
        let offset = at(entry, "lib::T") + "lib::".len() as u32;
        let hover = host
            .analysis()
            .hover(&path, offset)
            .expect("hover on the qualified type leaf");
        assert!(
            hover.contents_markdown.contains("struct T"),
            "resolves the imported struct, got {}",
            hover.contents_markdown
        );
    }

    #[test]
    fn hover_qualified_type_leaf_ignores_a_local_same_named_struct() {
        // A local `T` (a different kind) exists too; hovering the qualified
        // `lib::T` must still resolve the imported struct through the qualifier,
        // not degrade to the local same-named definition.
        let entry =
            "use lib;\nenum T { Local }\nfn main() -> i32 { let t: lib::T = lib::mk(); return t.v; }\n";
        let lib = "pub struct T { v: i32; }\npub fn mk() -> T { return T { v: 1 }; }\n";
        let (mut host, path) = with_lib(entry, lib);
        let offset = at(entry, "lib::T") + "lib::".len() as u32;
        let hover = host
            .analysis()
            .hover(&path, offset)
            .expect("hover on the qualified type leaf");
        assert!(
            hover.contents_markdown.contains("struct T"),
            "shows the imported struct; got {}",
            hover.contents_markdown
        );
        assert!(
            !hover.contents_markdown.contains("enum"),
            "must not degrade to the local same-named enum; got {}",
            hover.contents_markdown
        );
    }

    #[test]
    fn hover_function_type_parameter_shows_a_source_like_spelling() {
        // A function-typed parameter hovers as the source-like `fn(…) -> i32`,
        // never the checker-internal `Function<0, i32>` carrier. The parser drops
        // `fn(…)` parameter types (a pinned AST-parity quirk), so the written
        // params do not survive to the hover — the return type does — which is the
        // clearest form the available data allows.
        let source = "fn apply(f: fn(i32, bool) -> i32) -> i32 { return 0; }";
        let hover = hover_at(source, at(source, "f: fn")).expect("hover on the fn-typed param");
        assert_eq!(hover.contents_markdown, "```inference\nf: fn() -> i32\n```");
    }
}
