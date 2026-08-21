# Specification Obligations

This document describes what a `spec` function becomes in proof mode: how aggregate values,
element access and quantifier nesting are encoded as a `hassert` proof obligation, which
slot each value gets, and which properties are provable as a result. It is written for
someone reading an emitted `.v` file — usually because a proof did not close.

## Prerequisites

Readers should be familiar with:

- Inference `spec` blocks and the non-deterministic constructs `forall`, `exists`,
  `assume`, `unique` and uzumaki (`@`) — see
  [Inference Language Specification](https://github.com/Inferara/inference-language-spec)
- The proof-mode pipeline and the shape of an emitted `.v` file, described in
  [`core/wasm-to-v/ROCQ_CONTRACT.md`](../../wasm-to-v/ROCQ_CONTRACT.md)
- The overall compilation pipeline described in `core/wasm-codegen/README.md`

## The Two Kinds of Obligation

A `forall`-quantified (or plain) spec function states a property of *values*, and becomes a
`ValidSpec` payload: the downstream judgment quantifies every slot the payload reads, over
valuations it constrains in no way.

An `exists`- or `unique`-quantified spec function states that some run of *its own compiled
body* exists (or is unique), and becomes a reachability payload: the body is compiled to
real WebAssembly with one hidden trailing choice parameter per scalar `@`, and the judgment
runs it.

That difference decides almost everything below, and one sentence carries it:

> A `forall` specification is a claim about values, so an aggregate can become as many
> logical variables as it has scalar leaves; an `exists`/`unique` specification is a claim
> about one actual run of its own compiled body, where an aggregate is a single pointer into
> memory and each choice arrives as one scalar parameter — so there, aggregates must still
> be taken apart by hand.

## What a Quantified Variable States

A `ValidSpec` payload is evaluated over valuations the judgment constrains in no way, so
every universally quantified variable states its own hypothesis and the claim sits under it
as an antecedent. That hypothesis has two halves.

The **class** the readouts ride in — `HA_has_type (T_local i) T_i32` or `T_i64`. Only `i64`
and `u64` ride 64 bits; `bool`, every enum, and every sub-word integer ride `i32`.

The **value domain** the declaration admits, where that says more than the class does. A
declared `u8` is not every `i32`: it is `0..255`, and left unsaid, a property true of every
`u8` is unprovable at `256`, while its existential mirror — a witness the program can never
draw — discharges while saying nothing about the program. The domain is exactly the set code
generation's normalization of a draw produces, so the obligation and the compiled body range
over the same values.

| declared type | normalization in compiled code | value set | bound emitted over `x` |
| --- | --- | --- | --- |
| `u8` | `and 0xFF` | `0..255` | `x <u 256` |
| `u16` | `and 0xFFFF` | `0..65535` | `x <u 65536` |
| `i8` | `shl 24; shr_s 24` | `-128..127` | `-128 <=s x` **and** `x <s 128` |
| `i16` | `shl 16; shr_s 16` | `-32768..32767` | `-32768 <=s x` **and** `x <s 32768` |
| `bool` | `and 1` (a draw), `!= 0` (an entry parameter) | `{0, 1}` | `x <u 2` |
| `enum` with N ≥ 1 variants | `rem_u N` | `0..N-1` | `x <u N` |
| `enum` with no variants | — | ∅ | **P015** |
| `i32`, `u32`, `i64`, `u64` | none | the whole class | *none* |

Three properties of that table are load-bearing rather than cosmetic, and each is dictated by
what the downstream narrow-idiom lemmas take as their hypothesis:

- **Signedness follows the normalization, not the source spelling.** The widths whose
  normalization zero-extends — `u8`, `u16`, `bool`, an enum tag — are bounded *unsigned*; the
  widths whose normalization sign-extends, `i8` and `i16`, take a *signed* pair. A signed
  guard against an unsigned bound, or the reverse, is an obligation no lemma consumes.
- **The upper bound is strict, at the domain's exclusive top** — `<u 256`, `<s 128` — which
  is also the existing convention for a symbolic index's `i <u N`.
- **A signed width states both bounds.** There is no range predicate in the assertion
  language, so a one-sided signed bound characterizes nothing: every value below the lower
  end would still satisfy it.

The two halves ride as **one** hypothesis per introduction — `HA_and (HA_has_type x T_i32)
<bound>` — rather than as two adjacent conjuncts. That keeps an introduction's cost at one
level of the antecedent's conjunction spine whatever its declared type, and it is the shape
the hand-written downstream examples are written against.

Polarity belongs to the quantifier, not to the bound:

- **universally**, the hypothesis is an `Himpl` antecedent — it narrows what the claim has to
  cover;
- **existentially**, the bound is an `HA_and` conjunct *inside* the binder — it constrains the
  witness. An `Himpl` under an `HA_ex` would let a proof pick an out-of-domain witness, refute
  the antecedent, and discharge the obligation without ever meeting the payload.

A bound over a variable the claim never reads is **dropped**, like every other binder
definition nothing reads. A `let a: u8 = @;` inside an `exists` with nothing asserting
anything about `a` must stay the vacuous `HA_true` that **P010** refuses; conjoining a range
unconditionally would turn it into a trivially true claim that no longer looks vacuous.

A specification function is compiled in proof mode but never exported, so its narrow
parameters receive neither ABI normalization nor an entry tag guard. Their hypothesis is the
only place the declaration's meaning is written down, and it states the same set.

## Aggregates Are Their Scalar Leaves

There is no aggregate *term*. An aggregate — a compound `@`, a compound parameter, an array
or struct literal, or a copy of one — is translated to a shape-preserving tree whose leaves
are ordinary scalar terms. A field access or a constant index selects a child of that tree
at translation time and never appears in the obligation; only the leaves do.

### The Supported Shapes

| Shape | Supported |
| --- | --- |
| `[i32; 3]`, `[[i32; 2]; 2]`, arrays of scalars at any rank | yes |
| a struct whose fields are scalars, or *one-dimensional* scalar arrays | yes |
| `[Point; 2]` — an array of structs | no |
| a struct with a struct field, or a multidimensional array field | no |
| a field-less struct, or a zero-length array | no — and unreachable: **A045** rejects the first as a value type and the type checker rejects the second |

Note the rank asymmetry: an array of scalars nests to any depth, while a struct field may be
a scalar or a one-dimensional array of scalars and no deeper.

The rule behind the table: *a specification's aggregate support cannot exceed the executable
language's aggregate `@` support*, because proof mode lowers spec bodies through the same
unrolling. Analysis rules **A027** (struct with compound fields) and **A028** (array of
structs) draw that boundary, and they are mode-blind — they fire inside a spec body exactly
as they fire in executable code, so in a normal `infc` build an out-of-surface `@` is
reported by analysis first.

The translator keeps its own rejection for the same shape all the same, and that rejection is
not dead: the corpus gate and the unit-test pipelines run parse → type-check → codegen with
no analysis pass at all, so there it is the only guard. (This is the same reason **P014**
exists beside **A037**.) Out-of-surface *parameters* and *literals* have no analysis rule at
any time and are rejected by the translator alone, as **P004** and **P002** — so the two
surfaces stay the same width whichever position the aggregate appears in.

### Slot Allocation

Two rules fix every `T_local` index in an emitted payload:

- **Enumeration order** within one aggregate: arrays row-major, struct fields in layout
  order, recursing. Over the shapes both sides support — which, by the table above, is every
  shape that gets this far — it is the same order the runtime unrolling of a compound `@`
  uses.
- **Allocation order** within one spec function: parameters first, in declaration order,
  then each `@` in binding order — one slot per scalar leaf.

Leaf order stops being an implementation detail the moment a proof fails and you read
`T_local 2` in a goal, so these are part of the contract.

A *rejected* declaration still costs slots, which matters when you are counting your way back
from a goal to the source. A refused non-scalar parameter and a refused out-of-surface
compound `@` each consume exactly one slot, and an aggregate refused for overrunning the leaf
budget consumes its whole leaf count — in every case the counter advances without anything
being emitted, so every later slot number keeps the position the source gives it.

In a universal payload a hypothesis is emitted for **every** leaf, including leaves the
claim never reads. They are antecedents, so the extra ones only weaken the obligation;
uniformity is preferred to a use analysis that would make slot numbering depend on what the
body happens to mention. The cost is worth knowing: an N-leaf aggregate puts N hypotheses in
front of every obligation of its function. A literal's leaves are constants that neither bind
a variable nor guard one.

A leaf states what its own declared *element* or *field* type says, by the table above — so a
`[u8; 2]` bounds both of its leaves at `0..255`, a struct bounds each field at its own type,
and a leaf of an uninhabited enum is **P015**. An *existential* leaf carries no typing guard —
the prover chooses the value, so there is no unconstrained valuation to make denote — and
keeps its declared bound, as a conjunct inside its own binder, wherever the claim reads it.

### One Obligation, Fully Expanded

```inference
spec AggregateValues {
  fn leaf_bounds() forall {
    let a: [i32; 3] = @;
    assert(a[0] <= a[0]);
  }
}
```

emits (on one line — the layout below is this document's reflow, not the emitter's)

```coq
Definition spec_aggregate_values__AggregateValues_hspec1 : hassert :=
  Himpl (HA_and (HA_has_type (T_local 0%N) T_i32)
        (HA_and (HA_has_type (T_local 1%N) T_i32)
                (HA_has_type (T_local 2%N) T_i32)))
        (HA_not (term_eq (T_relop T_i32 (Relop_i (ROI_le SX_S))
                                  (T_local 0%N) (T_local 0%N))
                         (T_const (Vi32 0)))).
```

Three leaves, three slots, three guards drained together at the first structural statement;
the claim reads slot 0 twice, and slots 1 and 2 are quantified but unread. `a[0] <= a[0]` is
a *relop* rather than a logical comparison because the obligation must speak the arithmetic
the compiled program executes, at that operator's own width and signedness.

### Aggregate Equality

`a == b` at aggregate type, in assertion position, is the conjunction of per-leaf equalities;
`a != b` is the dual disjunction. This is the intended language rule: `==` compares values,
and an aggregate's value is exactly its ordered scalar leaves. Aggregate comparison in
*term* position (inside arithmetic) is rejected — there is no aggregate term to compute with.

## An Element at a Non-Constant Index

A constant index resolves at translation time. A non-constant one names an element the
translation cannot pick, so it binds a fresh logical variable `v` pinned by

```text
(i <u N) ∧ ⋀_{c<N} Himpl (i = c) (v = leaf c)
```

The range bound is the first conjunct, so a reader of a failing goal meets it before the case
split. A chain may carry **one** such index: constant steps descend eagerly, so `m[1][j]`
splits over the two elements of the already-selected row `[1]`, while `m[i][j]` is rejected —
the split would be the product of the two extents, and one obligation carries one case split
per chain.

### `a[i]` Means "The Element at `i`, Which Exists"

`a[i]` in a specification is not "whatever is stored at index `i`" — it is *the element at
index `i`, which exists*. The obligation defines that element only where `i` is in range, so
`assert(a[i] == a[i])` is not a tautology: it claims that `i` is a valid index and that the
element there equals itself. Constrain the index first, and the claim becomes provable:

```inference
forall {
  let a: [i32; 3] = @;
  let i: i32 = @;
  assume { assert(0 <= i && i < 3); }
  assert(a[i] == a[i]);
}
```

This is deliberate. The alternative — treating an out-of-range access as vacuously satisfied
— would hand you a proof that says nothing about most values of `i`, and Inference already
rejects an obligation that says nothing (**P010**).

Nothing in proof mode emits a bounds check, so this is a definedness rule rather than a
mirror of any runtime trap.

### A Signed Index Needs Both Bounds

The emitted range bound is a single **unsigned** comparison, `i <u N`, and no lower bound is
missing: a negative index arrives as a huge unsigned value and fails `<u N` on its own.

The consequence is a trap worth stating plainly. When the index is *signed*, an `assume` that
supplies only the upper bound compiles clean and produces an obligation that is **false**.
Nothing diagnoses it; it surfaces later as a Rocq goal that will not close.

```inference
// WRONG — compiles, emits an obligation that cannot be proved.
// `i = -1` satisfies the signed `i < 3` and fails the element's unsigned range bound,
// so the element is undefined at an index the assume admits.
forall {
  let a: [i32; 3] = @;
  let i: i32 = @;
  assume { assert(i < 3); }
  assert(a[i] == a[i]);
}
```

```inference
// RIGHT — the signed lower bound is what bridges to the unsigned range bound.
forall {
  let a: [i32; 3] = @;
  let i: i32 = @;
  assume { assert(0 <= i && i < 3); }
  assert(a[i] == a[i]);
}
```

Both bounds are *necessary*, not sufficient: they make the element denote, and nothing more.
A claim about the element's value needs hypotheses about the element's value as well, because
a compound `@` states of each leaf only what that leaf's declared element type says — over
`[i32; 3]` that is the typing alone, and the leaf ranges over the whole class.
So `assert(a[i] >= 0)` does not follow from the bounds alone; it follows from the bounds *and*
an assumption covering every element, which is what the corpus fixture states:

```inference
// From tests/test_data/inf/spec_bounded_iteration.inf — every obligation in that file is
// discharged against wasm-verifier, so this shape is known to close.
forall {
  let a: [i32; 3] = @;
  let i: i32 = @;
  assume { assert(a[0] >= 0 && a[1] >= 0); }
  assume { assert(a[2] >= 0); }
  assume { assert(0 <= i && i < 3); }
  assert(a[i] >= 0);
}
```

An unsigned index needs no lower bound at all, because its source guard lowers to an unsigned
comparison that exactly complements the emitted range bound. The obligation is
correspondingly simpler — one comparison in the antecedent instead of two, and one fewer
nested implication when the lower bound would have been written as its own `assume` block:

```inference
// SIMPLEST — `i < 3` lowers unsigned at `u32`, complementing the range bound directly.
forall {
  let a: [i32; 3] = @;
  let i: u32 = @;
  assume { assert(i < 3); }
  assert(a[i] == a[i]);
}
```

Each operator follows its own operand's type rather than a per-function mode, so an unsigned
index may compare unsigned while a claim about `i32` elements stays signed. Note also that
`i32` and `u32` are indistinguishable in the hypothesis a slot carries: both admit every
value of their class, so over `[i32; 3]` the index takes slot 3 (slots 0-2 are the array's
leaves) and is guarded by exactly `HA_has_type (T_local 3%N) T_i32` either way. The index's
signedness is therefore carried entirely by the relop spellings, and no downstream proof can
catch a wrong operator choice for you. A *narrow* index would differ — it states the values
its declaration admits alongside the width — but a narrow index needs no source guard at all
where its whole domain is already in bounds.

## Alternating Quantifiers

A `forall` block nested inside an `exists` or `assume` block of a universal spec function
binds a real universal variable and emits `Hall`, so `∃k. ∀x. P` stays in that order. Giving
the inner `@` a free slot instead would let the outer judgment quantify it, silently turning
the claim into `∀x. ∃k. P` — a different and weaker property.

The same nesting inside an `exists`/`unique`-quantified function is **P007**. There every `@`
is a hidden choice parameter the judgment quantifies operationally, so a universal binder
over one has no representation; move the universal claim into its own `forall`-bodied spec
function.

## Caps

- **64 scalar leaves per spec function**, cumulative across every aggregate introduction
  (**P013**). The resource being protected is assertion-tree depth, and what a leaf costs
  depends on where it comes from: a leaf of a universal `@` or parameter costs one hypothesis
  level; a leaf of a `@` inside a nested `forall` costs a hypothesis level *and* a `Hall`
  level; a leaf of an existential `@` costs one `HA_ex` level — two where a narrow one's bound
  rides inside it — and no typing guard at all (a prover-chosen value needs no stated
  typing); and a literal's leaves are constants that bind
  nothing and guard nothing, nesting only one conjunct apiece through a leafwise comparison.
  All of those accumulate across every introduction in the function until a structural
  statement drains the guards, so a per-introduction cap would not bound the resource it
  names.

  *Universally* a narrow leaf costs the same **one** level as a full-width one — its bound is
  grouped into that level, not added beside it — and only deepens that level's own subtree,
  on a branch the fold does not continue along. *Existentially* it costs **two**: a read
  narrow binder is `HA_ex (HA_and bound body)` where a full-width one's ⊤ is absorbed and
  leaves a bare `HA_ex`. Signedness makes no difference, because a signed pair's `HA_and` of
  two bounds sits inside that one conjunct node. Measured at the budget's exact ceiling with
  every leaf read:

  | shape | assertion depth |
  | --- | --- |
  | `[i32; 64]`, universal | 66 |
  | `[u8; 64]` / `[u16; 64]` / `[bool; 64]`, universal | 67 |
  | `[i8; 64]` / `[i16; 64]`, universal | 68 |
  | `[i32; 64]`, existential | 128 |
  | any narrow width at 64 leaves, existential | 192 |

  So the worst case is existential and narrow, at 192 of 256, leaving 64 levels for the
  claim. The executable unrolling's own cap is far larger; the two measure different things
  (instruction count versus obligation nesting depth).
- **One non-constant index per access chain** (**P002**).
- **`MAX_TREE_DEPTH` = 256 assertion-tree levels** overall, all of them usable — the gate
  refuses a tree *deeper* than the maximum, so one exactly at it passes. The depth counts
  assertion nodes only: a term is checked on a counter of its own, so it never extends the
  assertion depth, and printed-paren nesting in the emitted `.v` is a different number. Any
  scalar figure quoted against this cap has to name the shape it was measured on, since `N`
  draws drained at one `assert` and `N` draws each with their own `assert` are different
  trees. Enforced by a fail-closed pre-encode gate that names the offending spec and function
  (`CodegenError::HspecTreeTooDeep`).

## What Stays Rejected, and Why

| Code | Construct | Reason |
| --- | --- | --- |
| P002 | `loop`, `break` | a loop states a property only through an invariant this translation cannot infer; quantify an index and constrain it instead |
| P002 | `**`, string literals, nested `unique` blocks | no assertion encoding |
| P002 | an array/struct literal read in scalar term position, or written at an out-of-surface shape | a literal becomes one term per scalar leaf, so there is neither a whole-value term nor a leaf tree to build |
| P002 | an access chain with two non-constant indices, or one whose non-constant index lands on an aggregate rather than a scalar leaf | one obligation carries one case split per chain, and the split has to end at a leaf |
| P003 | reassignment | a specification names values, not storage — every name stands for one value throughout the claim |
| P004 | `unit`, a function type, an out-of-surface aggregate *parameter* | not representable |
| P004 | an aggregate read whole in term position, an aggregate call argument included | an aggregate is not a term; name the component you mean |
| P004 | an *aggregate* parameter of an `exists`/`unique` body | the obligation denotes against a real frame, where the parameter is one pointer local |
| P005 | a call that is not a `T_app`: an *unbound* `extern` callee (no `use … from` binding, so no module supplies a body), an instance method, one that does not resolve to a function this module defines or links, a non-deterministic-bodied callee, or — in term position only — one whose result is not a single scalar, compound or `unit` alike | the symbol has to name a function of the emitted module, applied at its real signature; a bare call *statement* is unaffected, becoming `HA_app_ok` at any result arity |
| P007 | `forall` inside an `exists`/`unique` body | see *Alternating Quantifiers* |
| P008 | an out-of-surface compound `@` in **any** body; any compound `@` in an `exists`/`unique` body | out of surface, there is no leaf tree to build; in a reachability body a choice arrives as one scalar parameter of the run |
| P014 | a constant-folded out-of-bounds index | the same fact analysis rule A037 states, at the spelling A037 cannot see |
| P015 | a parameter, a `@`, or an aggregate leaf at an `enum` declared with no variants | an uninhabited type has no value for the claim to range over, so the obligation would either say nothing or be unprovable for a reason unrelated to the program; A009 only *warns* about the declaration, so such an enum really does reach here |

Memory-content assertions — addresses, points-to, iterated heaps — are out of scope entirely.
The surface language cannot express them, and the properties this encoding *can* state are
claims about aggregate values rather than about particular heap cells, which is what quantified
scalar leaves express exactly.

## Related Documents

- [`core/wasm-to-v/ROCQ_CONTRACT.md`](../../wasm-to-v/ROCQ_CONTRACT.md) — the emitted `.v`
  contract, including the full `P001`–`P015` registry
- [`docs/arrays-and-memory.md`](arrays-and-memory.md) — the executable lowering of the same
  aggregates, including `compute_struct_field_layout`, whose order the leaf enumeration
  shares
