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

use crate::utils::{wasm_codegen_multi_file, wasm_codegen_multi_file_no_analysis};

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
        "output",
        inference_wasm_codegen::CodegenOptions {
            target: Target::Wasm32,
            mode: CompilationMode::Proof,
            opt_level: OptLevel::O3,
            features: inference_wasm_codegen::EmitFeatures::default(),
        },
    )
    .expect("multi-file proof codegen should succeed")
}

/// Like [`proof_codegen_multi_file`] but returns the codegen `Result`, for
/// negative tests asserting a proof-mode obligation-translation rejection (a
/// spec body with no `hassert` encoding is now a hard codegen error).
fn try_proof_codegen_multi_file(files: &[(Vec<&str>, &str)]) -> anyhow::Result<CodegenOutput> {
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
        "output",
        inference_wasm_codegen::CodegenOptions {
            target: Target::Wasm32,
            mode: CompilationMode::Proof,
            opt_level: OptLevel::O3,
            features: inference_wasm_codegen::EmitFeatures::default(),
        },
    )
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
fn duplicate_item_imports_of_same_target_call_executes() {
    // `f` is imported directly from `orig` and again through `proxy`'s `pub use
    // orig::{f}` re-export. Both name the identical function `orig::f`, so the
    // duplicate is benign: the bare call binds once and runs the one function.
    let main = "\
use orig::{f};
use proxy::{f};

pub fn run() -> i32 {
    return f();
}
";
    let orig = "pub fn f() -> i32 { return 42; }\n";
    let proxy = "pub use orig::{f};\n";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["orig"], orig),
        (vec!["proxy"], proxy),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 42);
}

#[test]
fn reexport_qualified_struct_literal_method_call_executes() {
    // A re-export-qualified struct literal bound to a local first, then used as a
    // method receiver, resolves the method against the literal's canonical struct
    // identity (`lib::geo::Point`) reached through `math`'s `pub use lib::geo`.
    // The method body `self.x + self.y` runs on the right struct → 42.
    let main = "\
use math;

pub fn run() -> i32 {
    let p: math::geo::Point = math::geo::Point { x: 30, y: 12 };
    return p.sum();
}
";
    let math = "pub use lib::geo;\n";
    let geo = "\
pub struct Point {
    x: i32;
    y: i32;

    pub fn sum(self) -> i32 {
        return self.x + self.y;
    }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["math"], math),
        (vec!["lib", "geo"], geo),
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
    // entries from different files stay distinct. Each spec function claims a
    // property over its own universal slot: a spec free function whose body only
    // computes has no obligation to carry and is rejected outright, so there
    // would be no per-spec entry left to key.
    let main = "\
spec EntrySpec {
    fn obligation() forall { let n: i32 = @; assert(n == n); }
}

pub fn main() {}
";
    let lib_checks = "\
spec LibSpec {
    fn obligation() forall { let n: i32 = @; assert(n == n); }
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
fn field_position_uzumaki_with_qualified_cross_file_struct_is_rejected() {
    // A field-position uzumaki on a qualified cross-file struct type
    // (`lib::geom::Point { x: @, y: @ }`) inside a spec's `forall`: the
    // qualified literal itself now resolves and binds a leaf value tree
    // (cross-file struct types classify exactly like local ones), but `@` is
    // an introduction form only where a `let` or a call argument binds it —
    // a literal field is neither, so proof-mode codegen rejects each
    // field-position `@` with `P006`.
    let main = "\
use lib::geom;

spec S {
    fn prop() forall {
        let p: lib::geom::Point = lib::geom::Point { x: @, y: @ };
        assert(p.x == p.x);
    }
}

pub fn main() {}
";
    let geom = "pub struct Point { x: i32; y: i32; }";

    let err = try_proof_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], geom)])
        .expect_err("a field-position uzumaki has no binding to introduce it");
    assert!(
        err.to_string().contains("P006"),
        "expected a P006 rejection for each field-position uzumaki; got:\n{err}"
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
    //
    // Depth-two nesting is past A026's one supported level, so this is a
    // codegen-only test (analysis skipped): it pins the layout-threading the
    // codegen path performs, independent of the analysis gate.
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

    let wasm = wasm_codegen_multi_file_no_analysis(&[
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

#[test]
fn qualified_enum_annotation_executes() {
    // A namespace-qualified enum annotation (`let x: geo::Level`) must lower to
    // the same enum tag the qualified value carries, so the `==` compares equal
    // and the branch is taken (#63).
    let main = "\
use geo;

pub fn run() -> i32 {
    let x: geo::Level = geo::Level::High;
    if x == geo::Level::High { return 2; }
    return 0;
}
";
    let geo = "pub enum Level { Low, Med, High }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["geo"], geo)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 2);
}

#[test]
fn three_segment_qualified_struct_annotation_executes() {
    // A 3-segment qualified struct annotation (`let p: lib::geom::Point`) must
    // parse, resolve the struct's cross-file layout, and read the right field
    // offset — returning the first field's value (#63).
    let main = "\
use lib::geom;

pub fn run() -> i32 {
    let p: lib::geom::Point = lib::geom::Point { x: 8, y: 9 };
    return p.x;
}
";
    let lib_geom = "pub struct Point { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], lib_geom)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 8);
}

#[test]
fn qualified_struct_annotation_in_return_executes() {
    // A qualified type in return position (`-> lib::geom::Point`) round-trips a
    // constructed struct across a function boundary; the caller reads the second
    // field to confirm the layout is consistent on both sides (#63).
    let main = "\
use lib::geom;

pub fn make() -> lib::geom::Point {
    return lib::geom::Point { x: 1, y: 7 };
}

pub fn run() -> i32 {
    let p: lib::geom::Point = make();
    return p.y;
}
";
    let lib_geom = "pub struct Point { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], lib_geom)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 7);
}

#[test]
fn root_qualified_param_type_executes() {
    // A non-entry function whose parameter is `root::`-qualified (naming an entry
    // struct) must lower the param as an I32 pointer and read its field — proving
    // `root::T` resolves to the entry file's canonical (bare) key (#63).
    let main = "\
use lib::b::{describe};

pub struct Pt { x: i32; }

pub fn run() -> i32 {
    let p: Pt = Pt { x: 5 };
    return describe(p);
}
";
    let lib_b = "\
use root;

pub fn describe(p: root::Pt) -> i32 {
    return p.x;
}
";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "b"], lib_b)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 5);
}

#[test]
fn uncalled_fn_with_qualified_param_type_compiles() {
    // An *uncalled* non-entry function whose parameter is a `root::`-qualified
    // type still reaches codegen for its signature. It must lower the qualified
    // param type without panicking — the regression that hit a `todo!()` (#63).
    let main = "\
use lib::b::{unused};

pub struct Pt { x: i32; }

pub fn run() -> i32 {
    return 1;
}
";
    let lib_b = "\
use root;

pub fn unused(o: root::Pt) -> i32 {
    return 7;
}
";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "b"], lib_b)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 1);
}

#[test]
fn qualified_struct_field_type_executes() {
    // A struct field declared with a `::`-qualified cross-file type
    // (`p: lib::geom::Point`) must lay out by the field struct's own defining
    // file and read the nested field at the right offset — the regression that
    // hit a byte-size `todo!()` for a `Qualified` field kind (#63).
    let main = "\
use lib::geom;

pub struct Wrapper {
    p: lib::geom::Point;
}

pub fn run() -> i32 {
    let w: Wrapper = Wrapper { p: lib::geom::Point { x: 3, y: 4 } };
    return w.p.x;
}
";
    let lib_geom = "pub struct Point { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], lib_geom)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 3);
}

#[test]
fn local_array_of_qualified_struct_element_executes() {
    // A local array whose element type is a `::`-qualified cross-file struct
    // (`[lib::geom::Point; 2]`) must resolve the element layout by the struct's
    // canonical key, not by bare name at the access site. A bare-name miss left the
    // element layout unresolved and routed a struct literal into the scalar path,
    // hitting an `unreachable!` in codegen. Reading both fields of the second
    // element confirms the per-element offsets are correct (#63).
    let main = "\
use lib::geom;

pub fn run() -> i32 {
    let arr: [lib::geom::Point; 2] = [lib::geom::Point { x: 1, y: 2 }, lib::geom::Point { x: 3, y: 4 }];
    return arr[1].x + arr[1].y;
}
";
    let lib_geom = "pub struct Point { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], lib_geom)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 7);
}

#[test]
fn local_array_of_item_imported_struct_element_executes() {
    // The item-import control for the qualified-element array: bringing `Point` in
    // by bare name lays out and executes identically.
    let main = "\
use lib::geom::{Point};

pub fn run() -> i32 {
    let arr: [Point; 2] = [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }];
    return arr[1].x + arr[1].y;
}
";
    let lib_geom = "pub struct Point { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], lib_geom)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 7);
}

#[test]
fn nested_array_of_qualified_struct_uses_definers_layout_despite_local_collision() {
    // The nested/multi-dim store path is the gap the single-dim fix missed: a
    // `[[lib::geom::Pt; 2]; 1]` literal descends through the `Array` arm to a
    // struct leaf whose layout was computed by bare name. With a same-named LOCAL
    // `struct Pt { y; x; }` (fields reversed), the store used the local layout
    // (y@0, x@4) while the member reads used the canonical `lib::geom::Pt` layout
    // (x@0, y@4) — a silent miscompile. The leaf must lay out by the element's
    // defining file so stores and reads agree: `m[0][0].x` is 100 and
    // `m[0][1].y` is 400, summing to 500 (#63).
    let main = "\
use lib::geom;

struct Pt { y: i32; x: i32; }

pub fn run() -> i32 {
    let m: [[lib::geom::Pt; 2]; 1] = [[lib::geom::Pt { x: 100, y: 1 }, lib::geom::Pt { x: 3, y: 400 }]];
    return m[0][0].x + m[0][1].y;
}
";
    let lib_geom = "pub struct Pt { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], lib_geom)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 500);
}

#[test]
fn nested_array_of_qualified_struct_compiles_with_layout_divergent_local_struct() {
    // A local `struct Pt { a; b; c; d }` with NO `x`/`y` fields would, under the
    // bare-name leaf lookup, be found for the qualified element's literal and then
    // panic looking up field `x` in the 4-field local layout. Resolving the leaf
    // by canonical key finds the cross-file `lib::geom::Pt` instead, so the literal
    // lays out and runs (#63).
    let main = "\
use lib::geom;

struct Pt { a: i32; b: i32; c: i32; d: i32; }

pub fn run() -> i32 {
    let m: [[lib::geom::Pt; 2]; 1] = [[lib::geom::Pt { x: 100, y: 1 }, lib::geom::Pt { x: 3, y: 400 }]];
    return m[0][0].x + m[0][1].y;
}
";
    let lib_geom = "pub struct Pt { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], lib_geom)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 500);
}

#[test]
fn nested_array_of_qualified_struct_without_local_collision_executes() {
    // With no local `Pt` at all, the bare-name lookup of the *qualified* leaf
    // `lib::geom::Pt` returned `None`, dropping the struct-literal element into the
    // scalar path and hitting an `unreachable!`. The canonical-key resolution finds
    // it, so the literal stores and reads back: 1 + 4 = 5 (#63).
    let main = "\
use lib::geom;

pub fn run() -> i32 {
    let m: [[lib::geom::Pt; 2]; 1] = [[lib::geom::Pt { x: 1, y: 2 }, lib::geom::Pt { x: 3, y: 4 }]];
    return m[0][0].x + m[0][1].y;
}
";
    let lib_geom = "pub struct Pt { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], lib_geom)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 5);
}

#[test]
fn three_dimensional_array_of_qualified_struct_executes() {
    // The recursion must thread the element's canonical key through every `Array`
    // level, not just the outermost: a `[[[lib::geom::Pt; 2]; 1]; 1]` literal
    // reaches its struct leaf two `Array` hops deep. Reading across the innermost
    // elements (10 + 40 = 50) confirms the deep leaf layout is the definer's (#63).
    let main = "\
use lib::geom;

pub fn run() -> i32 {
    let m: [[[lib::geom::Pt; 2]; 1]; 1] = [[[lib::geom::Pt { x: 10, y: 20 }, lib::geom::Pt { x: 30, y: 40 }]]];
    return m[0][0][0].x + m[0][0][1].y;
}
";
    let lib_geom = "pub struct Pt { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], lib_geom)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 50);
}

#[test]
fn nested_array_of_item_imported_struct_executes() {
    // The item-import control for the nested-array fix: bringing `Pt` in by bare
    // name lays out and executes the 2D literal identically (#63).
    let main = "\
use lib::geom::{Pt};

pub fn run() -> i32 {
    let m: [[Pt; 2]; 1] = [[Pt { x: 100, y: 1 }, Pt { x: 3, y: 400 }]];
    return m[0][0].x + m[0][1].y;
}
";
    let lib_geom = "pub struct Pt { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], lib_geom)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 500);
}

#[test]
fn two_segment_qualified_annotation_with_sibling_file_executes() {
    // The end-to-end twin of the type-checker regression: a `let p: g::Pt`
    // annotation, with a sibling `g/Pt.inf` in the closure (imported by `z`), must
    // resolve `Pt` to the type `g.inf` defines and execute, reading its field (5).
    let main = "\
use g;
use z;

pub fn run() -> i32 {
    let p: g::Pt = g::Pt::make();
    return p.x + z::touch();
}
";
    let g = "pub struct Pt { x: i32; pub fn make() -> Pt { return Pt { x: 5 }; } }";
    let z = "use g::Pt; pub fn touch() -> i32 { return 0; }";
    let g_pt = "pub fn make() -> i32 { return 999; }";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["g"], g),
        (vec!["z"], z),
        (vec!["g", "Pt"], g_pt),
    ]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 5);
}

