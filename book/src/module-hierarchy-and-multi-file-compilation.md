# Module Hierarchy & Multi-File Compilation

This chapter explains how Inference distributes source code across multiple `.inf` files
and flattens the whole program into a single self-contained WebAssembly module and a
single Rocq `.v` file. It covers the file-as-module model, the two forms of the `use`
directive, the import-closure walk, cross-file type identity, WASM function naming, and
the implications for formal verification.

## Why Multiple Files?

A language that restricts all code to a single file does not scale to real programs. As a
codebase grows, a single-file constraint forces either monolithic structures or custom
tooling to concatenate sources before compilation — neither is acceptable.

At the same time, Inference has a hard constraint inherited from its verification
architecture: the Rocq output must be **one self-contained `.v` file**. The proof
assistant reasons about the whole program simultaneously; splitting the proof into separate
modules would require constructing cross-module proof obligations, a significant increase in
complexity for proof authors. The same argument applies to the WebAssembly side: a
self-contained `.wasm` with no outstanding imports is simpler to verify and deploy than a
bundle of linked modules.

The design goal is therefore: **code organisation without losing whole-program compilation
or verification.** Multiple source files map to one output module.

## File-Based Modules

Each `.inf` file is a module. The module tree mirrors the directory tree under the source
root. The source root is the directory that contains the entry file — the file named on the
`infc` or `infs` command line.

The module's **canonical path** is the list of path segments from the source root to the
file, without the `.inf` extension:

```text
source root: src/

  src/main.inf        →  module path: []          (the entry; empty)
  src/util.inf        →  module path: ["util"]
  src/lib/arith.inf   →  module path: ["lib", "arith"]
  src/lib/geo.inf     →  module path: ["lib", "geo"]
```

The entry file always has the empty path. Its items keep unqualified names throughout
the compilation pipeline, so a single-file program produces byte-identical output to a
multi-file one where the entire code lives in the entry file.

## The Two Forms of `use`

The `use` directive has two syntactically distinct forms. They are easy to confuse but serve
entirely different purposes.

### Path-form `use` — source imports

The path form names a sibling `.inf` file to include in the compilation:

```inference
use util;
use lib::arith;
use lib::geo::{Point};
```

- `use util;` makes the file `<root>/util.inf` part of the compilation. Items in that file
  are accessed as `util::helper()`.
- `use lib::arith;` names `<root>/lib/arith.inf`. Items are accessed as
  `lib::arith::add(1, 2)`.
- `use lib::geo::{Point};` names the same file `<root>/lib/geo.inf` as the brace-free
  form. The braces select which items are accessible without the full path, but the file
  imported is identical: a braced item import and its brace-free equivalent are both resolved
  to the same file and both pull that file into the compilation closure.

`use root;` is a reserved handle that refers to the entry file itself; a literal
`src/root.inf` on disk is shadowed by the reserved name.

Glob imports (`use lib::arith::*;`) are not supported and produce a diagnostic.

### From-form `use` — external WASM imports

The from form binds an `external fn` declaration to a pre-compiled `.wasm` library:

```inference
external fn sort(mut ptr: i32, len: i32);
use { sort } from collections;
```

This form is **not** a source import. It names a logical WASM module identifier, not a
file path. The compiler-time front end (`core/project-model/src/project.rs`) skips it entirely
when walking the import closure. The from form is resolved at link time by
`inference-wasm-linker`.

The distinction is crisp: if the directive contains `from`, it is an external WASM binding;
otherwise it is a source file import. The two forms do not interact, and mixing them in one
file is valid:

```inference
use lib::arith::{add};          // source import: lib/arith.inf
use { sort } from collections;  // external WASM: resolved at link time
```

The from form is documented in detail in
[External Functions and WASM Linking](external-functions-and-wasm-linking.md) and
[The WASM Linker](the-wasm-linker.md).

## Qualified Access with `::`

Items from an imported file are accessed through the `::` path operator. The path is the
module's canonical path with `::` as the separator:

```inference
use util;
use lib::arith;
use lib::geo::{Point};

pub fn run() -> i32 {
    let p: Point = Point::at(8);  // constructor from lib::geo
    return util::helper();        // free function from util
}
```

