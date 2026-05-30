//! Hand-written, resilient parser for the Inference language.
//!
//! This crate replaces the `tree-sitter` + `tree-sitter-inference` front end
//! with a recursive-descent parser built on the rust-analyzer parser
//! architecture and matklad's "parsing advances" loop-progress guarantee.
//!
//! # Architecture
//!
//! ```text
//! .inf source ──► lexer ──► tokens ──► parser (events) ──► owned CST ──► lower ──► AstArena
//! ```
//!
//! - **Lexer**: hand-written, trivia-aware, produces a flat token stream with
//!   byte spans and joint bits (for `::` / `'` immediacy and operator gluing).
//! - **Parser**: event-based recursive descent with `Marker`s, a fuel counter
//!   and advance assertions so a stuck recovery loop fails loudly instead of
//!   looping forever.
//! - **Owned CST**: a simple immutable tree, internal to this crate, produced
//!   from the parser events with trivia re-attached.
//! - **Lowering**: walks the CST and allocates `inference_ast::arena::AstArena`
//!   nodes, producing an arena byte-identical to the one the legacy `Builder`
//!   produced from a tree-sitter CST.
//!
//! Parsing is **resilient**: it never panics on malformed input. It always
//! returns a [`Parse`] holding an `AstArena` plus a `Vec<ParseError>`; syntax
//! errors are collected rather than aborting the parse.

mod errors;
mod event;
mod grammar;
mod input;
mod lexer;
mod lower;
mod parser;
mod syntax_kind;
mod syntax_tree;
mod token_set;

pub use errors::{ParseError, ParserError};
pub use event::{Event, Step, process};
pub use input::Input;
pub use lexer::{Token, tokenize};
pub use parser::{CompletedMarker, Marker, Parser};
pub use syntax_kind::SyntaxKind;
pub use syntax_tree::{SyntaxElement, SyntaxNode, build_tree};
pub use token_set::TokenSet;

use inference_ast::arena::AstArena;

/// The result of parsing a source string.
///
/// Holds the produced AST arena together with any structured syntax errors
/// collected during a resilient parse.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "a parse result carries both the arena and any syntax errors"]
pub struct Parse {
    /// The arena of AST nodes produced for the source.
    pub arena: AstArena,
    /// Structured syntax errors collected during parsing.
    pub errors: Vec<ParseError>,
}

/// Parses an Inference source string into an [`AstArena`] plus syntax errors.
///
/// This is the public entry point and the drop-in replacement for the legacy
/// tree-sitter parse path. It is resilient and never panics on malformed input.
///
/// The pipeline runs `tokenize → grammar → owned CST → lower`, producing an
/// `AstArena` byte-identical to the legacy tree-sitter `Builder` on valid input
/// (issue #62, design §0). Syntax errors from parsing and any from lowering are
/// merged into the returned [`Parse`].
pub fn parse(src: &str) -> Parse {
    let (tree, mut errors) = parse_to_cst(src);
    let (arena, lower_errors) = lower::Lowering::new(src).lower(&tree);
    errors.extend(lower_errors);
    Parse { arena, errors }
}

/// Parses `src` into the owned concrete syntax tree plus structured syntax
/// errors, for testing the grammar's CST shape and recovery directly.
///
/// This is the seam Phase 5 lowering builds on: it exposes the [`SyntaxNode`]
/// the grammar produces, with trivia re-attached, before any AST lowering.
#[must_use]
pub fn parse_to_cst(src: &str) -> (SyntaxNode, Vec<ParseError>) {
    let tokens = tokenize(src);
    let input = Input::new(&tokens);
    let mut parser = Parser::new(&input);
    grammar::source_file(&mut parser);
    let steps = process(parser.finish());
    let errors = collect_errors(&tokens, &steps);
    let tree = build_tree(&tokens, steps);
    (tree, errors)
}

/// Assigns each [`Step::Error`] a source [`Location`] by tracking the token
/// cursor through the step stream: an error attaches to the next meaningful
/// token it precedes, or to the end-of-input sentinel when none remains.
fn collect_errors(tokens: &[Token], steps: &[Step]) -> Vec<ParseError> {
    let mut errors = Vec::new();
    let mut cursor = 0usize;
    for step in steps {
        match step {
            Step::Token => {
                cursor = next_meaningful(tokens, cursor) + 1;
            }
            Step::Error(message) => {
                let at = next_meaningful(tokens, cursor);
                let span = tokens
                    .get(at)
                    .or_else(|| tokens.last())
                    .map(|t| t.loc)
                    .unwrap_or_default();
                errors.push(ParseError {
                    span,
                    message: message.clone(),
                });
            }
            Step::Enter(_) | Step::Leave => {}
        }
    }
    errors
}

