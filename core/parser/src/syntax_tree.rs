//! The owned, lossless concrete syntax tree (CST) and its builder (design §7).
//!
//! Unlike rust-analyzer we do not use `rowan`; the CST is a plain owned tree of
//! [`SyntaxNode`]s and [`Token`]s. [`build_tree`] consumes the processed parser
//! [`Step`]s together with the *full* lexer stream (trivia included) and weaves
//! the trivia back in as [`SyntaxElement::Token`] children, so the tree is
//! **lossless**: a depth-first walk over every token child reproduces the source
//! byte-for-byte.
//!
//! # Node locations
//!
//! A [`SyntaxNode`]'s [`Location`] spans its first..last **non-trivia** descendant
//! token, exactly like tree-sitter node spans (extras excluded). A node with no
//! non-trivia descendant gets a zero-width location at the offset where it sits.
//! This parity matters: Phase 5 lowering computes AST node locations from CST node
//! locations and must match the legacy `builder.rs` byte-for-byte (design §0).

use inference_ast::nodes::Location;

use crate::event::Step;
use crate::lexer::Token;
use crate::syntax_kind::SyntaxKind;

/// A child of a [`SyntaxNode`]: either an interior node or a leaf token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxElement {
    /// An interior node.
    Node(SyntaxNode),
    /// A leaf token (including re-attached trivia).
    Token(Token),
}

/// An interior node of the owned CST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxNode {
    /// The node's syntactic kind (always a node kind, never a token kind).
    pub kind: SyntaxKind,
    /// Source span of the node's first..last non-trivia descendant token.
    pub loc: Location,
    /// The node's children in source order, trivia included.
    pub children: Vec<SyntaxElement>,
}

/// Discards the subtree iteratively, so dropping a tree costs one frame however
/// tall the tree is.
///
/// The implicit drop glue would descend once per level, which turns merely
/// *discarding* a deeply nested tree into a stack overflow — and an overflow aborts
/// the process rather than unwinding, so no caller can contain it. A deep tree is
/// most often exactly the one a caller wants to throw away. Detaching the children
/// into a worklist and draining it holds the depth at one: each node reached this
/// way has already had its children moved out, so its own drop finds nothing to
/// descend into. That makes any tree safe to discard on any thread, including a
/// bare test thread with the platform's default stack.
///
/// This makes a tree safe to *discard*, not safe to *build*. Two independent
/// height-recursions remain on the construction path, and together they bind sooner
/// than the drop glue did for a parsed tree. The grammar's own recursive descent is
/// one. The other is `Builder::leave`, which resolves every closed node's span
/// through `first_non_trivia` and `last_non_trivia`; those stop at the first
/// non-trivia *token*, so they cost nothing for a shape whose extremal children are
/// tokens — a parenthesized nest, whose parentheses bound both ends — but descend
/// the subtree's full height for one whose extremal children are nodes, such as a
/// left-leaning operator spine. Bounding either side takes a limit on the depth the
/// grammar will descend to, which does not exist yet; until it does, a tree deep
/// enough to need this drop can only be built directly rather than parsed.
///
/// The remaining height-recursive operations, none of which may be applied to a tree
/// of unbounded height, are `first_descendant_loc`, `debug_into`, and the derived
/// `Clone`, `PartialEq` and `Debug` on both this type and [`SyntaxElement`].
impl Drop for SyntaxNode {
    fn drop(&mut self) {
        let mut worklist = std::mem::take(&mut self.children);
        while let Some(element) = worklist.pop() {
            if let SyntaxElement::Node(mut node) = element {
                worklist.append(&mut node.children);
            }
        }
    }
}

/// Builds the owned CST from processed parser steps and the full token stream.
///
/// `tokens` is the complete lossless lexer output (trivia included, terminated by
/// `Eof`); `steps` is [`crate::event::process`]'s output. Token steps are matched
/// against the stream in order, and any trivia tokens preceding each consumed
/// token are attached as leaf children at the position they occur. The returned
/// root has kind [`SyntaxKind::SourceFile`].
#[must_use]
pub fn build_tree(tokens: &[Token], steps: Vec<Step>) -> SyntaxNode {
    Builder::new(tokens).build(steps)
}

