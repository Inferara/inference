# `wasmlib` — a foreign-toolchain external for the linker

`rustlib.wasm` is the unmodified output of `cargo build --release --target
wasm32-unknown-unknown` over `rustlib-src/`. It is the fixture behind the
acceptance criterion for issue #363: a `.wasm` that a *different* compiler
emitted, with no knowledge of Inference, folds into an Inference program by
static merge, executes, and translates to Rocq.

The `.wasm` is committed rather than built by the suite so that CI needs no
`wasm32` target installed. `rustlib-src/` is committed beside it so the bytes can
be regenerated and diffed rather than taken on trust.

This directory doubles as an `infc` search directory — `-L tests/test_data/wasmlib`
resolves the logical module `rustlib` to `rustlib.wasm` — so it can be used by
hand exactly as the tests use it:

```
infc -L tests/test_data/wasmlib --memory-pages 16 main.inf -v
```

`-L` resolution probes for a file by name and never scans the directory, so
`rustlib-src/` beside the artifact is invisible to it.

It is also a deliberate *sibling* of `test_data/codegen/` rather than something
under it. The golden gates walk that subtree and enroll every `.wasm` they find,
and this one is meant to satisfy none of them — it is linker input, not a golden.

## The committed artifact

| | |
|---|---|
| Size | 492 bytes |
| SHA-256 | `e233c35c2b340f55ce1d837990b403a7ea2cc61db1ecf25c8d3c0d2910c0227a` |
| Built with | `rustc 1.97.1 (8bab26f4f 2026-07-14)` / `cargo 1.97.1 (c980f4866 2026-06-30)` |

Sections, in order, with their total encoded size: type (9), function (6),
memory (5), global (11), export (40), code (113), then the three custom sections
`name` (70), `producers` (79) and `target_features` (151). Exports are
`clamp_add`, `mulhi`, `sum_n` and `memory`; the memory is sixteen pages and the
single global is lld's `__stack_pointer`.

## Why it is inside the merge envelope

Everything the linker rejects an external for is absent, and none of it was
arranged for the linker's benefit:

- **No data segment.** `#![no_std]` is the whole reason. A `std` binary carries
  segments for panic strings and formatting tables, and a data segment is a
  Tier-C rejection because an active one writes linear memory at instantiation
  no matter what the merged code does.
- **No element segment and no table.** Again `#![no_std]`: `std` puts an
  `(table 1 1 funcref)` into every artifact. Neither export uses indirect
  dispatch, so nothing here names the table space either. A table is easier to
  reintroduce than it looks — giving the crate two exports that compile to
  identical bodies makes rustc alias them through one, and a table appears — so
  check for one after adding an export.
- **No imports.** Nothing to merge them onto if there were.
- **The `__stack_pointer` global is inert.** Neither exported function has a
  stack frame, so no body reads or writes it and the merge drops it. Globals are
  classified on *use*, not on declaration, precisely so stock lld output links.

The artifact's `target_features` section advertises eight — `bulk-memory`,
`bulk-memory-opt`, `call-indirect-overlong`, `multivalue`, `mutable-globals`,
`nontrapping-fptoint`, `reference-types`, `sign-ext` — and the code section uses
none of them. The envelope is decided by the instructions present, not by what
the target claims. That distinction is invisible until a crate grows a
`copy_from_slice` and the section stops being a red herring.

The exports cover both admitted tiers, and between them the operator families the
envelope admits:

- `clamp_add` touches no memory at all — **Tier A**. Its closure contributes no
  memory to the merge, so a memoryless main stays memoryless.
- `mulhi` is also **Tier A**, and is the one carrying the width-changing
  conversions. See below for why it takes the shape it does.
- `sum_n` reads only through the pointer its caller supplies — **Tier B**. The
  merged module takes the sixteen-page memory, and the link raises
  `LinkWarning::TierBInMultiPageMemory` naming that page count, because the merge
  proves every address derives from a parameter and not that it stays inside the
  buffer the parameter points into.

The distinct operators each one reaches the linker with:

| export | operators |
|---|---|
| `clamp_add` | `local.get` `local.tee` `i32.const` `i32.add` `i32.shr_s` `i32.xor` `i32.lt_s` `select` |
| `mulhi` | `local.get` `i64.const` `i64.extend_i32_s` `i64.mul` `i64.shr_u` `i32.wrap_i64` |
| `sum_n` | `local.get` `local.set` `i32.const` `i32.add` `i32.gt_s` `i32.load` `select` `block` `loop` `br` `br_if` `return` |

