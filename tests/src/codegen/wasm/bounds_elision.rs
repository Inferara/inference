//! Differential soundness check for omitting the bounds guards analysis proved
//! dead (#215).
//!
//! Under the `omit-proven` bounds-check policy, code generation omits the
//! runtime guard of an array access A056's range analysis bounded within
//! `0..length`. Under the default policy the guard stays, and is what an
//! analysis bug falls back to: a wrong proof traps. Without it, a wrong proof
//! reads or writes outside the array, silently. These tests make such a bug
//! less likely to reach a build that opted out of the guard; they cannot give
//! back the guarantee the guard gives every program, only check the programs
//! they run, which is why omission is a policy a build chooses.
//!
//! Every program here is built twice from one typed context, each as `infc`
//! builds it: under `omit-proven`, whose analysis hands code generation the
//! accesses it proved, and under the default `all`, which keeps every guard.
//! The two modules are then run side by side on the same inputs. They differ
//! only in guards that are claimed never to fire, so on every input they must
//! return the same values, trap with the same trap, and leave linear memory
//! byte for byte the same. An elided guard that would have fired shows up as a
//! mismatch: the guarded build traps where the elided one reads a neighbour's
//! bytes or writes over them.
//!
//! The sweep runs over the whole single-file codegen corpus and over a set of
//! programs written to exercise each source of fact the analysis draws on.

#[cfg(test)]
mod bounds_elision_tests {
    use crate::corpus::{has_import_section, single_file_corpus_sources};
    use inference_type_checker::typed_context::TypedContext;
    use inference_wasm_codegen::{BoundsChecks, CodegenOptions, codegen_with_proven_in_bounds};
    use wasmtime::{Config, Engine, Instance, Linker, Module, Store, Trap, Val, ValType};

    /// Fuel for one call. A call that runs out in either build proves nothing
    /// about the other, so its input is skipped rather than compared.
    const FUEL: u64 = 2_000_000;

    /// Inputs every exported parameter is tried at: the bounds of the small
    /// arrays the programs index, the neighbours of each, and the extremes a
    /// sign or width confusion would produce.
    const EDGES: [i64; 24] = [
        0,
        1,
        2,
        3,
        4,
        5,
        7,
        8,
        9,
        15,
        16,
        127,
        128,
        255,
        256,
        257,
        -1,
        -2,
        -4,
        -128,
        -129,
        i32::MAX as i64,
        i32::MIN as i64,
        u32::MAX as i64,
    ];

    /// Pseudo-random inputs per function on top of the edge values, from a fixed
    /// seed so a failure reproduces.
    const RANDOM_CALLS: usize = 160;

    /// The two builds of one program.
    struct Builds {
        elided: Vec<u8>,
        guarded: Vec<u8>,
    }

    /// Builds `source` under `omit-proven` and under `all`, or `None` when the
    /// program does not reach code generation, or when no guard was elided and
    /// the two builds are the same bytes.
    fn builds(source: &str) -> Option<Builds> {
        let arena = crate::utils::try_build_ast(source.to_string()).ok()?;
        let ctx = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .ok()?
            .typed_context();
        let elided = build_under(&ctx, BoundsChecks::OmitProven)?;
        let guarded = build_under(&ctx, BoundsChecks::All)?;
        (elided != guarded).then_some(Builds { elided, guarded })
    }

    /// Builds `ctx` as `infc` does under `policy`: the analysis runs under it,
    /// and code generation is handed whatever that analysis proved.
    fn build_under(ctx: &TypedContext, policy: BoundsChecks) -> Option<Vec<u8>> {
        let options = CodegenOptions {
            bounds_checks: policy,
            ..Default::default()
        };
        let analysis =
            inference_analysis::analyze_with_options(ctx, crate::utils::analysis_options(&options))
                .ok()?;
        let output = codegen_with_proven_in_bounds(
            ctx,
            "output",
            options,
            analysis.proven_in_bounds().accesses(),
        )
        .ok()?;
        Some(output.wasm().to_vec())
    }

    fn engine() -> Engine {
        let mut config = Config::new();
        config.consume_fuel(true);
        Engine::new(&config).expect("an engine with fuel metering")
    }

