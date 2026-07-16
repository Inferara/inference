//! Recursive definition walk over a single source file.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, SourceFileId};
use inference_ast::nodes::Def;

/// Collects every definition in `file` in pre-order: each top-level definition,
/// then its nested definitions (a struct's methods, a spec's inner defs),
/// recursively.
///
/// `arena.function_def_ids()` alone misses struct methods and spec-nested defs,
/// so document symbols and by-name lookup that must see the full hierarchy walk
/// through here instead. A definition always precedes its children, so the flat
/// list still carries the nesting order later phases rebuild a tree from.
///
/// The walk is scoped to `file`'s own def list. It never touches another file,
/// which matters in the merged multi-file arena where a bare arena-wide scan
/// would mix files together.
#[must_use = "the collected definitions are the reason to call this"]
pub fn file_defs(arena: &AstArena, file: SourceFileId) -> Vec<DefId> {
    let mut defs = Vec::new();
    for &def in &arena[file].defs {
        collect_def(arena, def, &mut defs);
    }
    defs
}

/// Appends `def` and then, depth-first, every definition nested inside it.
fn collect_def(arena: &AstArena, def: DefId, out: &mut Vec<DefId>) {
    out.push(def);
    match &arena[def].kind {
        Def::Struct { methods, .. } => {
            for &method in methods {
                collect_def(arena, method, out);
            }
        }
        Def::Spec { defs, .. } => {
            for &nested in defs {
                collect_def(arena, nested, out);
            }
        }
        Def::Function { .. }
        | Def::ExternFunction { .. }
        | Def::Enum { .. }
        | Def::Constant { .. }
        | Def::TypeAlias { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_parser::parse;

    /// Parses a single-file program and returns its arena plus the sole file id.
    fn single_file(source: &str) -> (AstArena, SourceFileId) {
        let arena = parse(source).arena;
        let file = arena.source_file_ids().next().expect("one source file");
        (arena, file)
    }

    #[test]
    fn collects_top_level_functions() {
        let (arena, file) = single_file("fn a() {} fn b() {}");
        let names: Vec<&str> = file_defs(&arena, file)
            .iter()
            .map(|&d| arena.def_name(d))
            .collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn recurses_into_struct_methods() {
        let source = "struct Point { x: i32; fn get_x() -> i32 { return self.x; } fn set() {} }";
        let (arena, file) = single_file(source);
        let names: Vec<&str> = file_defs(&arena, file)
            .iter()
            .map(|&d| arena.def_name(d))
            .collect();
        // The struct precedes its methods (pre-order).
        assert_eq!(names, vec!["Point", "get_x", "set"]);
    }

    #[test]
    fn recurses_into_spec_defs() {
        let source = "spec S { fn prop() {} fn other() {} }";
        let (arena, file) = single_file(source);
        let names: Vec<&str> = file_defs(&arena, file)
            .iter()
            .map(|&d| arena.def_name(d))
            .collect();
        assert_eq!(names, vec!["S", "prop", "other"]);
    }

    #[test]
    fn recurses_into_spec_nested_struct_methods() {
        // A struct inside a spec: the walk must reach the struct's methods too.
        let source = "spec S { struct Inner { v: i32; fn get() -> i32 { return self.v; } } }";
        let (arena, file) = single_file(source);
        let names: Vec<&str> = file_defs(&arena, file)
            .iter()
            .map(|&d| arena.def_name(d))
            .collect();
        assert_eq!(names, vec!["S", "Inner", "get"]);
    }

    #[test]
    fn collects_mixed_top_level_kinds() {
        let source = "const N: i32 = 1; enum E { A, B } struct P { x: i32; } fn f() {}";
        let (arena, file) = single_file(source);
        let names: Vec<&str> = file_defs(&arena, file)
            .iter()
            .map(|&d| arena.def_name(d))
            .collect();
        assert_eq!(names, vec!["N", "E", "P", "f"]);
    }
}
