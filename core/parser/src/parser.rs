//! The recursive-descent parser engine: a cursor over [`Input`] that emits
//! [`Event`]s through [`Marker`]s (issue #62 design §5).
//!
//! The engine itself knows no grammar. It offers lookahead (`current`, `nth`,
//! `at`, `at_ts`), consumption (`bump`, `eat`, `expect`), node framing (`start`,
//! [`Marker::complete`], [`CompletedMarker::precede`]) and error recovery
//! (`error`, `err_and_bump`, `err_recover`). The grammar (Phase 3/4) is written
//! against this surface; a throwaway rule here drives the Phase 2 smoke test.
//!
//! # Advance guard (matklad AD-6)
//!
//! Resilient parsers loop until they make progress, and a logic bug can leave a
//! recovery loop spinning forever. To turn that into a loud, immediate failure in
//! development, the engine carries a [`fuel`](Parser) counter: every lookahead
//! decrements it and every unit of *progress* refills it. If the fuel hits zero —
//! meaning many lookaheads happened with no progress — a debug assertion fires.
//! On real input the parser always advances, so the guard never trips; it exists
//! purely as a development backstop.
//!
//! Progress is either consuming a token (`bump`) **or** completing a node
//! ([`Marker::complete`]). Completing a node counts because a deeply nested but
//! well-founded parse — e.g. hundreds of unterminated `fn f() {` blocks — reaches
//! end of input and then *unwinds*, closing one node per frame while peeking (but
//! not consuming) at the `Eof` sentinel. That unwind does many lookaheads at a
//! fixed cursor yet is strictly terminating, so it must not be mistaken for a
//! spin; each closed node refills the fuel. A true spin neither bumps nor
//! completes, so it still depletes the fuel and trips the guard.

use std::cell::Cell;

use crate::event::Event;
use crate::input::Input;
use crate::syntax_kind::SyntaxKind;
use crate::token_set::TokenSet;

/// Initial and refill value for the advance-guard fuel (design §5).
const FUEL: u32 = 256;

/// A cursor over [`Input`] that produces a parser [`Event`] stream.
pub struct Parser<'i> {
    input: &'i Input<'i>,
    /// Current meaningful-token position.
    pos: usize,
    /// Events emitted so far, in order.
    events: Vec<Event>,
    /// Advance-guard fuel: decremented on lookahead, refilled on `bump`.
    fuel: Cell<u32>,
}