    /// Host functions a program here may import, each returning a value
    /// outside the narrow type it is declared at.
    fn linker(engine: &Engine, host_results: &[(&'static str, i32)]) -> Linker<()> {
        let mut linker = Linker::new(engine);
        for &(field, value) in host_results {
            linker
                .func_wrap("env", field, move || value)
                .unwrap_or_else(|e| panic!("`env.{field}` is supplied by the host: {e}"));
        }
        linker
    }

    /// One instantiated build.
    struct Running {
        store: Store<()>,
        instance: Instance,
    }

    impl Running {
        fn new(engine: &Engine, linker: &Linker<()>, module: &Module) -> Self {
            let mut store = Store::new(engine, ());
            let instance = linker
                .instantiate(&mut store, module)
                .unwrap_or_else(|e| panic!("the module instantiates: {e}"));
            Self { store, instance }
        }

        /// Calls `export` with `args` and returns what a caller can observe:
        /// the results or the trap, and the whole of linear memory after it.
        /// `None` when the call ran out of fuel.
        fn call(&mut self, export: &str, args: &[Val], results: &[ValType]) -> Option<Observed> {
            self.store.set_fuel(FUEL).expect("fuel metering is on");
            let func = self
                .instance
                .get_func(&mut self.store, export)
                .expect("the export is a function");
            let mut out: Vec<Val> = results.iter().map(|ty| arg(ty, 0)).collect();
            let outcome = match func.call(&mut self.store, args, &mut out) {
                Ok(()) => Outcome::Returned(out.iter().map(scalar).collect()),
                Err(error) => match error.downcast_ref::<Trap>() {
                    Some(Trap::OutOfFuel) => return None,
                    Some(trap) => Outcome::Trapped(*trap),
                    None => panic!("`{export}` failed with something other than a trap: {error}"),
                },
            };
            let memory = self
                .instance
                .get_memory(&mut self.store, "memory")
                .map(|memory| memory.data(&self.store).to_vec())
                .unwrap_or_default();
            Some(Observed { outcome, memory })
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Outcome {
        Returned(Vec<i64>),
        Trapped(Trap),
    }

    struct Observed {
        outcome: Outcome,
        memory: Vec<u8>,
    }

    fn scalar(value: &Val) -> i64 {
        match value {
            Val::I32(v) => i64::from(*v),
            Val::I64(v) => *v,
            other => panic!("an Inference export returns only integers, got {other:?}"),
        }
    }

    /// A deterministic xorshift, so the random inputs are the same on every run.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        /// Mostly small values, where array bounds live, and some anywhere.
        #[allow(clippy::cast_possible_wrap)]
        fn input(&mut self) -> i64 {
            let raw = self.next();
            match raw % 4 {
                0 => (raw >> 8) as i64,
                _ => ((raw >> 8) % 600) as i64 - 300,
            }
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn arg(ty: &ValType, value: i64) -> Val {
        match ty {
            ValType::I32 => Val::I32(value as i32),
            ValType::I64 => Val::I64(value),
            other => panic!("an Inference export takes only integers, got {other:?}"),
        }
    }

    /// The argument vectors `export` is called with: every edge value in every
    /// position the arity allows cheaply, then pseudo-random vectors.
    fn inputs(params: &[ValType], seed: u64) -> Vec<Vec<Val>> {
        let mut inputs = Vec::new();
        match params.len() {
            0 => inputs.push(Vec::new()),
            1 => inputs.extend(EDGES.iter().map(|&v| vec![arg(&params[0], v)])),
            2 => {
                for &a in &EDGES {
                    for &b in &EDGES {
                        inputs.push(vec![arg(&params[0], a), arg(&params[1], b)]);
                    }
                }
            }
            _ => {
                for &v in &EDGES {
                    inputs.push(params.iter().map(|ty| arg(ty, v)).collect());
                }
            }
        }
        if !params.is_empty() {
            let mut rng = Rng(seed | 1);
            for _ in 0..RANDOM_CALLS {
                inputs.push(params.iter().map(|ty| arg(ty, rng.input())).collect());
            }
        }
        inputs
    }

    /// Runs every export of both builds side by side and returns each
    /// divergence, described, together with the number of calls compared.
    fn compare(
        name: &str,
        builds: &Builds,
        host_results: &[(&'static str, i32)],
    ) -> (Vec<String>, usize) {
        let engine = engine();
        let linker = linker(&engine, host_results);
        let elided = Module::new(&engine, &builds.elided)
            .unwrap_or_else(|e| panic!("{name}: the elided build is a valid module: {e}"));
        let guarded = Module::new(&engine, &builds.guarded)
            .unwrap_or_else(|e| panic!("{name}: the guarded build is a valid module: {e}"));

        let exports: Vec<(String, Vec<ValType>, Vec<ValType>)> = elided
            .exports()
            .filter_map(|export| {
                let func = export.ty().func()?.clone();
                Some((
                    export.name().to_string(),
                    func.params().collect(),
                    func.results().collect(),
                ))
            })
            .collect();

        let mut divergences = Vec::new();
        let mut compared = 0;
        for (index, (export, params, results)) in exports.iter().enumerate() {
            let mut left = Running::new(&engine, &linker, &elided);
            let mut right = Running::new(&engine, &linker, &guarded);
            for args in inputs(params, 0x9E37_79B9_7F4A_7C15 ^ index as u64) {
                let (Some(a), Some(b)) = (
                    left.call(export, &args, results),
                    right.call(export, &args, results),
                ) else {
                    left = Running::new(&engine, &linker, &elided);
                    right = Running::new(&engine, &linker, &guarded);
                    continue;
                };
                compared += 1;
                if a.outcome != b.outcome || a.memory != b.memory {
                    let memory = if a.memory == b.memory {
                        "same"
                    } else {
                        "different"
                    };
                    divergences.push(format!(
                        "{name}: `{export}({args:?})` returned {:?} with the guards elided and \
                         {:?} with every guard kept; memory afterwards is {memory}",
                        a.outcome, b.outcome
                    ));
                    break;
                }
                // A trap leaves the shadow-stack pointer where the trapping
                // frame moved it, which the next call would inherit.
                if matches!(a.outcome, Outcome::Trapped(_)) {
                    left = Running::new(&engine, &linker, &elided);
                    right = Running::new(&engine, &linker, &guarded);
                }
            }
        }
        (divergences, compared)
    }

    /// Every corpus program whose build omits a guard behaves exactly like the
    /// build that keeps them all.
    ///
    /// The floors keep the sweep from passing vacuously: a corpus in which no
    /// program elides anything, or in which no call is compared, would agree
    /// with any elision at all.
    #[test]
    fn corpus_builds_with_and_without_proven_guards_agree() {
        let mut divergences = Vec::new();
        let mut programs = 0;
        let mut calls = 0;
        for (path, source) in single_file_corpus_sources() {
            let Some(builds) = builds(&source) else {
                continue;
            };
            // An import needs a host this sweep does not supply.
            if has_import_section(&builds.elided) {
                continue;
            }
            programs += 1;
            let (found, compared) = compare(&path, &builds, &[]);
            divergences.extend(found);
            calls += compared;
        }
        assert!(
            divergences.is_empty(),
            "an elided guard would have fired:\n{}",
            divergences.join("\n")
        );
        assert!(
            programs >= 15,
            "only {programs} corpus programs elide a guard"
        );
        assert!(calls >= 5_000, "only {calls} calls were compared");
    }

    /// One program for each source of fact the analysis proves an index from,
    /// so each is exercised by execution and not only by the rule's own tests.
    const PROGRAMS: &[(&str, &str)] = &[
        (
            "binary search",
            "pub fn f(t: i32) -> i32 { let a: [i32; 8] = [2, 5, 8, 12, 16, 23, 38, 56]; \
             let mut r: i32 = 8; let mut lo: i32 = 0; let mut hi: i32 = 7; \
             loop lo <= hi { let mid: i32 = (lo + hi) / 2; let v: i32 = a[mid]; \
             if v == t { r = mid; break; } if v < t { lo = mid + 1; } else { hi = mid - 1; } } \
             return r; }",
        ),
        (
            "remainder and mask",
            "pub fn f(i: u32, k: i32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; \
             return a[i % 4] + a[k & 3]; }",
        ),
        (
            "signed remainder under a lower bound",
            "pub fn f(i: i32) -> i32 { let a: [i32; 5] = [1, 2, 3, 4, 5]; let mut r: i32 = 0; \
             if i >= 0 { r = a[i % 5]; } return r; }",
        ),
        (
            "early return",
            "pub fn f(i: u32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; \
             if i >= 4 { return 0; } return a[i]; }",
        ),
        (
            "offset index",
            "pub fn f(j: u32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = 0; \
             if j + 1 < 4 { r = a[j + 1]; } return r; }",
        ),
        (
            "both bounds on a signed index",
            "pub fn f(i: i32) -> i32 { let mut a: [i32; 4] = [1, 2, 3, 4]; let mut r: i32 = -1; \
             if i >= 0 && i < 4 { a[i] = i * 10; r = a[i]; } else { r = 0; } return r; }",
        ),
        (
            "short-circuit or",
            "pub fn f(i: u32) -> bool { let a: [i32; 4] = [1, 2, 3, 4]; \
             return i >= 4 || a[i] > 0; }",
        ),
        (
            "loop exited by a guarded break",
            "pub fn f(n: i32) -> i32 { let a: [i32; 4] = [1, 2, 3, 4]; let mut s: i32 = 0; \
             let mut i: i32 = 0; loop { if i >= 4 || i >= n { break; } s = s + a[i]; \
             i = i + 1; } return s; }",
        ),
        (
            "nested counting loops",
            "pub fn f(x: i32) -> i32 { let mut a: [i32; 6] = [5, 3, 8, 1, 9, 2]; a[0] = x; \
             let mut i: i32 = 0; loop i < 6 { let mut j: i32 = 0; loop j < 5 { \
             let k: i32 = j + 1; if a[j] > a[k] { let t: i32 = a[j]; a[j] = a[k]; a[k] = t; } \
             j = j + 1; } i = i + 1; } return a[0] * 100 + a[5]; }",
        ),
        (
            "nested access",
            "pub fn f(i: u32, j: u32) -> i32 { let g: [[i32; 3]; 2] = [[1, 2, 3], [4, 5, 6]]; \
             let mut r: i32 = 0; if i < 2 && j < 3 { r = g[i][j]; } return r; }",
        ),
        (
            "array of structs",
            "struct P { x: i32; y: i32; } \
             pub fn f(i: u32, v: i32) -> i32 { let mut ps: [P; 3] = \
             [P { x: 1, y: 2 }, P { x: 3, y: 4 }, P { x: 5, y: 6 }]; let mut r: i32 = 0; \
             if i < 3 { ps[i].y = v; r = ps[i].x + ps[i].y; } return r; }",
        ),
        (
            "array field through a method",
            "struct T { a: [i32; 4]; fn get(self, i: u32) -> i32 { let mut r: i32 = 0; \
             if i < 4 { r = self.a[i]; } return r; } } \
             pub fn f(i: u32) -> i32 { let t: T = T { a: [7, 8, 9, 10] }; return t.get(i); }",
        ),
        (
            "narrow index bounded by its type",
            "pub fn f(k: u8, s: i8) -> i32 { let a: [i32; 256] = [ELEMENTS]; \
             let mut r: i32 = a[k]; if s >= 0 { r = r + a[s]; } return r; }",
        ),
        (
            "named and computed constants",
            "pub fn f(x: i32) -> i32 { const K: i32 = 2; let mut a: [i32; 4] = [1, 2, 3, 4]; \
             a[K] = x; return a[K] + a[1 + 2]; }",
        ),
    ];

    /// `0, 1, …, 255`, spelled into the programs that index a `[i32; 256]`, so
    /// every in-range read returns its own index.
    fn elements() -> String {
        (0..256)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    #[test]
    fn programs_for_each_source_of_fact_agree_with_and_without_proven_guards() {
        let mut divergences = Vec::new();
        for (name, source) in PROGRAMS {
            let source = source.replace("ELEMENTS", &elements());
            let builds = builds(&source)
                .unwrap_or_else(|| panic!("{name}: must compile and elide at least one guard"));
            let (found, compared) = compare(name, &builds, &[]);
            assert!(compared > 0, "{name}: no call was compared");
            divergences.extend(found);
        }
        assert!(
            divergences.is_empty(),
            "an elided guard would have fired:\n{}",
            divergences.join("\n")
        );
    }

    /// An index produced by an `external fn` is bounded by its declared type
    /// alone, which only holds because the call canonicalizes the result: the
    /// host below returns values no `u8` or `i8` holds. The guard is elided, and
    /// both builds read the element the canonical value names.
    #[test]
    fn a_narrow_extern_result_indexes_within_its_type_once_its_guard_is_gone() {
        let source = format!(
            "external fn pick_u8() -> u8;\n\
             external fn pick_i8() -> i8;\n\
             use {{ pick_u8, pick_i8 }} from host::env;\n\
             pub fn f() -> i32 {{ let a: [i32; 256] = [{}]; let k: u8 = pick_u8(); \
             let s: i8 = pick_i8(); let mut r: i32 = a[k]; \
             if s >= 0 {{ r = r * 1000 + a[s]; }} return r; }}",
            elements()
        );
        let builds = builds(&source).expect("the program compiles and elides its guards");
        for (pick_u8, pick_i8, expected) in [
            (300, 100, 44 * 1000 + 100),
            (-1, 127, 255 * 1000 + 127),
            (511, 383, 255 * 1000 + 127),
            (256, -1, 0),
        ] {
            let hosts = [("pick_u8", pick_u8), ("pick_i8", pick_i8)];
            let (divergences, compared) = compare("narrow extern index", &builds, &hosts);
            assert_eq!(compared, 1, "`f` takes no argument and is called once");
            assert!(divergences.is_empty(), "{}", divergences.join("\n"));

            let engine = engine();
            let module = Module::new(&engine, &builds.elided).expect("the elided build is valid");
            let mut running = Running::new(&engine, &linker(&engine, &hosts), &module);
            let observed = running
                .call("f", &[], &[ValType::I32])
                .expect("`f` finishes");
            assert_eq!(
                observed.outcome,
                Outcome::Returned(vec![expected]),
                "host values ({pick_u8}, {pick_i8}) must index as their canonical u8 and i8"
            );
        }
    }
}