/// Stack-based tree assembler tracking the open node path and the token cursor.
struct Builder<'t> {
    tokens: &'t [Token],
    /// Index of the next *original* token to attach.
    cursor: usize,
    /// Stack of nodes currently open, root at the bottom.
    stack: Vec<SyntaxNode>,
}

impl<'t> Builder<'t> {
    fn new(tokens: &'t [Token]) -> Builder<'t> {
        Builder {
            tokens,
            cursor: 0,
            stack: Vec::new(),
        }
    }

    fn build(mut self, steps: Vec<Step>) -> SyntaxNode {
        for step in steps {
            match step {
                Step::Enter(kind) => self.enter(kind),
                Step::Leave => self.leave(),
                Step::Token => self.token(),
                Step::Error(_) => {
                    // Diagnostics are collected by the parse pipeline, not the
                    // tree; they leave no node in the lossless CST.
                }
            }
        }
        // Attach any trailing trivia (and the Eof) to the still-open root.
        self.attach_trivia_run(self.tokens.len());
        // The grammar opens exactly one root; pop it. Defensive fallback keeps
        // the builder total even on a malformed step stream.
        while self.stack.len() > 1 {
            self.leave();
        }
        self.stack.pop().unwrap_or_else(|| SyntaxNode {
            kind: SyntaxKind::SourceFile,
            loc: Location::default(),
            children: Vec::new(),
        })
    }

    /// Opens a node. Pending trivia is *not* flushed here: it is attached when
    /// the next real token is consumed, so it lands in the innermost open node.
    /// A node's [`Location`] is computed from its non-trivia descendants only, so
    /// where the trivia attaches does not affect spans, only losslessness.
    fn enter(&mut self, kind: SyntaxKind) {
        self.stack.push(SyntaxNode {
            kind,
            loc: Location::default(),
            children: Vec::new(),
        });
    }

    /// Closes the innermost node, computing its location from its non-trivia span
    /// and folding it into its parent (or leaving it as the result root).
    fn leave(&mut self) {
        let mut node = match self.stack.pop() {
            Some(node) => node,
            None => return,
        };
        node.loc = self.node_location(&node);
        match self.stack.last_mut() {
            Some(parent) => parent.children.push(SyntaxElement::Node(node)),
            None => self.stack.push(node),
        }
    }

    /// Attaches the next meaningful token (preceded by any trivia) to the
    /// innermost open node.
    fn token(&mut self) {
        let at = self.next_meaningful();
        self.attach_trivia_run(at);
        if at < self.tokens.len() {
            self.push_token(self.tokens[at].clone());
            self.cursor = at + 1;
        }
    }

    /// Attaches every trivia token from the cursor up to (not including) `limit`.
    fn attach_trivia_run(&mut self, limit: usize) {
        while self.cursor < limit && self.cursor < self.tokens.len() {
            let token = &self.tokens[self.cursor];
            if !token.kind.is_trivia() {
                break;
            }
            self.push_token(token.clone());
            self.cursor += 1;
        }
    }

    /// Pushes a leaf token into the innermost open node (or drops it if none is
    /// open, which only happens for trailing trivia before the root is created).
    fn push_token(&mut self, token: Token) {
        if let Some(top) = self.stack.last_mut() {
            top.children.push(SyntaxElement::Token(token));
        }
    }

    /// The original index of the next non-trivia token at or after the cursor, or
    /// `tokens.len()` if none remain.
    fn next_meaningful(&self) -> usize {
        let mut i = self.cursor;
        while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
            i += 1;
        }
        i
    }