#[test]
fn absolute_path_from_importing_non_entry_file_executes() {
    // The execution twin of the import-discipline fix: a non-entry file that
    // imported `lib::geom` may spell the deep `lib::geom::val()` it holds, and the
    // cross-file call lowers and runs (returns 7). The leak form (no import) is
    // rejected at type-check; this pins that the licensed long spelling executes (#63).
    let main = "\
use helper;

pub fn run() -> i32 {
    return helper::go();
}
";
    let helper = "\
use lib::geom;

pub fn go() -> i32 {
    return lib::geom::val();
}
";
    let lib_geom = "pub fn val() -> i32 { return 7; }";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["helper"], helper),
        (vec!["lib", "geom"], lib_geom),
    ]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 7);
}

#[test]
fn spec_helper_and_top_level_distinct_names_execute() {
    // The no-collision control for the spec/top-level duplicate guard: a spec
    // helper struct whose name differs from the top-level one compiles and runs.
    let main = "\
spec S { struct Inner { a: i32; b: i32; c: i32; } }

pub struct Point { v: i32; pub fn get(self) -> i32 { return self.v + 1; } }

pub fn run() -> i32 {
    let p: Point = Point { v: 41 };
    return p.get();
}
";

    let wasm = wasm_codegen_multi_file(&[(vec![], main)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 42);
}

#[test]
fn cross_file_method_dispatches_by_receiver_canonical_identity() {
    // A nested cross-file value reached through a same-named local struct: `o.inner`
    // has canonical type `lib::geo::Inner`, whose `get()` returns 99999. The entry
    // also defines its own `Inner` with a `get()` returning 11111. Dispatch must
    // follow the receiver's canonical identity (the foreign `Inner`), not the bare
    // name `Inner` resolved at the call site (which finds the entry's struct). The
    // method body that runs is the one belonging to the value's actual type.
    let main = "\
struct Inner { a: i32; b: i32; pub fn get(self) -> i32 { return 11111; } }
use lib::geo::{Outer, build};
pub fn main() -> i32 {
    let o: Outer = build();
    return o.inner.get();
}
";
    let geo = "\
pub struct Inner { secret: i32; tag: i32; pub fn get(self) -> i32 { return 99999; } }
pub struct Outer { inner: Inner; }
pub fn build() -> Outer {
    let i: Inner = Inner { secret: 1, tag: 2 };
    let o: Outer = Outer { inner: i };
    return o;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geo"], geo),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "main"), 99999);
}

#[test]
fn cross_file_method_misdispatch_does_not_read_past_receiver() {
    // The memory-safety face of the same bug: the two same-named `Inner` structs
    // have *different* field counts. `lib::geo::Inner` has one field (`a` at offset
    // 0); the entry's `Inner` has two (`p`, `b`) with `get(self){ return self.b; }`
    // reading offset 4. The receiver `o.inner` is a one-field `lib::geo::Inner`, so
    // a mis-dispatch to the entry's `get` would load offset 4 — past the receiver's
    // allocation — and return the adjacent `marker` (777). Correct dispatch runs
    // `lib::geo::Inner::get`, reading offset 0 and returning the stored 42.
    let main = "\
struct Inner { p: i32; b: i32; pub fn get(self) -> i32 { return self.b; } }
use lib::geo::{Outer, make};
pub fn main() -> i32 {
    let o: Outer = make();
    return o.inner.get();
}
";
    let geo = "\
pub struct Inner { a: i32; pub fn get(self) -> i32 { return self.a; } }
pub struct Outer { inner: Inner; marker: i32; }
pub fn make() -> Outer {
    let i: Inner = Inner { a: 42 };
    let o: Outer = Outer { inner: i, marker: 777 };
    return o;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geo"], geo),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "main"), 42);
}

#[test]
fn bare_and_namespaced_assoc_fn_pick_correct_same_named_struct() {
    // Two files each define `struct Counter` with an associated `fn make()`
    // returning a distinct value. A bare `Counter::make()` resolves to the entry's
    // struct (111); a namespaced `lib::x::Counter::make()` resolves to the imported
    // file's struct (222). Both spellings must dispatch by canonical identity.
    let main = "\
use lib::x;

struct Counter { w: i32; pub fn make() -> i32 { return 111; } }

pub fn entry_assoc() -> i32 {
    return Counter::make();
}

pub fn lib_assoc() -> i32 {
    return lib::x::Counter::make();
}
";
    let lib_x = "pub struct Counter { v: i32; pub fn make() -> i32 { return 222; } }";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "x"], lib_x),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "entry_assoc"), 111);
    assert_eq!(call_i32(&mut store, &instance, "lib_assoc"), 222);
}

