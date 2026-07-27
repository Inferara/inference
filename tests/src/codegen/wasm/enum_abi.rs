// Exported-function enum-tag ABI guard tests.
//
// Every enum-typed parameter of an *exported* (entry-file `pub fn`) function
// gets a prologue guard `tag >= N -> unreachable` (N = declared variant count),
// rejecting any host tag that names no variant. Negative tags arrive as huge
// unsigned values and are caught by the same `i32.ge_u`. In-language callers
// always pass declaration-derived tags, so the guard never fires for them.
//
// Kept in its own file (not folded into multi_file.rs) so it does not collide
// with the a042 branch's multi_file.rs rewrite. Multi-file cases assemble inline
// `(module_path, source)` pairs via `wasm_codegen_multi_file`; single-file cases
// use `wasm_codegen`.

#[cfg(test)]
mod enum_abi_tests {
    use crate::utils::{wasm_codegen, wasm_codegen_multi_file};
    use wasmtime::{Engine, Instance, Module, Store, Trap, TypedFunc};

    fn instantiate(wasm: &[u8]) -> (Store<()>, Instance) {
        inf_wasmparser::validate(wasm).unwrap_or_else(|e| panic!("generated module is invalid: {e}"));
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).unwrap_or_else(|e| panic!("module build: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("instantiate: {e}"));
        (store, instance)
    }

    fn assert_traps(store: &mut Store<()>, f: &TypedFunc<i32, i32>, arg: i32) {
        let err = f
            .call(&mut *store, arg)
            .expect_err("out-of-range enum tag must trap");
        assert_eq!(
            *err.downcast_ref::<Trap>().expect("wasmtime Trap"),
            Trap::UnreachableCodeReached,
            "enum tag {arg} should trap as unreachable",
        );
    }

    /// An item-imported bare-name enum parameter. `Level` is defined in
    /// `lib::shapes` and item-imported into the entry file, so its parameter type
    /// reaches codegen as a bare `Custom("Level")` carrier; `resolve_param_enum`
    /// resolves it through the import to the defining file's `EnumInfo`, and N (3)
    /// comes from that canonical definition.
    #[test]
    fn item_imported_bare_enum_param_is_guarded() {
        let entry = "\
use lib::shapes::{Level};

pub fn f(l: Level) -> i32 {
    return 0;
}
";
        let shapes = "pub enum Level { Low, Mid, High }\n";
        let wasm = wasm_codegen_multi_file(&[
            (vec![], entry),
            (vec!["lib", "shapes"], shapes),
        ]);
        let (mut store, instance) = instantiate(&wasm);
        let f: TypedFunc<i32, i32> = instance.get_typed_func(&mut store, "f").expect("get f");

        // Valid tags 0..=2 pass through the guard.
        assert_eq!(f.call(&mut store, 0).expect("f(0)"), 0);
        assert_eq!(f.call(&mut store, 1).expect("f(1)"), 0);
        assert_eq!(f.call(&mut store, 2).expect("f(2)"), 0);
        // N and a negative tag (huge u32) both trap.
        assert_traps(&mut store, &f, 3);
        assert_traps(&mut store, &f, -1);
    }

    /// A `::`-qualified enum parameter. `use lib::geo;` binds the `geo`
    /// namespace, so `geo::Level` reaches codegen as a `Qualified("geo::Level")`
    /// carrier; `resolve_param_enum` splits it and resolves via
    /// `lookup_enum_by_qualified_path`. This exact shape compiled unguarded before
    /// this change.
    #[test]
    fn qualified_path_enum_param_is_guarded() {
        let entry = "\
use lib::geo;

pub fn g(l: geo::Level) -> i32 {
    return 0;
}
";
        let geo = "pub enum Level { Low, Mid, High }\n";
        let wasm = wasm_codegen_multi_file(&[
            (vec![], entry),
            (vec!["lib", "geo"], geo),
        ]);
        let (mut store, instance) = instantiate(&wasm);
        let g: TypedFunc<i32, i32> = instance.get_typed_func(&mut store, "g").expect("get g");

        assert_eq!(g.call(&mut store, 0).expect("g(0)"), 0);
        assert_eq!(g.call(&mut store, 2).expect("g(2)"), 0);
        assert_traps(&mut store, &g, 3);
    }

    /// Only the entry file's `pub fn`s are exports, so only their enum params are
    /// guarded. An imported-file `pub fn` with an enum param is intra-project
    /// visibility, not an export, and gets no prologue guard. The mark count
    /// (exactly the one entry-file enum param) pins that.
    #[test]
    fn imported_pub_fn_enum_param_is_not_guarded() {
        cov_mark::check_count!(wasm_codegen_entry_enum_tag_guard, 1);
        let entry = "\
use lib::shapes::{Level};

pub fn entry_fn(l: Level) -> i32 {
    return 0;
}
";
        let shapes = "\
pub enum Level { Low, Mid, High }

pub fn imported_fn(l: Level) -> i32 {
    return 1;
}
";
        let wasm = wasm_codegen_multi_file(&[
            (vec![], entry),
            (vec!["lib", "shapes"], shapes),
        ]);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("generated module is invalid: {e}"));
    }

    /// A variantless enum is uninhabited, so `tag >= 0` is uniformly true and
    /// every host call traps — the correct degenerate, emitted with the same
    /// shape (`i32.const 0`). Uses the empty-enum-specific cov_mark.
    #[test]
    fn variantless_enum_param_always_traps() {
        cov_mark::check_count!(wasm_codegen_entry_enum_tag_guard_empty, 1);
        let source = "\
enum Void {}

pub fn takes_void(v: Void) -> i32 {
    return 0;
}
";
        let wasm = wasm_codegen(source);
        let (mut store, instance) = instantiate(&wasm);
        let takes_void: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "takes_void")
            .expect("get takes_void");

        // No inhabited tag exists, so every host call traps.
        assert_traps(&mut store, &takes_void, 0);
        assert_traps(&mut store, &takes_void, 1);
    }

    /// A struct parameter is not an enum: `resolve_param_enum` resolves the bare
    /// `Custom` name to a struct (not an enum) and returns `None`, so no tag guard
    /// is emitted. The zero mark count pins the struct-skip.
    #[test]
    fn struct_param_gets_no_enum_guard() {
        cov_mark::check_count!(wasm_codegen_entry_enum_tag_guard, 0);
        let source = "\
struct Point {
    x: i32;
    y: i32;
}

pub fn take_point(p: Point) -> i32 {
    return p.x;
}
";
        let wasm = wasm_codegen(source);
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("generated module is invalid: {e}"));
    }
}
