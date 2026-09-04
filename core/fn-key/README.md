# inference-fn-key

Canonical WASM function identity for the Inference compiler. Provides `FnKey`, the single shared type that codegen and static analysis both use to identify functions, so the two phases always agree on which function is which.

## Why This Crate Exists

An Inference program may span multiple source files, and a multi-file program compiles to a single WASM module. Two source files can each define a function called `add`; a struct in one file can have a method whose name matches a free function in a sibling file. Identifying functions by a flat string does not distinguish these cases: the string `a.mid.make` is produced both by a method `make` on struct `mid` (defined in file `a`) and by a free function `make` in file `a/mid`. A flat key conflates them.

`FnKey` is a structured enum that partitions the WASM function namespace by construction. It carries the defining file's path as an explicit field rather than folding it into a string, so a collision that is impossible in the source language is equally impossible in the key space.

Both the code generator (when assigning WASM function indices and emitting frame-size maps) and the analysis pass (when building the whole-program call graph for recursion and stack-depth checks) key functions on this single type. A single shared type means the two phases cannot drift apart by re-implementing string mangling differently.

**Why a separate leaf crate**: `analysis` must not depend on `wasm-codegen`, and `wasm-codegen` must not depend on `analysis`. Placing `FnKey` in either one would force a circular dependency or push backend-naming logic onto every consumer. Folding it into `ast` or `type-checker` would push code-generation naming concerns onto every syntax consumer. A zero-dependency leaf crate below both resolves the layering cleanly. The `[dependencies]` section of this crate's `Cargo.toml` is intentionally empty.

## The Four Variants

```rust
pub enum FnKey {
    Free      { module_path: Vec<String>, name: String },
    Method    { module_path: Vec<String>, struct_name: String, name: String },
    SpecFree  { module_path: Vec<String>, spec: String, name: String },
    SpecMethod{ module_path: Vec<String>, spec: String, struct_name: String, name: String },
}
```

| Variant | Represents |
|---------|------------|
| `Free` | A free function (not associated with a struct or spec) |
| `Method` | An instance or associated function on a named struct |
| `SpecFree` | A free function declared inside a `spec` block |
| `SpecMethod` | A method declared inside a `spec` block on a named struct |

Every variant carries `module_path`: the source-root-relative path segments of the **defining** file. For a method, this is the struct's defining file, not the call site's. The entry file has an empty `module_path`, so its functions keep unqualified keys — a single-file program produces byte-identical output to before file qualification existed.

The spec variants keep `module_path` and the bare `spec` name as structurally separate fields. They do not pre-fold the file into the `spec` string. This is what makes the key injective: `["lib", "checks"] + S` and `["lib_checks"] + S` are distinct keys even though their rendered display is identical. See the Identity vs Display section below.

## Constructors

Prefer the named constructors over direct enum construction.

```rust
// A free function in the entry file.
let key = FnKey::free_in(vec![], "add");

// A free function in a non-entry file.
let key = FnKey::free_in(vec!["lib".into(), "geometry".into()], "distance");

// A method on struct `Point`, defined in a non-entry file.
let key = FnKey::method_in(vec!["lib".into()], "Point", "new");

// A spec-inner free function (entry file, spec name stored verbatim).
let key = FnKey::spec_free("MySpec", "f");

// A spec-inner free function with a real defining file (preferred for non-entry files).
let module_path = vec!["lib".into(), "checks".into()];
let key = FnKey::spec_free_folded(&module_path, "S", "rec");

// A spec-inner method (entry file).
let key = FnKey::spec_method("MySpec", "Point", "init");

// A spec-inner method with a real defining file.
let key = FnKey::spec_method_folded(&module_path, "S", "Point", "init");
```

For the spec variants the `_folded` form is the preferred choice whenever the defining file is a non-entry file: it preserves the real `module_path` and bare `spec` name in the key, ensuring injectivity. The bare `spec_free` / `spec_method` forms are for the entry file (empty module path) or for callers that hold only an already-rendered spec name.

## Identity vs Display

`FnKey` derives `Hash` and `Eq` over all fields, including `module_path`. This is the identity comparison used as a map key in both codegen and analysis.

`Display` renders a human-readable mangled name:

- Free and method keys prefix the `.`-joined defining file: `lib.geometry.add`, `lib.geometry.Point.new`.
- Spec keys fold the defining file into the spec name with `_` (a Rocq-legal separator): `lib_checks_S.rec`, `lib_checks_S.Point.init`.

The spec fold is **not injective**: `["lib", "checks"] + S` and `["lib_checks"] + S` both render `lib_checks_S`. Two distinct `FnKey` values can therefore produce the same `Display` string.

**Do not use `Display` output or `to_string()` as a map key.** Use the `FnKey` value itself. The test suite contains an explicit assertion of this property:

```rust
let folded = FnKey::spec_free_folded(&["lib", "checks"], "S", "f");
let bare   = FnKey::spec_free("lib_checks_S", "f");

assert_ne!(folded, bare);          // distinct keys
assert_eq!(folded.to_string(), bare.to_string()); // same rendered name
```

