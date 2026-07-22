# inference-hassert

The `hassert` verification-obligation IR and its `inference.hspecs` custom-section codec. Provides the single shared data model and wire format that the Inference proof pipeline uses to move per-specification proof obligations from code generation, through the static linker, to the Rocq translator.

## Why This Crate Exists

A proof-mode Inference build turns each `forall`-quantified specification function into one logical obligation: a value of the wasm-verifier `hassert` assertion type (`theories/Assertions.v`). Three phases touch these obligations:

- **`wasm-codegen`** *produces* them from the typed AST and embeds them in the `inference.hspecs` WASM custom section.
- **`wasm-linker`** *carries* the section verbatim through a static merge.
- **`wasm-to-v`** *consumes* the section, resolving each obligation into a `ValidSpec` proof goal.

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
}
```

Each variant mirrors a wasm-verifier inductive (`HA_not`, `HA_and`, `Himpl`, `Hor`, `HA_ex`, `term_eq`, `HA_has_type`, `HA_defined`, `HA_app_ok`) — the variant doc comments name the counterpart.

### Deliberate deviations

The IR omits everything an Inference specification can never contain, so an ill-formed obligation is *unrepresentable* rather than merely rejected:

- **No floating point.** `HNumType` is `I32`/`I64` only; the language has no float types.
- **No `T_global`.** Specifications cannot reference globals.
- **No heap fragment.** `HA_emp`/`HA_star`/`HA_iter`/`HA_pto`/`HA_size` are absent; memory constructs are not translatable.
- **No general `HA_pred`.** `TermEq` is the only predicate form, enforcing wasm-verifier's `pred_eq`/2 discipline by construction.

**Implication and disjunction are explicit nodes** (`Imp`, `Or`), not their classical De Morgan encodings. wasm-verifier's `Himpl`/`Hor` are definitionally-transparent `Definition`s, so a downstream printer can render these nodes as `Himpl`/`Hor` without ever pattern-matching an encoding.

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
| `nz(t)`     | `¬(t = 0)` |
| `eqz(t)`    | `t = 0` |

`nz`/`eqz` compare against an i32 zero: every truthiness position (a relop result, a bool, a `&&`/`||` result, a bool-returning call) is i32 in WASM.

## The Codec

```rust
pub const HSPECS_SECTION_NAME: &str = "inference.hspecs";
pub const HSPECS_SECTION_VERSION: u32 = 1;

pub fn encode(map: &HSpecMap) -> Vec<u8>;
pub fn decode(data: &[u8]) -> Result<HSpecMap, DecodeError>;
```

`encode` produces the section *payload* (the enclosing custom-section framing is added by the emitter). The encoding is **canonical**: the symbol table and spec list are sorted, so equal maps encode to identical bytes regardless of insertion order, and `decode` rejects any non-canonical ordering — the two are mutual inverses on well-formed input.

The wire format is LEB128 throughout, version-led, with a shared function-symbol table followed by the specs. The full tag table lives in the `codec` module documentation.

### Hardening

`decode` never panics, never allocates unboundedly, and never overflows the stack on adversarial input. It rejects: an unrecognized version, truncation, malformed or over-`u32` LEB128, over-advertised counts (checked against the remaining bytes before allocation), over-long or empty or non-UTF-8 names, a non-ascending symbol table or spec list (which also rejects duplicates), out-of-range symbol indices, unknown tags, out-of-range constants, trailing bytes, and — via a depth counter matching `wasm-to-v`'s `MAX_EXPRESSION_DEPTH` — trees nested past `MAX_TREE_DEPTH`. That same depth cap bounds `encode` and the derived `Drop`: no value that round-trips through the codec is deeper than the cap.

## Usage

```toml
inference-hassert.workspace = true
```

```rust
use inference_hassert::{HAssert, HFnRef, HSpecEntry, HSpecMap, HTerm, encode, decode};

let obligation = HAssert::imp(
    HAssert::nz(HTerm::Local(0)),
    HAssert::eqz(HTerm::App(HFnRef("is_prime".into()), vec![HTerm::Local(0)])),
);

let mut map = HSpecMap::default();
map.insert("prime_properties".into(), vec![HSpecEntry::new(HFnRef("prime_spec".into()), obligation)]);

let payload = encode(&map);
assert_eq!(decode(&payload).unwrap(), map);
```

## Related Resources

- [`core/fn-key`](../fn-key/README.md) — the sibling leaf crate providing canonical function identity; producers derive `HFnRef` symbols from `FnKey::Display`.
- `core/wasm-codegen` — produces obligations and emits the `inference.hspecs` section.
- `core/wasm-to-v` — consumes the section and resolves each obligation to a Rocq `ValidSpec` goal.