    /// Computes a node's location: the span from its first to its last non-trivia
    /// descendant token. With no such token, a zero-width location anchored at the
    /// node's first child (or at the current cursor) — matching tree-sitter empty
    /// nodes.
    fn node_location(&self, node: &SyntaxNode) -> Location {
        let first = first_non_trivia(node);
        let last = last_non_trivia(node);
        match (first, last) {
            (Some(first), Some(last)) => Location::new(
                first.offset_start,
                last.offset_end,
                first.start_line,
                first.start_column,
                last.end_line,
                last.end_column,
            ),
            _ => self.empty_location(node),
        }
    }

    /// A zero-width location for a node with no non-trivia descendant, anchored at
    /// the node's first descendant token if any, else the current token cursor.
    fn empty_location(&self, node: &SyntaxNode) -> Location {
        if let Some(loc) = first_descendant_loc(node) {
            return Location::new(
                loc.offset_start,
                loc.offset_start,
                loc.start_line,
                loc.start_column,
                loc.start_line,
                loc.start_column,
            );
        }
        let anchor = self
            .tokens
            .get(self.cursor)
            .or_else(|| self.tokens.last())
            .map(|t| t.loc)
            .unwrap_or_default();
        Location::new(
            anchor.offset_start,
            anchor.offset_start,
            anchor.start_line,
            anchor.start_column,
            anchor.start_line,
            anchor.start_column,
        )
    }
}

/// The location of the first non-trivia descendant token of `node`.
fn first_non_trivia(node: &SyntaxNode) -> Option<Location> {
    for child in &node.children {
        match child {
            SyntaxElement::Token(t) if !t.kind.is_trivia() => return Some(t.loc),
            SyntaxElement::Node(n) => {
                if let Some(loc) = first_non_trivia(n) {
                    return Some(loc);
                }
            }
            SyntaxElement::Token(_) => {}
        }
    }
    None
}

/// The location of the last non-trivia descendant token of `node`.
fn last_non_trivia(node: &SyntaxNode) -> Option<Location> {
    for child in node.children.iter().rev() {
        match child {
            SyntaxElement::Token(t) if !t.kind.is_trivia() => return Some(t.loc),
            SyntaxElement::Node(n) => {
                if let Some(loc) = last_non_trivia(n) {
                    return Some(loc);
                }
            }
            SyntaxElement::Token(_) => {}
        }
    }
    None
}

/// Appends `depth` levels of two-space indentation to `out`.
fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// The location of the first descendant token of any kind (trivia included).
fn first_descendant_loc(node: &SyntaxNode) -> Option<Location> {
    for child in &node.children {
        match child {
            SyntaxElement::Token(t) => return Some(t.loc),
            SyntaxElement::Node(n) => {
                if let Some(loc) = first_descendant_loc(n) {
                    return Some(loc);
                }
            }
        }
    }
    None
}

impl SyntaxNode {
    /// The first direct child node of the given `kind`, if any.
    #[must_use]
    pub fn child(&self, kind: SyntaxKind) -> Option<&SyntaxNode> {
        self.children_of(kind).next()
    }

    /// All direct child nodes of the given `kind`, in source order.
    pub fn children_of(&self, kind: SyntaxKind) -> impl Iterator<Item = &SyntaxNode> {
        self.node_children().filter(move |n| n.kind == kind)
    }

    /// The `n`-th direct child node (of any kind), zero-based.
    #[must_use]
    pub fn nth_node(&self, n: usize) -> Option<&SyntaxNode> {
        self.node_children().nth(n)
    }

    /// The first direct child *token* of the given `kind`, if any.
    #[must_use]
    pub fn child_token(&self, kind: SyntaxKind) -> Option<&Token> {
        self.children.iter().find_map(|c| match c {
            SyntaxElement::Token(t) if t.kind == kind => Some(t),
            _ => None,
        })
    }

    /// The first direct child token whose kind is any of `kinds`, in source
    /// order. Useful where a position may hold one of several token kinds (e.g.
    /// the primitive type keywords).
    #[must_use]
    pub fn first_token_of_any(&self, kinds: &[SyntaxKind]) -> Option<&Token> {
        self.children.iter().find_map(|c| match c {
            SyntaxElement::Token(t) if kinds.contains(&t.kind) => Some(t),
            _ => None,
        })
    }