/// The index of the next non-trivia token at or after `from`, clamped to the
/// stream length.
fn next_meaningful(tokens: &[Token], from: usize) -> usize {
    let mut i = from;
    while i < tokens.len() && tokens[i].kind.is_trivia() {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::parse;

    /// The full public [`parse`] pipeline must never panic on any input —
    /// including malformed sources whose error-recovery CSTs leave a required
    /// child absent (issue #62, design §8). Earlier the lowering stage called
    /// `.expect()` on such children and aborted the whole parse; this corpus
    /// drives every adversarial input through `parse` under `catch_unwind` and
    /// asserts that none unwind.
    ///
    /// The `fuzz_lite_never_panics` test in `grammar.rs` covers only the CST
    /// stage (`parse_to_cst`), so these lowering panics slipped through; this
    /// test closes that gap by exercising the lowering stage too.
    #[test]
    fn parse_never_panics_on_adversarial_input() {
        // The seven inputs originally confirmed to panic through `parse`, each
        // exercising a CST whose required lowering child is absent:
        // member/type-member access with no name, a qualified name with no
        // trailing name, and an array type with no element.
        let confirmed_panics = [
            "fn f() { a. }",
            "fn f() { a.; }",
            "fn f() { x = a.; }",
            "fn f() { -a. }",
            "fn f() { a:: }",
            "fn f() { x = a::; }",
            "fn f() { let x: [ = 0; }",
        ];

        // A broad garbage set: truncated items, dangling operators, empty or
        // partial constructs, random bytes, and large repetitive strings. None
        // may panic.
        let truncated_items = [
            "fn",
            "spec",
            "struct S {",
            "fn f(",
            "enum E {",
            "use ;",
            "type T =",
        ];
        let dangling_operators = ["fn f() { a + }", "fn f() { !; }"];
        // EOF-truncated operands: `err_recover` at end of input records an error
        // without emitting an `Error` node, so the operand slot is genuinely
        // absent (no node fills it) — a stricter case than `… }` forms above.
        let truncated_operands = [
            "fn f() { a +",
            "fn f() { -",
            "fn f() { ~",
            "fn f() { (",
            "fn f() { a[",
            "fn f() { x =",
            "fn f() { assert",
            "fn f() { a.",
            "fn f() { a::",
            "fn f() { let x: [",
        ];
        let partial_constructs = [
            "fn f() { let x: ; }",
            "fn f() { g(a: ); }",
            "fn f() { S { a: }; }",
            "fn f() { a:: }",
            "fn f() { a. }",
        ];
        let random_bytes = ["@#$%^&*", ";;;;", "}{}{", "::::", "''''", "[[[["];
        let large_repetitive = [
            "(".repeat(500),
            "a.".repeat(500),
            // Deeply nested unterminated blocks reach EOF and then unwind, closing
            // one node per frame while only peeking at the `Eof` sentinel — the
            // case the advance-guard fuel must not mistake for a non-advancing
            // spin (see `parser` module docs).
            "fn f(){".repeat(200),
            "fn f() { if true {".repeat(200),
            "[".repeat(500),
            "a::".repeat(500),
        ];

        let mut corpus: Vec<String> = Vec::new();
        corpus.extend(confirmed_panics.iter().map(|s| (*s).to_string()));
        corpus.extend(truncated_items.iter().map(|s| (*s).to_string()));
        corpus.extend(dangling_operators.iter().map(|s| (*s).to_string()));
        corpus.extend(truncated_operands.iter().map(|s| (*s).to_string()));
        corpus.extend(partial_constructs.iter().map(|s| (*s).to_string()));
        corpus.extend(random_bytes.iter().map(|s| (*s).to_string()));
        corpus.extend(large_repetitive);

        let mut panicked: Vec<String> = Vec::new();
        for src in &corpus {
            let result = std::panic::catch_unwind(|| {
                let _ = parse(src);
            });
            if result.is_err() {
                panicked.push(src.clone());
            }
        }
        assert!(
            panicked.is_empty(),
            "parse() panicked on {} input(s): {:?}",
            panicked.len(),
            panicked
        );
    }
}
