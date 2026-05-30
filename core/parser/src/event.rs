//! Parser output as a flat event stream, decoupled from tree construction.
//!
//! The parser does not build the tree directly. Instead it emits a flat
//! `Vec<Event>` (issue #62 design §5, mirroring rust-analyzer): an interior node
//! is an [`Event::Start`]/[`Event::Finish`] pair, a leaf is an [`Event::Token`],
//! and a diagnostic is an [`Event::Error`]. This indirection is what lets the
//! parser decide a node's *kind* after it has already started — the engine for
//! left-associative and postfix operators.
//!
//! # `forward_parent` and `precede`
//!
//! When the parser has finished a node `A` and discovers it is actually the child
//! of a larger node `B` (e.g. an atom that turns out to be the left operand of a
//! binary expression), it cannot move the already-emitted `Start(A)`. Instead it
//! inserts a new `Start(B)` *before* `Start(A)` would naturally go and records a
//! `forward_parent` link from `A`'s start to `B`'s start. [`process`] later walks
//! these links and emits the enters in the correct, nested order.
//!
//! A [`tombstone`](Event::tombstone) start is a placeholder a [`crate::parser::Marker`]
//! pushes eagerly; if the marker is abandoned without being completed it is left
//! as a tombstone and skipped by [`process`].

use crate::syntax_kind::SyntaxKind;

/// A single parser event in emission order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Begins an interior node. `kind` is patched from the tombstone sentinel
    /// when the node's [`crate::parser::Marker`] is completed; `forward_parent`,
    /// if set, points to the start event of a node that should enclose this one.
    Start {
        /// The node kind, or [`SyntaxKind::Eof`] while still a tombstone.
        kind: SyntaxKind,
        /// Index of an enclosing node's `Start` event, set by `precede`.
        forward_parent: Option<u32>,
    },
    /// Ends the most recently started, still-open interior node.
    Finish,
    /// A leaf token consumed into the current node.
    Token {
        /// The token's kind.
        kind: SyntaxKind,
    },
    /// A diagnostic attached at the current position.
    Error {
        /// Human-readable description of the problem.
        msg: String,
    },
}

impl Event {
    /// The sentinel kind a still-incomplete or abandoned `Start` carries.
    ///
    /// [`process`] skips any `Start` left with this kind (an abandoned marker's
    /// tombstone).
    pub(crate) const TOMBSTONE: SyntaxKind = SyntaxKind::Eof;

    /// A fresh tombstone `Start`, pushed eagerly when a marker opens.
    #[must_use]
    pub(crate) fn tombstone() -> Event {
        Event::Start {
            kind: Event::TOMBSTONE,
            forward_parent: None,
        }
    }
}

/// A flattened, `forward_parent`-resolved parser step.
///
/// [`process`] turns the raw event list into this linear form, which the tree
/// builder consumes directly: each [`Step::Enter`] opens a node, [`Step::Leave`]
/// closes it, [`Step::Token`] is a leaf, and [`Step::Error`] is a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Open an interior node of the given kind.
    Enter(SyntaxKind),
    /// Close the innermost open node.
    Leave,
    /// Emit the next leaf token (kind matched against the lexed stream).
    Token,
    /// Emit a diagnostic at the current position.
    Error(String),
}

/// Resolves `forward_parent` chains into a linear [`Step`] stream.
///
/// Walks the events left to right. At each non-tombstone `Start`, it follows the
/// `forward_parent` links to gather the chain of enclosing kinds (outermost
/// last), emitting an [`Step::Enter`] for each in outermost-first order; every
/// slot it visits is blanked to a tombstone so it is not re-entered. `Finish`
/// becomes [`Step::Leave`], `Token`/`Error` map straight across, and tombstones
/// are skipped.
#[must_use]
pub fn process(mut events: Vec<Event>) -> Vec<Step> {
    let mut steps = Vec::with_capacity(events.len());
    // Scratch buffer for a forward_parent chain, reused across starts.
    let mut chain = Vec::new();

    for i in 0..events.len() {
        match std::mem::replace(&mut events[i], Event::tombstone()) {
            Event::Start {
                kind: Event::TOMBSTONE,
                forward_parent: None,
            } => {
                // An abandoned marker; nothing to emit.
            }
            Event::Start {
                kind,
                forward_parent,
            } => {
                chain.push(kind);
                let mut parent = forward_parent;
                while let Some(idx) = parent {
                    let slot = std::mem::replace(&mut events[idx as usize], Event::tombstone());
                    match slot {
                        Event::Start {
                            kind,
                            forward_parent,
                        } => {
                            chain.push(kind);
                            parent = forward_parent;
                        }
                        // A forward_parent must point at a Start; anything else
                        // is a malformed event stream, so stop the walk.
                        _ => parent = None,
                    }
                }
                // Outermost enclosing node first: the chain was gathered
                // innermost-first, so enter in reverse.
                for kind in chain.drain(..).rev() {
                    steps.push(Step::Enter(kind));
                }
            }
            Event::Finish => steps.push(Step::Leave),
            Event::Token { .. } => steps.push(Step::Token),
            Event::Error { msg } => steps.push(Step::Error(msg)),
        }
    }

    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_start_finish_pairs() {
        let events = vec![
            Event::Start {
                kind: SyntaxKind::SourceFile,
                forward_parent: None,
            },
            Event::Token {
                kind: SyntaxKind::Number,
            },
            Event::Finish,
        ];
        assert_eq!(
            process(events),
            vec![
                Step::Enter(SyntaxKind::SourceFile),
                Step::Token,
                Step::Leave,
            ]
        );
    }

    #[test]
    fn tombstone_starts_are_skipped() {
        let events = vec![
            Event::tombstone(),
            Event::Start {
                kind: SyntaxKind::SourceFile,
                forward_parent: None,
            },
            Event::Finish,
        ];
        assert_eq!(
            process(events),
            vec![Step::Enter(SyntaxKind::SourceFile), Step::Leave]
        );
    }

    #[test]
    fn forward_parent_nests_outer_around_inner() {
        // `A` was completed first, then `precede`d into `B`: the inner Start(A)
        // carries forward_parent → Start(B). Result must be B(A(...)).
        let events = vec![
            Event::Start {
                kind: SyntaxKind::Identifier,
                forward_parent: Some(3),
            },
            Event::Token {
                kind: SyntaxKind::Ident,
            },
            Event::Finish, // closes A
            Event::Start {
                kind: SyntaxKind::BinaryExpression,
                forward_parent: None,
            },
            Event::Finish, // closes B
        ];
        assert_eq!(
            process(events),
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
    fn error_event_passes_through() {
        let events = vec![
            Event::Start {
                kind: SyntaxKind::SourceFile,
                forward_parent: None,
            },
            Event::Error {
                msg: "boom".to_string(),
            },
            Event::Finish,
        ];
        assert_eq!(
            process(events),
            vec![
                Step::Enter(SyntaxKind::SourceFile),
                Step::Error("boom".to_string()),
                Step::Leave,
            ]
        );
    }
}
