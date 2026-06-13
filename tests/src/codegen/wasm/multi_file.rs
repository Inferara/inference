//! Multi-file codegen smoke tests.
//!
//! These exercise multi-file flattening end to end through Wasmtime rather than
//! against golden `.wasm` files. Each test assembles a multi-file arena from
//! inline `(module_path, source)` pairs via [`wasm_codegen_multi_file`], so no
//! filesystem fixtures are needed.
//!
//! Coverage:
//! - a re-export chain (`main` → `math` re-exports `lib::arith`) compiles to a
//!   single valid module whose cross-file call executes correctly under Wasmtime;
//! - an item import (`use lib::arith::{add};`) resolves a bare call across files;
//! - two files each defining a same-named struct get distinct layouts, so a
//!   field read picks the right offset for the file it is written in;
//! - only the entry file's `pub fn`s are WASM exports (root-only export policy).

use crate::utils::wasm_codegen_multi_file;

use inference_wasm_codegen::{CodegenOutput, CompilationMode, OptLevel, Target};
use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

/// Runs the multi-file pipeline in proof mode (no analysis, so spec-only
/// patterns are exercised) and returns the full [`CodegenOutput`] so a test can
/// inspect the per-spec index map.
fn proof_codegen_multi_file(files: &[(Vec<&str>, &str)]) -> CodegenOutput {
    let mut arena = inference_ast::arena::AstArena::default();
    for (module_path, source) in files {
        let module_path: Vec<String> = module_path.iter().map(|s| (*s).to_string()).collect();
        let parsed = inference_parser::parse_into(arena, source, module_path);
        assert!(
            parsed.errors.is_empty(),
            "multi-file proof source has syntax errors: {:?}",
            parsed.errors
        );
        arena = parsed.arena;
    }
    let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
        .expect("multi-file proof type check should succeed")
        .typed_context();
    inference_wasm_codegen::codegen(
        &typed_context,
        Target::Wasm32,
        CompilationMode::Proof,
        OptLevel::O3,
        "output",
    )
    .expect("multi-file proof codegen should succeed")
}

/// Instantiates `wasm_bytes` and returns the store + instance for calling
/// exported functions.
fn instantiate(wasm_bytes: &[u8]) -> (Store<()>, Instance) {
    inf_wasmparser::validate(wasm_bytes)
        .unwrap_or_else(|e| panic!("generated multi-file Wasm module is invalid: {e}"));
    let engine = Engine::default();
    let module = Module::new(&engine, wasm_bytes)
        .unwrap_or_else(|e| panic!("failed to create Wasm module: {e}"));
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[])
        .unwrap_or_else(|e| panic!("failed to instantiate Wasm module: {e}"));
    (store, instance)
}

/// Calls a zero-argument exported function returning `i32`.
fn call_i32(store: &mut Store<()>, instance: &Instance, name: &str) -> i32 {
    let f: TypedFunc<(), i32> = instance
        .get_typed_func(&mut *store, name)
        .unwrap_or_else(|e| panic!("failed to get '{name}': {e}"));
    f.call(&mut *store, ())
        .unwrap_or_else(|e| panic!("call to '{name}' failed: {e}"))
}

#[test]
fn re_export_chain_cross_file_call_executes() {
    // The corrected three-file example from the issue: `main` imports `math`,
    // which re-exports `lib::arith` with `pub use`, so `main` reaches
    // `math::arith::add` only through the re-export chain.
    let main = "\
use math;

pub fn run() -> i32 {
    return math::arith::add(1, 2);
}
";
    let math = "\
pub use lib::arith;

pub fn foo() {}
";
    let lib_arith = "\
pub fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["math"], math),
        (vec!["lib", "arith"], lib_arith),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 3);
}

#[test]
fn two_segment_namespace_call_executes() {
    // The basic file-import pattern: `use util;` binds namespace `util`, and the
    // two-segment `util::helper()` resolves to `util.inf`'s function. This is the
    // shortest cross-file call shape — no re-export chain, no braced item import.
    let main = "\
use util;

pub fn run() -> i32 {
    return util::helper();
}
";
    let util = "\
pub fn helper() -> i32 {
    return 7;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["util"], util),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 7);
}