The `math::arith::add(1, 2)` form works transitively through re-exports (see
[Re-exports](#re-exports) below).

When `use lib::geo::{Point};` brings a name into scope without a path prefix, the type name
is available unqualified, but the calling convention stays the same:

```inference
use lib::geo::{Point};

pub fn run() -> i32 {
    let p: Point = Point::at(8);  // `Point` unqualified; `Point::at` still uses `::`
    return p.dist();
}
```

The fixture in `tests/test_data/codegen/wasm/multi_file_golden/method_mangling/src/`
demonstrates this pattern: `main.inf` imports `{Point}` from `lib::geo` and calls
`Point::at(8)` and `p.dist()`.

## Visibility

Items are **private by default**. The `pub` keyword makes an item accessible from other
files:

```inference
// lib/arith.inf — private fn is invisible outside this file
fn internal_helper(x: i32) -> i32 { return x * 2; }

// pub fn is accessible as lib::arith::add
pub fn add(a: i32, b: i32) -> i32 { return a + b; }

// pub struct is accessible as lib::arith::Point
pub struct Point {
    x: i32;    // fields inherit the struct's visibility; no per-field modifier
    y: i32;
}
```

Visibility rules:

- A file's own private items are visible everywhere within that file.
- Cross-file access requires `pub`.
- Struct fields inherit the struct's visibility. There is no per-field `pub` modifier.
- `external fn` declarations are always file-local; they cannot be re-exported or accessed
  from other files.
- Spec blocks see their own file's private items, but when a spec in file A references a
  type or function from file B, only `pub` items in B are in scope.
- **WASM exports**: only `pub fn`s defined in the entry file are exported from the generated
  WASM module. A `pub fn` in an imported file has intra-project visibility — the function is
  compiled and callable within the program, but it does not appear in the WASM export
  section. This keeps the module's public ABI explicitly controlled.

## Re-exports

A `pub use` directive re-exports a module path, making it accessible through the
re-exporting file's namespace:

```inference
// math.inf
pub use lib::arith;
```

Callers that import `math` can then reach `lib::arith`'s items through the `math` namespace:

```inference
// main.inf
use math;

pub fn run() -> i32 {
    return math::arith::add(1, 2);
}
```

The fixture in `tests/test_data/codegen/wasm/multi_file_golden/re_export_chain/src/`
demonstrates this: `math.inf` re-exports `lib::arith` with `pub use lib::arith;`, and
`main.inf` calls `math::arith::add(1, 2)`.

Re-exports also drive the import closure: if `math.inf` carries `pub use lib::arith;`, the
compiler includes `lib/arith.inf` in the compilation even though `main.inf` names only
`math`. Both `pub use` and plain `use` pull the named file into the closure; visibility
affects name accessibility, not discovery.

Re-export chains can be arbitrarily deep. Circular re-exports between files are permitted
and terminate correctly (see [Import-Closure Walk](#import-closure-walk) below).

## Cross-File Type Identity

Two different files may each define a struct or enum with the same bare name:

```inference
// main.inf
pub struct Pair {
    a: i32;
    b: i32;
}

// lib/shapes.inf
pub struct Pair {
    a: i64;
    b: i32;
}
```

These are **distinct types**. The type checker keys each type by its **canonical type key** —
the `::` path of its defining file prefixed onto the bare name:

```text
Pair  defined in the entry file → canonical key:  "Pair"
Pair  defined in lib/shapes.inf → canonical key:  "lib::shapes::Pair"
```

This key is computed from the type's defining scope (not the call site's scope) and threads
consistently through every phase: the type checker uses it for identity comparisons and
layout lookups; codegen uses the same derivation to key memory layout and frame-slot
allocations. Two files may therefore define identically-named structs with completely
different field layouts, and the compiler keeps them distinct without any source-level
disambiguation.

The fixture in `tests/test_data/codegen/wasm/multi_file_golden/dup_struct/src/` exercises
this: both `main.inf` and `lib/shapes.inf` define `struct Pair`, but with different field
types. The codegen produces correct, separate frame layouts for each, and the type checker
rejects any attempt to use one where the other is expected.

## Import-Closure Walk

The compiler resolves the full set of files to compile through a **breadth-first walk** from
the entry file. This walk is implemented in `core/project-model/src/project.rs` — a leaf
crate (`inference-project-model`) extracted from the orchestration crate so that the batch
compiler and the [language server](the-language-server.md) share one closure walk. The
compiler enters it through `inference::parse_project` (re-exported unchanged), while the
IDE enters through `load_project_resilient`, which reads files through a `FileLoader` seam
so the editor's unsaved buffers take priority over what is on disk.

The algorithm:

1. Start with the entry file (module path `[]`) in the visited set and in the work queue.
2. Parse the front of the queue.
3. Extract all path-form `use` directives (from-form `use` directives are skipped).
4. For each named module path not already in the visited set, compute the corresponding
   filesystem path (`<root>/a/b.inf` for `["a", "b"]`), verify the file exists, add it to
   the visited set, and enqueue it.
5. Repeat until the queue is empty.

Properties of the walk:

- **Import cycles terminate.** The visited set is keyed by canonical module path, so a file
  reached by two different import chains is parsed once. A file that imports itself, or
  two files that import each other, is handled correctly.
- **All paths are src-root-relative.** A `use lib::arith;` directive written inside
  `deep/inner.inf` resolves to `<root>/lib/arith.inf`, not to `<root>/deep/lib/arith.inf`.
  All imports are relative to the one shared source root, not to the importer's location.
- **Unreachable files warn.** After the walk, the compiler scans every `.inf` file under the
  source root and emits a `ProjectWarning::UnreachableFile` for each file not reached by any
  import chain. The build still succeeds; the warning is advisory.

After discovery, the files are lowered into a single `AstArena` in **canonical order**:
the entry file first, then all imported files sorted lexicographically by module path. This
ordering is independent of discovery order (which is BFS and depends on the order `use`
directives appear in source). A project with the same files and the same code produces the
same canonical order regardless of which `use` comes first in any given file.

```text
entry: []
imports (lexicographic):
  ["lib", "arith"]
  ["lib", "geo"]
  ["math"]
  ["util"]
```

## WASM Codegen and Function Naming

All reachable files are flattened into **one WASM module**. The `FnKey` abstraction
(`core/fn-key/src/lib.rs`) provides a structured identity for every function across the
whole program, used by both codegen (to assign WASM function indices) and analysis (to build
the call graph). The four variants of `FnKey` are:

| Variant | Identifies |
|---------|------------|
| `Free` | A top-level free function with its defining file's module path |
| `Method` | A struct method or associated function with its struct's defining file |
| `SpecFree` | A spec-inner free function |
| `SpecMethod` | A spec-inner method |

Every variant carries the `module_path` of the **defining** file. This is what keeps two
`fn add` functions in two different files as distinct internal keys.

The function names written into the WASM name section (and visible in `.wat` output) use the
bare function name (or `StructName.method_name` for methods), not a file-prefixed form.
When two functions share a bare name across files, the WAT renderer deduplicates them with a
numbered suffix like `#func2 there_b`. The `FnKey` dot-qualified string (`lib.arith.add`) is
an internal key used for function-index lookup — it does not appear verbatim in the binary's
name section.

WASM exports are controlled by the entry file and by visibility. The rule:

- Only a `pub fn` defined **in the entry file** becomes a WASM export.
- A `pub fn` in an imported file is compiled and callable within the program, but is **not**
  exported from the WASM module.
- Methods and spec-inner functions are never exported.

This makes the module's external ABI explicit: whatever the entry file declares `pub` is the
interface; imported library code is internal.

```wat
;; root_only_export example: lib::arith::add is compiled but not exported
(module $output
  (export "run" (func $run))   ;; only the entry-file pub fn is exported
  (func $run ...)
  (func $add ...)              ;; add is compiled but unexported
)
```

## Specs in Multi-File Proofs

Spec blocks carry their own names in the Rocq output. When a spec is defined in a non-entry
file, its name is file-qualified by joining the module path with underscores, then
prepending to the bare spec name. This is done by the `fold_spec_name` function
(`core/fn-key/src/lib.rs`):

```text
module path: ["lib", "checks"]
spec name: "LibSpec"
folded name: "lib_checks_LibSpec"
```

The entry file's specs keep their bare names (the empty module path folds to nothing).

The fold uses underscores rather than dots because the result must be a legal Rocq
identifier. The fold is intentionally non-injective: the paths `["lib", "checks"]` and
`["lib_checks"]` both fold to the same prefix `lib_checks`. The `FnKey` type keeps the
module path and the bare spec name structurally separate (not pre-folded) to preserve
injectivity as an *internal key*; the fold is applied only at rendering time.

The fixture in `tests/test_data/codegen/wasm/multi_file_golden/proof_specs/src/`
illustrates this:

```inference
// lib/checks.inf
fn lib_value() -> i32 {
    return 2;
}

spec LibSpec {
    fn obligation() forall {
        assert(lib_value() == 2);
    }
}
```

```inference
// main.inf
use lib::checks;

fn entry_value() -> i32 {
    return 1;
}

spec EntrySpec {
    fn obligation() forall {
        assert(entry_value() == 1);
    }
}
```

The computing helpers sit at file scope rather than inside the `spec` block: a
specification function must state a property, and one whose body only computes
yields an obligation any proof discharges without reading the program, which
code generation rejects as `P010`. A specification function still applies a
file-scope function as a `T_app`, so the helper loses nothing by moving out.

`EntrySpec` (entry file, empty path) keeps its name in the Rocq output; `LibSpec` (defined
in `lib/checks.inf`) is rendered as `lib_checks_LibSpec`.

A file-qualified spec name must remain a valid Rocq identifier after folding. In particular,
a file stem or spec name that contains a `__` run, or begins or ends with `_`, can produce a
name that conflicts with Rocq's reserved `<module>__<spec>` separator in the emitted proof
grammar. Codegen detects this and reports a `CodegenError::SpecNameReservesSeparator` with a
rename suggestion before emitting any output. This restriction applies to spec names in proof
mode; it is not a restriction on file names used by import directives.

## Current Limitations

- **No import aliasing.** `use lib::arith as ar;` is not supported. The module must be
  accessed through its canonical path.
- **No per-field visibility.** Struct fields inherit the struct's `pub` or private
  visibility. There is no `pub` modifier on individual fields.
- **Circular const/type-alias value initialization is an error.** Import cycles between
  files are legal and handled correctly, but circular *value* initialization — a constant
  whose value depends (through a chain of constants or type aliases) on itself — is rejected
  with a `CircularDefinition` error.
- **Bare `infc src/main.inf` treats `src/` as the source root.** In this mode, sibling
  `.inf` files under `src/` that are not reachable from `main.inf` produce unreachable
  warnings. The `infs` project toolchain (see
  [Projects and the infs Toolchain](projects-and-the-infs-toolchain.md)) handles
  project-level discovery and build coordination.

## Comparison with Other Languages

| Property | Rust | Zig | Inference |
|---|---|---|---|
| Module granularity | Declared with `mod`; file maps to module only when named by `mod file;` | Each file is a struct; imported with `@import("path.zig")` | Each `.inf` file is a module; imported with `use path;` |
| Import cycles | Rejected at compile time | Allowed | Allowed; terminated via visited set |
| Canonical order | Declaration order within `mod` | Import order | Entry-first, then lexicographic by module path |
| Output granularity | One crate = one `.rlib` or binary | Configurable | One project = one `.wasm` + one `.v` |
| Re-exports | `pub use module::item` | Return the imported struct | `pub use module::path` |
| Visibility | `pub`, `pub(crate)`, `pub(super)`, private | `pub` or private | `pub` or private; no `pub(super)` |

The Inference model is closer to Zig than to Rust: files are modules implicitly, without
explicit `mod` declarations, and import cycles are allowed. Inference differs from both in
that all files flatten into one compilation unit; there is no concept of separate crates or
build artifacts for sub-libraries within a project.

## Formal Implications

The whole-program flattening has a direct consequence for formal verification: **every
function the proof references is defined in the same `.v` file.** There are no cross-module
lemma imports, no proof namespace hierarchies, and no linker-time proof composition. A proof
obligation on `main::run` can refer directly to `lib::arith::add`'s definition without any
module-qualification syntax in the Rocq source.

The canonical per-file type identity ensures soundness across the flattening: two
same-named structs from two different files carry distinct canonical keys through the
type-checker, layout computation, and codegen. A Rocq proof that reasons about one struct
cannot inadvertently apply to the other, because the generated `.v` definitions carry
different names derived from the canonical keys.

## Related Resources

- `core/project-model/src/project.rs` — `parse_project`, `load_project_resilient`,
  the import-closure walk, `collect_unreachable_warnings`
- `core/fn-key/src/lib.rs` — `FnKey` variants, `fold_spec_name`, `qualify_dotted`
- `core/type-checker/src/symbol_table.rs` — `canonical_key_for_scope`,
  `file_module_path_of_scope`
- `core/ast/src/nodes.rs` — `Visibility`, `UseDirective`
- `tests/test_data/codegen/wasm/multi_file_golden/` — golden fixture files for all
  multi-file scenarios: `two_file`, `item_import`, `method_mangling`, `re_export_chain`,
  `dup_struct`, `cross_file_struct`, `proof_specs`, `root_only_export`
- [External Functions and WASM Linking](external-functions-and-wasm-linking.md) — the from
  form of `use`, external WASM binding, and the link step
- [The WASM Linker](the-wasm-linker.md) — merge algorithm for pre-compiled `.wasm`
  dependencies
- [Projects and the infs Toolchain](projects-and-the-infs-toolchain.md) — project-aware
  build and the `infs` CLI
- [Compilation Targets](compilation_targets.md) — compile vs. proof modes and the supported
  backends
