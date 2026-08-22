//! End-to-end test for external `.wasm` linking (issue #9, Phase 4).
//!
//! Drives the full pipeline an `infc` invocation with `-L` runs: a program that
//! `use`s a function from an external module is compiled, the external module is
//! resolved off a search path and statically merged in, and the result is
//! translated to Rocq. The two end-to-end guarantees the merge makes are
//! asserted directly:
//!
//! - the unified `.wasm` has **no cross-module imports** — the external bytes
//!   are folded in, not referenced; and
//! - the unified `.v` carries the merged function as an **ordinary named
//!   definition** with **no orphan `Mi` import record** for the merged module.
//!
//! The external fixture is itself produced by the Inference compiler: a tiny
//! library exporting `pub fn sum` lowers to a `.wasm` whose `sum` export backs
//! the main program's `external fn sum`.

#[cfg(test)]
mod extern_link_tests {
    use std::path::{Path, PathBuf};

    use inf_wasmparser::{Parser, Payload, TypeRef};
    use inference::wasm_link::{resolve_external_modules, SearchPath};
    use inference::{codegen, link, parse, type_check, wasm_to_v, FxHashMap};
    use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

    /// Compiles `source` to a `.wasm` with the default settings, skipping the
    /// analysis phase — this codegen path does not need it. (The library sources
    /// passed here define no externs; in the main programs that do, A024 accepts
    /// the bound externs and rejects only unbound ones.)
    fn compile_wasm(source: &str, module_name: &str) -> Vec<u8> {
        let arena = parse(source).expect("library source parses");
        let typed = type_check(arena).expect("library source type-checks");
        let output = codegen(&typed, module_name).expect("library codegen succeeds");
        output.wasm().to_vec()
    }

    /// A throwaway directory under the system temp dir, unique to this test run,
    /// removed on drop.
    struct TempLibDir {
        path: PathBuf,
    }