#[test]
fn entry_file_import_does_not_hijack_lib_bare_assoc_call() {
    // The entry file imports `lib::Point`, which names *both* the struct `Point`
    // defined inside `lib.inf` and the sibling file `lib/Point.inf`. That brace-free
    // entry import must not leak its file-namespace binding across the file boundary:
    // inside `lib.inf`, a bare `Point::new()` is the local struct's associated
    // function (7), never the sibling file's free `new` (5). Leaking the binding
    // selected the sibling file and silently returned 5 — a miscompile (#63).
    let main = "\
use lib;
use lib::Point;

pub fn run() -> i32 {
    return lib::struct_new();
}
";
    let lib = "\
pub struct Point {
    v: i32;

    pub fn new() -> i32 {
        return 7;
    }
}

pub fn struct_new() -> i32 {
    return Point::new();
}
";
    let lib_point = "\
pub fn new() -> i32 {
    return 5;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "Point"], lib_point),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 7);
}

#[test]
fn entry_file_import_does_not_hijack_lib_bare_assoc_call_control() {
    // Control for the leak test: the entry drops `use lib::Point;` but the sibling
    // file `lib/Point.inf` stays in the closure (the entry still imports `lib`,
    // which is enough to assemble all three files here). A bare `Point::new()` inside
    // `lib.inf` already means the local struct (7); the leak test asserts adding the
    // entry import changes nothing.
    let main = "\
use lib;

pub fn run() -> i32 {
    return lib::struct_new();
}
";
    let lib = "\
pub struct Point {
    v: i32;

    pub fn new() -> i32 {
        return 7;
    }
}

pub fn struct_new() -> i32 {
    return Point::new();
}
";
    let lib_point = "\
pub fn new() -> i32 {
    return 5;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "Point"], lib_point),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 7);
}

#[test]
fn entry_file_import_does_not_break_lib_struct_construction() {
    // The sret variant of the leak: `lib.inf` builds its own `Point` via the local
    // struct's `Point::new()`. The leaked sibling-file `new` returns `i32`, so the
    // entry import previously caused a false type rejection (`expected lib::Point,
    // found i32`). With the boundary honored, `Point::new()` is the struct's
    // constructor and the field sum is read correctly (300) (#63).
    let main = "\
use lib;
use lib::Point;

pub fn run() -> i32 {
    return lib::build_and_read();
}
";
    let lib = "\
pub struct Point {
    x: i32;
    y: i32;

    pub fn new() -> Point {
        return Point { x: 100, y: 200 };
    }
}

pub fn build_and_read() -> i32 {
    let p: Point = Point::new();
    return p.x + p.y;
}
";
    let lib_point = "\
pub fn new() -> i32 {
    return 7;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "Point"], lib_point),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 300);
}

#[test]
fn qualified_struct_assoc_call_wins_over_same_named_sibling_file() {
    // A type defined in a file wins over a same-named sibling file when resolving a
    // qualified `parent::Name::member` path: `lib::Point::new()` means the struct
    // `Point` in `lib.inf` (1), not the sibling file `lib/Point.inf`'s free `new`
    // (1000). The meaning is deterministic — it must not flip based on whether the
    // sibling file happens to be in the import closure (#63).
    let main = "\
use lib;
use lib::Point;

pub fn run() -> i32 {
    return lib::Point::new();
}
";
    let lib = "\
pub struct Point {
    v: i32;

    pub fn new() -> i32 {
        return 1;
    }
}
";
    let lib_point = "\
pub fn new() -> i32 {
    return 1000;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "Point"], lib_point),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 1);
}

// Closure-dependent tail-precedence flip (#63).
//
// `a::mid::make()` written with `use a::mid;` (the SUB-FILE) must reach
// `a/mid.inf`'s free `make` (2), never `a.inf`'s `struct mid` associated
// `make` (1). The accessing file imported the sub-file, not its parent `a`,
// so a same-named `struct mid` in the parent the walk lands in must not flip
// the meaning. Critically, the meaning must be DETERMINISTIC: it must not
// depend on whether a *third* file dragged `a.inf` into the import closure.
//
// The fix gates the tail type-precedence break on the accessing file having
// imported the parent file the walk stands in (`file_imports_namespace_key`),
// so the flip can no longer be triggered by another file's `use a;`.

/// The third-file-cannot-flip pin: `main` imports only `a::mid` (the sub-file),
/// while `other` imports the parent `a`, dragging `a.inf` into the closure.
/// `a::mid::make()` must still resolve to the sub-file's free `make` (2) — the
/// parent's `struct mid::make` (1) must not win, because `main` never imported
/// `a`. Before the fix this returned 1, flipped by `other`'s `use a;`.
#[test]
fn sub_file_call_not_flipped_by_third_file_importing_parent() {
    let main = "\
use a::mid;
use other;

pub fn entry() -> i32 {
    return a::mid::make();
}
";
    let a = "\
pub struct mid {
    junk: i32;

    pub fn make() -> i32 {
        return 1;
    }
}
";
    let a_mid = "\
pub fn make() -> i32 {
    return 2;
}
";
    let other = "\
use a;

pub fn touch() -> i32 {
    return 0;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["a"], a),
        (vec!["a", "mid"], a_mid),
        (vec!["other"], other),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "entry"), 2);
}

/// The no-third-file determinism pin: the identical program WITHOUT the
/// parent-importing third file must give the SAME answer (2). Pairing this with
/// [`sub_file_call_not_flipped_by_third_file_importing_parent`] proves the
/// resolution is closure-independent — the two halves of the architect's matrix.
#[test]
fn sub_file_call_resolves_to_sub_file_without_third_file() {
    let main = "\
use a::mid;

pub fn entry() -> i32 {
    return a::mid::make();
}
";
    let a = "\
pub struct mid {
    junk: i32;

    pub fn make() -> i32 {
        return 1;
    }
}
";
    let a_mid = "\
pub fn make() -> i32 {
    return 2;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["a"], a),
        (vec!["a", "mid"], a_mid),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "entry"), 2);
}

/// A same-named parent struct whose only matching member is an INSTANCE method
/// must NOT pre-empt the sub-file namespace segment. `lib` defines `struct geom`
/// with an instance `mk(self)`, but `lib::geom::mk` is a free fn in the sub-file
/// `lib/geom.inf`. `lib::geom::mk()` is an associated-style call: an instance
/// method can never satisfy it, so the viability probe must reject the type-shadow
/// break and consume `geom` as the sub-file namespace, resolving the free `mk`
/// (77). Treating the instance method as viable would instead reject the call as
/// "instance method requires a receiver".
#[test]
fn instance_method_shadow_resolves_sub_file_free_fn() {
    let main = "\
use lib;
use lib::geom;

pub fn entry() -> i32 {
    let r: lib::geom::Widget = lib::geom::mk();
    return r.size();
}
";
    let lib = "\
pub struct geom {
    junk: i32;

    pub fn mk(self) -> i32 {
        return 1;
    }
}