impl<'i> Parser<'i> {
    /// Creates a parser positioned at the start of `input`.
    #[must_use]
    pub fn new(input: &'i Input<'i>) -> Parser<'i> {
        Parser {
            input,
            pos: 0,
            events: Vec::new(),
            fuel: Cell::new(FUEL),
        }
    }

    /// The kind of the current token (or [`SyntaxKind::Eof`] at end of input).
    #[must_use]
    pub fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    /// The kind `n` tokens ahead of the cursor.
    ///
    /// Consumes one unit of advance-guard fuel; see the module docs.
    #[must_use]
    pub fn nth(&self, n: usize) -> SyntaxKind {
        assert!(
            self.fuel.get() != 0,
            "parser stuck: too much lookahead without consuming a token"
        );
        self.fuel.set(self.fuel.get() - 1);
        self.input.nth(self.pos, n)
    }

    /// Whether the current token is `kind`.
    #[must_use]
    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.nth_at(0, kind)
    }

    /// Whether the token `n` ahead is `kind`.
    #[must_use]
    pub fn nth_at(&self, n: usize, kind: SyntaxKind) -> bool {
        self.nth(n) == kind
    }

    /// Whether the current token is a member of `set`.
    #[must_use]
    pub fn at_ts(&self, set: TokenSet) -> bool {
        set.contains(self.current())
    }

    /// Whether the current token abuts the next meaningful token with no trivia
    /// between them (the `token.immediate` predicate, design §4/§6).
    #[must_use]
    pub fn at_joint(&self) -> bool {
        self.input.is_joint(self.pos)
    }

    /// Whether the previously consumed token abuts the current one with no
    /// trivia between them.
    ///
    /// This is the `token.immediate` predicate from the *current* token's point
    /// of view — "no whitespace precedes this token" — used for glued `::` and
    /// `'` in postfix position, where the cursor already sits on the immediate
    /// token. At the start of input there is no predecessor, so it is `false`.
    #[must_use]
    pub fn prev_joint(&self) -> bool {
        self.pos
            .checked_sub(1)
            .is_some_and(|prev| self.input.is_joint(prev))
    }

    /// Whether the cursor is at end of input.
    #[must_use]
    pub fn at_eof(&self) -> bool {
        self.at(SyntaxKind::Eof)
    }

    /// The source spelling of the current token.
    ///
    /// Grammar decisions are made on [`SyntaxKind`]; this is for the rules that
    /// must also see *what the author wrote* — a diagnostic quoting the
    /// offending text, or a lexical malformation the token kinds alone do not
    /// distinguish. At end of input it is `""`.
    #[must_use]
    pub fn current_text(&self) -> &'i str {
        self.input.text(self.pos)
    }

    /// The current meaningful-token position.
    ///
    /// Used by item loops to assert forward progress: a handler that completes
    /// without consuming a token leaves this unchanged, which the loop detects
    /// and recovers from rather than spinning forever.
    #[must_use]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Consumes the current token if it is `kind`, reporting whether it did.
    pub fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.do_bump(kind);
            true
        } else {
            false
        }
    }

    /// Consumes the current token, asserting it is `kind`.
    ///
    /// Use only where the caller has already checked the kind; a mismatch is a
    /// grammar bug and panics in debug builds.
    pub fn bump(&mut self, kind: SyntaxKind) {
        assert!(
            self.at(kind),
            "bump expected {kind:?} but found {:?}",
            self.current()
        );
        self.do_bump(kind);
    }

    /// Consumes the current token whatever its kind.
    pub fn bump_any(&mut self) {
        let kind = self.current();
        if kind == SyntaxKind::Eof {
            return;
        }
        self.do_bump(kind);
    }

    /// Consumes the current token but records it under a different `kind`.
    ///
    /// Used to retag a token in context — e.g. lexing `@` as a punctuation token
    /// but recording it as the uzumaki keyword node's leaf.
    pub fn bump_remap(&mut self, kind: SyntaxKind) {
        if self.current() == SyntaxKind::Eof {
            return;
        }
        self.do_bump(kind);
    }

    /// Consumes the current token if it is `kind`; otherwise emits an error and
    /// reports `false`.
    pub fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.error(format!("expected {kind:?}"));
        false
    }

    /// Opens a new node, returning a [`Marker`] to be completed or abandoned.
    pub fn start(&mut self) -> Marker {
        let pos = self.events.len() as u32;
        self.events.push(Event::tombstone());
        Marker::new(pos)
    }

    /// Records a diagnostic at the current position without consuming a token.
    pub fn error(&mut self, msg: impl Into<String>) {
        self.events.push(Event::Error { msg: msg.into() });
    }

    /// Emits an error and consumes the offending token inside its own `Error`
    /// node, guaranteeing progress.
    pub fn err_and_bump(&mut self, msg: impl Into<String>) {
        self.err_recover(msg, TokenSet::EMPTY);
    }

    /// Emits an error and recovers: if the current token is in `recovery` (or is
    /// `Eof`), it leaves it for an enclosing rule; otherwise it wraps the
    /// offending token in an `Error` node so the parser always advances.
    pub fn err_recover(&mut self, msg: impl Into<String>, recovery: TokenSet) {
        if self.at_ts(recovery) || self.at_eof() {
            self.error(msg);
            return;
        }
        let marker = self.start();
        self.error(msg);
        self.bump_any();
        marker.complete(self, SyntaxKind::Error);
    }

    /// Consumes the current token as a leaf, refilling the advance-guard fuel.
    fn do_bump(&mut self, kind: SyntaxKind) {
        self.pos += 1;
        self.fuel.set(FUEL);
        self.events.push(Event::Token { kind });
    }

    /// Finishes parsing, returning the raw event stream for [`crate::event::process`].
    #[must_use]
    pub fn finish(self) -> Vec<Event> {
        self.events
    }
}