`Display` is used for diagnostic messages and panic descriptions. The codegen-to-analysis frame-size interchange map (`CodegenOutput::frame_sizes`, `estimate_frame_sizes`) is keyed by the structured `FnKey` itself, not by `Display` — keying it by the rendered string would let two keys that render identically collapse into one slot, which is exactly what the structured key exists to prevent. `.wat` output is a separate consumer of a separate rendering, [`name_section_symbol()`](#the-wasm-name-section-namespace) — for the `Free`/`Method` variants the two happen to produce the same string (`name_section_symbol()` delegates to `Display` for exactly those two), but for a spec key they diverge (`Display` folds the file in, `name_section_symbol()` deliberately does not), so `.wat` output should be attributed to `name_section_symbol()`, not to `Display` itself.

## `fold_spec_name`

```rust
pub fn fold_spec_name(module_path: &[String], spec: &str) -> String
```

Folds a defining file's module path into a spec name for display and the Rocq proof grammar. An empty `module_path` returns the spec name unchanged; a non-empty path joins its segments with `_` ahead of the spec name (`["lib", "checks"] + S` → `lib_checks_S`). Underscore rather than `.` keeps the result a legal Rocq identifier.

This function is the single implementation of the spec-name fold. Both `FnKey::Display` and the code generator's `qualified_spec_name` delegate to it, so every phase produces byte-identical spec identifiers.

## The WASM Name-Section Namespace

The WASM name section that code generation, the static-merge linker, and the proof translation all touch has two producers, and this crate defines the grammar that keeps their strings apart.

- **Compiled functions** — `FnKey::name_section_symbol()` — join Inference identifiers with `.`, so a symbol is always drawn from the alphabet `[A-Za-z0-9_.]`:

  ```rust
  assert_eq!(FnKey::free_in(vec![], "add").name_section_symbol(), "add"); // entry file, == Display
  assert_eq!(
      FnKey::free_in(vec!["lib".into(), "arith".into()], "add").name_section_symbol(),
      "lib.arith.add", // non-entry file, unlike Display this is intentional and always written
  );
  ```

  A spec-inner key (`SpecFree`/`SpecMethod`) is the deliberate exception: `name_section_symbol()` leaves it bare (`ex_double`, not `lib.checks.ex_double`), because spec membership already travels as indices in `inference.spec_funcs` and the translator recovers the bare name by stripping the folded spec prefix. Qualifying spec keys would move every `Definition` name a spec function emits.

- **Merged external bodies** — `merged_name::{root, callee, anonymous}` — join with `MERGED_SEPARATOR` (`"::"`), a sequence a compiled symbol can never contain:

  ```rust
  assert_eq!(merged_name::root("mathlib", "double"), "mathlib::double");
  assert_eq!(merged_name::callee("mathlib", "helper"), "mathlib::#helper");
  assert_eq!(merged_name::anonymous("mathlib", 7), "mathlib::#func_7");
  ```

  `callee` and `anonymous` also carry `MERGED_INTERNAL_MARK` (`"#"`) immediately after the module, marking a linked module's own private function as distinct from one of its `root` exports — an export field is always an Inference identifier and so can never itself start with `#`.

The two sets are disjoint by construction — no compiled symbol can contain `:`, and no merged name can omit it — which is what lets the proof translation resolve an `hspecs` obligation's applied symbol by plain string equality over the whole post-merge name section without a source-level function ever being answered for by a linked external's body, or the reverse. `METHOD_SEPARATOR` stays crate-private; nothing outside needs the dot. Of the two merged-name separators, only `MERGED_SEPARATOR` has callers outside this crate — the linker and the translator each use it for the `contains` check that finds a name's merged half; `MERGED_INTERNAL_MARK` has no callers outside this crate — it is `pub(crate)`, not `pub` — and in production code it is applied only inside `merged_name` itself (by `callee` and `anonymous`), the only place that needs to tell a merged root from a merged inner callee; this crate's own test suite reads the raw constant too, but only to assert on that internal behavior. See [`core/wasm-linker`](../wasm-linker/README.md#proof-mode-custom-sections) for how the linker writes `merged_name` strings and enforces that every applied obligation symbol resolves, and [`core/wasm-to-v/ROCQ_CONTRACT.md`](../wasm-to-v/ROCQ_CONTRACT.md#t_app-resolution-discipline) for how the translator reads them back.

## Usage

Add the dependency in `Cargo.toml`:

```toml
inference-fn-key.workspace = true
```

Typical call-graph construction in analysis:

```rust
use inference_fn_key::FnKey;
use std::collections::HashMap;

let mut graph: HashMap<FnKey, Vec<FnKey>> = HashMap::new();

// Key a free function defined in "lib/math.inf".
let caller = FnKey::free_in(vec!["lib".into(), "math".into()], "compute");
let callee = FnKey::free_in(vec![], "add"); // entry file

graph.entry(caller).or_default().push(callee);
```

## Related Resources

- [`core/analysis`](../analysis/README.md) — consumes `FnKey` to key the whole-program call graph (A035, A036)
- [`core/wasm-codegen`](../wasm-codegen/README.md) — uses `FnKey` to assign WASM function indices, emit frame-size maps, and write `name_section_symbol()` into the WASM name section
- [`core/wasm-linker`](../wasm-linker/README.md) — writes `merged_name` strings for a merged external body and checks that every `hspecs` obligation symbol resolves to exactly one function
- [`core/wasm-to-v`](../wasm-to-v/ROCQ_CONTRACT.md) — resolves an obligation's applied symbol against the post-merge name section