pub fn unused() -> i32 {
    return 0;
}
";
    let lib_geom = "\
pub struct Widget {
    s: i32;

    pub fn size(self) -> i32 {
        return self.s;
    }
}

pub fn mk() -> Widget {
    return Widget { s: 77 };
}
";
    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "geom"], lib_geom),
    ]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "entry"), 77);
}

/// Closure-independence: the same `lib::geom::mk()` resolves to the sub-file
/// free `mk` (77) whether or not the unrelated `use lib;` (which drags the
/// same-named `struct geom` into scope) is present — the instance method must
/// never flip the resolution. Companion to
/// [`instance_method_shadow_resolves_sub_file_free_fn`] (the WITH-`use lib;` half).
#[test]
fn instance_method_shadow_resolution_is_closure_independent() {
    let main = "\
use lib::geom;

pub fn entry() -> i32 {
    let r: lib::geom::Widget = lib::geom::mk();
    return r.size();
}
";
    let lib = "\
pub struct geom {
    junk: i32;

    pub fn mk(self) -> i32 {
        return 1;
    }
}

pub fn unused() -> i32 {
    return 0;
}
";
    let lib_geom = "\
pub struct Widget {
    s: i32;

    pub fn size(self) -> i32 {
        return self.s;
    }
}

pub fn mk() -> Widget {
    return Widget { s: 77 };
}
";
    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "geom"], lib_geom),
    ]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "entry"), 77);
}

/// Struct-wins-with-parent: the companion to the flip pin. When the accessing
/// file DOES import the parent (`use lib;`), the walked-into parent's same-named
/// `struct Point` correctly pre-empts the sibling file at the tail —
/// `lib::Point::new()` is the struct's associated `new` (1), not the sibling
/// `lib/Point.inf`'s free `new` (1000). This mirrors the existing pinning test
/// and confirms the tail conjunct still fires when the parent IS imported.
#[test]
fn struct_wins_at_tail_when_parent_is_imported() {
    let main = "\
use lib;
use lib::Point;

pub fn run() -> i32 {
    return lib::Point::new();
}
";
    let lib = "\
pub struct Point {
    v: i32;

    pub fn new() -> i32 {
        return 1;
    }
}
";
    let lib_point = "\
pub fn new() -> i32 {
    return 1000;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "Point"], lib_point),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 1);
}

/// A NON-entry accessor pins the same property off the entry file: a coverage
/// gap the architect flagged (all existing flip tests put the accessor in the
/// entry). `consumer.inf` imports only `a::mid` and calls `a::mid::make()`; the
/// entry separately imports the parent `a` (dragging `a.inf` into the closure).
/// The non-entry consumer must still reach the sub-file's `make` (2).
#[test]
fn non_entry_sub_file_call_not_flipped_by_entry_importing_parent() {
    let main = "\
use a;
use consumer;

pub fn entry() -> i32 {
    return consumer::run();
}
";
    let a = "\
pub struct mid {
    junk: i32;

    pub fn make() -> i32 {
        return 1;
    }
}
";
    let a_mid = "\
pub fn make() -> i32 {
    return 2;
}
";
    let consumer = "\
use a::mid;

pub fn run() -> i32 {
    return a::mid::make();
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["a"], a),
        (vec!["a", "mid"], a_mid),
        (vec!["consumer"], consumer),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "entry"), 2);
}

/// A single-file byte-identity guard: a lone entry file with a `Type::assoc()`
/// call (no imports, no sibling files) must compile to exactly the same module
/// the tail-precedence conjunct never touches. This pins that the cross-file
/// precedence changes are inert for the common single-file shape.
#[test]
fn single_file_assoc_call_unaffected_by_fix17() {
    let main = "\
pub struct Counter {
    v: i32;

    pub fn start() -> i32 {
        return 42;
    }
}

pub fn run() -> i32 {
    return Counter::start();
}
";

    let wasm = wasm_codegen_multi_file(&[(vec![], main)]);
    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 42);
}

#[test]
fn non_entry_files_resolve_namespace_aliases_against_their_own_imports() {
    // A direct file-boundary regression: two non-entry files each import a
    // *different* file under the same local alias `n` (`use a::n;` and `use b::n;`
    // both bind the last segment `n`). Each file's two-segment `n::value()` must
    // resolve against its own import, never the sibling file's identically-named
    // binding. A parent-chain walk that ignored the file boundary would let one
    // file's `n` shadow the other through a shared ancestor. `left` reads 11,
    // `right` reads 22 (#63).
    let main = "\
use left;
use right;

pub fn run_left() -> i32 {
    return left::pick();
}

pub fn run_right() -> i32 {
    return right::pick();
}
";
    let left = "\
use a::n;

pub fn pick() -> i32 {
    return n::value();
}
";
    let right = "\
use b::n;

pub fn pick() -> i32 {
    return n::value();
}
";
    let a_n = "pub fn value() -> i32 { return 11; }";
    let b_n = "pub fn value() -> i32 { return 22; }";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["left"], left),
        (vec!["right"], right),
        (vec!["a", "n"], a_n),
        (vec!["b", "n"], b_n),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run_left"), 11);
    assert_eq!(call_i32(&mut store, &instance, "run_right"), 22);
}

#[test]
fn method_on_imported_fn_return_value_dispatches_by_canonical_identity() {
    // The receiver is the *return value* of an imported function rather than a
    // nested field. `pt()` returns a `lib::geo::Point`; the value is bound and
    // `.sum()` called on it must run `lib::geo::Point::sum` (10 + 20 = 30) even
    // though the entry defines its own same-named `Point` whose `sum()` returns a
    // sentinel. (The receiver is `let`-bound rather than chained directly off the
    // call because analysis rule A018 rejects a method chain on a compound return.)
    let main = "\
use lib::geo::{Point, pt};

pub fn run() -> i32 {
    let p: Point = pt();
    return p.sum();
}
";
    let geo = "\
pub struct Point {
    x: i32;
    y: i32;

    pub fn sum(self) -> i32 {
        return self.x + self.y;
    }
}

pub fn pt() -> Point {
    return Point { x: 10, y: 20 };
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geo"], geo),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 30);
}

#[test]
fn chained_method_returning_cross_file_type_dispatches_correctly() {
    // A method chain whose intermediate result is a cross-file type: `make()`
    // returns a `lib::geo::Wrap`; `.inner()` returns its `lib::geo::Point`; `.sum()`
    // runs on that `Point`. Every link must dispatch by the value's canonical
    // identity rather than the entry's same-named `Point`/`Wrap`.
    let main = "\
use lib::geo::{Wrap, Point, make};

pub fn run() -> i32 {
    let w: Wrap = make();
    let p: Point = w.inner();
    return p.sum();
}
";
    let geo = "\
pub struct Point {
    x: i32;
    y: i32;

    pub fn sum(self) -> i32 {
        return self.x + self.y;
    }
}

pub struct Wrap {
    p: Point;

    pub fn inner(self) -> Point {
        return self.p;
    }
}

pub fn make() -> Wrap {
    return Wrap { p: Point { x: 100, y: 23 } };
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geo"], geo),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    // `w.inner()` yields a `lib::geo::Point`; its `sum()` reads 100 + 23 = 123.
    assert_eq!(call_i32(&mut store, &instance, "run"), 123);
}