    /// The source text this node spans, sliced from `src` by the node's byte
    /// offsets.
    #[must_use]
    pub fn text<'s>(&self, src: &'s str) -> &'s str {
        let start = self.loc.offset_start as usize;
        let end = self.loc.offset_end as usize;
        src.get(start..end).unwrap_or("")
    }

    /// The direct child nodes, in source order (tokens skipped).
    pub fn node_children(&self) -> impl Iterator<Item = &SyntaxNode> {
        self.children.iter().filter_map(|c| match c {
            SyntaxElement::Node(n) => Some(n),
            SyntaxElement::Token(_) => None,
        })
    }

    /// Renders an indented S-expression of the subtree for snapshot tests:
    /// `KIND@start..end "text"` per line, two spaces per depth level.
    #[must_use]
    pub fn debug_tree(&self, src: &str) -> String {
        let mut out = String::new();
        self.debug_into(src, 0, &mut out);
        out
    }

    fn debug_into(&self, src: &str, depth: usize, out: &mut String) {
        use std::fmt::Write;

        indent(out, depth);
        let _ = writeln!(
            out,
            "{:?}@{}..{}",
            self.kind, self.loc.offset_start, self.loc.offset_end
        );
        for child in &self.children {
            match child {
                SyntaxElement::Node(n) => n.debug_into(src, depth + 1, out),
                SyntaxElement::Token(t) => {
                    indent(out, depth + 1);
                    let text = src
                        .get(t.loc.offset_start as usize..t.loc.offset_end as usize)
                        .unwrap_or("");
                    let _ = writeln!(
                        out,
                        "{:?}@{}..{} {:?}",
                        t.kind, t.loc.offset_start, t.loc.offset_end, text
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::process;
    use crate::input::Input;
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    /// The Phase 2 throwaway rule: a `Number` as `NumberLiteral` inside an
    /// `ExpressionStatement` under a `SourceFile`.
    fn parse_number(p: &mut Parser) {
        let file = p.start();
        if p.at(SyntaxKind::Number) {
            let stmt = p.start();
            let lit = p.start();
            p.bump(SyntaxKind::Number);
            lit.complete(p, SyntaxKind::NumberLiteral);
            stmt.complete(p, SyntaxKind::ExpressionStatement);
        }
        file.complete(p, SyntaxKind::SourceFile);
    }

    fn build(src: &str) -> (Vec<Token>, SyntaxNode) {
        let toks = tokenize(src);
        let input = Input::new(src, &toks);
        let mut p = Parser::new(&input);
        parse_number(&mut p);
        let steps = process(p.finish());
        let tree = build_tree(&toks, steps);
        (toks, tree)
    }

    /// Depth-first concatenation of every leaf token's source slice.
    fn reassemble(node: &SyntaxNode, src: &str) -> String {
        let mut out = String::new();
        collect(node, src, &mut out);
        out
    }

    fn collect(node: &SyntaxNode, src: &str, out: &mut String) {
        for child in &node.children {
            match child {
                SyntaxElement::Node(n) => collect(n, src, out),
                SyntaxElement::Token(t) => {
                    out.push_str(&src[t.loc.offset_start as usize..t.loc.offset_end as usize]);
                }
            }
        }
    }

    #[test]
    fn smoke_number_tree_shape_and_locations() {
        let (_toks, tree) = build("42");
        assert_eq!(tree.kind, SyntaxKind::SourceFile);
        let stmt = tree.child(SyntaxKind::ExpressionStatement).unwrap();
        let lit = stmt.child(SyntaxKind::NumberLiteral).unwrap();
        // The Number node covers bytes 0..2.
        assert_eq!(lit.loc.offset_start, 0);
        assert_eq!(lit.loc.offset_end, 2);
        let num = lit.child_token(SyntaxKind::Number).unwrap();
        assert_eq!(num.loc.offset_start, 0);
        assert_eq!(num.loc.offset_end, 2);
    }

    #[test]
    fn debug_tree_renders_indented_sexpr() {
        let (_toks, tree) = build("42");
        let rendered = tree.debug_tree("42");
        let expected = "\
SourceFile@0..2
  ExpressionStatement@0..2
    NumberLiteral@0..2
      Number@0..2 \"42\"
";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn tree_is_lossless_with_trivia() {
        // Leading/trailing whitespace and a comment must survive as token leaves.
        let src = "  // note\n  42  ";
        let (_toks, tree) = build(src);
        assert_eq!(
            reassemble(&tree, src),
            src,
            "tree must reproduce the source"
        );
    }

    #[test]
    fn node_span_excludes_surrounding_trivia() {
        let src = "  42  ";
        let (_toks, tree) = build(src);
        let stmt = tree.child(SyntaxKind::ExpressionStatement).unwrap();
        // The statement spans only `42` (bytes 2..4), not the surrounding spaces.
        assert_eq!(stmt.loc.offset_start, 2);
        assert_eq!(stmt.loc.offset_end, 4);
    }

    #[test]
    fn text_slices_by_node_offsets() {
        let src = "  42  ";
        let (_toks, tree) = build(src);
        let lit = tree
            .child(SyntaxKind::ExpressionStatement)
            .and_then(|s| s.child(SyntaxKind::NumberLiteral))
            .unwrap();
        assert_eq!(lit.text(src), "42");
    }

    #[test]
    fn empty_source_yields_zero_width_root() {
        let (_toks, tree) = build("");
        assert_eq!(tree.kind, SyntaxKind::SourceFile);
        assert_eq!(tree.loc.offset_start, 0);
        assert_eq!(tree.loc.offset_end, 0);
        // No statement child for empty input.
        assert!(tree.child(SyntaxKind::ExpressionStatement).is_none());
    }

    /// A tower of `depth` single-child nodes under a `SourceFile` root.
    ///
    /// Built directly rather than through [`build`], whose throwaway rule cannot
    /// produce nesting, and rather than through the real grammar, which would
    /// make the test measure parsing rather than dropping. Every level is a real
    /// [`SyntaxNode`], which is all the drop glue under test sees.
    fn nest(depth: usize) -> SyntaxNode {
        let mut node = SyntaxNode {
            kind: SyntaxKind::SourceFile,
            loc: Location::default(),
            children: Vec::new(),
        };
        for _ in 0..depth {
            node = SyntaxNode {
                kind: SyntaxKind::ParenthesizedExpression,
                loc: Location::default(),
                children: vec![SyntaxElement::Node(node)],
            };
        }
        node
    }

    /// The height of a single-child tower, counted without recursing.
    fn tower_height(root: &SyntaxNode) -> usize {
        let mut height = 0;
        let mut cursor = root;
        while let Some(SyntaxElement::Node(child)) = cursor.children.first() {
            height += 1;
            cursor = child;
        }
        height
    }

    /// Dropping a tree costs one frame regardless of its height.
    ///
    /// The derived drop glue descended once per level, so merely discarding a
    /// tree a few thousand levels tall aborted the process — and an abort cannot
    /// be caught, so the only way to assert the iterative drop is to perform it
    /// on a thread whose stack is too small for the recursive one and require
    /// the thread to finish. The stack here is the platform default a bare
    /// thread gets; the height is two orders of magnitude past the roughly 8,200
    /// levels the recursive drop managed on it.
    ///
    /// `Clone`, `PartialEq` and `Debug` are still recursive, so the tree is only
    /// walked iteratively and never compared or printed.
    #[test]
    fn deep_tree_drops_on_a_default_sized_stack() {
        const DEPTH: usize = 500_000;
        const STACK_BYTES: usize = 2 * 1024 * 1024;

        let worker = std::thread::Builder::new()
            .stack_size(STACK_BYTES)
            .spawn(|| {
                let tree = nest(DEPTH);
                tower_height(&tree)
            })
            .expect("failed to spawn the drop-test thread");

        assert_eq!(
            worker.join().ok(),
            Some(DEPTH),
            "the tree must be built to full height and dropped without the thread dying"
        );
    }
}