#[test]
fn item_import_bare_cross_file_call_executes() {
    // A braced item import binds `add` directly; the bare call must resolve to
    // the foreign file's function index.
    let main = "\
use lib::arith::{add};

pub fn run() -> i32 {
    return add(40, 2);
}
";
    let lib_arith = "\
pub fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "arith"], lib_arith),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 42);
}

#[test]
fn same_named_structs_in_two_files_get_distinct_layouts() {
    // `Pair` in the entry file is `{ a: i32, b: i32 }` (b at offset 4); `Pair`
    // in `other.inf` is `{ a: i64, b: i32 }` (b at offset 8). Each file reads
    // `.b` on its own `Pair`, so a single shared layout would mis-read one of
    // them. Both reads returning the stored sentinel proves distinct layouts.
    let main = "\
use lib::shapes;

pub struct Pair {
    a: i32;
    b: i32;
}

pub fn here_b() -> i32 {
    let p: Pair = Pair { a: 10, b: 20 };
    return p.b;
}

pub fn there_b() -> i32 {
    return lib::shapes::there_b();
}
";
    let shapes = "\
pub struct Pair {
    a: i64;
    b: i32;
}

pub fn there_b() -> i32 {
    let p: Pair = Pair { a: 100, b: 200 };
    return p.b;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "shapes"], shapes),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "here_b"), 20);
    assert_eq!(call_i32(&mut store, &instance, "there_b"), 200);
}

#[test]
fn only_entry_file_pub_fns_are_exported() {
    // `lib::arith::add` is `pub` but lives in an imported file, so it is
    // intra-project visible, not a WASM export. Only the entry file's `pub fn`s
    // appear in the export section.
    let main = "\
use lib::arith;

pub fn run() -> i32 {
    return lib::arith::add(2, 3);
}
";
    let lib_arith = "\
pub fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "arith"], lib_arith),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    // The entry export is callable...
    assert_eq!(call_i32(&mut store, &instance, "run"), 5);
    // ...and the imported file's `pub fn` is not exported.
    assert!(
        instance.get_func(&mut store, "add").is_none(),
        "imported `pub fn add` must not be a WASM export"
    );
}

#[test]
fn proof_mode_specs_are_file_qualified_across_files() {
    // An entry-file spec keeps its bare name in the spec section; a spec in an
    // imported file is qualified by that file's module path, so the per-spec
    // entries from different files stay distinct.
    let main = "\
spec EntrySpec {
    fn obligation() -> i32 {
        return 1;
    }
}

pub fn main() {}
";
    let lib_checks = "\
spec LibSpec {
    fn obligation() -> i32 {
        return 2;
    }
}
";

    let output = proof_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "checks"], lib_checks),
    ]);
    let by_spec = output.spec_func_indices_by_spec();

    assert!(
        by_spec.contains_key("EntrySpec"),
        "entry-file spec must keep its bare name; keys: {:?}",
        by_spec.keys().collect::<Vec<_>>()
    );
    assert!(
        by_spec.contains_key("lib_checks_LibSpec"),
        "imported-file spec must be file-qualified with an underscore-joined, \
         Rocq-legal key; keys: {:?}",
        by_spec.keys().collect::<Vec<_>>()
    );
    assert!(
        !by_spec.contains_key("LibSpec"),
        "the imported spec must not also appear under its bare name; keys: {:?}",
        by_spec.keys().collect::<Vec<_>>()
    );
}

#[test]
fn namespace_qualified_assoc_fn_executes() {
    // The plan's normative `geo::Point::new(...)`: a file-imported namespace
    // reaches a struct's associated function inside another file. The result is a
    // `Point` whose `sum()` method runs end to end.
    let main = "\
use lib::geo;
use lib::geo::{Point};

pub fn run() -> i32 {
    let p: Point = geo::Point::new(3, 4);
    return p.sum();
}
";
    let geo = "\
pub struct Point {
    x: i32;
    y: i32;

    pub fn new(a: i32, b: i32) -> Point {
        return Point { x: a, y: b };
    }

    pub fn sum(self) -> i32 {
        return self.x + self.y;
    }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geo"], geo),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 7);
}

#[test]
fn namespace_qualified_struct_literal_field_read_executes() {
    // A namespace-qualified struct literal (`geo::Point { .. }`) constructs the
    // imported struct; its field is read back at the right offset.
    let main = "\
use lib::geo;
use lib::geo::{Point};

pub fn run() -> i32 {
    let p: Point = geo::Point { x: 10, y: 20 };
    return p.x + p.y;
}
";
    let geo = "pub struct Point { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geo"], geo),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 30);
}