#[test]
fn proof_artifact_dispatches_method_by_canonical_identity() {
    // The Rocq `.v` artifact is the product of proof mode, so it must reason about
    // the *correct* method body. Repro A's two same-named `Inner::get` bodies
    // (entry returns 11111, `lib::geo` returns 99999) both survive into the module
    // as distinct functions; `main`'s call must target the `lib::geo` body, since
    // `o.inner` has canonical type `lib::geo::Inner`. Resolving by the call-site
    // bare name would emit a call to the entry's body and silently verify the wrong
    // method.
    let main = "\
struct Inner { a: i32; b: i32; pub fn get(self) -> i32 { return 11111; } }
use lib::geo::{Outer, build};
pub fn main() -> i32 {
    let o: Outer = build();
    return o.inner.get();
}
";
    let geo = "\
pub struct Inner { secret: i32; tag: i32; pub fn get(self) -> i32 { return 99999; } }
pub struct Outer { inner: Inner; }
pub fn build() -> Outer {
    let i: Inner = Inner { secret: 1, tag: 2 };
    let o: Outer = Outer { inner: i };
    return o;
}
";

    let output = proof_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geo"], geo),
    ]);
    let empty = rustc_hash::FxHashMap::default();
    let v = inference::wasm_to_v(
        "Mod",
        output.wasm(),
        &empty,
        &inference::HSpecMap::default(),
    )
    .expect("proof translation should succeed");

    // Both bodies survive as distinct functions (a collapse would lose one).
    assert!(
        v.contains("Vi32 99999") && v.contains("Vi32 11111"),
        "both same-named method bodies must survive into the proof artifact:\n{v}"
    );
    // `main`'s call must target the function whose body returns 99999, proving the
    // proof reasons about `lib::geo::Inner::get` rather than the entry's body.
    let main_call_idx = main_method_call_index(&v);
    let target_body = nth_func_body_constant(&v, main_call_idx);
    assert_eq!(
        target_body, 99999,
        "main must call the canonical (`lib::geo`) `Inner::get` (99999), \
         not the entry's body (11111); v:\n{v}"
    );
}

// The following four sources previously exercised cross-file / spec-local method
// dispatch *from inside a spec body* by inspecting the translated `.v`. Under the
// wasm-verifier contract a spec function is OMITTED from the module record and,
// more fundamentally, its obligation cannot mention an instance-method call
// (`P005`, "instance methods operate on memory") or a struct value (`P005`, "its
// result is not a single scalar") — such a spec has no `hassert` encoding, so
// proof-mode codegen now rejects it. The executable-path dispatch soundness these
// scenarios protect lives on in `cross_file_method_dispatches_by_receiver_canonical_identity`
// and `cross_file_method_misdispatch_does_not_read_past_receiver` (which drive the
// same dispatch through `main`); these become negative tests pinning the rejection.

#[test]
fn spec_inner_cross_file_method_call_is_rejected() {
    // A spec body calling `o.inner.get()` on a cross-file receiver: the call to
    // the compound-returning `build()` and the instance method `get()` both lack
    // an assertion encoding, so codegen rejects the spec with `P005`.
    let main = "\
struct Inner { a: i32; b: i32; pub fn get(self) -> i32 { return 11111; } }
use lib::geo::{Outer, build};
spec Dispatch {
    fn check() -> i32 {
        let o: Outer = build();
        return o.inner.get();
    }
}
pub fn main() -> i32 {
    let o: Outer = build();
    return o.inner.get();
}
";
    let geo = "\
pub struct Inner { secret: i32; tag: i32; pub fn get(self) -> i32 { return 99999; } }
pub struct Outer { inner: Inner; }
pub fn build() -> Outer {
    let i: Inner = Inner { secret: 1, tag: 2 };
    let o: Outer = Outer { inner: i };
    return o;
}
";

    let err = try_proof_codegen_multi_file(&[(vec![], main), (vec!["lib", "geo"], geo)])
        .expect_err("a spec body calling an instance method has no assertion encoding");
    assert!(
        err.to_string().contains("P005"),
        "expected a P005 rejection for the spec's instance-method call; got:\n{err}"
    );
}

#[test]
fn spec_with_own_same_named_struct_cross_file_call_is_rejected() {
    // A spec defining its own `Helper` and calling `e.tag()` on a cross-file
    // `lib::ext::Helper`: both the compound-returning `mk()` and the instance
    // method `tag()` are `P005` in an obligation, so the spec is rejected.
    let main = "\
use lib::ext;
spec GSpec {
    struct Helper {
        x: i32;
        fn tag(self) -> i32 { return 1; }
    }
    fn check() -> i32 {
        let e: lib::ext::Helper = lib::ext::mk();
        return e.tag();
    }
}
pub fn main() -> i32 { return 0; }
";
    let ext = "\
pub struct Helper {
    v: i32;
    pub fn tag(self) -> i32 { return 2; }
}
pub fn mk() -> Helper { return Helper { v: 7 }; }
";

    let err = try_proof_codegen_multi_file(&[(vec![], main), (vec!["lib", "ext"], ext)])
        .expect_err("a spec body calling an instance method has no assertion encoding");
    assert!(
        err.to_string().contains("P005"),
        "expected a P005 rejection for the spec's cross-file method call; got:\n{err}"
    );
}

#[test]
fn spec_own_inner_struct_method_call_is_rejected() {
    // A spec calling `Helper::make()` (compound result) then `h.tag()` (instance
    // method) on its own inner struct: still `P005`, no assertion encoding.
    let main = "\
spec GSpec {
    struct Helper {
        x: i32;
        fn tag(self) -> i32 { return 42; }
        fn make() -> Helper { return Helper { x: 1 }; }
    }
    fn check() -> i32 {
        let h: Helper = Helper::make();
        return h.tag();
    }
}
pub fn main() -> i32 { return 0; }
";

    let err = try_proof_codegen_multi_file(&[(vec![], main)])
        .expect_err("a spec body calling an instance method has no assertion encoding");
    assert!(
        err.to_string().contains("P005"),
        "expected a P005 rejection for the spec's own-struct method call; got:\n{err}"
    );
}

/// Returns the index of the *second* `BI_call` in `main`'s body — the method
/// dispatch (the first call is the free `build()`). Used by the proof artifact
/// test to confirm method dispatch targets the canonical body.
fn main_method_call_index(v: &str) -> usize {
    nth_function_second_call_index(v, "main")
}

/// Returns the index of the *second* `BI_call` in the body of the function
/// named `func_name`. Each driver under test issues exactly two calls — the
/// free `build()` first, then the method dispatch — so the second call is the
/// one whose target body the test verifies.
fn nth_function_second_call_index(v: &str, func_name: &str) -> usize {
    let marker = format!("Definition {func_name} : module_func");
    let fn_start = v
        .find(&marker)
        .unwrap_or_else(|| panic!("v must define a `{func_name}` function"));
    let fn_body = &v[fn_start..];
    let fn_end = fn_body.find("|}.").expect("function body must terminate");
    let fn_body = &fn_body[..fn_end];
    let calls: Vec<usize> = fn_body
        .match_indices("BI_call ")
        .map(|(pos, _)| {
            let rest = &fn_body[pos + "BI_call ".len()..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().expect("BI_call must carry a numeric index")
        })
        .collect();
    assert_eq!(
        calls.len(),
        2,
        "`{func_name}` should call build() then the method; calls: {calls:?}"
    );
    calls[1]
}

/// Returns the `Vi32` constant in the body of the `n`th function listed in the
/// module's `mod_funcs` definition. The method bodies under test each consist of
/// a single integer return, so the first `Vi32` in the function is its result.
fn nth_func_body_constant(v: &str, n: usize) -> i32 {
    let funcs_start = v
        .find("mod_funcs :=")
        .expect("v must list mod_funcs");
    let funcs_list = &v[funcs_start..];
    let funcs_end = funcs_list.find("nil;").expect("mod_funcs must terminate");
    let func_name = funcs_list[..funcs_end]
        .lines()
        .filter_map(|line| {
            let name = line.trim().trim_end_matches(" ::").trim();
            (!name.is_empty() && name != "mod_funcs :=").then(|| name.to_string())
        })
        .nth(n)
        .expect("mod_funcs must list the called function");
    let def_marker = format!("Definition {func_name} : module_func");
    let def_start = v
        .find(&def_marker)
        .unwrap_or_else(|| panic!("v must define function `{func_name}`"));
    let def_body = &v[def_start..];
    let vi32 = def_body
        .find("Vi32 ")
        .expect("called function body must contain a constant");
    let rest = &def_body[vi32 + "Vi32 ".len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().expect("Vi32 must carry a numeric constant")
}

// Head precedence: a struct/enum defined in the accessing file pre-empts a
// same-named sibling FILE at the head of a `::` call. The meaning of a file's
// own `foo::pick()` must not depend on whether an unrelated sibling dragged a
// root-child `foo.inf` into the import closure.

#[test]
fn local_struct_assoc_call_wins_over_sibling_file_pulled_into_closure() {
    // `bar.inf` defines `struct foo` with associated `pick() -> 11` and calls
    // `foo::pick()`. An unrelated `keeper.inf` does `use foo;` (a sibling
    // root-child file whose free `pick() -> 888`), dragging `foo.inf` into the
    // import closure. `bar`'s own `foo::pick()` must still mean its local struct's
    // associated function (11), not the sibling file (888) — a silent rebind would
    // make a value depend on code `bar` cannot see.
    let main = "\
use bar;
use keeper;

pub fn run() -> i32 {
    return bar::probe();
}
";
    let foo = "pub fn pick() -> i32 { return 888; }";
    let keeper = "\
use foo;

pub fn k() -> i32 {
    return foo::pick();
}
";
    let bar = "\
pub struct foo {
    x: i32;

    pub fn pick() -> i32 {
        return 11;
    }
}

pub fn probe() -> i32 {
    return foo::pick();
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["bar"], bar),
        (vec!["foo"], foo),
        (vec!["keeper"], keeper),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 11);
}