### Writing in 64 bits does not get you 64-bit operators

`clamp_add` is written as a 64-bit widening add followed by a clamp, and LLVM
recognises that as a saturating add and emits it branchlessly in 32 bits. Its
`i64` intermediate leaves no trace in the artifact. `sum_n` is likewise not what
its source says: the scaled index becomes a pointer bump, a `loop` / `block` /
`br_if` / `br` around an `i32.load` off a running pointer, which is the
loop-carried form the provenance analysis has to follow to admit it.

That is why `mulhi` is here. An optimizer narrows any intermediate whose result
it can reach more cheaply, so the width conversions survive only where 64 bits
are load-bearing — and the high half of a 32x32 product is the smallest natural
case, since no 32-bit-only lowering computes it. It is the artifact's only source
of `i64.extend_i32_s` and `i32.wrap_i64`, which are also the only reason the
`BI_cvtop` constructors reach `coqc` from a real artifact rather than from a
hand-assembled module.

`sum_n` also opens with a `select` of its own, clamping `n` to `n > 0 ? n : 0` —
so the artifact contains **two**, and a claim about the clamp's branchless
lowering has to name the function it is about or the loop's guard satisfies it
alone.

### Optimization is load-bearing

Rebuilding this crate at each `opt-level` and linking the result gives:

| `opt-level` | links |
|---|---|
| `0` | **no** |
| `1`, `2`, `3`, `"s"`, `"z"` | yes |

At `0` every function gets a real stack frame, so `clamp_add` — which touches no
memory at all once optimized — opens by computing `__stack_pointer - 16` and
spilling through it. That address derives from a global rather than from a
parameter, which is a Tier-C rejection, and it is the *reason* an unoptimized
build is out: the frame, not the arithmetic. `clamp_add` also keeps its own
`i64.extend_i32_s` pair there, which is what makes it a poor witness for the
conversions despite reading like one in source — the build that has them is the
build that does not link.

So the envelope covers optimized foreign artifacts, which is what `--release`
delivers, and a debug build of the same crate is outside it. Worth stating
plainly, because "my crate does not link" will usually have this answer.

## Regenerating

```
rustup target add wasm32-unknown-unknown
cd tests/test_data/wasmlib/rustlib-src
cargo build --release --target wasm32-unknown-unknown
cmp target/wasm32-unknown-unknown/release/rustlib.wasm ../rustlib.wasm
```

`rustlib-src/` is its own workspace root, which is load-bearing twice over: a
package nested inside the `tests` workspace member cannot be reached by the root
manifest's `workspace.exclude`, so without the empty `[workspace]` table `cargo`
refuses to run here at all; and cargo ignores a non-root package's `[profile]`
table, so the `panic`, `opt-level` and `lto` settings that shape these bytes
would be silently dropped.

The crate must stay dependency-free. `Cargo.lock` is gitignored repo-wide, so a
dependency would make the recipe above unreproducible; it would also likely drag
in the data segments that put an artifact outside the envelope in the first
place.

The `cmp` reproduces byte-for-byte only under the recorded toolchain. A different
`rustc` may select different instructions or stamp a different `producers`
section, and that is not by itself a problem — nothing asserts these bytes. Which
half of the file differs matters, though, because 300 of the 492 bytes are custom
sections:

- `name`, `producers` and `target_features` churn on any toolchain bump, and a
  diff confined to them means codegen was unchanged.
- A difference in the type, function, memory, global, export or **code** section
  is a change in what the fixture tests. Read it before committing it.

What the suite asserts is the artifact's *shape* — no data segment, no element
segment, no table, a `producers` section naming `rustc` — the *behaviour* of the
merged bodies, and the Rocq instruction forms they translate to. So a regenerated
artifact that still fits the envelope and still lowers the same way keeps every
test green; one that leaves the envelope fails naming the construct that pushed
it out; and one that merely lowers differently fails in
`tests/src/rocq_typecheck.rs` naming the form it stopped emitting, which is a
decision to record rather than a break to patch around.

To adopt a regenerated artifact, copy it over `rustlib.wasm` and update the size,
hash and toolchain row above.

## What consumes it

- `tests/src/codegen/wasm/extern_link_toolchain.rs` — links it against
  `infc`-produced mains and executes the merged bodies under `wasmtime`.
- `tests/src/rocq_typecheck.rs` — merges it into
  `tests/test_data/inf/spec_linked_toolchain.inf` in proof mode and compiles the
  resulting `.v` with `coqc`.
