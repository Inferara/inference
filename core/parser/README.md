# inference-parser

Hand-written, resilient recursive-descent parser for the Inference language. This crate
replaced the `tree-sitter` + `tree-sitter-inference` front end (issue #62) with a pure-Rust
implementation modeled on the rust-analyzer parser architecture and matklad's "Resilient LL
Parsing" approach. It produces an `inference_ast::arena::AstArena` byte-identical to the
one the legacy `Builder` produced from a tree-sitter CST, so all downstream phases
(type-checker, analysis, codegen, wasm-to-v) are unaffected. Parsing is resilient and never
panics on malformed input: it always returns a `Parse` holding the arena plus a
`Vec<ParseError>`; syntax errors are collected rather than aborting the parse.

## Pipeline

```
.inf source
    │
    ▼
lexer.rs        tokenize(&str) -> Vec<Token>
                trivia-aware (whitespace, comments, docstrings preserved),
                joint bits track immediate adjacency for `::` and `'`
    │
    ▼
input.rs        Input: trivia-free view of the token stream for the parser
    │
    ▼
grammar/        recursive-descent grammar rules (items, types, stmts, exprs)
parser.rs       Marker / precede / forward_parent; fuel counter + advance guard
event.rs        flat Event stream → Vec<Step>
    │
    ▼
syntax_tree.rs  build_tree: re-attaches trivia → owned SyntaxNode (lossless CST)
    │
    ▼
lower.rs        Lowering: walks the CST, allocates AstArena nodes
                (alloc order mirrors the deleted builder.rs exactly)
    │
    ▼
AstArena        inference_ast::arena::AstArena (unchanged public type)
```

## Module Map

| Module | Role |
|--------|------|
| `lib.rs` | Public API: `parse`, `parse_to_cst`, `Parse`; re-exports |
| `errors.rs` | `ParserError` enum (thiserror) + `ParseError { span: Location, message }` |
| `syntax_kind.rs` | Single `SyntaxKind` enum: token kinds first, then node kinds from the computed `FIRST_NODE` boundary; keyword table; `is_trivia`, `from_keyword`, `is_token`. Contextual keywords (`self`/`type`/`from`/`spec`) are handled in `grammar/types.rs` via the `IDENT_LIKE` token set, not here |
| `lexer.rs` | `tokenize(&str) -> Vec<Token>`; handles trivia, joint bits, greedy `-N`, unterminated strings |
| `token_set.rs` | `TokenSet(u128)` bitset over `SyntaxKind` discriminants for O(1) recovery sets |
| `input.rs` | Trivia-free token view the parser cursor operates on |
| `event.rs` | `Event` enum + `process` producing `Vec<Step>` consumed by `build_tree` |
| `parser.rs` | `Parser` cursor; `Marker` / `CompletedMarker`; fuel counter; advance guard |
| `syntax_tree.rs` | Owned immutable CST (`SyntaxNode`, `SyntaxElement`); `build_tree`; navigation helpers |
| `grammar.rs` | Entry point `source_file`; top-level dispatch |
| `grammar/items.rs` | Top-level definitions: `fn`, `spec`, `struct`, `enum`, `const`, `type`, `use`, `external fn` |
| `grammar/types.rs` | Type grammar: primitives, `[T; N]`, `fn(..)->T`, generic names, qualified names |
| `grammar/params.rs` | Argument lists, `self`/ignore/typed args, type-parameter lists |
| `grammar/stmt.rs` | Statements and blocks: `let`, assign, `return`, `loop`, `if`/`else`, non-det blocks, `assert`, `break` |
| `grammar/expr.rs` | Pratt expression parser; prefix unary; postfix `.`/`::`/call/index; all atoms |
| `lower.rs` | `Lowering` struct: walks CST, allocates `AstArena` nodes with identical order to the legacy `Builder` |

## Public API

```rust
use inference_parser::{parse, parse_to_cst, Parse, ParseError};

// Primary entry point — full pipeline to AstArena.
let parse_result: Parse = inference_parser::parse(src);

// The arena is ready for type-checking, analysis, and codegen.
let arena = parse_result.arena;

// Syntax errors collected during resilient parsing (empty on valid input).
let errors: Vec<ParseError> = parse_result.errors;
for err in &errors {
    // err.span: inference_ast::nodes::Location (byte offsets + 1-based line/col)
    // err.message: String
    eprintln!("{}:{}: {}", err.span.start_line, err.span.start_column, err.message);
}

// CST-level entry point — useful for testing grammar shape and recovery.
let (tree, errors) = inference_parser::parse_to_cst(src);
```

`Parse` carries `#[must_use]` so callers cannot accidentally discard syntax errors.

## Design Notes

### Event / Marker model (rust-analyzer style)

The parser never builds the tree directly. Instead, `Parser::start()` opens a `Marker`
and `Marker::complete(p, kind)` records an `Event::Start` / `Event::Finish` pair.
`Marker::precede` retrofits a parent node over an already-completed subtree, enabling
left-associative and postfix constructs without backtracking. `process` flattens the
event list into a `Vec<Step>` that `build_tree` consumes to assemble the owned CST.

### Fuel counter and advance guard (matklad)

A `fuel: Cell<u32>` (initialized to 256) lives on the `Parser`. `Parser::nth` — the single
lookahead primitive every `current`/`at`/`nth_at`/`at_ts` call funnels through — decrements
it on each peek and asserts it is non-zero, so a recovery loop that peeks without making
progress trips the assertion in debug builds. Progress refills the fuel: consuming a token
(`do_bump`) or completing a node (`Marker::complete`). Completing a node counts because a
deeply nested but well-founded parse reaches end of input and then unwinds, closing one node
per frame while only peeking at the `Eof` sentinel; that bounded, terminating unwind must not
be mistaken for a non-advancing spin. A genuine spin neither bumps nor completes, so it still
depletes the fuel and fails loudly rather than looping forever.

### Single `SyntaxKind` enum

One enum covers both token kinds and node kinds following rust-analyzer's design. The token
variants come first and the node variants follow; the boundary is the computed
`FIRST_NODE` const (`SyntaxKind::SourceFile as u16`), and `SyntaxKind::is_token` tests a
discriminant against it. `TokenSet(u128)` stores a bitset of kinds, which requires every
token discriminant below 128 — a `const` assertion (`FIRST_NODE <= 128`) pins this at
compile time. The lexer, parser, CST, and lowering all share this single vocabulary.

### Lossless, trivia-attached CST

`tokenize` preserves whitespace, `//` comments, and `///` doc comments as trivia tokens
in the flat stream. `build_tree` re-attaches them when constructing the `SyntaxNode`
tree, so the CST is lossless. Node locations span the first to last non-trivia token of
each construct (matching the tree-sitter convention the old `Builder` used).

### Parity-with-Builder contract

`lower.rs` is a near-mechanical port of the deleted `core/ast/src/builder.rs`. Arena IDs
are sequential `la_arena` indices, so producing a byte-identical `AstArena` requires
allocating every node in the exact same order `Builder` did. Key ordering rules:
function arguments alloc type then name; `return;` with no expression still allocates a
`UnitLiteral`; binary expressions alloc left then right; `SourceFile` is allocated last
with a location spanning `0..src.len()`.

### Precedence table

From `grammar.js PRECEDENCE`, reproduced verbatim (highest binds tightest):

| Operators | Level | Notes |
|-----------|-------|-------|
| `.` member, `::` type-member, `[` index, `(` call | 2000 / 1500 | postfix |
| `! - ~` | 1000 | prefix unary |
| `**` | 990 | right-associative |
| `* / %` | 980 | |
| `+ -` | 970 | |
| `<< >>` | 800 | |
| `< <= > >=` | 700 | |
| `== !=` | 600 | |
| `&` | 590 | |
| `^` | 580 | |
| `\|` | 570 | |
| `&&` | 490 | |
| `\|\|` | 480 | |

Assignment (`=`) is a statement (`assign_statement`), not an expression operator.

## Testing

The crate contains **135 unit tests** distributed across the lexer, engine, grammar, and
syntax tree modules. Test coverage includes per-token-class lexer round-trips, joint-bit
edge cases (`-42` vs `- 42`, `Vec i32'`, `a::b`), grammar CST-shape assertions for every
construct, precedence-climb fixtures, struct-vs-block disambiguation, and resilience tests
(malformed inputs must reach EOF with scoped ERROR nodes and never panic).

The migration was guarded by a byte-exact equivalence oracle (`assert_parsers_agree`)
that ran both parsers over the full corpus — 161 source files — and asserted identical
`AstArena` output before tree-sitter was removed. At cutover the oracle was retired; the
entire existing test suite (AST builder tests, four-tier codegen golden files, Wasmtime
execution tests) now runs against this parser with zero golden-file edits, providing
end-to-end equivalence proof.

## Scope / Not Included

- No incremental reparsing: every call to `parse` processes the full source from scratch.
- The owned `SyntaxNode` CST is internal to this crate; it is not a public red/green tree.
- The Inference language has no generics, traits, closures, or lifetimes — the grammar is
  intentionally small and the parser does not need to handle them.
- Richer diagnostic surfaces (labels, notes, quick-fixes) are deferred; the structured
  `ParseError { span, message }` substrate is in place for future IDE work.

## Related Resources

- [`core/ast`](../ast/README.md) — `AstArena`, node types, and arena IDs that this crate produces
- [`core/inference`](../inference/README.md) — public `parse()` entry that delegates to this crate
- matklad, [Resilient LL Parsing Tutorial (2023)](https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html)
- matklad, [Parsing Advances (2025)](https://matklad.github.io/2025/12/28/parsing-advances.html)
- Inference grammar (source of truth): [tree-sitter-inference grammar.js](https://github.com/Inferara/tree-sitter-inference/blob/main/grammar.js)