#[test]
fn entry_local_struct_assoc_call_wins_over_sibling_file() {
    // The entry-file variant: the entry defines `struct foo` with `pick() -> 11`
    // and a sibling `bar.inf` does `use foo;`, pulling the root-child `foo.inf`
    // (free `pick() -> 888`) into the closure. The entry's own `foo::pick()` must
    // still resolve to its local struct (11).
    let main = "\
use bar;

pub struct foo {
    x: i32;

    pub fn pick() -> i32 {
        return 11;
    }
}

pub fn run() -> i32 {
    return foo::pick();
}
";
    let foo = "pub fn pick() -> i32 { return 888; }";
    let bar = "\
use foo;

pub fn b() -> i32 {
    return foo::pick();
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["bar"], bar),
        (vec!["foo"], foo),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 11);
}

#[test]
fn local_struct_and_sibling_import_resolve_independently_in_one_program() {
    // The soundness proof in a single program: `bar.inf` defines `struct foo`
    // (local `pick() -> 11`), while `keeper.inf` writes its own `use foo;` and so
    // its `foo::pick()` means the sibling file (`888`). Each file resolves the
    // same spelling `foo::pick()` against its own definitions and imports — the
    // local struct wins in `bar`, the imported file wins in `keeper`.
    let main = "\
use bar;
use keeper;

pub fn run_bar() -> i32 {
    return bar::probe();
}

pub fn run_keeper() -> i32 {
    return keeper::k();
}
";
    let foo = "pub fn pick() -> i32 { return 888; }";
    let keeper = "\
use foo;

pub fn k() -> i32 {
    return foo::pick();
}
";
    let bar = "\
pub struct foo {
    x: i32;

    pub fn pick() -> i32 {
        return 11;
    }
}

pub fn probe() -> i32 {
    return foo::pick();
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["bar"], bar),
        (vec!["foo"], foo),
        (vec!["keeper"], keeper),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run_bar"), 11);
    assert_eq!(call_i32(&mut store, &instance, "run_keeper"), 888);
}

// Distinct same-named cross-file structs are not a cycle: a one-level nested
// field typed as a same-named struct in another file lays out and reads back.

#[test]
fn distinct_same_named_cross_file_nested_struct_lays_out_and_reads() {
    // The entry `Wrap` has a field typed as `lib::m::Wrap`, a *different* struct
    // that happens to share the bare name. This is one level of nesting (the inner
    // `Wrap` has only a scalar field), so it must compile; `w.inner.v + w.tag`
    // reads 5 + 9 = 14.
    let main = "\
use lib::m;

pub struct Wrap {
    inner: lib::m::Wrap;
    tag: i32;
}

pub fn run() -> i32 {
    let w: Wrap = Wrap { inner: lib::m::Wrap { v: 5 }, tag: 9 };
    return w.inner.v + w.tag;
}
";
    let lib_m = "pub struct Wrap { v: i32; }";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "m"], lib_m),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 14);
}

#[test]
fn intermediate_namespace_segment_collides_with_parent_struct() {
    // `lib::geom::Point` where the parent file `lib.inf` defines a `struct geom`
    // that collides with the *intermediate* path segment `geom`. An intermediate
    // segment followed by a further `::Point` cannot be a type-member access (a
    // struct has no member that is itself a type), so `geom` must be consumed as
    // the sub-file namespace `lib/geom.inf`, not the parent's `struct geom`. The
    // qualified type, literal, and method call all resolve to the real `Point`.
    let main = "\
use lib;
use lib::geom;

pub fn run() -> i32 {
    let p: lib::geom::Point = lib::geom::Point { x: 100, y: 23 };
    return p.sum() + lib::tagval();
}
";
    let lib = "\
struct geom { tag: i32; }
pub fn tagval() -> i32 { return 1; }
";
    let geom = "\
pub struct Point {
    x: i32;
    y: i32;

    pub fn sum(self) -> i32 { return self.x + self.y; }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "geom"], geom),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 124);
}

#[test]
fn intermediate_namespace_segment_collides_with_parent_enum() {
    // Same intermediate-segment collision as above, but the colliding parent
    // definition is an `enum geom`. An enum has no member type either, so the
    // intermediate `geom` is still the sub-file namespace and `lib::geom::Point`
    // resolves to the real struct.
    let main = "\
use lib;
use lib::geom;

pub fn run() -> i32 {
    let p: lib::geom::Point = lib::geom::Point { x: 10, y: 4 };
    return p.sum() + lib::tagval();
}
";
    let lib = "\
enum geom { A, B }
pub fn tagval() -> i32 { return 1; }
";
    let geom = "\
pub struct Point {
    x: i32;
    y: i32;

    pub fn sum(self) -> i32 { return self.x + self.y; }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "geom"], geom),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 15);
}

#[test]
fn intermediate_namespace_segment_collides_at_three_levels() {
    // A three-level path `lib::sub::geom::Point` where the mid-level intermediate
    // segment `sub` collides with a `struct sub` declared in the parent file
    // `lib.inf`. `sub` is followed by two more `::`-segments, so the type reading
    // is impossible and the namespace walk must continue through `lib/sub.inf`.
    let main = "\
use lib;
use lib::sub::geom;

pub fn run() -> i32 {
    let p: lib::sub::geom::Point = lib::sub::geom::Point { x: 100, y: 23 };
    return p.sum() + lib::tagval();
}
";
    let lib = "\
struct sub { tag: i32; }
pub fn tagval() -> i32 { return 1; }
";
    let sub = "pub fn placeholder() -> i32 { return 0; }";
    let geom = "\
pub struct Point {
    x: i32;
    y: i32;

    pub fn sum(self) -> i32 { return self.x + self.y; }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "sub"], sub),
        (vec!["lib", "sub", "geom"], geom),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 124);
}

#[test]
fn collision_free_intermediate_namespace_path_executes() {
    // The control for the intermediate-collision repros: with no same-named
    // `struct`/`enum` in the parent file, the path resolves the same way, proving
    // the collision (not the path shape) is what the fix addresses.
    let main = "\
use lib;
use lib::geom;

pub fn run() -> i32 {
    let p: lib::geom::Point = lib::geom::Point { x: 100, y: 23 };
    return p.sum() + lib::tagval();
}
";
    let lib = "\
struct other { tag: i32; }
pub fn tagval() -> i32 { return 1; }
";
    let geom = "\
pub struct Point {
    x: i32;
    y: i32;

    pub fn sum(self) -> i32 { return self.x + self.y; }
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "geom"], geom),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 124);
}

#[test]
fn leaf_struct_assoc_still_wins_over_sibling_file() {
    // Regression guard: `lib::Point::new()` where `lib.inf` defines a
    // `struct Point` *and* a sibling `lib/Point.inf` exists. Here `Point` is the
    // second-to-last segment with a single trailing `new`, so the type
    // interpretation is viable and the struct's associated `new` wins (returns 1),
    // not the sibling file's free `new` (1000). The intermediate-collision fix
    // must not weaken this leaf precedence.
    let main = "\
use lib;

pub fn run() -> i32 {
    return lib::Point::new();
}
";
    let lib = "\
pub struct Point {
    x: i32;

    pub fn new() -> i32 { return 1; }
}
";
    let sibling = "pub fn new() -> i32 { return 1000; }";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "Point"], sibling),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 1);
}