#[test]
fn namespace_qualified_enum_variant_executes() {
    // A namespace-qualified enum variant (`geo::Signal::Stop`) lowers to its
    // declaration-order tag (Go=0, Slow=1, Stop=2).
    let main = "\
use lib::geo;
use lib::geo::{Signal};

pub fn run() -> i32 {
    let s: Signal = geo::Signal::Stop;
    return pick(s);
}

fn pick(s: Signal) -> i32 {
    return 0;
}
";
    let geo = "pub enum Signal { Go, Slow, Stop }";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geo"], geo),
    ]);

    // Compiles, validates, and instantiates; the variant lowering is exercised by
    // the `let s = geo::Signal::Stop` initializer reaching codegen.
    let (_store, _instance) = instantiate(&wasm);
}

#[test]
fn non_entry_fn_imported_struct_param_executes() {
    // A non-entry file's function takes an item-imported struct by value; the
    // entry constructs that struct and passes it. The param's type must carry the
    // imported struct's canonical key so the call type-checks, then the value is
    // passed and a field read inside the callee returns the right offset (#63).
    let main = "\
use lib::geo::{Point};
use lib::ops::{flip};

pub fn run() -> i32 {
    let p: Point = Point { x: 1, y: 2 };
    return flip(p);
}
";
    let geo = "pub struct Point { x: i32; y: i32; }";
    let ops = "\
use lib::geo::{Point};

pub fn flip(p: Point) -> i32 {
    return p.y;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geo"], geo),
        (vec!["lib", "ops"], ops),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 2);
}

#[test]
fn nested_cross_file_struct_field_laid_out_by_definer() {
    // `Outer` (in `lib/a.inf`) nests an `Inner { a; b }` (8 bytes), so `tag` sits
    // at offset 8. The entry imports `Outer` and defines its *own* same-named
    // `Inner { a }` (4 bytes). Reading `o.tag` from the entry must use the
    // definer's layout (offset 8) rather than the entry's smaller `Inner` (which
    // would mis-read offset 4 and return the smuggled `Inner.b`). A single shared
    // layout would also let a larger entry `Inner` write past the struct (#63).
    let main = "\
use lib::a::{Outer};

struct Inner {
    a: i32;
}

pub fn run() -> i32 {
    let o: Outer = Outer::make();
    return o.tag;
}
";
    let lib_a = "\
struct Inner {
    a: i32;
    b: i32;
}

pub struct Outer {
    inner: Inner;
    tag: i32;

    pub fn make() -> Outer {
        return Outer { inner: Inner { a: 111, b: 222 }, tag: 333 };
    }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "a"], lib_a),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 333);
}

#[test]
fn nested_cross_file_struct_read_via_method_laid_out_by_definer() {
    // The same layout divergence reached through a `&self` method: `read_tag`
    // loads `self.tag` at the definer's offset 8, so the call returns 333 even
    // though the entry's same-named `Inner` is a different size (#63).
    let main = "\
use lib::a::{Outer};

struct Inner {
    a: i32;
}

pub fn run() -> i32 {
    let o: Outer = Outer::make();
    return o.read_tag();
}
";
    let lib_a = "\
struct Inner {
    a: i32;
    b: i32;
}

pub struct Outer {
    inner: Inner;
    tag: i32;

    pub fn make() -> Outer {
        return Outer { inner: Inner { a: 111, b: 222 }, tag: 333 };
    }

    pub fn read_tag(self) -> i32 {
        return self.tag;
    }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "a"], lib_a),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 333);
}

#[test]
fn depth_two_nested_cross_file_struct_laid_out_by_definer() {
    // Two levels of cross-file nesting: `Outer { mid: Mid }` and
    // `Mid { inner: Inner }` all live in `lib/a.inf`, while the entry defines
    // unrelated same-named `Inner` and `Mid`. Laying `Outer` out must thread each
    // struct's own defining file at every level, so `tag` lands past `Mid`'s full
    // 12 bytes and `o.tag` returns 444 (#63).
    let main = "\
use lib::a::{Outer};

struct Inner {
    x: i32;
}

struct Mid {
    z: i32;
}

pub fn run() -> i32 {
    let o: Outer = Outer::make();
    return o.tag;
}
";
    let lib_a = "\
struct Inner {
    x: i32;
    y: i32;
}