    impl TempLibDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "inference-extern-link-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp lib dir");
            TempLibDir { path }
        }

        /// Writes `bytes` to `<dir>/<relative>`, creating parent directories.
        fn write_module(&self, relative: &Path, bytes: &[u8]) {
            let dest = self.path.join(relative);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).expect("create module parent dir");
            }
            std::fs::write(dest, bytes).expect("write external module");
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempLibDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// The `(module, field)` of every function import in `wasm`.
    fn function_imports(wasm: &[u8]) -> Vec<(String, String)> {
        let mut imports = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::ImportSection(reader) = payload.expect("valid payload") {
                for import in reader {
                    let import = import.expect("valid import");
                    if matches!(import.ty, TypeRef::Func(_)) {
                        imports.push((import.module.to_string(), import.name.to_string()));
                    }
                }
            }
        }
        imports
    }

    /// The number of functions **defined** in `wasm`, excluding imports.
    ///
    /// After a merge every function is defined, so this counts the program's own
    /// functions plus one body per merged external — the number that tells two
    /// declarations sharing one import apart from two separate imports.
    fn defined_function_count(wasm: &[u8]) -> usize {
        let mut count = 0;
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::FunctionSection(reader) = payload.expect("valid payload") {
                count += reader.count() as usize;
            }
        }
        count
    }

    /// Runs the pipeline an `infc -L <lib_dir> main.inf -v` invocation runs and
    /// returns the unified `.wasm` together with its Rocq translation.
    ///
    /// Analysis is part of that pipeline and is run here for the same reason
    /// `infc` runs it: a fixture that skipped it could compile, link and assert
    /// green while being a program `infc` refuses. That matters most for the
    /// rules about what may cross an extern call boundary — A024 and A047 —
    /// whose whole subject is the shape these fixtures are built out of.
    fn compile_and_link(main_source: &str, lib_dir: &Path, module_name: &str) -> (Vec<u8>, String) {
        let arena = parse(main_source).expect("main source parses");
        let typed = type_check(arena).expect("main source type-checks");
        inference_analysis::analyze(&typed).expect("main source passes analysis");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("external modules resolve and validate");
        let external_bytes = externals.module_bytes();

        let codegen_output = codegen(&typed, module_name).expect("main codegen succeeds");

        // Sanity guard: the *unlinked* codegen output carries the import, so its
        // translation contains an `Mi` record. This makes the post-link absence
        // of `Mi` a real difference rather than a vacuous pass.
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let pre_link_rocq = wasm_to_v(
            module_name,
            codegen_output.wasm(),
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("unlinked wasm-to-v succeeds");
        assert!(
            pre_link_rocq.contains("Mi "),
            "the unlinked module must still carry an import record; .v was:\n{pre_link_rocq}"
        );

        let unified = link(
            codegen_output.wasm(),
            &external_bytes,
            Some(&externals.contracts),
        )
        .expect("link succeeds");
        let rocq = wasm_to_v(
            module_name,
            &unified,
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("wasm-to-v succeeds");
        (unified, rocq)
    }

    /// The merged module's `__stack_pointer`.
    ///
    /// The merge re-emits the main module's globals and its non-function
    /// exports under their original indices, so a linked program keeps the
    /// shadow stack the compiler gave it. Reading it before and after a probe
    /// states the other half of a write-through test: the probes below assert
    /// that a foreign body reached only the bytes it was meant to, and this
    /// asserts that the frames those bytes lived in were unwound — a callee
    /// that skipped its epilogue, or one whose prologue was emitted without a
    /// matching restore, walks the pointer down and is invisible in every value
    /// a short program returns.
    fn stack_pointer(store: &mut Store<()>, instance: &Instance) -> i32 {
        instance
            .get_global(&mut *store, "__stack_pointer")
            .expect("the merge must preserve the main module's `__stack_pointer` export")
            .get(&mut *store)
            .i32()
            .expect("__stack_pointer is an i32 global")
    }

    #[test]
    fn single_extern_links_to_self_contained_wasm_and_v() {
        // The external library exports `sum`; the main program binds it via
        // `use { sum } from arith;` and calls it.
        let lib_wasm = compile_wasm(
            "pub fn sum(a: i32, b: i32) -> i32 { return a + b; }",
            "arith",
        );

        let lib_dir = TempLibDir::new("single");
        // Logical module `arith` resolves to `<dir>/arith.wasm`.
        lib_dir.write_module(Path::new("arith.wasm"), &lib_wasm);

        let main_source = "external fn sum(a: i32, b: i32) -> i32;\n\
             use { sum } from arith;\n\
             pub fn add_three(x: i32) -> i32 { return sum(x, 3); }";

        let (unified, rocq) = compile_and_link(main_source, lib_dir.path(), "extern_link");

        // The unified module is valid and self-contained: no import references
        // the external module any more.
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");
        assert!(
            function_imports(&unified).is_empty(),
            "unified module must have no cross-module imports, found {:?}",
            function_imports(&unified)
        );

        // The merged `sum` reads as an ordinary named Rocq definition, prefixed
        // with its logical module so two libraries exporting the same field can
        // never collide in the name section. The linker emits `arith.sum`, which
        // wasm-to-v sanitizes (every non-alphanumeric to `_`) to `arith_sum`.
        assert!(
            rocq.contains("Definition arith_sum"),
            "merged function must be a module-prefixed named Rocq definition; .v was:\n{rocq}"
        );

        // No orphan import record survives for the merged module: the linker
        // removed the import section, so wasm-to-v emits no `Mi` for it.
        assert!(
            !rocq.contains("Mi \"arith\""),
            "merged module must leave no orphan `Mi` import record; .v was:\n{rocq}"
        );
        assert!(
            !rocq.contains("MID_func"),
            "a self-contained module imports nothing, so no `MID_func` should appear; .v was:\n{rocq}"
        );
    }

    #[test]
    fn nested_module_path_resolves_and_links() {
        // A `::`-separated logical module must resolve to a nested file
        // (`crypto::adder` -> `<dir>/crypto/adder.wasm`) and link identically.
        let lib_wasm = compile_wasm(
            "pub fn combine(a: i32, b: i32) -> i32 { return a + b; }",
            "adder",
        );

        let lib_dir = TempLibDir::new("nested");
        lib_dir.write_module(Path::new("crypto").join("adder.wasm").as_path(), &lib_wasm);

        let main_source = "external fn combine(a: i32, b: i32) -> i32;\n\
             use { combine } from crypto::adder;\n\
             pub fn run(x: i32) -> i32 { return combine(x, x); }";

        let (unified, rocq) = compile_and_link(main_source, lib_dir.path(), "nested_link");

        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");
        assert!(
            function_imports(&unified).is_empty(),
            "no cross-module imports may remain, found {:?}",
            function_imports(&unified)
        );
        // The merged `combine` is prefixed with its `::`-separated logical
        // module: the linker emits `crypto::adder.combine`, which wasm-to-v
        // sanitizes (every non-alphanumeric to `_`, then `__` runs collapsed) to
        // `crypto_adder_combine`.
        assert!(
            rocq.contains("Definition crypto_adder_combine"),
            "merged function must be a module-prefixed named Rocq definition; .v was:\n{rocq}"
        );
        assert!(
            !rocq.contains("Mi \"crypto::adder\""),
            "no orphan `Mi` import record for the merged module; .v was:\n{rocq}"
        );
    }

    #[test]
    fn program_without_externs_is_unchanged_by_the_link_step() {
        // A program that binds no externs resolves to an empty external set and
        // the link step is a byte-identical pass-through of codegen output.
        let main_source = "pub fn double(x: i32) -> i32 { return x + x; }";
        let arena = parse(main_source).expect("parses");
        let typed = type_check(arena).expect("type-checks");

        let externals =
            resolve_external_modules(&typed, &SearchPath::new(), None).expect("no externs");
        assert!(
            externals.modules.is_empty(),
            "program binds no external modules"
        );
        assert!(
            externals.contracts.is_empty(),
            "a program that declares no extern declares no write set either"
        );

        let codegen_output = codegen(&typed, "plain").expect("codegen succeeds");
        let unified = link(codegen_output.wasm(), &[], Some(&externals.contracts))
            .expect("link is a no-op");
        assert_eq!(
            unified,
            codegen_output.wasm(),
            "the link step must not alter an extern-free module"
        );
    }

    #[test]
    fn link_with_no_externals_does_not_silently_pass_through_dangling_imports() {
        // Fail-closed: the empty-externals fast path is keyed on the module being
        // import-free, not merely on the externals slice being empty. A module
        // that still carries an import but is given no externals to satisfy it
        // must error (unsatisfied import), never pass through with the import
        // intact. (In the CLI flow externals are always resolved first; this
        // guards the public `inference::link` contract against misuse.)
        let import_bearing = wat::parse_str(
            r#"(module
                 (import "arith" "sum" (func (param i32 i32) (result i32)))
                 (func (export "run") (result i32)
                   i32.const 1 i32.const 2 call 0))"#,
        )
        .expect("fixture assembles");
        assert!(
            link(&import_bearing, &[], None).is_err(),
            "a module with an unsatisfied import must not pass through as Ok"
        );

        // And malformed bytes must surface a parse error, not Ok(garbage).
        assert!(
            link(&[0x00, 0x61, 0x73, 0x6d, 0xff], &[], None).is_err(),
            "malformed main bytes must be a link error, not a silent pass-through"
        );
    }

    /// A writing external, hand-written in WAT: `sort_pair(ptr)` sorts the two
    /// `i32`s at `[ptr]` and `[ptr+4]` ascending, swapping through the caller's
    /// pointer. Taken verbatim from the linker's own execution fixtures.
    ///
    /// This has to stay WAT rather than become an Inference library: an
    /// Inference `fn sort_pair(p: Pair)` would copy `p` on entry and could never
    /// write through the caller's address, which is the whole mechanism under
    /// test.
    const SORTLIB_WAT: &str = r#"
        (module
          (type (;0;) (func (param i32)))
          (memory (;0;) 1)
          ;; swap(ptr): exchange [ptr] and [ptr+4]
          (func (;0;) (type 0) (param i32)
            (local i32 i32)
            local.get 0
            i32.load
            local.set 1
            local.get 0
            i32.const 4
            i32.add
            i32.load
            local.set 2
            local.get 0
            local.get 2
            i32.store
            local.get 0
            i32.const 4
            i32.add
            local.get 1
            i32.store)
          ;; sort_pair(ptr): if [ptr] > [ptr+4], swap
          (func (;1;) (type 0) (param i32)
            local.get 0
            i32.load
            local.get 0
            i32.const 4
            i32.add
            i32.load
            i32.gt_s
            if
              local.get 0
              call 0
            end)
          (export "sort_pair" (func 1)))
        "#;

    /// Issue #329: a `self` receiver forwarded to a writing external must not let
    /// that external reach the caller's struct.
    ///
    /// The first two probes differ only in how the receiver arrives — a `mut
    /// self` and an ordinary by-value parameter — and each packs what the callee
    /// saw together with what the caller has afterwards, so a single number pins
    /// the whole outcome. The bug returned `20050205` for the first probe: the
    /// caller's `Pair { a: 5, b: 2 }` came back sorted, because `touch` was
    /// frameless and handed `probe_mut_self`'s own frame pointer to the foreign
    /// body.
    ///
    /// Both halves of each value are load-bearing. Checking only that the caller
    /// survived would accept a fix that stages a copy at the *call site* instead
    /// of on entry: the external would then sort a temporary the method never
    /// reads, `touch` would see its receiver unsorted, and the probe would return
    /// `50020502` — caller intact, callee semantics quietly changed.
    ///
    /// What this pins is caller-side value semantics only. Inside the method the
    /// external still writes through the callee's own copy, so `touch` observes
    /// its own receiver sorted — the `2005` half of the expected value. The
    /// named-parameter probe has behaved that way all along (its `25` half).
    ///
    /// The receiver is `mut self` because `sort_pair` now declares `mut p`:
    /// forwarding an *immutable* receiver to a parameter the foreign body may
    /// write through is an A047 error, which
    /// [`immutable_bindings_forwarded_to_a_writing_extern_are_rejected`] pins.
    /// The receiver's mutability is what changed; the values did not, because
    /// `mut` on the external's parameter keeps every entry copy in place.
    ///
    /// The third probe runs the opposite decision in the same merged module. A
    /// compound parameter that never reaches the external is passed by reference
    /// — no frame slot, no entry copy — so what the callee dereferences is a raw
    /// address in the caller's frame rather than a region of its own. Linking is
    /// what makes that worth executing here: the foreign body is folded into this
    /// module and onto this linear memory, so the linker is precisely the
    /// component whose addressing assumptions an elided parameter could falsify,
    /// and no other end-to-end program in the suite runs one. `peek` reads
    /// through the elided pointer on both sides of a call that hands the same
    /// struct to the writing external, so its number states three things: the
    /// address was good before the foreign body ran (`52`), the sort reached only
    /// the callee's copy (`2005`), and the address was still good afterwards
    /// (`52`).
    #[test]
    fn mut_self_forwarded_to_writing_extern_leaves_the_caller_intact() {
        // Of the three compound parameters in this program only `peek`'s is
        // passed by reference; the other two reach the external and keep their
        // copies. Without this the third probe would be equally satisfied by a
        // copy, and the merged module's handling of a raw caller address would go
        // unrun.
        cov_mark::check_count!(wasm_codegen_param_by_reference, 1);
        let lib_wasm = wat::parse_str(SORTLIB_WAT).expect("sortlib WAT assembles");
        let lib_dir = TempLibDir::new("self_extern");
        // The `.wasm` extension is required: `resolve_external_modules` maps the
        // logical module `sortlib` onto `<dir>/sortlib.wasm`.
        lib_dir.write_module(Path::new("sortlib.wasm"), &lib_wasm);

        let main_source = "\
external fn sort_pair(mut p: Pair);
use { sort_pair } from sortlib;

struct Pair {
    a: i32;
    b: i32;

    fn touch(mut self) -> i32 {
        sort_pair(self);
        return self.a * 1000 + self.b;
    }
}

fn touch_param(mut p: Pair) -> i32 {
    sort_pair(p);
    return p.a * 10 + p.b;
}

fn peek(p: Pair) -> i32 {
    return p.a * 10 + p.b;
}

pub fn probe_mut_self() -> i32 {
    let p: Pair = Pair { a: 5, b: 2 };
    let inner: i32 = p.touch();
    return inner * 10000 + p.a * 100 + p.b;
}

pub fn probe_named_param() -> i32 {
    let p: Pair = Pair { a: 5, b: 2 };
    let inner: i32 = touch_param(p);
    return inner * 100 + p.a * 10 + p.b;
}

pub fn probe_by_reference() -> i32 {
    let p: Pair = Pair { a: 5, b: 2 };
    let before: i32 = peek(p);
    let inner: i32 = p.touch();
    let after: i32 = peek(p);
    return before * 1000000 + inner * 100 + after;
}
";

        let (unified, _rocq) = compile_and_link(main_source, lib_dir.path(), "self_extern");
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");

        let engine = Engine::default();
        let module = Module::new(&engine, &unified)
            .unwrap_or_else(|e| panic!("merged module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("merged module failed to instantiate: {e}"));

        let call = |store: &mut Store<()>, name: &str| -> i32 {
            let probe: TypedFunc<(), i32> = instance
                .get_typed_func(&mut *store, name)
                .unwrap_or_else(|e| panic!("merged module must export `{name}`: {e}"));
            probe
                .call(&mut *store, ())
                .unwrap_or_else(|e| panic!("`{name}` failed: {e}"))
        };

        let initial_sp = stack_pointer(&mut store, &instance);

        assert_eq!(
            call(&mut store, "probe_mut_self"),
            20_050_502,
            "a `self` receiver must be copied into the method's own frame before \
             it reaches a writing external: the callee sees the sorted pair (2005) \
             and the caller still holds Pair {{ a: 5, b: 2 }} (502). 20050205 is the \
             #329 bug (caller mutated); 50020502 would mean the copy was staged at \
             the call site instead of on entry. `mut self` says the method's own \
             copy may change, which is exactly what the sort does to it — it says \
             nothing about the caller's"
        );
        assert_eq!(
            call(&mut store, "probe_named_param"),
            2552,
            "the by-value parameter control is unchanged too: a named compound \
             parameter copies on entry today, so the callee sees the sorted pair \
             (25) and the caller keeps its own (52)"
        );
        assert_eq!(
            call(&mut store, "probe_by_reference"),
            52_200_552,
            "a compound parameter that never reaches the external is passed by \
             reference, and reading through that raw caller address must survive \
             the merge: `peek` reads Pair {{ a: 5, b: 2 }} before the foreign body \
             runs (52) and reads the same bytes back after it (the trailing 52), \
             while `touch` still sees the sort in its own copy (2005). A trailing \
             25 would mean the foreign store reached the caller after all, and a \
             leading value other than 52 would mean the elided pointer did not \
             address the caller's struct in the merged module"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "three probes have entered and left frames — two of them holding an \
             entry copy of a receiver — so the shadow stack must be exactly where \
             it started; a drift here means some prologue in the merged module \
             was never matched by its epilogue"
        );
    }

    /// The same write-through guarantee for a named **array** parameter.
    ///
    /// The sibling test above covers the struct arm three ways — an immutable
    /// receiver, a `mut self`, and a by-value `Pair` parameter — and all three
    /// are copied as one untyped region. An array parameter is copied element by
    /// element by a different emitter, reached through a different arm of the
    /// entry-copy loop, so neither of them says anything about it. This is the
    /// only end-to-end statement that an array parameter handed to a foreign
    /// body that stores through the pointer still leaves the caller's array
    /// intact.
    ///
    /// The external is the same `sortlib`: a compound parameter reaches it as a
    /// bare `i32` address whatever its declared shape, so a `[i32; 2]` and a
    /// `Pair` present it with the identical ABI. The declared type has to match
    /// at the Inference call site, though, which is why this program declares
    /// its own `external fn` rather than sharing the one above.
    ///
    /// Both halves of the returned number are load-bearing, as in the sibling
    /// test: `25` says the callee did see the sorted pair, so the copy is made
    /// on entry and not staged at the call site, and `52` says the caller's own
    /// array never changed.
    ///
    /// `touch_array` takes `mut a` because `sort_pair` declares `mut p`: an
    /// immutable array forwarded to a parameter the foreign body may write
    /// through is an A047 error. The asserted value is unchanged by that — `mut`
    /// on the external's parameter is what keeps the entry copy in place.
    #[test]
    fn named_array_param_forwarded_to_writing_extern_leaves_the_caller_intact() {
        let lib_wasm = wat::parse_str(SORTLIB_WAT).expect("sortlib WAT assembles");
        let lib_dir = TempLibDir::new("array_extern");
        lib_dir.write_module(Path::new("sortlib.wasm"), &lib_wasm);

        let main_source = "\
external fn sort_pair(mut p: [i32; 2]);
use { sort_pair } from sortlib;

fn touch_array(mut a: [i32; 2]) -> i32 {
    sort_pair(a);
    return a[0] * 10 + a[1];
}

pub fn probe_named_array() -> i32 {
    let arr: [i32; 2] = [5, 2];
    let inner: i32 = touch_array(arr);
    return inner * 100 + arr[0] * 10 + arr[1];
}
";

        let (unified, _rocq) = compile_and_link(main_source, lib_dir.path(), "array_extern");
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");

        let engine = Engine::default();
        let module = Module::new(&engine, &unified)
            .unwrap_or_else(|e| panic!("merged module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("merged module failed to instantiate: {e}"));

        let initial_sp = stack_pointer(&mut store, &instance);

        let probe: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, "probe_named_array")
            .unwrap_or_else(|e| panic!("merged module must export `probe_named_array`: {e}"));
        assert_eq!(
            probe
                .call(&mut store, ())
                .unwrap_or_else(|e| panic!("`probe_named_array` failed: {e}")),
            2552,
            "an array parameter reaching a writing external must be copied element \
             by element into the callee's own frame: the callee sees the sorted pair \
             (25) and the caller still holds [5, 2] (52). 2525 would mean the foreign \
             store reached the caller's array"
        );

        assert_eq!(
            stack_pointer(&mut store, &instance),
            initial_sp,
            "the probe and the copying callee each took a frame, so both must have \
             given it back"
        );
    }

    /// A read/write asymmetric external, hand-written in WAT: `copy_first(dst,
    /// src)` copies the `i32` at `[src]` to `[dst]`.
    ///
    /// Its write set is `{0}` and never `{0, 1}`, which is the point: an
    /// analysis that simply returned "every parameter" would satisfy every
    /// acceptance test in this file, and only a rejection that names `dst` while
    /// `src` is declared shows the set was actually derived.
    const COPYLIB_WAT: &str = r#"
        (module
          (type (;0;) (func (param i32 i32)))
          (memory (;0;) 1)
          (func (;0;) (type 0) (param i32 i32)
            local.get 0
            local.get 1
            i32.load
            i32.store)
          (export "copy_first" (func 0)))
        "#;

    /// Runs [`compile_and_link`]'s pipeline but hands back the link failure
    /// instead of panicking on it, for the tests whose subject is the rejection.
    ///
    /// Analysis runs here too, and its own rejection is reported distinctly: a
    /// program A047 refuses never reaches the linker, so a test asserting a link
    /// error against such a program would otherwise pass on the wrong diagnostic.
    fn try_compile_and_link(
        main_source: &str,
        lib_dir: &Path,
        module_name: &str,
    ) -> Result<Vec<u8>, String> {
        let arena = parse(main_source).expect("main source parses");
        let typed = type_check(arena).expect("main source type-checks");
        inference_analysis::analyze(&typed).map_err(|e| format!("analysis rejected: {e}"))?;

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .map_err(|e| format!("resolution rejected: {e}"))?;
        let external_bytes = externals.module_bytes();
        let codegen_output = codegen(&typed, module_name).expect("main codegen succeeds");
        link(
            codegen_output.wasm(),
            &external_bytes,
            Some(&externals.contracts),
        )
        .map_err(|e| e.to_string())
    }

    /// The rendered analysis rejection of `main_source`, which must be rejected.
    fn analysis_rejection(main_source: &str) -> String {
        let arena = parse(main_source).expect("main source parses");
        let typed = type_check(arena).expect("main source type-checks");
        inference_analysis::analyze(&typed)
            .expect_err("the program must be rejected by analysis")
            .to_string()
    }

    /// Instantiates a merged module and calls a nullary `i32`-returning export.
    fn call_probe(unified: &[u8], name: &str) -> i32 {
        inf_wasmparser::validate(unified).expect("unified module is valid wasm");
        let engine = Engine::default();
        let module =
            Module::new(&engine, unified).unwrap_or_else(|e| panic!("merged module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("merged module failed to instantiate: {e}"));
        let probe: TypedFunc<(), i32> = instance
            .get_typed_func(&mut store, name)
            .unwrap_or_else(|e| panic!("merged module must export `{name}`: {e}"));
        probe
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("`{name}` failed: {e}"))
    }

    /// **The acceptance criterion for issue #333.** A merged body that stores
    /// through a parameter its declaration did not mark `mut` is rejected, and
    /// the *only* difference between the rejected program and the accepted one is
    /// that keyword.
    ///
    /// The control is what makes this a statement about the contract rather than
    /// about `sortlib` being unmergeable: both programs resolve the same library,
    /// compute the same closure and reach the same Tier-B verdict, so the sole
    /// remaining variable is what the declaration claimed. Without it a rejection
    /// for any unrelated reason — a resolution failure, a Tier-C reason, a
    /// provenance refusal — would read as success.
    #[test]
    fn a_store_through_an_undeclared_parameter_is_rejected_at_link() {
        let lib_wasm = wat::parse_str(SORTLIB_WAT).expect("sortlib WAT assembles");
        let lib_dir = TempLibDir::new("undeclared_write");
        lib_dir.write_module(Path::new("sortlib.wasm"), &lib_wasm);

        // One template; `{declaration}` is the entire difference between the two
        // programs. The local is `mut` in both, so A047 fires in neither and the
        // linker is the component under test.
        let program = |declaration: &str| {
            format!(
                "\
{declaration}
use {{ sort_pair }} from sortlib;

pub fn probe() -> i32 {{
    let mut arr: [i32; 2] = [5, 2];
    sort_pair(arr);
    return arr[0] * 10 + arr[1];
}}
"
            )
        };

        let rejection = try_compile_and_link(
            &program("external fn sort_pair(p: [i32; 2]);"),
            lib_dir.path(),
            "undeclared_write",
        )
        .expect_err("a body that stores through an undeclared parameter must not link");
        assert!(
            rejection.contains("sortlib::sort_pair")
                && rejection.contains("parameter 0 (`p`)")
                && rejection.contains("does not declare `mut`"),
            "the rejection must name the external, the offending parameter and the \
             missing declaration; got: {rejection}"
        );
        assert!(
            rejection.contains("mut p"),
            "the rejection must spell the fix out for a named parameter; got: {rejection}"
        );

        let accepted = try_compile_and_link(
            &program("external fn sort_pair(mut p: [i32; 2]);"),
            lib_dir.path(),
            "declared_write",
        )
        .expect("the same program with `mut p` declared must link");
        assert!(
            function_imports(&accepted).is_empty(),
            "the accepted control must still be a self-contained merge, found {:?}",
            function_imports(&accepted)
        );
    }

    /// A parameter the external provably never writes through stays outside the
    /// required declaration, and one it does write through cannot be swapped for
    /// it.
    ///
    /// This is what separates a derived write set from `0..arity`. `copy_first`
    /// writes through `dst` and only reads through `src`, so declaring `mut dst`
    /// alone links — a write set equal to the full arity would have demanded
    /// `mut src` too — while declaring `mut src` alone is rejected *naming
    /// `dst`*, which a set that under-reported to `{}` could not do either.
    #[test]
    fn the_write_set_names_the_written_parameter_and_not_the_read_one() {
        let lib_wasm = wat::parse_str(COPYLIB_WAT).expect("copylib WAT assembles");
        let lib_dir = TempLibDir::new("asymmetric_write");
        lib_dir.write_module(Path::new("copylib.wasm"), &lib_wasm);

        let program = |declaration: &str| {
            format!(
                "\
{declaration}
use {{ copy_first }} from copylib;

pub fn probe() -> i32 {{
    let mut dst: [i32; 1] = [7];
    let mut src: [i32; 1] = [9];
    copy_first(dst, src);
    return dst[0] * 10 + src[0];
}}
"
            )
        };

        let unified = try_compile_and_link(
            &program("external fn copy_first(mut dst: [i32; 1], src: [i32; 1]);"),
            lib_dir.path(),
            "write_dst_only",
        )
        .expect("declaring only the parameter the body writes through must be enough");
        assert_eq!(
            call_probe(&unified, "probe"),
            99,
            "`copy_first` copies [src] over [dst], so both hold 9; the value states \
             that the merged body ran and wrote where the declaration said it would"
        );

        let rejection = try_compile_and_link(
            &program("external fn copy_first(dst: [i32; 1], mut src: [i32; 1]);"),
            lib_dir.path(),
            "write_src_only",
        )
        .expect_err("declaring the parameter the body only reads must not cover the write");
        assert!(
            rejection.contains("parameter 0 (`dst`)"),
            "the rejection must name the parameter actually written, not merely \
             report that some declaration was missing; got: {rejection}"
        );
    }

    /// Issue #429: a compound `let` local handed straight to a writing external.
    ///
    /// Unlike a parameter, a local has no entry copy to contain the foreign
    /// store, so the callee writes the caller's own bytes — which is correct only
    /// if the source said the local may change. Three rows cover the whole
    /// decision, and the first two are the ones #429 is about: before this
    /// contract existed the program below built silently and mutated an
    /// immutable `let`.
    #[test]
    fn a_compound_local_passed_to_a_writing_extern_must_be_declared_and_mut() {
        let lib_wasm = wat::parse_str(SORTLIB_WAT).expect("sortlib WAT assembles");
        let lib_dir = TempLibDir::new("local_extern");
        lib_dir.write_module(Path::new("sortlib.wasm"), &lib_wasm);

        let program = |declaration: &str, binding: &str| {
            format!(
                "\
{declaration}
use {{ sort_pair }} from sortlib;

pub fn probe_direct() -> i32 {{
    let {binding} arr: [i32; 2] = [5, 2];
    sort_pair(arr);
    return arr[0] * 10 + arr[1];
}}
"
            )
        };

        // (a) The declaration claims nothing, so the store has no cover and the
        //     program cannot be built at all. This is the row that closes #429:
        //     the same source used to link and return 25.
        let rejection = try_compile_and_link(
            &program("external fn sort_pair(p: [i32; 2]);", ""),
            lib_dir.path(),
            "local_undeclared",
        )
        .expect_err("an undeclared write into a caller's local must not link");
        assert!(
            rejection.contains("may store through the address parameter 0 (`p`)"),
            "the rejection must come from the write-set check; got: {rejection}"
        );

        // (b) The declaration is honest, so the linker is satisfied — and the
        //     rejection moves to the call site, where an immutable local is
        //     handed to a parameter the body may write through.
        let rejection = analysis_rejection(&program("external fn sort_pair(mut p: [i32; 2]);", ""));
        assert!(
            rejection.contains("[A047]") && rejection.contains("`arr`"),
            "an immutable local at a `mut` extern parameter must be an A047 error \
             naming the binding; got: {rejection}"
        );

        // (c) Both halves stated: the program builds, runs, and the mutation the
        //     source authorised is observed in the caller's own local.
        let unified = try_compile_and_link(
            &program("external fn sort_pair(mut p: [i32; 2]);", "mut"),
            lib_dir.path(),
            "local_declared_mut",
        )
        .expect("a `mut` local at a declared `mut` parameter must link");
        assert_eq!(
            call_probe(&unified, "probe_direct"),
            25,
            "a `mut` local is passed as a raw pointer into the caller's frame, so \
             the foreign sort reaches it and `arr` is [2, 5] afterwards. 52 would \
             mean the write was contained in a copy the source never asked for"
        );
    }

    /// An unnamed parameter cannot carry a write set, and the rejection has to
    /// say so — because unlike every other rejection in this file, "declare it
    /// `mut`" is not a fix the author can apply.
    ///
    /// The two unnamed forms occupy a real ABI slot but carry no mutability
    /// field, and the grammar has no slot for one. So an external that writes
    /// through such a parameter is unlinkable until the declaration names it, and
    /// a message that only said "declare it `mut`" would send the author looking
    /// for a spelling that does not exist.
    #[test]
    fn an_unnamed_parameter_cannot_declare_a_write_set() {
        let lib_wasm = wat::parse_str(SORTLIB_WAT).expect("sortlib WAT assembles");
        let lib_dir = TempLibDir::new("unnamed_param");
        lib_dir.write_module(Path::new("sortlib.wasm"), &lib_wasm);

        for declaration in [
            "external fn sort_pair([i32; 2]);",
            "external fn sort_pair(_: [i32; 2]);",
        ] {
            let source = format!(
                "\
{declaration}
use {{ sort_pair }} from sortlib;

pub fn probe() -> i32 {{
    let mut arr: [i32; 2] = [5, 2];
    sort_pair(arr);
    return arr[0] * 10 + arr[1];
}}
"
            );
            let rejection = try_compile_and_link(&source, lib_dir.path(), "unnamed_param")
                .expect_err("an unnamed parameter declares no write set, so the store is uncovered");
            assert!(
                rejection.contains("unnamed form") && rejection.contains("give it a name first"),
                "`{declaration}` must be rejected with the name-it-first fix, since \
                 `mut` has nowhere to go; got: {rejection}"
            );
            assert!(
                !rejection.contains("(`"),
                "the rejection must not quote a parameter name the declaration does \
                 not have; got: {rejection}"
            );
        }
    }

    /// The capability AD-3 removes: an immutable compound binding may no longer
    /// be forwarded to a parameter a merged body may write through.
    ///
    /// These are the three shapes the #220/#329 acceptance fixtures used to
    /// assert as *working* — an immutable `self` receiver, an immutable by-value
    /// struct parameter, and an immutable array parameter, each handed straight
    /// to `sort_pair`. They were correct while `sort_pair` claimed nothing; once
    /// it declares `mut p`, each one is a value that changes across a call the
    /// source never said could change it, which is exactly A047's subject. The
    /// removal is pinned here so it reads as a decision rather than as coverage
    /// that quietly disappeared.
    #[test]
    fn immutable_bindings_forwarded_to_a_writing_extern_are_rejected() {
        for (shape, forwarder) in [
            (
                "an immutable `self` receiver",
                "\
struct Pair {
    a: i32;
    b: i32;

    fn touch(self) -> i32 {
        sort_pair(self);
        return self.a;
    }
}
",
            ),
            (
                "an immutable by-value struct parameter",
                "\
struct Pair {
    a: i32;
    b: i32;
}

fn touch_param(p: Pair) -> i32 {
    sort_pair(p);
    return p.a;
}
",
            ),
        ] {
            let source = format!(
                "\
external fn sort_pair(mut p: Pair);
use {{ sort_pair }} from sortlib;

{forwarder}"
            );
            let rejection = analysis_rejection(&source);
            assert!(
                rejection.contains("[A047]"),
                "forwarding {shape} to a `mut` extern parameter must be an A047 \
                 error; got: {rejection}"
            );
        }

        let rejection = analysis_rejection(
            "\
external fn sort_pair(mut p: [i32; 2]);
use { sort_pair } from sortlib;

fn touch_array(a: [i32; 2]) -> i32 {
    sort_pair(a);
    return a[0];
}
",
        );
        assert!(
            rejection.contains("[A047]") && rejection.contains("`a`"),
            "forwarding an immutable array parameter must be an A047 error naming \
             the binding; got: {rejection}"
        );
    }

    #[test]
    fn proof_mode_spec_omission_renumbers_the_call_to_the_merged_extern() {
        // C1: a proof-mode program that binds an extern AND declares a spec.
        // Post-link function order is add_three=0, spec `check`=1, merged `sum`=2
        // (the import is removed and the external appended). The spec function is
        // OMITTED from the emitted `.v` module record, so `sum` (index 2) shifts
        // down to instantiated index 1 — and `add_three`'s executable call to
        // `sum` must be renumbered to `BI_call 1%N`. This pins two things at once:
        // that the embedded `inference.spec_funcs` section names `check` (so the
        // *right* function is omitted — omitting `sum` instead would fail-close on
        // `add_three`'s now-dangling call), and that the omission-driven call
        // renumbering is correct. Translating with empty explicit maps adopts the
        // post-link embedded sections as the source of truth.
        let lib_wasm = compile_wasm(
            "pub fn sum(a: i32, b: i32) -> i32 { return a + b; }",
            "arith",
        );
        let lib_dir = TempLibDir::new("c1_spec");
        lib_dir.write_module(Path::new("arith.wasm"), &lib_wasm);

        // `check` must be translatable to an obligation, so it does not call the
        // extern (an external call has no verified body — `P005`), and it must
        // claim something, since an obligation that collapses to `HA_true` is
        // rejected outright. Its claim is deliberately self-contained: the
        // cross-call half is what the companion test below adds. `add_three`
        // (executable) carries the extern call whose operand the omission
        // renumbers.
        let main_source = "external fn sum(a: i32, b: i32) -> i32;\n\
             use { sum } from arith;\n\
             pub fn add_three(x: i32) -> i32 { return sum(x, 3); }\n\
             spec MySpec {\n\
                 fn check() forall { let x: i32 = @; assert(x == x); }\n\
             }";

        let arena = parse(main_source).expect("main parses");
        let typed = type_check(arena).expect("main type-checks");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let externals =
            resolve_external_modules(&typed, &search_path, None).expect("external modules resolve");
        let external_bytes = externals.module_bytes();

        let codegen_output = inference_wasm_codegen::codegen(
            &typed,
            "c1prog",
            inference_wasm_codegen::CodegenOptions {
                mode: inference_wasm_codegen::CompilationMode::Proof,
                ..Default::default()
            },
        )
        .expect("proof-mode codegen succeeds");

        let unified = link(
            codegen_output.wasm(),
            &external_bytes,
            Some(&externals.contracts),
        )
        .expect("link succeeds");
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");

        // Empty explicit maps: the post-link embedded sections are the source of
        // truth (the pre-link codegen indices would be stale here).
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let rocq = wasm_to_v("c1prog", &unified, &empty, &inference::HSpecMap::default())
            .expect("wasm-to-v succeeds");

        // MySpec surfaces with the new obligation-list shape.
        assert!(
            rocq.contains("Definition c1prog__MySpec_specs : list hassert :="),
            "MySpec must surface a per-spec obligation list; .v was:\n{rocq}"
        );
        // The omission of the spec function at index 1 renumbers `sum` from
        // instantiated index 2 to 1, so `add_three`'s call reads `BI_call 1%N`.
        assert!(
            rocq.contains("BI_call 1%N"),
            "add_three's call to the merged `sum` must be renumbered past the \
             omitted spec function (to instantiated index 1); .v was:\n{rocq}"
        );
        // The merged `sum` executable body (its `a + b`) survives in the record.
        assert!(
            rocq.contains("BI_binop T_i32 (Binop_i BOI_add)"),
            "the merged extern `sum` body must survive into the module record; .v was:\n{rocq}"
        );
    }

    #[test]
    fn proof_mode_hspec_t_app_resolves_across_the_link() {
        // The companion to `proof_mode_spec_omission_renumbers_the_call_to_the_merged_extern`:
        // that test pins the *executable* `BI_call` renumbering, and its spec
        // claims a property over its own universal slot only — no cross-call.
        // This one adds the load-bearing half: an obligation whose `T_app` must
        // resolve through the same post-link remap.
        //
        // Post-link function order is add_three=0, is_prime=1, spec `prop`=2,
        // merged `sum`=3. The spec function is OMITTED from the `.v` module record,
        // so both index streams shift: `add_three`'s executable call to `sum`
        // renumbers to `BI_call 2%N`, and the obligation's call to the surviving
        // `is_prime` resolves by its verbatim name-section symbol to defined-fn
        // index 1 (`T_app 1`) — proving the obligation and the executable body
        // agree on the post-link numbering. Translating with empty explicit maps
        // adopts the post-link embedded `inference.spec_funcs` / `inference.hspecs`
        // sections as the source of truth (the CLI-equivalent, defer-to-embedded
        // flow), which is what makes the `T_app` resolution genuinely post-link.
        let lib_wasm = compile_wasm(
            "pub fn sum(a: i32, b: i32) -> i32 { return a + b; }",
            "arith",
        );
        let lib_dir = TempLibDir::new("hspec_tapp");
        lib_dir.write_module(Path::new("arith.wasm"), &lib_wasm);

        // `add_three` (executable) carries the extern call the omission renumbers;
        // `is_prime` (executable, defined) is the obligation's `T_app` target — it
        // does not call the extern, since a spec obligation cannot reference an
        // unverified external body (`P005`).
        let main_source = "external fn sum(a: i32, b: i32) -> i32;\n\
             use { sum } from arith;\n\
             pub fn add_three(x: i32) -> i32 { return sum(x, 3); }\n\
             fn is_prime(n: i32) -> bool { return n > 1; }\n\
             spec MySpec {\n\
                 fn prop() forall {\n\
                     let n: i32 = @;\n\
                     assume { assert(n > 1); }\n\
                     assert(is_prime(n));\n\
                 }\n\
             }";

        let arena = parse(main_source).expect("main parses");
        let typed = type_check(arena).expect("main type-checks");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let externals =
            resolve_external_modules(&typed, &search_path, None).expect("external modules resolve");
        let external_bytes = externals.module_bytes();

        let codegen_output = inference_wasm_codegen::codegen(
            &typed,
            "hprog",
            inference_wasm_codegen::CodegenOptions {
                mode: inference_wasm_codegen::CompilationMode::Proof,
                ..Default::default()
            },
        )
        .expect("proof-mode codegen succeeds");

        let unified = link(
            codegen_output.wasm(),
            &external_bytes,
            Some(&externals.contracts),
        )
        .expect("link succeeds");
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");

        // Empty explicit maps: the post-link embedded sections are the source of
        // truth (the pre-link codegen indices are stale after the merge).
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let rocq = wasm_to_v("hprog", &unified, &empty, &inference::HSpecMap::default())
            .expect("wasm-to-v succeeds");

        // The obligation is emitted as a first-class `hassert` for `MySpec`.
        assert!(
            rocq.contains("Definition hprog__MySpec_hspec1 : hassert :="),
            "MySpec must surface a first-class hassert obligation; .v was:\n{rocq}"
        );
        // Executable stream: `add_three`'s call to the merged `sum` renumbers past
        // the omitted spec function to instantiated index 2.
        assert!(
            rocq.contains("BI_call 2%N"),
            "add_three's call to the merged `sum` must renumber to index 2; .v was:\n{rocq}"
        );
        // Obligation stream: the `is_prime` cross-call resolves by its post-link
        // name to defined-fn index 1 — the same numbering the executable bodies use.
        assert!(
            rocq.contains("T_app 1 ((T_local 0%N) :: nil)"),
            "the obligation's `is_prime` call must resolve to defined-fn index 1; .v was:\n{rocq}"
        );
        // The `T_app` target's own body survives in the record (a `> 1` compare).
        assert!(
            rocq.contains("BI_relop T_i32 (Relop_i (ROI_gt SX_S))"),
            "the `is_prime` body must survive into the module record; .v was:\n{rocq}"
        );
    }

    /// The exists-kind sibling of the two forall tests above: post-link index
    /// arithmetic shifts DIFFERENTLY per kind, because an exists spec function
    /// is RETAINED in the `.v` module record where a forall one is omitted.
    ///
    /// Post-link function order is add_three=0, is_pos=1, spec `witness`=2,
    /// merged `sum`=3. In the forall companion the spec function's omission
    /// pulled every later index down by one; here nothing is omitted, so
    /// `add_three`'s executable call to the merged `sum` keeps the unshifted
    /// `BI_call 3%N`, the retained `witness` appears as an ordinary
    /// `module_func` definition, and its obligation names it by the unshifted
    /// `reach_func := 2%N`. The obligation's `is_pos` cross-call still
    /// resolves through the post-link name section to `T_app 1` — the same
    /// numbering the executable bodies use, now with a retained spec function
    /// sitting between the callee and the merged extern.
    #[test]
    fn proof_mode_exists_retention_keeps_the_post_link_numbering_unshifted() {
        let lib_wasm = compile_wasm(
            "pub fn sum(a: i32, b: i32) -> i32 { return a + b; }",
            "arith",
        );
        let lib_dir = TempLibDir::new("exists_retention");
        lib_dir.write_module(Path::new("arith.wasm"), &lib_wasm);

        // The exists body claims `is_pos` of its named choice under a filter —
        // a defined callee (an extern callee would be P005) and a non-vacuous
        // claim (a collapsed obligation would be P010). `add_three`
        // (executable) carries the extern call whose operand retention leaves
        // unshifted.
        let main_source = "external fn sum(a: i32, b: i32) -> i32;\n\
             use { sum } from arith;\n\
             pub fn add_three(x: i32) -> i32 { return sum(x, 3); }\n\
             fn is_pos(n: i32) -> bool { return n > 0; }\n\
             spec MySpec {\n\
                 fn witness() exists {\n\
                     let n: i32 = @;\n\
                     assume { assert(n > 3); }\n\
                     assert(is_pos(n));\n\
                 }\n\
             }";

        let arena = parse(main_source).expect("main parses");
        let typed = type_check(arena).expect("main type-checks");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let externals =
            resolve_external_modules(&typed, &search_path, None).expect("external modules resolve");
        let external_bytes = externals.module_bytes();

        let codegen_output = inference_wasm_codegen::codegen(
            &typed,
            "exprog",
            inference_wasm_codegen::CodegenOptions {
                mode: inference_wasm_codegen::CompilationMode::Proof,
                ..Default::default()
            },
        )
        .expect("proof-mode codegen succeeds");

        let unified = link(
            codegen_output.wasm(),
            &external_bytes,
            Some(&externals.contracts),
        )
        .expect("link succeeds");
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");

        // Empty explicit maps: the post-link embedded sections are the source
        // of truth (the pre-link codegen indices are stale after the merge).
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let rocq = wasm_to_v("exprog", &unified, &empty, &inference::HSpecMap::default())
            .expect("wasm-to-v succeeds");

        assert!(
            rocq.contains("Definition witness : module_func :="),
            "the exists spec function must be retained in the module record; .v was:\n{rocq}"
        );
        assert!(
            rocq.contains("BI_call 3%N"),
            "with the spec function retained, add_three's call to the merged \
             `sum` must keep the unshifted post-link index 3; .v was:\n{rocq}"
        );
        assert!(
            rocq.contains("reach_func := 2%N; reach_entry_arity := 0%nat"),
            "the obligation must name the retained function's unshifted \
             `mod_funcs` index; .v was:\n{rocq}"
        );
        assert!(
            rocq.contains("(T_app 1 ((T_local 0%N) :: nil))"),
            "the obligation's `is_pos` call must resolve to the unshifted \
             post-link index 1; .v was:\n{rocq}"
        );
        assert!(
            rocq.contains(
                "Theorem valid_exists_exprog__MySpec : ValidExistsSpec exprog \
                 exprog__MySpec_ex_specs."
            ),
            "the exists partition must survive the link and select \
             `ValidExistsSpec`; .v was:\n{rocq}"
        );
    }

    /// The unique-kind entry survives the link too: the linker round-trips
    /// `inference.hspecs` opaquely, so the kind and its metadata must come out
    /// the other side selecting `ValidUniqueSpec` over the retained body.
    /// Lowering and index arithmetic are byte-for-byte the exists sibling's —
    /// only the selected predicate differs — so this pins the kind tag's
    /// round-trip rather than re-deriving the numbering.
    #[test]
    fn proof_mode_unique_kind_survives_the_link() {
        let lib_wasm = compile_wasm(
            "pub fn sum(a: i32, b: i32) -> i32 { return a + b; }",
            "arith",
        );
        let lib_dir = TempLibDir::new("unique_retention");
        lib_dir.write_module(Path::new("arith.wasm"), &lib_wasm);

        let main_source = "external fn sum(a: i32, b: i32) -> i32;\n\
             use { sum } from arith;\n\
             pub fn add_three(x: i32) -> i32 { return sum(x, 3); }\n\
             spec MySpec {\n\
                 fn sole() unique {\n\
                     let n: i32 = @;\n\
                     assume { assert(n == 7); }\n\
                     assert(n > 0);\n\
                 }\n\
             }";

        let arena = parse(main_source).expect("main parses");
        let typed = type_check(arena).expect("main type-checks");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let externals =
            resolve_external_modules(&typed, &search_path, None).expect("external modules resolve");
        let external_bytes = externals.module_bytes();

        let codegen_output = inference_wasm_codegen::codegen(
            &typed,
            "uqprog",
            inference_wasm_codegen::CodegenOptions {
                mode: inference_wasm_codegen::CompilationMode::Proof,
                ..Default::default()
            },
        )
        .expect("proof-mode codegen succeeds");

        let unified = link(
            codegen_output.wasm(),
            &external_bytes,
            Some(&externals.contracts),
        )
        .expect("link succeeds");
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");

        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let rocq = wasm_to_v("uqprog", &unified, &empty, &inference::HSpecMap::default())
            .expect("wasm-to-v succeeds");

        assert!(
            rocq.contains("Definition sole : module_func :="),
            "the unique spec function must be retained in the module record; .v was:\n{rocq}"
        );
        assert!(
            rocq.contains("Definition uqprog__MySpec_uq_specs : list reachability_spec :="),
            "the unique partition must survive the link; .v was:\n{rocq}"
        );
        assert!(
            rocq.contains(
                "Theorem valid_unique_uqprog__MySpec : ValidUniqueSpec uqprog \
                 uqprog__MySpec_uq_specs."
            ),
            "the unique kind must select `ValidUniqueSpec` post-link; .v was:\n{rocq}"
        );
        assert!(
            rocq.contains("reach_visible_locs := (0%N :: nil)"),
            "the named choice must stay the one source-visible slot after the \
             round-trip; .v was:\n{rocq}"
        );
    }

    /// The end-to-end proof path for a linked external: one `.wasm` merged in,
    /// one `spec` naming it, one obligation applying it.
    ///
    /// This is the case the whole linker envelope exists for. An obligation
    /// about an `external fn` is only meaningful post-merge — before it the
    /// external is an import, and the downstream realization obligation has no
    /// body to reduce — so the symbol codegen writes must be the name the
    /// linker gives the merged body, and the resolution must happen against the
    /// linked module.
    ///
    /// The three assertions cover the three ways this can silently go wrong:
    /// the merged body must appear in the module record (an obligation about a
    /// function the record omits is unprovable), the application must resolve
    /// to *its* index rather than the main-side function's, and the claim's
    /// right-hand side must survive so the obligation says something the
    /// program can be wrong about.
    #[test]
    fn an_obligation_about_a_linked_extern_resolves_to_its_merged_body() {
        let lib_wasm = compile_wasm("pub fn double(a: i32) -> i32 { return a + a; }", "mathlib");
        let lib_dir = TempLibDir::new("extern_obligation");
        lib_dir.write_module(Path::new("mathlib.wasm"), &lib_wasm);

        let main_source = "external fn double(x: i32) -> i32;\n\
             use { double } from mathlib;\n\
             pub fn twice(x: i32) -> i32 { return double(x); }\n\
             spec DoubleSpec {\n\
                 fn doubles() forall {\n\
                     let x: i32 = @;\n\
                     assert(double(x) == x + x);\n\
                 }\n\
             }";

        let arena = parse(main_source).expect("main parses");
        let typed = type_check(arena).expect("main type-checks");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let externals =
            resolve_external_modules(&typed, &search_path, None).expect("external modules resolve");
        let external_bytes = externals.module_bytes();

        let codegen_output = inference_wasm_codegen::codegen(
            &typed,
            "linked_double",
            inference_wasm_codegen::CodegenOptions {
                mode: inference_wasm_codegen::CompilationMode::Proof,
                ..Default::default()
            },
        )
        .expect("proof-mode codegen succeeds");

        // Before the merge the obligation names a function the module only
        // imports, and translation names the missing step rather than emitting
        // a `T_app` that resolves to nothing.
        let empty: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        let pre_link = wasm_to_v(
            "linked_double",
            codegen_output.wasm(),
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect_err("an unlinked module cannot resolve an obligation about its extern");
        assert!(
            pre_link
                .to_string()
                .contains("Link the module before translating it"),
            "the pre-link rejection must name the missing step; got: {pre_link}"
        );

        let unified = link(
            codegen_output.wasm(),
            &external_bytes,
            Some(&externals.contracts),
        )
        .expect("link succeeds");
        inf_wasmparser::validate(&unified).expect("unified module is valid wasm");
        let rocq = wasm_to_v(
            "linked_double",
            &unified,
            &empty,
            &inference::HSpecMap::default(),
        )
        .expect("wasm-to-v succeeds on the linked module");

        // Post-link order is twice=0, merged `mathlib.double`=1.
        assert!(
            rocq.contains("Definition mathlib_double : module_func"),
            "the merged extern body must be an ordinary definition in the record; \
             .v was:\n{rocq}"
        );
        assert!(
            rocq.contains("T_app 1 ((T_local 0%N) :: nil)"),
            "the obligation must apply the merged extern at its own defined-fn \
             index; .v was:\n{rocq}"
        );
        assert!(
            rocq.contains("T_binop T_i32 (Binop_i BOI_add) (T_local 0%N) (T_local 0%N)"),
            "the claim the extern is measured against must survive; .v was:\n{rocq}"
        );
    }

    /// Compiles a multi-file program and links its externals, the pipeline an
    /// `infs build` of a project runs.
    ///
    /// `files` is `(module_path, source)` pairs with the entry file first, folded
    /// into one arena exactly as the project front end folds a source tree.
    fn compile_and_link_multi_file(
        files: &[(Vec<&str>, &str)],
        lib_dir: &Path,
        module_name: &str,
    ) -> Vec<u8> {
        let mut arena = inference_ast::arena::AstArena::default();
        for (module_path, source) in files {
            let module_path: Vec<String> = module_path.iter().map(|s| (*s).to_string()).collect();
            let parsed = inference_parser::parse_into(arena, source, module_path);
            assert!(
                parsed.errors.is_empty(),
                "multi-file source has syntax errors: {:?}",
                parsed.errors
            );
            arena = parsed.arena;
        }
        let typed = type_check(arena).expect("multi-file source type-checks");
        inference_analysis::analyze(&typed).expect("multi-file analysis succeeds");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.to_path_buf());
        let externals = resolve_external_modules(&typed, &search_path, None)
            .expect("external modules resolve and validate");
        let external_bytes = externals.module_bytes();

        let codegen_output = codegen(&typed, module_name).expect("multi-file codegen succeeds");
        link(
            codegen_output.wasm(),
            &external_bytes,
            Some(&externals.contracts),
        )
        .expect("link succeeds")
    }

    /// Instantiates a self-contained merged module and calls `name` with one
    /// `i32` argument.
    fn call_i32(unified: &[u8], name: &str, argument: i32) -> i32 {
        inf_wasmparser::validate(unified).expect("unified module is valid wasm");
        assert!(
            function_imports(unified).is_empty(),
            "a merged module must be self-contained, found {:?}",
            function_imports(unified)
        );
        let engine = Engine::default();
        let module =
            Module::new(&engine, unified).unwrap_or_else(|e| panic!("merged module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("merged module failed to instantiate: {e}"));
        let func: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, name)
            .unwrap_or_else(|e| panic!("merged module must export `{name}`: {e}"));
        func.call(&mut store, argument)
            .unwrap_or_else(|e| panic!("`{name}({argument})` failed: {e}"))
    }

    /// Two files each declare `external fn scale`, bind it to a **different**
    /// library, and call it. Executed end to end: each file's call must reach
    /// its own library.
    ///
    /// Nothing before execution can tell the two apart. The declarations agree
    /// on name and signature, so the module type-checks, links and validates
    /// whichever import each call is wired to; only the returned value says
    /// which library ran. `libA` multiplies by 999 and `libB` by 7, values
    /// nothing else in the program can produce.
    #[test]
    fn two_files_binding_one_name_to_two_libraries_each_call_their_own() {
        let lib_a = compile_wasm("pub fn scale(a: i32) -> i32 { return a * 999; }", "libA");
        let lib_b = compile_wasm("pub fn scale(a: i32) -> i32 { return a * 7; }", "libB");
        let lib_dir = TempLibDir::new("two-modules");
        lib_dir.write_module(Path::new("libA.wasm"), &lib_a);
        lib_dir.write_module(Path::new("libB.wasm"), &lib_b);

        let unified = compile_and_link_multi_file(
            &[
                (
                    vec![],
                    "use sib;\n\
                     external fn scale(a: i32) -> i32;\n\
                     use { scale } from libA;\n\
                     pub fn from_a(x: i32) -> i32 { return scale(x); }\n\
                     pub fn from_b(x: i32) -> i32 { return sib::via_b(x); }\n",
                ),
                (
                    vec!["sib"],
                    "external fn scale(a: i32) -> i32;\n\
                     use { scale } from libB;\n\
                     pub fn via_b(x: i32) -> i32 { return scale(x); }\n",
                ),
            ],
            lib_dir.path(),
            "two_modules",
        );

        assert_eq!(
            call_i32(&unified, "from_a", 2),
            1998,
            "the entry file's `scale` is bound to `libA`, so `from_a(2)` is 2 * 999"
        );
        assert_eq!(
            call_i32(&unified, "from_b", 2),
            14,
            "the sibling's `scale` is bound to `libB`, so `from_b(2)` is 2 * 7; \
             1998 would mean both files' calls funnelled into one import"
        );
    }

    /// Two files declaring and binding the same `(module, field)` share one
    /// import, and both calls reach it.
    ///
    /// The two declarations are distinct, but the import they need is the same
    /// one; the merge folds a single `libA.scale` body in, and neither call is
    /// left pointing at a second copy or at nothing.
    #[test]
    fn two_files_binding_one_library_share_a_single_merged_body() {
        let lib_a = compile_wasm("pub fn scale(a: i32) -> i32 { return a * 999; }", "libA");
        let lib_dir = TempLibDir::new("one-module");
        lib_dir.write_module(Path::new("libA.wasm"), &lib_a);

        let unified = compile_and_link_multi_file(
            &[
                (
                    vec![],
                    "use sib;\n\
                     external fn scale(a: i32) -> i32;\n\
                     use { scale } from libA;\n\
                     pub fn direct(x: i32) -> i32 { return scale(x); }\n\
                     pub fn through_sibling(x: i32) -> i32 { return sib::via(x); }\n",
                ),
                (
                    vec!["sib"],
                    "external fn scale(a: i32) -> i32;\n\
                     use { scale } from libA;\n\
                     pub fn via(x: i32) -> i32 { return scale(x); }\n",
                ),
            ],
            lib_dir.path(),
            "one_module",
        );

        assert_eq!(call_i32(&unified, "direct", 2), 1998);
        assert_eq!(call_i32(&unified, "through_sibling", 2), 1998);
        assert_eq!(
            defined_function_count(&unified),
            4,
            "`direct`, `through_sibling`, `via` and one merged `libA.scale`: the \
             two declarations must share a single import, not merge two copies"
        );
    }

    /// Two files bind the same `(module, field)` and **disagree** on its arity.
    /// The program must be rejected.
    ///
    /// The linker satisfies an import on `(module, field)` alone and compares no
    /// signatures, so a declaration whose signature never reaches validation is
    /// linked to the library regardless. Arity is the shape that makes the
    /// consequence an artifact rather than a late error: re-validating the
    /// merged module catches a disagreement in value *types*, but a surplus
    /// operand pushed into a narrower callee leaves the module well typed,
    /// because `return` is stack-polymorphic. The merged program then runs, and
    /// the extra argument is silently discarded.
    ///
    /// So validation has to reach *every* bound declaration, not one per
    /// `(module, field)`.
    #[test]
    fn two_files_disagreeing_on_one_externs_signature_are_rejected() {
        let lib_a = compile_wasm("pub fn scale(a: i32) -> i32 { return a * 999; }", "libA");
        let lib_dir = TempLibDir::new("signature-clash");
        lib_dir.write_module(Path::new("libA.wasm"), &lib_a);

        let mut arena = inference_ast::arena::AstArena::default();
        for (module_path, source) in [
            (
                Vec::new(),
                "use sib;\n\
                 external fn scale(a: i32) -> i32;\n\
                 use { scale } from libA;\n\
                 pub fn direct(x: i32) -> i32 { return scale(x); }\n\
                 pub fn through_sibling(a: i32, b: i32) -> i32 { return sib::via(a, b); }\n",
            ),
            (
                vec!["sib".to_string()],
                "external fn scale(a: i32, b: i32) -> i32;\n\
                 use { scale } from libA;\n\
                 pub fn via(a: i32, b: i32) -> i32 { return scale(a, b); }\n",
            ),
        ] {
            let parsed = inference_parser::parse_into(arena, source, module_path);
            assert!(
                parsed.errors.is_empty(),
                "multi-file source has syntax errors: {:?}",
                parsed.errors
            );
            arena = parsed.arena;
        }
        let typed = type_check(arena).expect("both declarations are well formed on their own");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let error = resolve_external_modules(&typed, &search_path, None)
            .expect_err("a declaration the library does not match must be rejected");
        let rendered = error.to_string();
        assert!(
            rendered.contains("signature mismatch for external function `scale`")
                && rendered.contains("declared (i32, i32) -> (i32)"),
            "the rejection must name the mismatching extern and the signature that \
             disagrees; got: {rendered}"
        );
    }

    /// Two files bind the same `(module, field)` and **disagree** on its write
    /// set. The program must be rejected, naming both files.
    ///
    /// This is the one shape where the contract could otherwise be applied to a
    /// program it does not describe. Both declarations survive resolution — they
    /// are distinct `external fn`s — while codegen folds them onto a *single*
    /// WASM import, pinned by
    /// [`two_files_binding_one_library_share_a_single_merged_body`]. The linker
    /// then runs one write-set check, and codegen consults each declaration
    /// separately, so accepting the `mut` file's claim would compile the
    /// non-`mut` file's calls against a contract that file never made.
    ///
    /// The existing cross-declaration gate cannot catch it: `mut` has no
    /// counterpart in a WASM signature, so both declarations lower identically
    /// and both validate against the library. Hence a check of its own, and a
    /// hard error rather than a reconciliation — a union licenses the non-`mut`
    /// file, an intersection refuses the `mut` file's legitimate link, and taking
    /// the first match is the miscompile itself.
    #[test]
    fn two_files_disagreeing_on_one_externs_write_set_are_rejected() {
        let lib_wasm = wat::parse_str(SORTLIB_WAT).expect("sortlib WAT assembles");
        let lib_dir = TempLibDir::new("write-set-clash");
        lib_dir.write_module(Path::new("sortlib.wasm"), &lib_wasm);

        let mut arena = inference_ast::arena::AstArena::default();
        for (module_path, source) in [
            (
                Vec::new(),
                "use sib;\n\
                 external fn sort_pair(mut p: [i32; 2]);\n\
                 use { sort_pair } from sortlib;\n\
                 pub fn direct() -> i32 {\n\
                     let mut arr: [i32; 2] = [5, 2];\n\
                     sort_pair(arr);\n\
                     return arr[0];\n\
                 }\n\
                 pub fn through_sibling() -> i32 { return sib::via(); }\n",
            ),
            (
                vec!["sib".to_string()],
                "external fn sort_pair(p: [i32; 2]);\n\
                 use { sort_pair } from sortlib;\n\
                 pub fn via() -> i32 {\n\
                     let arr: [i32; 2] = [5, 2];\n\
                     sort_pair(arr);\n\
                     return arr[0];\n\
                 }\n",
            ),
        ] {
            let parsed = inference_parser::parse_into(arena, source, module_path);
            assert!(
                parsed.errors.is_empty(),
                "multi-file source has syntax errors: {:?}",
                parsed.errors
            );
            arena = parsed.arena;
        }
        let typed = type_check(arena).expect("both declarations are well formed on their own");

        let mut search_path = SearchPath::new();
        search_path.push_lib_dir(lib_dir.path().to_path_buf());
        let error = resolve_external_modules(&typed, &search_path, None)
            .expect_err("two declarations disagreeing on the write set must be rejected");
        let rendered = error.to_string();
        assert!(
            rendered.contains("conflicting write sets for external function `sort_pair`")
                && rendered.contains("module `sortlib`"),
            "the rejection must name the extern and the module it is bound to; got: {rendered}"
        );
        assert!(
            rendered.contains("the entry file") && rendered.contains("`sib`"),
            "the rejection must name BOTH declaring files, since the fix is to make \
             them agree and neither is wrong on its own; got: {rendered}"
        );
    }
}
