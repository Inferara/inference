//! Reader for the declarations a Rocq `.v` file binds.
//!
//! Three questions are asked of it. `rocq_typecheck`'s coverage audit asks what
//! the *vendored stub* declares, so that every declaration can be held to
//! having a producer. `rocq_stub_drift` asks the same of the stub and then asks
//! the mirror question of the *real* upstream libraries, so that a stub
//! declaration with no upstream counterpart becomes a failure rather than a
//! fiction the local gate happily elaborates. And `rocq_typecheck` asks
//! everything the stub *binds*, type names included, to hold the translator's
//! `ROCQ_CONTRACT_NAMES` to it. That list is production code: every function
//! name the translator emits is escaped off it, so this reader decides which
//! user functions are renamed.
//!
//! Those questions need two readers, and the asymmetry is deliberate rather
//! than an oversight. [`declared_names`] reads the restricted vernacular the
//! stub is written in — signatures only, no proofs, no tactics — and reads it
//! narrowly: a shape it silently misses is a declaration exempted from needing a
//! producer, which is the failure the coverage audit exists to prevent.
//! [`upstream_declared_names`] reads full Rocq, and reads it broadly, because a
//! miss is the costly error in both of its uses. Over the upstream libraries, a
//! binding form it missed would condemn a perfectly real name as fiction. Over
//! the stub, it would leave a contract name off the deny-list, free for a
//! function to shadow.
//!
//! Reading broadly has a cost, though: it collects names a `.v` cannot actually
//! shadow. The one such name in the stub is `int_of_Z`, collected from inside
//! `Module Wasm_int` although nothing outside that module reaches it
//! unqualified. The contract deny-list leaves it out, and the drift test that
//! holds the list to this reader allows it by name, failing if the reader
//! stops collecting it. A name collected in excess costs one named allowance;
//! a missed contract name costs a `.v` that `coqc` rejects, so the reader keeps
//! erring towards collecting.

/// A Rocq token, at the resolution the declaration parser needs.
///
/// Identifiers are what the audits are about; a string literal is distinguished
/// only because it is all the *name* of a string-named `Notation "…" := …` is,
/// and such a notation binds no identifier for an audit to demand a producer
/// for — reading its quoted name as one would invent a declaration. Everything
/// else — numbers, `%`, `->` — comes through one character at a time as
/// [`Tok::Punct`], which is enough for the `|`, `:=` and `{ … ; … }` landmarks
/// the parsers steer by.
// the token stream as source, which is all this tokenizer needs.
pub(crate) enum Tok<'a> {
    Ident(&'a str),
    Str,
    Punct(char),
}

pub(crate) fn tokenize(source: &str) -> Vec<Tok<'_>> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c == '"' {
            // Rocq escapes a quote inside a string literal by doubling it,
            // and pairing quotes off in order handles that without a special
            // case: a literal `"c0""c1""c2"` is read as three adjacent
            // literals covering exactly the same span, because each escape
            // contributes two quotes and leaves no characters between the
            // pair it splits. Nothing inside a literal can therefore reach
            // the token stream as source, which is all this tokenizer needs.
            for (_, c) in chars.by_ref() {
                if c == '"' {
                    break;
                }
            }
            tokens.push(Tok::Str);
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let mut end = start + c.len_utf8();
            while let Some(&(at, c)) = chars.peek() {
                if c.is_ascii_alphanumeric() || c == '_' || c == '\'' {
                    end = at + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Tok::Ident(&source[start..end]));
            continue;
        }
        tokens.push(Tok::Punct(c));
    }
    tokens
}

pub(crate) fn ident_at<'a>(tokens: &[Tok<'a>], at: usize) -> Option<&'a str> {
    match tokens.get(at) {
        Some(Tok::Ident(name)) => Some(name),
        _ => None,
    }
}

