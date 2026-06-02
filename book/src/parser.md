# The Inference Parser

This document explains how the Inference compiler turns `.inf` source text into a
typed AST. It covers the resilient parser that lives in the
`core/parser` crate (`inference-parser`), the rust-analyzer-derived architecture
it is built on, and how it recovers from syntax errors without panicking.

## Overview

Parsing is the first phase of the compilation pipeline. It is responsible for one
job: read a string of Inference source and produce an
[`AstArena`](memory-allocation-in-wasm-codegen.md) — the arena-allocated, typed
Abstract Syntax Tree that every later phase (type checking, static analysis, code
generation, Rocq translation) consumes.

The parser is a single, self-contained component. Its design goals
shape everything that follows:

- **No C toolchain dependency.** The parser is pure Rust, with no build script and
  no generated C scanner, which keeps cross-compilation (notably on Windows)
  simple.
- **Resilient, structured errors.** The parser *never aborts and never panics* —
  on any input it returns a tree plus a list of structured
  `ParseError { span, message }` values, so later phases and the IDE always have
  something to work with.
- **Speed.** The engine is fast (see [Performance](#performance)) and lowers
  straight into a compact arena-based AST.

The design follows two references closely:

- The [rust-analyzer parser](https://github.com/rust-lang/rust-analyzer/tree/master/crates/parser)
  for the event-based engine (`Marker`s, `forward_parent`, a flat event stream).
- matklad's
  [Resilient LL Parsing Tutorial](https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html)
  and [Parsing Advances](https://matklad.github.io/2025/12/28/parsing-advances.html)
  for error recovery and the loop-progress ("fuel" + advance-assertion) guarantee.

The parser implements the Inference language syntax directly; this crate is the
syntactic source of truth for the compiler.

## The pipeline

A single call to `inference_parser::parse` runs four internal stages:

```text
  .inf source (&str)
        │
        ▼
  ┌───────────┐   Vec<Token>      ┌───────────┐   events        ┌──────────────┐
  │   lexer   │ ────────────────► │  parser   │ ──────────────► │  build_tree  │
  │ (lexer.rs)│  kind+span+joint  │(parser.rs)│  Start/Token/   │(syntax_tree) │
  └───────────┘  incl. trivia     │ + grammar │  Finish/Error   └──────┬───────┘
                                  └───────────┘                        │ owned CST
                                                                       ▼
                                                                 ┌───────────┐
                                                                 │   lower   │
                                                                 │ (lower.rs)│
                                                                 └─────┬─────┘
                                                                       ▼
                                                          AstArena + Vec<ParseError>
                                                                  (Parse)
```

The public surface is small (`core/parser/src/lib.rs`):

```rust,ignore
pub fn parse(src: &str) -> Parse;

pub struct Parse {
    pub arena: AstArena,
    pub errors: Vec<ParseError>,
}

// The CST seam, exposed mainly for testing grammar shape and recovery directly:
pub fn parse_to_cst(src: &str) -> (SyntaxNode, Vec<ParseError>);
```

The orchestration crate `core/inference` calls `parse` from `inference::parse`,
mapping a non-empty `errors` list onto an `anyhow::Error` to fit the
`&str -> Result<AstArena>` contract the rest of the compiler expects.

### Crate layout

| Module | Responsibility |
|--------|----------------|
| `lexer.rs` | Trivia-aware tokenizer → `Vec<Token>` |
| `syntax_kind.rs` | The single `SyntaxKind` enum (every token *and* node kind) |
| `token_set.rs` | `TokenSet(u128)` bitset, used for recovery sets |
| `input.rs` | A trivia-free *view* over the tokens, with joint bits |
| `event.rs` | The `Event` / `Step` model and `process()` |
| `parser.rs` | The cursor, `Marker`s, fuel + advance guard |
| `syntax_tree.rs` | The owned CST (`SyntaxNode`) and `build_tree` |
| `grammar.rs` + `grammar/{items,types,stmt,expr,params}.rs` | The recursive-descent rules |
| `lower.rs` | CST → `AstArena` lowering |
| `errors.rs` | `ParseError` and the crate's `ParserError` enum |

The dependency direction is `inference → parser → ast`. The parser depends on
`inference-ast` only to allocate AST nodes; `inference-ast` knows nothing about the
parser.

## Stage 1: the lexer

The lexer (`core/parser/src/lexer.rs`) makes a single pass over the source bytes
and emits a flat `Vec<Token>`:

```rust,ignore
pub struct Token {
    pub kind: SyntaxKind,
    pub loc: Location,   // byte offsets + 1-based line/column
    pub joint: bool,     // is this token byte-adjacent to the next?
}
```

Three properties matter:

- **Trivia is preserved.** Whitespace, `//` comments, and `///` doc comments are
  emitted as `Whitespace` / `Comment` / `DocComment` tokens rather than discarded.
  This keeps the token stream *lossless*: concatenating every token's source slice
  reproduces the input byte-for-byte. (The parser later sees a trivia-free view;
  trivia is re-attached when the tree is built.)
- **Joint bits drive `token.immediate` semantics.** A token is `joint` when its
  end offset equals the next token's start offset (no whitespace between). The
  grammar needs this for constructs that must not have intervening whitespace: the
  path separator `::`, the generic-argument terminator `'` (as in `Vec i32'`), and
  the unit literal/type `()`.
- **The lexer is total.** It never panics and always terminates. Every byte is
  covered by exactly one token. Malformed input — an unterminated string, an
  unknown character — becomes an `Error` token rather than an exception, so even
  the lexer participates in error recovery.

A few Inference-specific lexical rules are worth calling out, because they affect
parsing downstream:

- **Greedy negative numbers.** A leading `-` is part of a number literal when it
  is immediately followed by a digit, so `-42` lexes as a single `Number` token,
  not `Minus` then `Number`. (`- 42`, with a space, lexes as two tokens and parses
  as a unary negation.)
- **Reserved-but-not-keyword identifiers.** `constructor`, `proof`, and `uzumaki`
  lex as ordinary identifiers, matching the grammar's reserved-identifier rule.

## Stage 2: SyntaxKind and TokenSet

Like rust-analyzer, Inference uses a **single** `SyntaxKind` enum
(`core/parser/src/syntax_kind.rs`) that holds *both* token kinds and node kinds:

```rust,ignore
#[repr(u16)]
pub enum SyntaxKind {
    // token kinds first: trivia, literals, keywords, punctuation, operators…
    Whitespace, Comment, DocComment, Number, String, Ident,
    FnKw, LetKw, /* … */ Plus, Minus, StarStar, ColonColon, At, Tick, /* … */
    Error, Eof,
    // …then node kinds:
    SourceFile, FunctionDefinition, BinaryExpression, /* … */ UnaryBitnot,
}
```

The boundary between the two groups is the associated constant
`SyntaxKind::FIRST_NODE` (the `SourceFile` variant). A compile-time assertion keeps
all *token* kinds below 128:

```rust,ignore
const _: () = assert!(
    (SyntaxKind::FIRST_NODE as usize) <= 128,
    "token kinds must fit in TokenSet (u128)"
);
```

That bound exists because `TokenSet` (`core/parser/src/token_set.rs`) is a
`u128` bitset over token discriminants. Recovery sets and multi-token `at(…)`
checks are then constant-time bit operations:

```rust,ignore
const STMT_START: TokenSet = TokenSet::new(&[LetKw, ReturnKw, IfKw, LoopKw, /* … */]);
```

Using one enum for tokens and nodes means the lexer, the parser, the CST, and the
lowering all speak the same vocabulary — no conversions at the boundaries, and the
CST is homogeneous (`Node(kind)` / `Token(kind)`).

## Stage 3: the event-based parser

The parser (`core/parser/src/parser.rs`) does **not** build a tree directly.
Following rust-analyzer, it consumes a trivia-free `Input` and emits a flat list of
events:

```rust,ignore
pub enum Event {
    Start { kind: SyntaxKind, forward_parent: Option<u32> },
    Token { kind: SyntaxKind },
    Finish,
    Error { msg: String },
}
```

Grammar rules bracket their work with **markers** instead of nesting function
results. A rule opens a marker, consumes tokens and sub-rules, then completes the
marker with the node kind it turned out to be:

```rust,ignore
fn function_definition(p: &mut Parser) {
    let m = p.start();          // open a marker (a tombstone Start event)
    p.eat(SyntaxKind::PubKw);   // optional visibility
    p.bump(SyntaxKind::FnKw);
    name(p);
    argument_list(p);
    // …
    m.complete(p, SyntaxKind::FunctionDefinition); // fill in the node kind
}
```

This indirection buys the parser its most important trick: **`precede` /
`forward_parent`**, which retroactively wraps an already-parsed node in a new
parent. That is how left-associative and postfix forms are built — e.g. after
parsing `a`, seeing `.b` reparents `a` under a `MemberAccessExpression`:

```rust,ignore
let lhs = atom(p);                  // CompletedMarker for `a`
let m = lhs.precede(p);             // open a parent *before* `a`
p.bump(SyntaxKind::Dot);
name(p);
m.complete(p, SyntaxKind::MemberAccessExpression);
```

### Expressions: Pratt parsing

Binary and unary expressions are parsed by precedence climbing
(`core/parser/src/grammar/expr.rs`). Each binary operator has a single **binding
power** plus a `right_assoc` flag — the numbers come from the Inference grammar's
`PRECEDENCE` map, so the parse shapes match the language definition exactly (higher
binds tighter):

| Operators | Binding power | Associativity |
|-----------|--------------:|---------------|
| `\|\|` | 48 | left |
| `&&` | 49 | left |
| `\|` | 57 | left |
| `^` | 58 | left |
| `&` | 59 | left |
| `==` `!=` | 60 | left |
| `<` `<=` `>` `>=` | 70 | left |
| `<<` `>>` | 80 | left |
| `+` `-` | 97 | left |
| `*` `/` `%` | 98 | left |
| `**` | 99 | **right** |

The Pratt loop folds in any operator that binds tighter than the caller's floor.
Right-associativity is handled with two small adjustments: a right-associative
operator at *exactly* the floor still recurses (so `a ** b ** c` nests to the
right), and it recurses with `op_bp - 1` as the new floor:

```rust,ignore
while let Some((op_bp, right_assoc)) = binary_bp(p.current()) {
    if op_bp <= min_bp && !(right_assoc && op_bp == min_bp) {
        break;
    }
    let m = lhs.precede(p);          // reparent the LHS under a BinaryExpression
    p.bump_any();                    // the operator token
    let next_min = if right_assoc { op_bp - 1 } else { op_bp };
    expr_bp(p, next_min, allow_struct);
    lhs = m.complete(p, SyntaxKind::BinaryExpression);
}
```

Prefix operators (`!`, `-`, `~`) bind tighter than any binary operator, and the
postfix forms — call `f(…)`, index `a[…]`, member `.x`, type-member `::x` — bind
tightest of all. Assignment is deliberately **not** an expression operator: `=` is
handled at statement level (`assign_statement`), matching the grammar.

### Disambiguations the parser handles explicitly

An LL parser has to resolve a few grammar ambiguities by hand. The
notable ones:

- **`-42` vs `- x`** — resolved in the lexer (greedy negative numbers, above).
- **`::` and `'` immediacy** — only treated as a path separator / generic
  terminator when the joint bit says there is no intervening whitespace.
- **Struct literal `{` vs block `{`** — `S { a: 1 }` is a struct literal, but in
  an `if`/`loop` *condition* a `{` opens the body. The expression parser threads a
  `no_struct` flag through condition parsing to forbid struct literals there.
- **Contextual keywords** — `self`, `type`, `from`, and `spec` are keywords only
  in the rules that introduce them; everywhere an identifier is expected they are
  ordinary identifiers (so `self.type = …` and `spec::Auction::new()` parse). The
  parser accepts these tokens where an identifier is wanted and remaps them to
  `Ident`.

## Stage 4: the owned CST

`process()` (`core/parser/src/event.rs`) resolves the `forward_parent` chains and
flattens the events into a linear `Vec<Step>` of `Enter(kind)` / `Token` /
`Leave` / `Error`. `build_tree` (`core/parser/src/syntax_tree.rs`) then walks the
steps together with the *full* token list (trivia included) to produce an owned,
immutable concrete syntax tree:

```rust,ignore
pub enum SyntaxElement {
    Node(SyntaxNode),
    Token(Token),
}

pub struct SyntaxNode {
    pub kind: SyntaxKind,
    pub loc: Location,
    pub children: Vec<SyntaxElement>,
}
```

Trivia tokens are re-attached as `Token` children at the position they occur, so
the tree is lossless. Each node's `loc` spans its first-to-last *non-trivia*
descendant token. The CST is private to the crate — it is an intermediate, not a
public artifact. (Exposing a persistent red/green tree for IDE use is possible
future work but out of scope.)

`SyntaxNode` provides kind-based navigation helpers (`child(kind)`,
`children_of(kind)`, `nth_node(n)`, `child_token(kind)`, `text(src)`) that the
lowering uses to find the children it needs.

## Stage 5: lowering to the AstArena

`lower.rs` walks the CST and allocates typed AST nodes into the `AstArena`. This is
the largest module in the crate (~1,600 lines).

`AstArena` indices are sequential `la_arena` slots, so the *order* in which nodes
are allocated is part of the AST's identity — every later phase indexes into the
arena by slot. The lowering therefore allocates in a fixed, deterministic order,
and a few allocation-order rules are worth calling out:

- function parameters allocate **type before name**;
- a bare `return;` still allocates a `UnitLiteral` expression;
- named call/struct arguments allocate the name **as an expression** before the
  value;
- the `SourceFileData` node is allocated **last**, after all of its definitions.

The methods (`lower_source_file`, `lower_definition`, `lower_statement`,
`lower_expression`, `lower_type`, …) follow the structure of the grammar one rule
at a time, which keeps the allocation order easy to read off and to test (see
[Testing](#testing-and-verification)).

## Error recovery and the never-panic guarantee

The parser's central contract is that **`parse` never panics on any input** and
always returns a `Parse`. Resilience is built in at three levels.

**Resilient LL recovery.** When the parser hits a token it cannot use, it does not
unwind — it emits an `Error` event and, where appropriate, consumes the offending
token into an `Error` node so that progress is guaranteed. Two strategies are used
depending on context:

- Top-level item parsing, which has no closing delimiter, uses
  `err_recover(RECOVERY_SET)` to resynchronize on the start of the next item.
- Delimited bodies (a `{ … }` block, a spec body, a struct body) use
  `err_and_bump`, which *always* consumes a token. Using a recovery *set* inside a
  delimited body can spin if the stuck token is itself in the set, so the
  always-consume rule is the safe choice there.

**The fuel + advance guard.** A resilient LL parser can, if a rule is written
incorrectly, loop forever by retrying a recovery path that consumes nothing. The
parser borrows matklad's safeguard: a fuel counter
(`const FUEL: u32 = 256`, a `Cell<u32>`) is decremented on every lookahead and
refilled whenever real progress is made — a token is bumped *or* a node is
completed. An assertion fires (in debug *and* release) the moment fuel hits zero,
turning a would-be infinite loop into an immediate, localized crash during
development rather than a hang in production. On well-formed grammar rules the
guard never trips; it is a backstop, not part of normal control flow.

**Lowering is total.** Because the grammar can produce a node whose expected child
was lost to recovery (e.g. `a.` with no name after the dot), the lowering never
unwraps a child that recovery might have dropped. Every such site falls back to a
synthesized `<error>` placeholder node and records a `ParseError`. The adversarial
test corpus — truncated items, dangling operators, random bytes, and deeply nested
unterminated input like `"fn f(){".repeat(200)` — is run through the full public
`parse` under `catch_unwind` to prove zero panics.

Errors surface as structured values (`core/parser/src/errors.rs`):

```rust,ignore
pub struct ParseError {
    pub span: Location,
    pub message: String,
}
```

Each error is given a source span by tracking the token cursor through the step
stream, so a message such as `expected ;` points at the right place. The richer
diagnostics this enables (labels, notes, IDE squiggles) are intentionally left for
later work; for now `inference::parse` aggregates the messages into one error to
keep its existing contract.

## Testing and verification

The parser is covered at two levels: in-crate unit tests for every stage, and the
compiler's end-to-end suites that exercise it as the real front end.

In-crate, `core/parser` carries ~200 unit tests spanning:

- the lexer (every token class, joint bits, round-trip losslessness, edge cases
  like `-42` and unterminated strings);
- the engine (markers, `forward_parent`/`precede`, the advance guard);
- the grammar (CST shape per construct, precedence and associativity);
- the lowering (arena structure per construct, plus the error-recovery fallbacks);
- resilience (the full adversarial corpus through `parse`, asserting no panic).

End to end, the AST tests in `tests/src/ast` and the four-tier WASM codegen golden
suite run on the arenas this parser produces, so any change in parsing that would
alter the AST — and therefore the generated WebAssembly — is caught by a golden
diff. To support exact arena comparisons in tests, `AstArena` derives
`PartialEq`/`Eq`.

## Performance

The parser is substantially faster than the tree-sitter front end it
replaced. Measured on the same inputs (release build, full `source → AstArena`
path):

| Input | tree-sitter + Builder | `inference-parser` | Speedup |
|-------|----------------------:|-------------------:|--------:|
| `example.inf` (13 KB) | 967 µs | 177 µs | ~5.5× |
| 80 KB corpus | 7.1 ms | 1.4 ms | ~5.1× |
| 100 synthetic functions (38 KB) | 3.9 ms | 0.6 ms | ~6.3× |

The speedup is intrinsic to the engine — it holds even when only the parse stage
(no AST lowering) is measured, and tree-sitter's per-call parser-construction cost
turned out to be negligible. Removing the tree-sitter C dependency is a separate,
additional win for build times and cross-platform robustness.

The one trade-off is allocation: the owned CST is built from many small per-node
`Vec`s, so the new parser performs more allocations than tree-sitter's single
internal arena. This is transient (well under 20 MB for an 80 KB file) and is the
natural target for future optimization (arena-allocating CST children, or lowering
directly from the event stream).

## Summary

The Inference parser is a recursive-descent parser modeled on
rust-analyzer's event engine and matklad's resilient-LL techniques. It lexes
trivia-aware tokens, parses them into a flat event stream via markers, builds a
lossless owned CST, and lowers that CST into the `AstArena` that the rest of the
compiler consumes. It is fast, depends on no C toolchain, never panics on malformed
input, and is covered by in-crate unit tests together with the compiler's
end-to-end AST and golden-codegen suites.

## References

- [rust-analyzer parser crate](https://github.com/rust-lang/rust-analyzer/tree/master/crates/parser)
- matklad, [Resilient LL Parsing Tutorial](https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html)
- matklad, [Simple but Powerful Pratt Parsing](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html)
- matklad, [Parsing Advances](https://matklad.github.io/2025/12/28/parsing-advances.html)
