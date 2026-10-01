//! Code generation for the repeated array literal `[value; N]` (#218).
//!
//! The lowering stores the value once into element 0 and copies element 0
//! into the rest by doubling regions, so what can go wrong is a copy at the
//! wrong displacement, a doubling step that leaves the tail short when `N` is
//! not a power of two, or a value whose stores were elided where the frame was
//! not zero. Every function below therefore reads back elements across the
//! whole array, at counts that are and are not powers of two, and the module
//! is run at both instruction levels: the WebAssembly 1.0 copies and the
//! `memory.copy` a bulk-memory build emits instead.

#[cfg(test)]
mod array_repeat_tests {
    use crate::utils::{wasm_codegen, wasm_codegen_with_features};
    use inference_wasm_codegen::EmitFeatures;
    use wasmtime::{Engine, Instance, Module, Store, WasmResults};

    const SOURCE: &str = r#"
struct P { x: i64; y: i64; }
struct S { xs: [i32; 3]; n: i32; }

fn square(v: i32) -> i32 { return v * v; }

pub fn scalar_sum() -> i32 {
    let a: [i32; 4] = [7; 4];
    return a[0] + a[1] + a[2] + a[3];
}

pub fn odd_counts() -> i32 {
    let five: [i32; 5] = [1; 5];
    let seven: [i32; 7] = [10; 7];
    let thirteen: [i32; 13] = [100; 13];
    let mut s: i32 = 0;
    let mut i: i32 = 0;
    loop i < 5 { s = s + five[i]; i = i + 1; }
    i = 0;
    loop i < 7 { s = s + seven[i]; i = i + 1; }
    i = 0;
    loop i < 13 { s = s + thirteen[i]; i = i + 1; }
    return s;
}

pub fn wide_sum() -> i64 {
    let a: [i64; 1000] = [3; 1000];
    let mut s: i64 = 0;
    let mut i: i32 = 0;
    loop i < 1000 { s = s + a[i]; i = i + 1; }
    return s;
}

pub fn narrow_last() -> u8 {
    let a: [u8; 13] = [200; 13];
    return a[12];
}

pub fn narrow_first() -> u8 {
    let a: [u8; 13] = [200; 13];
    return a[0];
}

pub fn from_a_binding() -> i32 {
    let v: i32 = 41;
    let a: [i32; 6] = [v + 1; 6];
    return a[0] + a[5];
}

pub fn called_once() -> i32 {
    let a: [i32; 9] = [square(3); 9];
    return a[8];
}

pub fn struct_sum() -> i64 {
    let ps: [P; 6] = [P { x: 1, y: 2 }; 6];
    let mut s: i64 = 0;
    let mut i: i32 = 0;
    loop i < 6 { s = s + ps[i].y; i = i + 1; }
    return s + ps[5].x;
}

pub fn nested_sum() -> i32 {
    let g: [[i32; 3]; 5] = [[2; 3]; 5];
    let mut s: i32 = 0;
    let mut i: i32 = 0;
    loop i < 5 {
        let mut j: i32 = 0;
        loop j < 3 { s = s + g[i][j]; j = j + 1; }
        i = i + 1;
    }
    return s;
}

pub fn row_of_rows() -> i32 {
    let row: [i32; 3] = [1, 2, 3];
    let g: [[i32; 3]; 4] = [row; 4];
    return g[0][0] + g[3][2] + g[2][1];
}

pub fn reassigned() -> i32 {
    let mut a: [i32; 4] = [1, 2, 3, 4];
    a = [9; 4];
    return a[0] + a[1] + a[2] + a[3];
}

pub fn self_referencing_scalar() -> i32 {
    let mut a: [i32; 4] = [1, 2, 3, 4];
    a = [a[1]; 4];
    return a[0] + a[3];
}

pub fn self_referencing_struct() -> i64 {
    let mut ps: [P; 3] = [P { x: 5, y: 7 }, P { x: 0, y: 0 }, P { x: 0, y: 0 }];
    ps = [P { x: ps[0].x + 1, y: ps[0].y + ps[0].x }; 3];
    return ps[2].y * 100 + ps[1].y;
}

fn make() -> [i32; 5] { return [4; 5]; }

pub fn returned() -> i32 {
    let a: [i32; 5] = make();
    return a[0] + a[4];
}

fn make_points() -> [P; 3] { return [P { x: 2, y: 3 }; 3]; }

pub fn returned_structs() -> i64 {
    let ps: [P; 3] = make_points();
    return ps[0].y + ps[2].y;
}

pub fn struct_field() -> i32 {
    let s: S = S { xs: [6; 3], n: 1 };
    return s.xs[0] + s.xs[2] + s.n;
}

pub fn constant() -> i32 {
    const A: [i32; 3] = [5; 3];
    return A[0] + A[2];
}

pub fn rezeroed_in_a_loop() -> i32 {
    let mut seen: i32 = 0;
    let mut i: i32 = 0;
    loop i < 2 {
        let mut a: [i32; 4] = [0; 4];
        seen = seen + a[0] + a[3];
        a[0] = 5;
        a[3] = 6;
        i = i + 1;
    }
    return seen;
}
"#;

    /// Runs `name` in the module built from [`SOURCE`] at both instruction
    /// levels, asserting they agree, and returns the result.
    fn call<R: WasmResults + PartialEq + std::fmt::Debug>(name: &str) -> R {
        let lowered = run::<R>(&wasm_codegen(SOURCE), name);
        let bulk = run::<R>(
            &wasm_codegen_with_features(SOURCE, EmitFeatures { bulk_memory: true }),
            name,
        );
        assert_eq!(lowered, bulk, "`{name}` differs between instruction levels");
        lowered
    }