pub(crate) fn is_punct(tokens: &[Tok<'_>], at: usize, c: char) -> bool {
    matches!(tokens.get(at), Some(Tok::Punct(got)) if *got == c)
}

/// Rocq comments nest — `(* a (* b *) c *)` is one comment, and a parser
/// that stops at the first `*)` would read the tail as source. Newlines are
/// preserved so a stripped file keeps its line structure, and a string
/// literal is copied through untouched so a `(*` inside one cannot open a
/// comment. Rocq's escaped quote `""` needs no special case here for the
/// reason it needs none in [`tokenize`]: the pair puts no characters
/// between the literal it closes and the one it reopens, so the stripper is
/// never outside a literal at a position that holds anything.
///
/// One divergence from Rocq's own lexer, deliberately not modelled: Rocq
/// recognises a string literal *inside* a comment, so `(* "*)" *)` is one
/// comment, while this ends it at the inner `*)` and reads the rest as an
/// unterminated literal that swallows the file.
///
/// This reads three sources, and on the two this repository writes the
/// readings agree. The emitter's comments are its fixed proof placeholder
/// and its `(*name*)` annotations, which carry quotes only in even runs:
/// local names have every `"` doubled, and an even run lexes as complete
/// strings that end before the closer. The stub's comments are prose whose
/// quoted spans are balanced and contain no comment delimiter. When this was
/// written, a Rocq-faithful lexer read the stub and every `.v` golden exactly
/// as this does. The third source, the pinned upstream libraries the drift
/// lane strips, lives outside this repository, so agreement there is assumed
/// rather than checked.
///
/// Where the readings differ, the error can run either way, and the file can
/// still compile. This reads code that Rocq comments out when a comment holds
/// an odd quote: such a quote in a local name once made Rocq comment out the
/// instructions between two names while `coqc` accepted the file. And it
/// hides code that Rocq reads when a comment holds a quoted `*)`, as in the
/// example above.
pub(crate) fn strip_rocq_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut depth = 0usize;
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            in_string = c != '"';
        } else if depth == 0 && c == '"' {
            out.push(c);
            in_string = true;
        } else if c == '(' && chars.peek() == Some(&'*') {
            chars.next();
            depth += 1;
        } else if depth > 0 && c == '*' && chars.peek() == Some(&')') {
            chars.next();
            depth -= 1;
        } else if depth == 0 || c == '\n' {
            // Inside a comment only the newlines survive, so a stripped
            // file keeps the line numbering of the original.
            out.push(c);
        }
    }
    out
}

/// Every name a stub `.v` file declares, in source order.
///
/// The four shapes that matter are inductive constructors, record field
/// names, and the top-level `Definition`/`Parameter`/`Axiom` and
/// identifier-named `Notation` bindings. Inductive and record *type* names
/// are deliberately not collected — see
/// `every_stub_declaration_has_a_producer` — and neither are the scope
/// declarations (`Declare Scope`, `Delimit Scope`) or the `host` `Class`,
/// none of which name a term an emitted module could apply.
pub(crate) fn declared_names(source: &str) -> Vec<String> {
    let source = strip_rocq_comments(source);
    let tokens = tokenize(&source);
    let mut names = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        let Some(keyword) = ident_at(&tokens, at) else {
            // A constructor, in the leading-`|` form every inductive in the
            // stub is written with.
            if is_punct(&tokens, at, '|')
                && is_punct(&tokens, at + 2, ':')
                && let Some(name) = ident_at(&tokens, at + 1)
            {
                names.push(name.to_string());
            }
            at += 1;
            continue;
        };
        match keyword {
            "Inductive" | "Variant" => {
                // Rocq lets the *first* constructor omit its leading `|`, so
                // it is read here, off the `:=`; the `|` arm above picks up
                // the rest either way.
                while at + 1 < tokens.len()
                    && !(is_punct(&tokens, at, ':') && is_punct(&tokens, at + 1, '='))
                {
                    at += 1;
                }
                at += 2;
                if is_punct(&tokens, at + 1, ':')
                    && let Some(name) = ident_at(&tokens, at)
                {
                    names.push(name.to_string());
                }
            }
            "Record" => at = push_record_fields(&tokens, at, &mut names),
            "Definition" => {
                if let Some(name) = ident_at(&tokens, at + 1) {
                    names.push(name.to_string());
                }
                at += 1;
            }
            // `Parameter a b : T.` binds every name before the colon.
            "Parameter" | "Axiom" => {
                at += 1;
                while let Some(name) = ident_at(&tokens, at) {
                    names.push(name.to_string());
                    at += 1;
                }
            }
            // A string-named notation binds no identifier.
            "Notation" => {
                if let Some(name) = ident_at(&tokens, at + 1) {
                    names.push(name.to_string());
                }
                at += 1;
            }
            _ => at += 1,
        }
    }
    names
}

