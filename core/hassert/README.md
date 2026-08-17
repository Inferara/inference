# inference-hassert

The `hassert` verification-obligation IR and its `inference.hspecs` custom-section codec. Provides the single shared data model and wire format that the Inference proof pipeline uses to move per-specification proof obligations from code generation, through the static linker, to the Rocq translator.

## Why This Crate Exists

A proof-mode Inference build turns each specification free function into one logical obligation: a value of the `hassert` assertion type defined by wasm-verifier (a private Inferara repository; the vendored stub in `core/wasm-to-v/rocq-stub/` mirrors, in-repo, the subset of that interface the Rocq emitter can print), tagged with the function's quantifier kind — `Forall` for a universal payload, `Exists`/`Unique` for a reachability payload carrying its entry arity and source-visible frame slots. Three phases touch these obligations:

- **`wasm-codegen`** *produces* them from the typed AST and embeds them in the `inference.hspecs` WASM custom section.
- **`wasm-linker`** *carries* the section verbatim through a static merge.
- **`wasm-to-v`** *consumes* the section, resolving each obligation into a kind-selected proof goal: `ValidSpec` for `Forall` entries, `ValidExistsSpec`/`ValidUniqueSpec` over `reachability_spec` records for the others.

Putting the IR and its wire format in any one of those crates would force the other two to depend on it — the same pressure that made the linker keep a hand-copied duplicate of the `inference.spec_funcs` codec. A dependency-light leaf crate below all three gives every phase one source of truth for both the data model and the bytes on disk. This mirrors the layering rationale of [`inference-fn-key`](../fn-key/README.md).

## The IR

```rust
pub enum HTerm {
    Const(HConst), LVar(u32), Local(u32),
    App(HFnRef, Vec<HTerm>),
    Binop(HNumType, HBinop, Box<HTerm>, Box<HTerm>),
    Relop(HNumType, HRelop, Box<HTerm>, Box<HTerm>),
}

pub enum HAssert {
    True, False,
    Not(Box<HAssert>), And(Box<HAssert>, Box<HAssert>),
    Imp(Box<HAssert>, Box<HAssert>), Or(Box<HAssert>, Box<HAssert>),
    Ex(Box<HAssert>),
    TermEq(HTerm, HTerm), HasType(HTerm, HNumType), Defined(HTerm),
    AppOk(HFnRef, Vec<HTerm>),
    All(Box<HAssert>),
}
```

Each variant mirrors a wasm-verifier inductive (`HA_not`, `HA_and`, `Himpl`, `Hor`, `HA_ex`, `term_eq`, `HA_has_type`, `HA_defined`, `HA_app_ok`, `Hall`) — the variant doc comments name the counterpart.

`All` sits last against its meaning: the wire tags follow declaration order and are part of the format, so a new variant is appended rather than filed beside the relative it reads like.

Each obligation entry carries its quantifier kind alongside the tree:

```rust
pub enum SpecKind { Forall, Exists(ReachMeta), Unique(ReachMeta) }
pub struct ReachMeta { pub entry_arity: u32, pub visible_locs: Vec<u32> }
pub struct HSpecEntry { pub fn_symbol: HFnRef, pub hassert: HAssert, pub kind: SpecKind }
```

The metadata rides only on the reachability kinds — a `Forall` entry *cannot* carry stray reachability fields, keeping the ill-formed combination unrepresentable. `entry_arity` is the source parameter count of the retained function (the hidden choice suffix begins after it) and `visible_locs` the producer-declared source-visible frame slots `ValidUniqueSpec` compares exits through; both are carried on the wire rather than re-derived downstream, and `wasm-to-v` cross-checks them against the located function.

### Deliberate deviations

The IR omits everything an Inference specification can never contain, so an ill-formed obligation is *unrepresentable* rather than merely rejected:

- **No floating point.** `HNumType` is `I32`/`I64` only; the language has no float types.
- **No `T_global`.** Specifications cannot reference globals.
- **No heap fragment.** `HA_emp`/`HA_star`/`HA_iter`/`HA_pto`/`HA_size` are absent; memory constructs are not translatable.
- **No general `HA_pred`.** `TermEq` is the only predicate form, enforcing wasm-verifier's `pred_eq`/2 discipline by construction.

**Implication, disjunction and universal quantification are explicit nodes** (`Imp`, `Or`, `All`), not their classical De Morgan encodings. wasm-verifier's `Himpl`/`Hor`/`Hall` are definitionally-transparent `Definition`s, so a downstream printer can render these nodes by name without ever pattern-matching an encoding.

`All` earns its place twice over. Beyond legibility, the downstream `ValidSpec` judgment quantifies the payload's free variables universally from outside, so an inner universal encoded as anything but a binder of its own would be bound out there instead — turning `∃x. ∀y. P` into `∀y. ∃x. P` without a trace.

### Symbolic function references

`HFnRef(pub String)` stores a WASM name-section symbol (what code generation writes via `FnKey::Display`, e.g. `is_prime`, `lib.arith.add`, `Point.new`). The crate treats the string as opaque and non-empty; it never resolves it. Because the static linker deletes imports and shifts every function index, an index-based reference would need remapping at link time, whereas a symbolic one is carried through the merge untouched and resolved to a `mod_funcs` index only by `wasm-to-v`, which alone knows the emitted module's final function layout.

### Smart constructors