#[test]
fn qualified_struct_param_is_passed_by_value_not_aliased() {
    // Soundness: a parameter typed with a `::`-qualified path (`p: lib::big::Big`)
    // is passed by value, exactly like a bare item-imported struct parameter.
    // Mutating a field of the parameter inside the callee must not be observable
    // in the caller's struct. A regression aliased the caller's storage (no frame
    // slot, no copy), so the mutation through the parameter clobbered the caller's
    // value and `run` returned the mutated 999 instead of the original 1.
    cov_mark::check!(wasm_codegen_emit_struct_param_copy);

    let main = "\
use lib::big;

pub fn mutate(mut p: lib::big::Big) -> i32 {
    p.a = 999;
    return p.a;
}

pub fn run() -> i32 {
    let mut orig: lib::big::Big = lib::big::Big { a: 1, b: 2 };
    let inside: i32 = mutate(orig);
    return orig.a;
}
";
    let big = "pub struct Big { a: i32; b: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "big"], big)]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 1);
}

#[test]
fn qualified_struct_param_by_value_with_multiple_args() {
    // The by-value copy must apply per qualified-struct parameter, not only to a
    // single one: mutating both parameters leaves both caller structs intact, so
    // their original fields sum to the pre-call total.
    let main = "\
use lib::big;

pub fn mutate_both(mut p: lib::big::Big, mut q: lib::big::Big) -> i32 {
    p.a = 999;
    q.a = 888;
    return p.a + q.a;
}

pub fn run() -> i32 {
    let first: lib::big::Big = lib::big::Big { a: 1, b: 0 };
    let second: lib::big::Big = lib::big::Big { a: 2, b: 0 };
    let touched: i32 = mutate_both(first, second);
    return first.a + second.a;
}
";
    let big = "pub struct Big { a: i32; b: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "big"], big)]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 3);
}

#[test]
fn qualified_struct_param_read_only_reads_correct_fields() {
    // A read-only qualified-struct parameter must still read the caller's field
    // values correctly through its copy. This guards the read path the by-value
    // fix shares with the original qualified-parameter support.
    let main = "\
use lib::big;

pub fn total(p: lib::big::Big) -> i32 {
    return p.a + p.b;
}

pub fn run() -> i32 {
    let v: lib::big::Big = lib::big::Big { a: 10, b: 7 };
    return total(v);
}
";
    let big = "pub struct Big { a: i32; b: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "big"], big)]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 17);
}

#[test]
fn qualified_struct_param_matches_item_import_form_at_runtime() {
    // The same program written with a `::`-qualified parameter type and with a
    // braced item import (`use lib::big::{Big};`) must produce identical runtime
    // behavior: both forms name the same struct and both pass it by value with a
    // frame copy, so a mutation in the callee is invisible to the caller either way.
    let qualified_main = "\
use lib::big;

pub fn mutate(mut p: lib::big::Big) -> i32 {
    p.a = 999;
    return p.a;
}

pub fn run() -> i32 {
    let mut orig: lib::big::Big = lib::big::Big { a: 5, b: 6 };
    let inside: i32 = mutate(orig);
    return orig.a;
}
";
    let item_import_main = "\
use lib::big::{Big};

pub fn mutate(mut p: Big) -> i32 {
    p.a = 999;
    return p.a;
}

pub fn run() -> i32 {
    let mut orig: Big = Big { a: 5, b: 6 };
    let inside: i32 = mutate(orig);
    return orig.a;
}
";
    let big = "pub struct Big { a: i32; b: i32; }";

    let qualified_wasm =
        wasm_codegen_multi_file(&[(vec![], qualified_main), (vec!["lib", "big"], big)]);
    let item_import_wasm =
        wasm_codegen_multi_file(&[(vec![], item_import_main), (vec!["lib", "big"], big)]);

    let (mut q_store, q_instance) = instantiate(&qualified_wasm);
    let (mut i_store, i_instance) = instantiate(&item_import_wasm);
    assert_eq!(
        call_i32(&mut q_store, &q_instance, "run"),
        call_i32(&mut i_store, &i_instance, "run"),
    );
    assert_eq!(call_i32(&mut q_store, &q_instance, "run"), 5);
}

#[test]
fn mutable_self_method_on_qualified_type_copies_receiver() {
    // A method receiver derived from a qualified-type struct must still copy on a
    // `mut self` call: mutating the receiver inside the method leaves the caller's
    // struct untouched. This guards that the by-value parameter fix does not
    // regress the `mut self` copy path.
    let main = "\
use lib::counter;

pub fn run() -> i32 {
    let mut c: lib::counter::Counter = lib::counter::Counter { value: 1 };
    let bumped: i32 = c.bump();
    return c.value;
}
";
    let counter = "\
pub struct Counter {
    value: i32;

    pub fn bump(mut self) -> i32 {
        self.value = 100;
        return self.value;
    }
}
";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "counter"], counter)]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 1);
}

/// Extracts the printed `(func $name ...)` text for one function from a module's
/// WAT, so two programs can be compared on a single function body rather than the
/// whole module (which differs in unrelated functions). Returns the substring
/// from the matching `(func $name` up to the matching closing paren, found by
/// paren-depth tracking.
fn function_wat(wasm_bytes: &[u8], name: &str) -> String {
    let wat = wasmprinter::print_bytes(wasm_bytes).expect("module should print to WAT");
    let start = wat
        .find(&format!("(func ${name} "))
        .unwrap_or_else(|| panic!("function `{name}` not found in WAT:\n{wat}"));
    let mut depth = 0usize;
    for (offset, ch) in wat[start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return wat[start..=start + offset].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced parentheses extracting `{name}` from WAT:\n{wat}");
}

#[test]
fn qualified_struct_param_body_is_byte_identical_to_item_import_form() {
    // The fix's thesis: a `::`-qualified struct parameter must lower exactly like
    // the braced item-import form. Comparing the emitted `mutate` body directly
    // (not just the runtime result) guards against a divergence that happens to
    // return the same value — both forms must produce the same frame prologue,
    // copy-on-entry, parameter-rebind, and body.
    let qualified_main = "\
use lib::big;

pub fn mutate(mut p: lib::big::Big) -> i32 {
    p.a = 999;
    return p.a;
}

pub fn run() -> i32 {
    let mut orig: lib::big::Big = lib::big::Big { a: 1, b: 2 };
    let inside: i32 = mutate(orig);
    return orig.a;
}
";
    let item_import_main = "\
use lib::big::{Big};

pub fn mutate(mut p: Big) -> i32 {
    p.a = 999;
    return p.a;
}

pub fn run() -> i32 {
    let mut orig: Big = Big { a: 1, b: 2 };
    let inside: i32 = mutate(orig);
    return orig.a;
}
";
    let big = "pub struct Big { a: i32; b: i32; }";

    let qualified_wasm =
        wasm_codegen_multi_file(&[(vec![], qualified_main), (vec!["lib", "big"], big)]);
    let item_import_wasm =
        wasm_codegen_multi_file(&[(vec![], item_import_main), (vec!["lib", "big"], big)]);

    assert_eq!(
        function_wat(&qualified_wasm, "mutate"),
        function_wat(&item_import_wasm, "mutate"),
        "the qualified-parameter `mutate` body must be byte-identical to the item-import form"
    );
}

#[test]
fn qualified_enum_param_is_not_copied() {
    // An enum reached through a `::`-qualified parameter type is a scalar tag, not
    // a struct: it must get NO frame slot and NO copy-on-entry. The fix widened
    // the slot-allocation arms to include the qualified carrier, gated on the path
    // resolving to a *struct*; this guards that an enum carrier is turned away, so
    // a qualified-enum parameter stays a bare i32 like the bare-enum form.
    cov_mark::check_count!(wasm_codegen_emit_struct_param_copy, 0);

    let main = "\
use lib::sig;

pub fn classify(s: lib::sig::Signal) -> i32 {
    return 0;
}

pub fn run() -> i32 {
    let s: lib::sig::Signal = lib::sig::Signal::Stop;
    return classify(s);
}
";
    let sig = "pub enum Signal { Go, Slow, Stop }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "sig"], sig)]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 0);
}

#[test]
fn qualified_struct_param_lays_out_by_definer_despite_same_named_local_struct() {
    // The qualified parameter must lay out by its *defining* file even when the
    // entry file declares a same-named struct with a different field order. A
    // bare-name layout lookup against the access site would pick the entry's
    // `Big { b; a }` (so `p.a` would read offset 4), miscompiling the field read.
    // Resolving through the qualified path keeps the definer's `Big { a; b }`
    // layout, so `p.a` reads offset 0.
    let main = "\
use lib::big;

struct Big { b: i32; a: i32; }

pub fn first_field(p: lib::big::Big) -> i32 {
    return p.a;
}

pub fn run() -> i32 {
    let v: lib::big::Big = lib::big::Big { a: 11, b: 22 };
    return first_field(v);
}
";
    let big = "pub struct Big { a: i32; b: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "big"], big)]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 11);
}

#[test]
fn three_segment_qualified_struct_param_is_passed_by_value() {
    // A three-segment qualified parameter type (`lib::geom::Point`) must walk the
    // full namespace path to its struct and still be copied by value. Mutating the
    // parameter leaves the caller's struct intact, exactly as for the two-segment
    // form, exercising the multi-hop path walk on the parameter copy path.
    let main = "\
use lib::geom;

pub fn shift(mut p: lib::geom::Point) -> i32 {
    p.x = 999;
    return p.x;
}

pub fn run() -> i32 {
    let mut origin: lib::geom::Point = lib::geom::Point { x: 3, y: 4 };
    let moved: i32 = shift(origin);
    return origin.x;
}
";
    let geom = "pub struct Point { x: i32; y: i32; }";

    let wasm = wasm_codegen_multi_file(&[(vec![], main), (vec!["lib", "geom"], geom)]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 3);
}

// The cross-file descent gate must not over-reject legitimate resolution. These
// compile AND run through Wasmtime, so they pin not only that the gate admits the
// hop but that the resolved target is the right one (#63).
// The rejection / closure-independence / diagnostic-quality halves live in the
// type-checker matrix; here we prove the no-over-rejection controls execute.

/// Leaf-type-wins with the sibling file present: `main` imports the parent
/// file `a::b::c` directly, so the bound name anchors INSIDE `a::b::c` and the leaf
/// `Node` is the struct defined there — no cross-file hop, so the descent gate is
/// never consulted, and the sibling `a/b/c/Node.inf` must not shadow the type. The
/// constructed value's field read must run to 7, not the sibling's free `make`.
#[test]
fn descent_gate_allows_leaf_type_via_direct_parent_import() {
    let main = "\
use a::b::c;

pub fn run() -> i32 {
    let n: a::b::c::Node = a::b::c::Node::make();
    return n.v;
}
";
    let a_b_c = "\
pub struct Node {
    v: i32;

    pub fn make() -> Node {
        return Node { v: 7 };
    }
}
";
    let a_b_c_node = "\
pub fn make() -> i32 {
    return 999;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["a", "b", "c"], a_b_c),
        (vec!["a", "b", "c", "Node"], a_b_c_node),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 7);
}