    fn run<R: WasmResults>(wasm: &[u8], name: &str) -> R {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).unwrap_or_else(|e| panic!("module rejected: {e}"));
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .unwrap_or_else(|e| panic!("module failed to instantiate: {e}"));
        instance
            .get_typed_func::<(), R>(&mut store, name)
            .unwrap_or_else(|e| panic!("`{name}` is not exported with that signature: {e}"))
            .call(&mut store, ())
            .unwrap_or_else(|e| panic!("`{name}` trapped: {e}"))
    }

    #[test]
    fn every_element_holds_the_value() {
        cov_mark::check!(wasm_codegen_emit_array_repeat);
        assert_eq!(call::<i32>("scalar_sum"), 28);
    }

    /// Five, seven and thirteen elements leave the doubling a partial last
    /// copy each: 1 → 2 → 4 → 5, 1 → 2 → 4 → 7, and 1 → 2 → 4 → 8 → 13.
    #[test]
    fn counts_that_are_not_powers_of_two_fill_the_tail() {
        assert_eq!(call::<i32>("odd_counts"), 5 + 70 + 1300);
    }

    /// 8000 bytes: the large doubling steps exceed the straight-line copy
    /// limit and take the looped copy.
    #[test]
    fn a_long_array_takes_the_looped_copy() {
        assert_eq!(call::<i64>("wide_sum"), 3000);
    }

    /// A 13-byte region is copied in 8, 4 and 1-byte units, so both ends of a
    /// byte array are worth reading.
    #[test]
    fn narrow_elements_are_copied_whole() {
        assert_eq!(call::<i32>("narrow_first"), 200);
        assert_eq!(call::<i32>("narrow_last"), 200);
    }

    #[test]
    fn the_value_may_be_any_expression() {
        assert_eq!(call::<i32>("from_a_binding"), 84);
        assert_eq!(call::<i32>("called_once"), 9);
    }

    #[test]
    fn struct_elements_are_copied_field_for_field() {
        assert_eq!(call::<i64>("struct_sum"), 12 + 1);
    }

    #[test]
    fn repeats_nest_and_repeat_arrays() {
        assert_eq!(call::<i32>("nested_sum"), 30);
        assert_eq!(call::<i32>("row_of_rows"), 1 + 3 + 2);
    }

    #[test]
    fn a_repeat_reassigns_every_element() {
        assert_eq!(call::<i32>("reassigned"), 36);
    }

    /// A value that reads the array it is assigned to sees the array before
    /// the assignment. For a scalar that is `a[1]` before any store; for a
    /// struct it is every field of `ps[0]`, which an in-place store of field
    /// `x` would clobber before field `y` reads it — so the result is
    /// `y = 7 + 5`, not `7 + 6`.
    #[test]
    fn a_self_referencing_repeat_reads_the_old_array() {
        assert_eq!(call::<i32>("self_referencing_scalar"), 4);
        assert_eq!(call::<i64>("self_referencing_struct"), 1200 + 12);
    }

    #[test]
    fn a_repeat_is_returned_through_the_result_pointer() {
        assert_eq!(call::<i32>("returned"), 8);
        assert_eq!(call::<i64>("returned_structs"), 6);
    }

    #[test]
    fn a_repeat_fills_a_struct_field_and_a_constant() {
        assert_eq!(call::<i32>("struct_field"), 13);
        assert_eq!(call::<i32>("constant"), 10);
    }

    /// The zero stores of `[0; N]` are elided only where the frame is known to
    /// be zero. Inside a loop it is not on the second iteration, so the repeat
    /// must store its zeros there, or the second iteration reads 5 and 6.
    #[test]
    fn a_zero_repeat_inside_a_loop_re_zeroes_its_array() {
        assert_eq!(call::<i32>("rezeroed_in_a_loop"), 0);
    }

    /// The value is evaluated once: the call that computes it appears once in
    /// the function, however many elements it fills.
    #[test]
    fn the_value_is_evaluated_once() {
        let wat = wasmprinter::print_bytes(wasm_codegen(SOURCE)).expect("printable");
        let body = function_body(&wat, "called_once");
        assert_eq!(body.matches("call ").count(), 1, "{body}");
    }

    /// `[0; N]` outside a loop stores nothing: the prologue has zeroed the
    /// frame, as it has for a list of zeros.
    #[test]
    fn a_zero_repeat_outside_a_loop_stores_nothing() {
        cov_mark::check!(wasm_codegen_array_repeat_zero_elided);
        let source = "pub fn z() -> i32 { let a: [i32; 64] = [0; 64]; return a[63]; }";
        let wat = wasmprinter::print_bytes(wasm_codegen(source)).expect("printable");
        let body = function_body(&wat, "z");
        assert!(!body.contains("i32.store"), "{body}");
        assert_eq!(run::<i32>(&wasm_codegen(source), "z"), 0);
    }

    /// The text of the function named `name` in `wat`, up to the next function.
    fn function_body<'a>(wat: &'a str, name: &str) -> &'a str {
        let start = wat
            .find(&format!("(func ${name} "))
            .unwrap_or_else(|| panic!("no function `{name}` in:\n{wat}"));
        let rest = &wat[start..];
        let end = rest[1..].find("(func ").map_or(rest.len(), |i| i + 1);
        &rest[..end]
    }
}