Constructors on `HAssert` apply the `True`-simplifications that keep a translated obligation free of trivial noise:

| Constructor | Simplification |
|-------------|----------------|
| `and(a, b)` | `⊤ ∧ x = x`, `x ∧ ⊤ = x` |
| `imp(p, q)` | `⊤ → q = q`, `p → ⊤ = ⊤` |
| `or(a, b)`  | `⊤ ∨ x = ⊤`, `x ∨ ⊤ = ⊤` (`⊤` absorbing, the dual of its being the identity for `and`) |
| `ex(body)`  | `∃x. ⊤ = ⊤` |
| `all(body)` | `∀x. ⊤ = ⊤` (the dual of `ex`, sound because the domain is never empty) |
| `nz(t)`     | `¬(t = 0)` |
| `eqz(t)`    | `t = 0` |

`nz`/`eqz` compare against an i32 zero: every truthiness position (a relop result, a bool, a `&&`/`||` result, a bool-returning call) is i32 in WASM.

## The Codec

```rust
pub const HSPECS_SECTION_NAME: &str = "inference.hspecs";
pub const HSPECS_SECTION_VERSION: u32 = 2;

pub fn encode(map: &HSpecMap) -> Vec<u8>;
pub fn decode(data: &[u8]) -> Result<HSpecMap, DecodeError>;
pub fn validate(map: &HSpecMap) -> Result<(), PayloadError>;
```

`encode` produces the section *payload* (the enclosing custom-section framing is added by the emitter). The encoding is **canonical**: the symbol table and spec list are sorted, so equal maps encode to identical bytes regardless of insertion order, and `decode` rejects any non-canonical ordering — the two are mutual inverses on well-formed input.

The wire format is LEB128 throughout, version-led, with a shared function-symbol table followed by the specs; each entry carries a kind byte (`0x00` Forall, `0x01` Exists, `0x02` Unique) followed, for the reachability kinds only, by the `ReachMeta` fields, then the tree. Version 1 (kindless entries) is superseded and rejected on decode — the section is proof-mode intermediate data, so recompilation, not migration, is the compatibility story. The full tag table lives in the `codec` module documentation.

### The `encode` contract

`encode` is infallible by signature but carries a contract: the map must satisfy `validate` — every spec name and function symbol non-empty and at most `MAX_NAME_LEN` bytes, every obligation tree at most `MAX_TREE_DEPTH` deep, every reachability entry's `visible_locs` strictly ascending with count and values within `MAX_VISIBLE_LOCS`. This is exactly the input contract `decode` enforces. An unvalidated map could otherwise serialize into a payload the codec's own hardened decoder rejects (a corrupt artifact), or overflow the stack while encoding a pathologically deep tree, so `encode` calls `validate` first and **panics** on a violation. A documented contract panic is strictly safer than either alternative.

Callers therefore pass one of:

- **Decode output.** Anything `decode` accepts satisfies `validate` (pinned by the crate test `every_decoded_map_satisfies_validate`), so re-encoding a decoded map — as the linker's merge does — can never panic.
- **Data run through `validate` first**, lifting the returned `PayloadError` into the caller's own diagnostic. Code generation, the one producer of fresh maps, does this: it gates on `validate` before emitting the section and turns a violation into a clean `CodegenError` (`HspecTreeTooDeep` / `HspecNameTooLong`) naming the offending spec and identifier, rather than reaching the panic.

### Hardening

`decode` never panics, never allocates unboundedly, and never overflows the stack on adversarial input. It rejects: an unrecognized version, truncation, malformed or over-`u32` LEB128, over-advertised counts (checked against the remaining bytes before allocation), over-long or empty or non-UTF-8 names, a non-ascending symbol table or spec list (which also rejects duplicates), out-of-range symbol indices, an out-of-range kind tag, non-ascending or over-cap `visible_locs` (count and value both bounded by `MAX_VISIBLE_LOCS`), unknown tags, out-of-range constants, trailing bytes, and — via a depth counter matching `wasm-to-v`'s `MAX_EXPRESSION_DEPTH` — trees nested past `MAX_TREE_DEPTH`. That same depth cap bounds `encode` and the derived `Drop`: no value that round-trips through the codec is deeper than the cap.

## Usage

```toml
inference-hassert.workspace = true
```

```rust
use inference_hassert::{HAssert, HFnRef, HSpecEntry, HSpecMap, HTerm, SpecKind, encode, decode};

let obligation = HAssert::imp(
    HAssert::nz(HTerm::Local(0)),
    HAssert::eqz(HTerm::App(HFnRef("is_prime".into()), vec![HTerm::Local(0)])),
);

let mut map = HSpecMap::default();
map.insert(
    "prime_properties".into(),
    vec![HSpecEntry::new(HFnRef("prime_spec".into()), obligation, SpecKind::Forall)],
);

let payload = encode(&map);
assert_eq!(decode(&payload).unwrap(), map);
```

## Related Resources

- [`core/fn-key`](../fn-key/README.md) — the sibling leaf crate providing canonical function identity; producers derive `HFnRef` symbols from `FnKey::Display`.
- `core/wasm-codegen` — produces obligations and emits the `inference.hspecs` section.
- `core/wasm-to-v` — consumes the section and resolves each obligation to its kind-selected Rocq goal (`ValidSpec`, or `ValidExistsSpec`/`ValidUniqueSpec` over `reachability_spec` records).