struct Mid {
    inner: Inner;
    m: i32;
}

pub struct Outer {
    mid: Mid;
    tag: i32;

    pub fn make() -> Outer {
        return Outer { mid: Mid { inner: Inner { x: 1, y: 2 }, m: 3 }, tag: 444 };
    }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "a"], lib_a),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 444);
}

#[test]
fn imported_struct_with_private_field_type_compiles_and_runs() {
    // The entry imports `Outer` but defines *no* `Inner` of its own. `Outer`'s
    // field type `Inner` is private to `lib/a.inf`, so resolving it relative to
    // the entry would fail to find any `Inner`. Laying `Outer` out by its definer
    // resolves the private `Inner` from `lib/a.inf` regardless of the access site,
    // so the legitimate program compiles and `o.tag` returns 333 (#63).
    let main = "\
use lib::a::{Outer};

pub fn run() -> i32 {
    let o: Outer = Outer::make();
    return o.tag;
}
";
    let lib_a = "\
struct Inner {
    a: i32;
    b: i32;
}

pub struct Outer {
    inner: Inner;
    tag: i32;

    pub fn make() -> Outer {
        return Outer { inner: Inner { a: 111, b: 222 }, tag: 333 };
    }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "a"], lib_a),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 333);
}

#[test]
fn entry_self_root_qualified_call_executes() {
    // `use root;` binds the entry file as a namespace exposing its own `pub`
    // items; `root::helper()` is an entry-self qualified call whose recorded
    // target's defining file *is* the entry. It must lower like a bare
    // `helper()` call rather than be mistaken for a struct associated function
    // (which previously panicked the code generator) (#63).
    let main = "\
use root;

pub fn helper() -> i32 {
    return 9;
}

pub fn run() -> i32 {
    return root::helper();
}
";

    let wasm = wasm_codegen_multi_file(&[(vec![], main)]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 9);
}

#[test]
fn nested_cross_file_struct_typed_field_read_uses_definers_layout() {
    // The gap the prior layout tests missed: they only read `o.tag` (a scalar on
    // `Outer`, served by the cached frame layout keyed on the identifier `o`).
    // This reads *through* the nested struct field — `o.mid.a` and `o.mid.b` —
    // whose receiver `o.mid` is itself a `MemberAccess`, not an identifier, so it
    // takes the slow path that re-resolves the field's struct by name. `Outer`
    // (in `lib/a.inf`) nests `lib::b::Mid { a; b }`; the entry imports a *different*
    // `other::c::Mid { b; a }` with the fields reversed. Reading `o.mid.a` must use
    // the definer's `b::Mid` (a at offset 0 = 22), not the entry-visible `c::Mid`
    // (which would read a at offset 4 = 33). The whole value must be 11223344 (#63).
    let main = "\
use lib::a::{Outer};
use lib::a;
use other::c::{Mid};

pub fn run() -> i32 {
    let o: Outer = a::make();
    return o.head*1000000 + o.mid.a*10000 + o.mid.b*100 + o.tail;
}
";
    let lib_a = "\
use lib::b::{Mid};

pub struct Outer {
    head: i32;
    mid: Mid;
    tail: i32;

    pub fn make_outer() -> Outer {
        return Outer { head: 11, mid: Mid { a: 22, b: 33 }, tail: 44 };
    }
}

pub fn make() -> Outer {
    return Outer { head: 11, mid: Mid { a: 22, b: 33 }, tail: 44 };
}
";
    let lib_b = "pub struct Mid { a: i32; b: i32; }\n";
    let other_c = "pub struct Mid { b: i32; a: i32; }\n";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "a"], lib_a),
        (vec!["lib", "b"], lib_b),
        (vec!["other", "c"], other_c),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 11223344);
}