/// Pushes the field names of the `Record` whose keyword sits at `at`,
/// returning the index just past its closing brace. A field is the
/// identifier that opens each `;`-separated chunk of the brace block, which
/// makes the scan independent of how the record is laid out across lines.
pub(crate) fn push_record_fields(tokens: &[Tok<'_>], at: usize, names: &mut Vec<String>) -> usize {
    let mut at = at + 1;
    while at < tokens.len() && !is_punct(tokens, at, '{') {
        at += 1;
    }
    at += 1;
    let mut depth = 1usize;
    let mut at_field = true;
    while at < tokens.len() && depth > 0 {
        if is_punct(tokens, at, '{') {
            depth += 1;
            at_field = false;
        } else if is_punct(tokens, at, '}') {
            depth -= 1;
            at_field = false;
        } else if depth == 1 && is_punct(tokens, at, ';') {
            at_field = true;
        } else {
            if at_field
                && depth == 1
                && is_punct(tokens, at + 1, ':')
                && let Some(name) = ident_at(tokens, at)
            {
                names.push(name.to_string());
            }
            at_field = false;
        }
        at += 1;
    }
    at
}

/// The vernacular keywords that bind their following identifiers directly, as
/// `Keyword name₁ … nameₙ :` or `Keyword name …`.
///
/// This list errs towards inclusion. It is consulted for two sets: the
/// *upstream* name universe a stub declaration is looked up in, where a keyword
/// missing here would report a real upstream name as absent, and the stub's
/// vocabulary that the translator's contract deny-list is held to, where a
/// missing keyword would leave a contract name free to be shadowed. Proof
/// vernacular is included for the first reason — `Lemma`, `Theorem` and
/// friends do bind a name, and an upstream library is free to expose one the
/// stub mirrors. What a keyword here must not do is precede identifiers it does
/// not bind: each one is collected, and over the stub that renames every user
/// function spelled like it.
const BINDING_KEYWORDS: &[&str] = &[
    "Definition",
    "Parameter",
    "Parameters",
    "Axiom",
    "Axioms",
    "Notation",
    "Fixpoint",
    "CoFixpoint",
    "Lemma",
    "Theorem",
    "Corollary",
    "Remark",
    "Fact",
    "Property",
    "Proposition",
    "Instance",
    "Let",
    "Example",
    "Conjecture",
    "Hypothesis",
    "Hypotheses",
    "Variable",
    "Variables",
    "Context",
    "Coercion",
];

/// Every name a full-Rocq `.v` file binds, in source order, read broadly: an
/// upstream library's, or the vendored stub's when it is read as the contract
/// vocabulary a generated `.v` imports.
///
/// Where [`declared_names`] reads the stub's restricted signature vernacular and
/// deliberately skips inductive and record *type* names, this reads full Rocq
/// and keeps everything: type names, constructors, record and class fields, and
/// every binding the keywords above introduce. The two differences from
/// [`declared_names`] are both forced by which direction a miss fails in.
///
/// A constructor is accepted here without the `:` that [`declared_names`]
/// requires, because upstream writes its nullary constructors bare — coq-wasm's
/// `number_type` is `| T_i32 | T_i64` with no annotation anywhere — and a reader
/// demanding the colon would find neither and then report the stub's own
/// `T_i32` as a name upstream does not have.
///
/// `Class` and `Structure` are read like `Record` for the same reason: coq-wasm
/// states its host interface as a `Class`, and a reader that skipped it would
/// condemn every field of it.
///
/// It does not track scope. A name declared inside a `Module` wrapper is
/// collected bare, like a top-level one, although outside the module it is
/// reachable only qualified; the wrapper's own name is not collected at all,
/// since `Module` is not a binding keyword here. That is how the stub's
/// `int_of_Z`, declared inside `Module Wasm_int`, is collected while
/// `Wasm_int` is not; the contract deny-list leaves `int_of_Z` out, and its
/// drift test allows the excess by name.
///
/// It is a reader, not a Rocq parser, and it has blind spots: the second type
/// of a mutual `Inductive a … with b …` is missed, because `with` is not a
/// binding keyword here (teaching it one would collect every `match … with`
/// too). Over the stub, a miss like that is caught wherever `coqc` runs, by
/// `the_contract_deny_list_matches_what_coqc_records`, which compares the
/// contract deny-list with the declarations `coqc` itself records.
pub(crate) fn upstream_declared_names(source: &str) -> Vec<String> {
    let source = strip_rocq_comments(source);
    let tokens = tokenize(&source);
    let mut names = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        let Some(keyword) = ident_at(&tokens, at) else {
            // A constructor arm. Upstream writes both `| Name : ty` and the bare
            // `| Name`, the latter recognised by what follows: the next arm, the
            // end of the inductive, or the `:=` of a defaulted argument.
            if is_punct(&tokens, at, '|')
                && let Some(name) = ident_at(&tokens, at + 1)
                && (is_punct(&tokens, at + 2, ':')
                    || is_punct(&tokens, at + 2, '|')
                    || is_punct(&tokens, at + 2, '.'))
            {
                names.push(name.to_string());
            }
            at += 1;
            continue;
        };
        match keyword {
            "Inductive" | "Variant" => {
                if let Some(name) = ident_at(&tokens, at + 1) {
                    names.push(name.to_string());
                }
                while at + 1 < tokens.len()
                    && !(is_punct(&tokens, at, ':') && is_punct(&tokens, at + 1, '='))
                {
                    at += 1;
                }
                at += 2;
                // The first constructor may omit its leading `|`; the arm above
                // collects the rest.
                if let Some(name) = ident_at(&tokens, at)
                    && (is_punct(&tokens, at + 1, ':') || is_punct(&tokens, at + 1, '|'))
                {
                    names.push(name.to_string());
                }
            }
            "Record" | "Class" | "Structure" => {
                if let Some(name) = ident_at(&tokens, at + 1) {
                    names.push(name.to_string());
                }
                at = push_record_fields(&tokens, at, &mut names);
            }
            _ if BINDING_KEYWORDS.contains(&keyword) => {
                at += 1;
                while let Some(name) = ident_at(&tokens, at) {
                    names.push(name.to_string());
                    at += 1;
                }
            }
            _ => at += 1,
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::{declared_names, upstream_declared_names};

    /// The declaration parser is the audit's measuring instrument: a shape it
    /// silently misses is a declaration exempted from needing a producer, which
    /// is the failure the audit exists to prevent. This pins every shape the
    /// parser handles — the stub's current declarations are a subset, and a
    /// shape it grows back must not have to be discovered — plus the three
    /// traps: a nested comment (a parser that closed on the first `*)` would
    /// declare `commented_out`), a string-named notation (which must not be
    /// read as an identifier binding), and a `Module` wrapper (whose contents
    /// are declarations all the same).
    ///
    /// The escaped-quote line is not a fourth trap, and is not claimed as one:
    /// pairing quotes off in order already reads it correctly, for the reason
    /// [`tokenize`] gives. It is pinned rather than argued so that a later
    /// hand-written `""` case cannot get it wrong — `phantom` is what a parser
    /// that ended the literal at the first half of the pair would declare.
    ///
    /// What is *not* pinned, because it is not handled: a string literal
    /// inside a comment, which Rocq recognises and [`strip_rocq_comments`]
    /// does not. That limit is stated there rather than covered here.
    #[test]
    fn declared_names_reads_every_stub_declaration_shape() {
        // A `##` delimiter: the byte-notation line below contains `"#`, which
        // would close a plain `r#"…"#` raw string.
        let source = r##"
    (* A comment (* nested (* twice *) *) hiding Parameter commented_out : Type. *)
    Require Import BinNat.
    Declare Scope fake_scope.
    Delimit Scope fake_scope with fake.
    Class a_class : Type := { }.
    Parameter opaque_type : Type.
    Parameter first second : nat.
    Axiom an_axiom : nat.
    Notation "#00" := (encode 0%Z) : fake_scope.
    Notation "escaped ""Parameter phantom"" tail" := (list) : fake_scope.
    Notation an_alias := list.
    Unset Elimination Schemes.
    Inductive piped : Type :=
    | Ctor_a : piped
    | Ctor_b : nat -> piped.
    Set Elimination Schemes.
    Inductive unpiped : Type := Ctor_c : unpiped | Ctor_d : unpiped.
    Record a_record : Type := {
      field_one : nat;
      field_two : option nat
    }.
    Module A_module.
      Parameter inner : nat.
    End A_module.
    Definition a_definition (x : nat) : nat := x.
    "##;
        assert_eq!(
            declared_names(source),
            [
                "opaque_type",
                "first",
                "second",
                "an_axiom",
                "an_alias",
                "Ctor_a",
                "Ctor_b",
                "Ctor_c",
                "Ctor_d",
                "field_one",
                "field_two",
                "inner",
                "a_definition",
            ]
        );
    }

    /// [`upstream_declared_names`] defines the contract vocabulary the
    /// translator escapes function names off, so a shape it misses is a
    /// contract name a function can shadow, and a binder it misreads as a
    /// declaration renames functions for nothing. This pins every shape it
    /// handles against both outcomes.
    ///
    /// What it must collect: inductive (`Inductive` and `Variant`), record,
    /// class and structure *type* names, which [`declared_names`] leaves out;
    /// constructors with and without an annotation, with and without the first
    /// one's leading `|`; record, class and structure fields, and an empty
    /// class's type name with no field invented for it; every name of a
    /// multi-name `Parameter`; an `Axiom`, an identifier-named `Notation`, a
    /// `Definition` and a `Fixpoint`; and the name a `Lemma` binds.
    ///
    /// What it must not: anything inside a nested comment; a string-named
    /// notation, or a declaration quoted inside one; the arms of a `match`,
    /// which share the constructor's `|` but are followed by a pattern; and a
    /// binder inside a declaration's type, as in the stub's
    /// ``Parameter ValidSpec : forall `{ho : host}, …``.
    ///
    /// The `Module` wrapper is pinned as the documented over-collection: the
    /// name declared inside it is collected bare, the way the stub's
    /// `int_of_Z` is, and the wrapper's own name is not.
    #[test]
    fn upstream_declared_names_reads_every_binding_shape() {
        let source = r##"
    (* A comment (* nested (* twice *) *) hiding Parameter commented_out : Type. *)
    Require Import BinNat.
    Declare Scope fake_scope.
    Delimit Scope fake_scope with fake.
    Parameter opaque_type : Type.
    Parameter first second : nat.
    Axiom an_axiom : nat.
    Parameter a_predicate : forall `{binder : a_class}, nat -> Prop.
    Notation "#00" := (encode 0%Z) : fake_scope.
    Notation "escaped ""Parameter phantom"" tail" := (list) : fake_scope.
    Notation an_alias := list.
    Inductive piped : Type :=
    | Ctor_a : piped
    | Ctor_b : nat -> piped.
    Inductive unpiped : Type := Ctor_c : unpiped | Ctor_d : unpiped.
    Inductive bare : Type := | Bare_a | Bare_b.
    Variant bare_unpiped := Bare_c | Bare_d.
    Record a_record : Type := {
      field_one : nat;
      field_two : option nat
    }.
    Class a_class : Type := { class_field : nat }.
    Class empty_class : Type := { }.
    Structure a_structure := { structure_field : nat }.
    Module A_module.
      Parameter inner : nat.
    End A_module.
    Definition a_definition (x : nat) : nat := x.
    Fixpoint a_fixpoint (n : nat) : nat :=
      match n with
      | O => O
      | S m => a_fixpoint m
      end.
    Lemma a_lemma : True.
    Proof. exact I. Qed.
    "##;
        assert_eq!(
            upstream_declared_names(source),
            [
                "opaque_type",
                "first",
                "second",
                "an_axiom",
                "a_predicate",
                "an_alias",
                "piped",
                "Ctor_a",
                "Ctor_b",
                "unpiped",
                "Ctor_c",
                "Ctor_d",
                "bare",
                "Bare_a",
                "Bare_b",
                "bare_unpiped",
                "Bare_c",
                "Bare_d",
                "a_record",
                "field_one",
                "field_two",
                "a_class",
                "class_field",
                "empty_class",
                "a_structure",
                "structure_field",
                "inner",
                "a_definition",
                "a_fixpoint",
                "a_lemma",
            ]
        );
    }
}
