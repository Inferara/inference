//! Resilient parser for the Inference language.
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
//! - **Lexer**: trivia-aware, produces a flat token stream with
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

/// Parses `src` as one file of a multi-file program, lowering its nodes into the
/// existing `arena` under the namespace named by `module_path`.
///
/// This is the seam a project front end uses to fold every reachable file into a
/// single arena. The first file lowered should carry an empty `module_path` (the
/// entry); each subsequent file carries its source-root-relative segments
/// (e.g. `["lib", "arith"]`). The parser performs no filesystem access: the
/// caller reads each file and decides its module path.
///
/// The arena is moved in and returned inside the [`Parse`] so the next file can
/// be lowered into it, accumulating all files. Syntax errors for this file are
/// returned in `errors`; the caller is responsible for aggregating them.
pub fn parse_into(arena: AstArena, src: &str, module_path: Vec<String>) -> Parse {
    let (tree, mut errors) = parse_to_cst(src);
    let (arena, lower_errors) = lower::Lowering::into_arena(arena, src, module_path).lower(&tree);
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

/// Parser-level (filesystem-free) tests for [`parse_into`]: folding several
/// files into one arena, module-path stamping, per-file attribution of defs and
/// directives, and node-id non-collision. These exercise the seam the
/// `core/inference` project front end is built on without touching disk.
#[cfg(test)]
mod parse_into_tests {
    use super::{parse, parse_into};
    use inference_ast::arena::AstArena;
    use inference_ast::nodes::Directive;

    /// The `::`-joined segments of a path-form `use` directive.
    fn use_path(arena: &AstArena, directive: &Directive) -> String {
        let Directive::Use(use_dir) = directive;
        use_dir
            .segments
            .iter()
            .map(|&id| arena.ident_name(id))
            .collect::<Vec<_>>()
            .join("::")
    }

    #[test]
    fn single_parse_stamps_entry_identity() {
        // AD-16: the string-based `parse` always yields a single entry file with
        // an empty module path.
        let parsed = parse("pub fn main() -> i32 { return 0; }");

        assert!(parsed.errors.is_empty());
        let files: Vec<_> = parsed.arena.source_files().collect();
        assert_eq!(files.len(), 1);
        assert!(files[0].is_entry());
        assert!(files[0].module_path.is_empty());
    }

    #[test]
    fn two_files_fold_into_one_arena() {
        // Lower an entry then an imported file into the SAME arena; both
        // `SourceFileData` entries coexist with their own module paths.
        let entry = parse_into(AstArena::default(), "pub fn main() {}", Vec::new());
        assert!(entry.errors.is_empty());

        let both = parse_into(
            entry.arena,
            "pub fn add(a: i32, b: i32) -> i32 { return a + b; }",
            vec!["lib".to_string(), "arith".to_string()],
        );
        assert!(both.errors.is_empty());

        let files: Vec<_> = both.arena.source_files().collect();
        assert_eq!(files.len(), 2, "both files live in one arena");
        assert!(files[0].is_entry(), "first file is the entry");
        assert_eq!(
            files[1].module_path,
            vec!["lib".to_string(), "arith".to_string()],
        );
        assert!(!files[1].is_entry());
    }

    #[test]
    fn defs_attributed_to_their_own_file() {
        // Each file's definitions belong to that file's `SourceFileData.defs`,
        // not the other's.
        let entry = parse_into(
            AstArena::default(),
            "pub fn main() {}\nfn helper() {}",
            Vec::new(),
        );
        let arena = parse_into(
            entry.arena,
            "pub fn add(a: i32, b: i32) -> i32 { return a + b; }",
            vec!["lib".to_string(), "arith".to_string()],
        )
        .arena;

        let files: Vec<_> = arena.source_files().collect();

        let entry_names: Vec<&str> = files[0]
            .defs
            .iter()
            .map(|&def_id| arena.def_name(def_id))
            .collect();
        assert_eq!(entry_names, vec!["main", "helper"]);

        let lib_names: Vec<&str> = files[1]
            .defs
            .iter()
            .map(|&def_id| arena.def_name(def_id))
            .collect();
        assert_eq!(lib_names, vec!["add"]);
    }

    #[test]
    fn directives_attributed_to_their_own_file() {
        // A `use` in the entry and a different `use` in the imported file each
        // attach to the correct `SourceFileData.directives`.
        let entry = parse_into(
            AstArena::default(),
            "use math;\npub fn main() {}",
            Vec::new(),
        );
        let arena = parse_into(
            entry.arena,
            "use lib::arith;\npub fn foo() {}",
            vec!["math".to_string()],
        )
        .arena;

        let files: Vec<_> = arena.source_files().collect();

        assert_eq!(files[0].directives.len(), 1);
        assert_eq!(use_path(&arena, &files[0].directives[0]), "math");

        assert_eq!(files[1].directives.len(), 1);
        assert_eq!(use_path(&arena, &files[1].directives[0]), "lib::arith");
    }

    #[test]
    fn node_ids_do_not_collide_across_files() {
        // Two files each defining a function named `f` with identical bodies must
        // still allocate DISTINCT def ids — folding into one arena must not
        // alias nodes by name or shape.
        let body = "pub fn f() -> i32 { return 7; }";
        let entry = parse_into(AstArena::default(), body, Vec::new());
        let arena = parse_into(entry.arena, body, vec!["other".to_string()]).arena;

        let files: Vec<_> = arena.source_files().collect();
        let id_a = files[0].defs[0];
        let id_b = files[1].defs[0];

        assert_ne!(
            id_a, id_b,
            "same-named, same-bodied functions in two files get distinct ids"
        );
        // Both ids index real, independent definitions in the shared arena.
        assert_eq!(arena.def_name(id_a), "f");
        assert_eq!(arena.def_name(id_b), "f");
    }

    #[test]
    fn empty_module_path_into_arena_stamps_entry() {
        // `parse_into` with an empty module path is the explicit entry form; it
        // matches the implicit entry identity of `parse`.
        let parsed = parse_into(AstArena::default(), "pub fn main() {}", Vec::new());
        let files: Vec<_> = parsed.arena.source_files().collect();
        assert_eq!(files.len(), 1);
        assert!(files[0].is_entry());
    }

    #[test]
    fn three_files_preserve_insertion_order_and_paths() {
        // `parse_into` stores files in call order (the project front end sorts
        // before lowering; here we pin that the parser itself preserves the
        // order it is handed and stamps each path verbatim).
        let a = parse_into(AstArena::default(), "pub fn main() {}", Vec::new());
        let b = parse_into(a.arena, "pub fn fa() {}", vec!["a".to_string()]);
        let c = parse_into(
            b.arena,
            "pub fn fb() {}",
            vec!["lib".to_string(), "b".to_string()],
        );

        let paths: Vec<Vec<String>> = c
            .arena
            .source_files()
            .map(|sf| sf.module_path.clone())
            .collect();
        assert_eq!(
            paths,
            vec![
                Vec::<String>::new(),
                vec!["a".to_string()],
                vec!["lib".to_string(), "b".to_string()],
            ],
        );
    }
}