#[test]
fn nested_cross_file_struct_typed_field_read_via_method_uses_definers_layout() {
    // The same nested-field read reached inside a method body on `Outer`. The
    // method reads `self.mid.a`/`self.mid.b`, again resolving `mid`'s struct by
    // the definer's `b::Mid`, so the call returns 11223344 even though the entry
    // sees a reversed-field `c::Mid` (#63).
    let main = "\
use lib::a::{Outer};
use lib::a;
use other::c::{Mid};

pub fn run() -> i32 {
    let o: Outer = a::make();
    return o.combined();
}
";
    let lib_a = "\
use lib::b::{Mid};

pub struct Outer {
    head: i32;
    mid: Mid;
    tail: i32;

    pub fn combined(self) -> i32 {
        return self.head*1000000 + self.mid.a*10000 + self.mid.b*100 + self.tail;
    }
}

pub fn make() -> Outer {
    return Outer { head: 11, mid: Mid { a: 22, b: 33 }, tail: 44 };
}
";
    let lib_b = "pub struct Mid { a: i32; b: i32; }\n";
    let other_c = "pub struct Mid { b: i32; a: i32; }\n";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "a"], lib_a),
        (vec!["lib", "b"], lib_b),
        (vec!["other", "c"], other_c),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 11223344);
}

#[test]
fn nested_cross_file_struct_typed_field_write_stays_in_bounds() {
    // A *write* through the nested struct field: `o.mid.b = 999`. The store must
    // target the definer's `b::Mid` offset for `b` (4), leaving `a` and `tail`
    // untouched. Against the entry-visible `c::Mid` the field order is reversed,
    // so the wrong offset would corrupt a sibling. After the write, a=22, b=999,
    // tail=44, head=11: 11*1000000 + 22*10000 + 999*100 + 44 = 11319944 (#63).
    let main = "\
use lib::a::{Outer};
use lib::a;
use other::c::{Mid};

pub fn run() -> i32 {
    let mut o: Outer = a::make();
    o.mid.b = 999;
    return o.head*1000000 + o.mid.a*10000 + o.mid.b*100 + o.tail;
}
";
    let lib_a = "\
use lib::b::{Mid};

pub struct Outer {
    head: i32;
    mid: Mid;
    tail: i32;
}

pub fn make() -> Outer {
    return Outer { head: 11, mid: Mid { a: 22, b: 33 }, tail: 44 };
}
";
    let lib_b = "pub struct Mid { a: i32; b: i32; }\n";
    let other_c = "pub struct Mid { b: i32; a: i32; }\n";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "a"], lib_a),
        (vec!["lib", "b"], lib_b),
        (vec!["other", "c"], other_c),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 11319944);
}

#[test]
fn nested_cross_file_struct_field_read_when_entry_imports_only_outer() {
    // The over-correction guard: when the entry imports *only* `Outer` (not the
    // inner `Mid`), `o.mid.a` must still compile and run. `Mid` is `pub` and
    // reached through the accessible `Outer`, so resolving the field type through
    // `Outer`'s defining file finds it even though the entry cannot name `Mid`
    // by itself (Rule 4) (#63).
    let main = "\
use lib::a::{Outer};
use lib::a;

pub fn run() -> i32 {
    let o: Outer = a::make();
    return o.head*1000000 + o.mid.a*10000 + o.mid.b*100 + o.tail;
}
";
    let lib_a = "\
use lib::b::{Mid};

pub struct Outer {
    head: i32;
    mid: Mid;
    tail: i32;
}

pub fn make() -> Outer {
    return Outer { head: 11, mid: Mid { a: 22, b: 33 }, tail: 44 };
}
";
    let lib_b = "pub struct Mid { a: i32; b: i32; }\n";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "a"], lib_a),
        (vec!["lib", "b"], lib_b),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 11223344);
}

#[test]
fn entry_type_not_ambiently_reachable_from_non_entry_file_executes_through_own_import() {
    // FIX 2 executable twin: a non-entry file (`container`) item-imports its own
    // `Inner` and calls a bare `Inner::tag()`. The entry defines a *same-named*
    // `Inner` with a different `tag` body. The bare call must bind the file's own
    // imported `Inner` (returning 1), never leaking to the entry's (which would
    // return 99) — and must not panic the code generator (#63).
    let main = "\
use container;

pub struct Inner {
    a: i32;

    pub fn tag() -> i32 {
        return 99;
    }
}

pub fn run() -> i32 {
    return container::run();
}
";
    let container = "\
use lib::types::{Inner};

pub fn run() -> i32 {
    return Inner::tag();
}
";
    let lib_types = "\
pub struct Inner {
    v: i32;

    pub fn tag() -> i32 {
        return 1;
    }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["container"], container),
        (vec!["lib", "types"], lib_types),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 1);
}