/// An open node: either completed into a [`CompletedMarker`] or abandoned.
///
/// A `Marker` carries a debug "bomb": dropping it without completing or
/// abandoning is a grammar bug and panics in debug builds, so every opened node
/// is accounted for.
#[must_use]
pub struct Marker {
    /// Index of this marker's tombstone `Start` event.
    pos: u32,
    /// Whether the marker was completed or abandoned (defused).
    defused: bool,
}

impl Marker {
    fn new(pos: u32) -> Marker {
        Marker {
            pos,
            defused: false,
        }
    }

    /// Completes the node as `kind`, patching its `Start` and pushing `Finish`.
    ///
    /// Completing a node is structural progress, so it refills the advance-guard
    /// fuel (see the module docs): an unwinding deep parse closes one node per
    /// frame and must not be mistaken for a non-advancing spin.
    pub fn complete(mut self, p: &mut Parser, kind: SyntaxKind) -> CompletedMarker {
        self.defused = true;
        let idx = self.pos as usize;
        match &mut p.events[idx] {
            Event::Start { kind: slot, .. } => *slot = kind,
            _ => unreachable!("marker must point at its Start event"),
        }
        p.events.push(Event::Finish);
        p.fuel.set(FUEL);
        CompletedMarker {
            pos: self.pos,
            kind,
        }
    }

    /// Discards the node, leaving its `Start` as a tombstone that
    /// [`crate::event::process`] skips.
    pub fn abandon(mut self, p: &mut Parser) {
        self.defused = true;
        let idx = self.pos as usize;
        // If this is the most recent event, pop it so the stream stays tight;
        // otherwise leave the tombstone in place for `process` to skip.
        if idx == p.events.len() - 1 {
            match p.events.pop() {
                Some(Event::Start {
                    kind: Event::TOMBSTONE,
                    forward_parent: None,
                }) => {}
                other => unreachable!("abandoned a non-tombstone marker: {other:?}"),
            }
        }
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        // A marker must be completed or abandoned; dropping it silently would
        // leak an unbalanced Start. Only enforce in debug builds and not while
        // already unwinding from another panic.
        if !self.defused && !std::thread::panicking() {
            debug_assert!(self.defused, "Marker dropped without complete/abandon");
        }
    }
}

/// A finished node, addressable so a later rule can wrap it via [`Self::precede`].
#[derive(Clone, Copy)]
pub struct CompletedMarker {
    /// Index of the completed node's `Start` event.
    pos: u32,
    /// The kind this node was completed as.
    kind: SyntaxKind,
}

impl CompletedMarker {
    /// Opens a new node that will enclose this one, via a `forward_parent` link.
    ///
    /// This is how left-associative and postfix grammar rules retroactively wrap
    /// an already-parsed operand: `lhs.precede(p)` starts the binary expression
    /// node whose first child is `lhs`.
    pub fn precede(self, p: &mut Parser) -> Marker {
        let new = p.start();
        match &mut p.events[self.pos as usize] {
            Event::Start { forward_parent, .. } => *forward_parent = Some(new.pos),
            _ => unreachable!("completed marker must point at its Start event"),
        }
        new
    }