/// A deep namespace that IS properly imported resolves and runs: `use
/// lib::geom::sub;` then `lib::geom::sub::deep()` returns 40. The prefix descent
/// predicate licenses the pass-through hops `lib` and `lib::geom` to reach the
/// imported `lib::geom::sub` in long form (D1).
#[test]
fn descent_gate_allows_long_form_of_deep_import() {
    let main = "\
use lib::geom::sub;

pub fn run() -> i32 {
    return lib::geom::sub::deep();
}
";
    let lib_geom_sub = "\
pub fn deep() -> i32 {
    return 40;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geom", "sub"], lib_geom_sub),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 40);
}

/// Parent AND child both imported: `use lib::geom;` and `use lib::geom::sub;` let
/// the file reach both surfaces; `lib::geom::area() + lib::geom::sub::deep()` runs
/// to the sum (2 + 40 = 42). Each `use` licenses its own namespace.
#[test]
fn descent_gate_allows_parent_and_child_both_imported() {
    let main = "\
use lib::geom;
use lib::geom::sub;

pub fn run() -> i32 {
    return lib::geom::area() + lib::geom::sub::deep();
}
";
    let lib_geom = "\
pub fn area() -> i32 {
    return 2;
}
";
    let lib_geom_sub = "\
pub fn deep() -> i32 {
    return 40;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib", "geom"], lib_geom),
        (vec!["lib", "geom", "sub"], lib_geom_sub),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 42);
}

// The viability-gated type-shadow break must resolve the RIGHT target, not merely
// accept. These compile AND run through Wasmtime so the resolved value
// distinguishes the sub-file's free fn from a same-named struct's assoc fn (#63).

/// `lib::geom::mk()` where `lib.inf` defines `struct geom` (colliding with the
/// intermediate segment) and `lib/geom.inf` defines the free `mk` returning a
/// `Point`. `mk` is not an assoc of `struct geom`, so the walk consumes `geom` as
/// the sub-file and resolves the free `mk`; `p.x` (9) + `lib::helper()` (1) = 10.
/// Without the viability gate this is rejected as `undefined function lib::geom::mk`.
#[test]
fn intermediate_struct_shadow_resolves_free_fn_runs() {
    let main = "\
use lib;
use lib::geom;

pub fn run() -> i32 {
    let p: lib::geom::Point = lib::geom::mk();
    let h: i32 = lib::helper();
    return p.x + h;
}
";
    let lib = "\
pub struct geom {
    g: i32;
}

pub fn helper() -> i32 {
    return 1;
}
";
    let lib_geom = "\
pub struct Point {
    x: i32;
    y: i32;
}

pub fn mk() -> Point {
    return Point { x: 9, y: 0 };
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "geom"], lib_geom),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 10);
}

/// The struct still wins when viable: `lib::Point::new()` where `new` IS an assoc
/// of `struct Point` (and a sibling `lib/Point.inf` has a free `new` returning
/// 1000). The viable type interpretation makes the type-shadow break fire, so the
/// struct's `new` (1) runs — not the sibling's 1000.
#[test]
fn struct_assoc_wins_when_viable_runs() {
    let main = "\
use lib;
use lib::Point;

pub fn run() -> i32 {
    return lib::Point::new();
}
";
    let lib = "\
pub struct Point {
    v: i32;

    pub fn new() -> i32 {
        return 1;
    }
}
";
    let lib_point = "\
pub fn new() -> i32 {
    return 1000;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "Point"], lib_point),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 1);
}

/// ADVERSARIAL RESIDUAL: `a::b::deep()` where `a.inf`'s `struct b` HAS an assoc
/// `deep` (1) and `a/b.inf` has a free `deep` (4242), accessing file imports ONLY
/// `a`. The type interpretation is viable (deep is a real assoc of struct b), so
/// the struct's `deep` (1) wins — the sub-file's free `deep` (4242), which the file
/// never imported, must not be reached.
#[test]
fn struct_assoc_viable_wins_over_sibling_free_parent_only_runs() {
    let main = "\
use a;

pub fn run() -> i32 {
    return a::b::deep();
}
";
    let a = "\
pub struct b {
    v: i32;

    pub fn deep() -> i32 {
        return 1;
    }
}
";
    let a_b = "\
pub fn deep() -> i32 {
    return 4242;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["a"], a),
        (vec!["a", "b"], a_b),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 1);
}

/// The member-miss case: `lib::Point::missing()` where
/// `struct Point` has no `missing` but the sibling `lib/Point.inf` does. The type
/// interpretation is not viable (missing is not an assoc of Point), so the walk
/// consumes `Point` as the sub-file namespace and resolves the sibling's free
/// `missing` (9). This is the value-pin for the matrix's
/// `qualified_path_struct_member_miss_falls_back_to_sibling_free_fn`.
#[test]
fn struct_member_miss_falls_back_to_sibling_free_fn_runs() {
    let main = "\
use lib;
use lib::Point;

pub fn run() -> i32 {
    return lib::Point::missing();
}
";
    let lib = "\
pub struct Point {
    v: i32;

    pub fn new() -> i32 {
        return 1;
    }
}
";
    let lib_point = "\
pub fn missing() -> i32 {
    return 9;
}
";

    let wasm = wasm_codegen_multi_file(&[
        (vec![], main),
        (vec!["lib"], lib),
        (vec!["lib", "Point"], lib_point),
    ]);

    let (mut store, instance) = instantiate(&wasm);
    assert_eq!(call_i32(&mut store, &instance, "run"), 9);
}