    /// The kind this node was completed as.
    #[must_use]
    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Step, process};
    use crate::lexer::tokenize;

    /// Parses a single `Number` token as `NumberLiteral` inside an
    /// `ExpressionStatement` under a `SourceFile` — the Phase 2 throwaway rule.
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

    #[test]
    fn throwaway_rule_emits_nested_events() {
        let toks = tokenize("42");
        let input = Input::new("42", &toks);
        let mut p = Parser::new(&input);
        parse_number(&mut p);
        let steps = process(p.finish());
        assert_eq!(
            steps,
            vec![
                Step::Enter(SyntaxKind::SourceFile),
                Step::Enter(SyntaxKind::ExpressionStatement),
                Step::Enter(SyntaxKind::NumberLiteral),
                Step::Token,
                Step::Leave,
                Step::Leave,
                Step::Leave,
            ]
        );
    }

    #[test]
    fn eat_consumes_only_matching_kind() {
        let toks = tokenize("42");
        let input = Input::new("42", &toks);
        let mut p = Parser::new(&input);
        assert!(!p.eat(SyntaxKind::Plus));
        assert!(p.eat(SyntaxKind::Number));
        assert!(p.at_eof());
    }

    #[test]
    fn at_ts_uses_token_set() {
        let toks = tokenize("+ a");
        let input = Input::new("+ a", &toks);
        let p = Parser::new(&input);
        let ops = TokenSet::new(&[SyntaxKind::Plus, SyntaxKind::Minus]);
        assert!(p.at_ts(ops));
        assert!(!p.at_ts(TokenSet::new(&[SyntaxKind::Ident])));
    }

    #[test]
    fn expect_reports_error_on_mismatch() {
        let toks = tokenize("42");
        let input = Input::new("42", &toks);
        let mut p = Parser::new(&input);
        assert!(!p.expect(SyntaxKind::Semi));
        let events = p.finish();
        assert!(matches!(events.first(), Some(Event::Error { .. })));
    }

    #[test]
    fn precede_wraps_completed_node() {
        // Build `Identifier`, then precede into `BinaryExpression`.
        let toks = tokenize("a");
        let input = Input::new("a", &toks);
        let mut p = Parser::new(&input);
        let inner = p.start();
        p.bump(SyntaxKind::Ident);
        let completed = inner.complete(&mut p, SyntaxKind::Identifier);
        let outer = completed.precede(&mut p);
        outer.complete(&mut p, SyntaxKind::BinaryExpression);
        let steps = process(p.finish());
        assert_eq!(
            steps,
            vec![
                Step::Enter(SyntaxKind::BinaryExpression),
                Step::Enter(SyntaxKind::Identifier),
                Step::Token,
                Step::Leave,
                Step::Leave,
            ]
        );
    }

    #[test]
    fn err_recover_bumps_unexpected_token_into_error_node() {
        let toks = tokenize("#");
        let input = Input::new("#", &toks);
        let mut p = Parser::new(&input);
        p.err_recover("unexpected", TokenSet::EMPTY);
        assert!(p.at_eof(), "the offending token must be consumed");
        let steps = process(p.finish());
        assert_eq!(
            steps,
            vec![
                Step::Enter(SyntaxKind::Error),
                Step::Error("unexpected".to_string()),
                Step::Token,
                Step::Leave,
            ]
        );
    }

    #[test]
    fn err_recover_leaves_token_in_recovery_set() {
        let toks = tokenize(";");
        let input = Input::new(";", &toks);
        let mut p = Parser::new(&input);
        let recovery = TokenSet::new(&[SyntaxKind::Semi]);
        p.err_recover("unexpected", recovery);
        assert!(p.at(SyntaxKind::Semi), "the ; must be left for the caller");
        let steps = process(p.finish());
        assert_eq!(steps, vec![Step::Error("unexpected".to_string())]);
    }

    #[test]
    fn abandon_drops_the_node() {
        let toks = tokenize("a");
        let input = Input::new("a", &toks);
        let mut p = Parser::new(&input);
        let outer = p.start();
        let inner = p.start();
        inner.abandon(&mut p);
        p.bump(SyntaxKind::Ident);
        outer.complete(&mut p, SyntaxKind::SourceFile);
        let steps = process(p.finish());
        assert_eq!(
            steps,
            vec![
                Step::Enter(SyntaxKind::SourceFile),
                Step::Token,
                Step::Leave,
            ]
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn advance_guard_panics_on_stuck_loop() {
        // A deliberately non-advancing loop: peek forever without consuming.
        // The fuel runs out and the debug assertion fires. We catch the panic
        // rather than using #[should_panic] (banned) and never trip this on real
        // input — it is purely a development backstop.
        let result = std::panic::catch_unwind(|| {
            let toks = tokenize("a");
            let input = Input::new("a", &toks);
            let p = Parser::new(&input);
            for _ in 0..(FUEL + 1) {
                let _ = p.current();
            }
        });
        assert!(
            result.is_err(),
            "stuck lookahead must panic in debug builds"
        );
    }
}
