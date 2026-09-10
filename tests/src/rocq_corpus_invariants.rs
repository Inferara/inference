//! Standing invariants over the emitted-obligation corpus.
//!
//! The `coqc` gate beside this module asks whether an emitted `.v` *compiles*.
//! Nothing there, and nothing anywhere else in the toolchain, asks whether an
//! obligation is **true**: the translator emits `Proof. (* TODO *) Admitted.`
//! itself, so a false theorem ships as green as a true one. These tests cover
//! the one falsification shape the compiler can recognise without a prover.
//!
//! The shape is this. `HA_app_ok f τs` unfolds downstream to total correctness
//! — `interp_realized` → `fun_computes` → `sem_mx`, with no `AI_trap` outcome —
//! so an obligation applying a callee that *traps* at some admitted argument
//! vector is false rather than vacuous. `strictify` demands `HA_defined` of the
//! applied term and `interp_realized_universal` is one-directional, so there is
//! no arm in which the claim quietly narrows to the arguments that work.
//!
//! Arithmetic overflow is such a trap wherever a `+`, `-`, `*` or unary `-` is
//! compiled with a guard. So: a universal obligation that applies a guarded
//! callee must also *bound* the arguments it applies, in an antecedent, or the
//! theorem it states is false at the first argument vector that overflows.
//!
//! The audit reads the compiler's own records rather than the emitted text —
//! the obligation map for the applications, the guarded-function list for the
//! callees — because a regular expression over a `.v` would be measuring the
//! printer instead of the program.
//!
//! Only frame slots (`T_local`) are tracked as variables. A logical variable
//! (`T_lvar`) is a de Bruijn index rewritten to its own binder depth, so the
//! same source variable is a different index in an antecedent than it is in the
//! consequent and comparing the two would relate unrelated things. An argument
//! that is a logical variable is therefore outside this audit's reach: nothing
//! here will claim it is bounded, and nothing here will report it as unbounded
//! either.

#![cfg(test)]

use inference_hassert::{
    HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HSpecEntry, HTerm, SpecKind,
};
use inference_wasm_codegen::CompilationMode;

use crate::rocq_test_support::compile_fixture_output;
use crate::rocq_typecheck::gate::CORPUS;

/// A corpus fixture allowed to apply a guarded callee without bounding its
/// arguments, with the reason why the obligation is nonetheless not false.
///
/// Each entry has to claim the obligation is *true*, never that the violation
/// is small or expected: the whole point of the audit is that a false theorem
/// is invisible downstream, so "we know about it" is not a reason. The list is
/// empty, and was still empty after the language's default became the checked
/// one and every unmarked operator in the corpus started carrying a guard: each
/// universal obligation applying a guarded callee either bounds every variable
/// it applies or applies constants only.
const OBLIGATIONS_WITHOUT_A_BOUNDING_ANTECEDENT: &[(&str, &str)] = &[];

/// One application of a guarded callee found inside a universal obligation.
struct Application {
    spec: String,
    obligation: String,
    callee: String,
    /// The variables the arguments are built from. A derived argument such as
    /// `f(a + 1)` contributes the variables inside it, because the value that
    /// reaches the callee is unbounded exactly when `a` is. A constant argument
    /// contributes nothing: it is already a single value, and whether *that*
    /// value overflows is not something an antecedent can change.
    variables: Vec<HTerm>,
}

/// No universal obligation in the corpus applies a callee whose body traps on
/// overflow without an antecedent bounding the arguments it applies.
///
/// The audit would be vacuous over a corpus with nothing guarded in it — no
/// application found, no assertion executed — so it also counts what it looked
/// at and fails if that count is zero. Every corpus fixture whose executable
/// bodies do arithmetic now keeps it above zero, because the operators that
/// carry a guard are the unmarked ones; a corpus in which every such body was
/// marked modular would leave this test green while measuring nothing, which is
/// the state the count exists to refuse.
#[test]
fn no_universal_obligation_applies_an_unbounded_guarded_callee() {
    let mut violations: Vec<String> = Vec::new();
    let mut refusals: Vec<String> = Vec::new();
    let mut examined = 0usize;
    for (file, module) in CORPUS {
        let output = match compile_fixture_output(file, module, CompilationMode::Proof) {
            Ok(output) => output,
            Err(refusal) => {
                // A corpus fixture the compiler refuses has no obligations to
                // audit, so it is reported apart from the findings: this test
                // fails for it, but not under a headline saying an obligation is
                // false. Nothing was examined, which is the whole complaint.
                refusals.push(format!(
                    "{file}: proof-mode code generation refused it: {refusal}"
                ));
                continue;
            }
        };
        let guarded: Vec<String> = output
            .guarded_functions()
            .iter()
            .map(ToString::to_string)
            .collect();
        if guarded.is_empty() {
            continue;
        }
        for (spec, entries) in output.hspecs() {
            for entry in entries {
                for application in universal_applications(spec, entry, &guarded) {
                    examined += 1;
                    let unbounded = unbounded_variables(&application, &entry.hassert);
                    if unbounded.is_empty() {
                        continue;
                    }
                    if OBLIGATIONS_WITHOUT_A_BOUNDING_ANTECEDENT
                        .iter()
                        .any(|(exempt, _)| exempt == file)
                    {
                        continue;
                    }
                    violations.push(format!(
                        "{file}: `{}` in spec `{}` applies `{}`, whose body traps on overflow, \
                         with {unbounded:?} unbounded by any relop in an antecedent",
                        application.obligation, application.spec, application.callee
                    ));
                }
            }
        }
    }
    assert!(
        refusals.is_empty(),
        "a corpus fixture could not be compiled or translated, so its obligations were not \
         examined at all; this audit cannot speak for a fixture it never read:\n{}",
        refusals.join("\n")
    );
    assert!(
        violations.is_empty(),
        "a universal obligation applying a trapping callee at an unbounded argument is FALSE, \
         and the Rocq gate admits open proofs so nothing downstream will say so:\n{}",
        violations.join("\n")
    );
    assert!(
        examined > 0,
        "no corpus obligation applies a guarded callee, so this audit executed no assertion \
         at all; a fixture whose executable body carries an overflow guard and whose \
         specification applies it is what makes the audit measure anything"
    );
}

/// The exemption list carries a reason for every entry, and names a fixture the
/// corpus actually has.
///
/// An exemption whose fixture was renamed away stops exempting anything and
/// starts hiding the fact that it does.
#[test]
fn every_exemption_names_a_corpus_fixture_and_gives_a_reason() {
    for (file, reason) in OBLIGATIONS_WITHOUT_A_BOUNDING_ANTECEDENT {
        assert!(
            CORPUS.iter().any(|(corpus_file, _)| corpus_file == file),
            "`{file}` is exempted but is not in the corpus"
        );
        assert!(
            reason.len() > 40,
            "the exemption for `{file}` must say why the obligation is true, not that it is \
             known: {reason}"
        );
    }
}

/// An argument built out of a variable rather than being one is still an
/// argument the callee can overflow on.
///
/// `fixmul(a + 1, 7)` hands the callee a term that is not itself a slot, and a
/// reading that only looked at the top of each argument would find nothing to
/// bound and pass the application as safe. Both halves are asserted here: the
/// derived argument is reported when nothing constrains `a`, and the same
/// application passes once an antecedent does.
#[test]
fn a_derived_argument_is_bounded_only_when_the_variable_inside_it_is() {
    let derived = HTerm::Binop(
        HNumType::I64,
        HBinop::Add,
        Box::new(HTerm::Local(0)),
        Box::new(HTerm::Const(HConst::I64(1))),
    );
    let applies = HAssert::AppOk(
        HFnRef("fixmul".to_string()),
        vec![derived, HTerm::Const(HConst::I64(7))],
    );
    let guarded = vec!["fixmul".to_string()];

    let unconstrained = HSpecEntry::new(
        HFnRef("fixmul_is_realized".to_string()),
        applies.clone(),
        SpecKind::Forall,
    );
    let found = universal_applications("Overflow", &unconstrained, &guarded);
    assert_eq!(found.len(), 1, "the application itself must still be found");
    assert_eq!(
        unbounded_variables(&found[0], &unconstrained.hassert),
        vec![HTerm::Local(0)],
        "a derived argument must contribute the variables it is built from"
    );

    let constrained = HSpecEntry::new(
        HFnRef("fixmul_is_realized".to_string()),
        HAssert::Imp(
            Box::new(HAssert::TermEq(
                HTerm::Relop(
                    HNumType::I64,
                    HRelop::LeS,
                    Box::new(HTerm::Local(0)),
                    Box::new(HTerm::Const(HConst::I64(1024))),
                ),
                HTerm::Const(HConst::I32(1)),
            )),
            Box::new(applies),
        ),
        SpecKind::Forall,
    );
    let found = universal_applications("Overflow", &constrained, &guarded);
    assert_eq!(found.len(), 1);
    assert!(
        unbounded_variables(&found[0], &constrained.hassert).is_empty(),
        "an antecedent constraining `a` bounds `a + 1` as well"
    );
}

/// The variables an application hands its callee that no antecedent of the
/// obligation constrains.
fn unbounded_variables(application: &Application, obligation: &HAssert) -> Vec<HTerm> {
    let bounded = bounding_antecedents(obligation);
    application
        .variables
        .iter()
        .filter(|term| !bounded.contains(term))
        .cloned()
        .collect()
}

/// Every application of a guarded callee inside one universal obligation.
///
/// A reachability obligation is skipped: `P018` refuses one that reaches a
/// guard at all, so an application here would mean that rule let something
/// through, which is its own test's business rather than this one's.
fn universal_applications(spec: &str, entry: &HSpecEntry, guarded: &[String]) -> Vec<Application> {
    if !matches!(entry.kind, SpecKind::Forall) {
        return Vec::new();
    }
    let mut found = Vec::new();
    walk_assert(&entry.hassert, &mut |assertion| {
        if let HAssert::AppOk(callee, args) = assertion
            && guarded.contains(&callee.0)
        {
            found.push(Application {
                spec: spec.to_string(),
                obligation: entry.fn_symbol.0.clone(),
                callee: callee.0.clone(),
                variables: variables_in(args),
            });
        }
    });
    walk_assert(&entry.hassert, &mut |assertion| {
        walk_assert_terms(assertion, &mut |term| {
            if let HTerm::App(callee, args) = term
                && guarded.contains(&callee.0)
            {
                found.push(Application {
                    spec: spec.to_string(),
                    obligation: entry.fn_symbol.0.clone(),
                    callee: callee.0.clone(),
                    variables: variables_in(args),
                });
            }
        });
    });
    found
}

/// Every variable the argument terms are built from, at any depth.
///
/// The descent is what makes a derived argument count: `f(a + 1)` applies a
/// term that is not itself a variable, and reading only the top of each
/// argument would score that application as having nothing to bound. The same
/// descent runs over antecedents in [`bounding_antecedents`], so an argument is
/// found bounded exactly when the variables it is built from are.
fn variables_in(args: &[HTerm]) -> Vec<HTerm> {
    let mut variables = Vec::new();
    for arg in args {
        walk_term(arg, &mut |term| {
            if is_variable(term) {
                variables.push(term.clone());
            }
        });
    }
    variables
}

/// The variable terms an antecedent constrains with a relational operator.
///
/// "Bounding" is read structurally and generously: any `T_relop` anywhere in an
/// antecedent that mentions a variable counts as bounding it. A tighter reading
/// — that the relop is an inequality against a constant at the right
/// signedness — would be closer to what actually makes an obligation provable,
/// but it would also be this test deciding what a good envelope looks like,
/// which is the proof's job. What the audit is for is the case with no
/// constraint on the argument at all.
fn bounding_antecedents(obligation: &HAssert) -> Vec<HTerm> {
    let mut bounded = Vec::new();
    collect_antecedents(obligation, &mut |antecedent| {
        walk_assert(antecedent, &mut |assertion| {
            walk_assert_terms(assertion, &mut |term| {
                if let HTerm::Relop(_, _, left, right) = term {
                    for side in [left.as_ref(), right.as_ref()] {
                        walk_term(side, &mut |inner| {
                            if is_variable(inner) {
                                bounded.push(inner.clone());
                            }
                        });
                    }
                }
            });
        });
    });
    bounded
}

/// Applies `visit` to the antecedent of every implication in `obligation`,
/// including the nested ones a chain of `assume` blocks produces.
fn collect_antecedents(obligation: &HAssert, visit: &mut impl FnMut(&HAssert)) {
    walk_assert(obligation, &mut |assertion| {
        if let HAssert::Imp(antecedent, _) = assertion {
            visit(antecedent);
        }
    });
}

/// Whether `term` names a frame slot the audit can follow between an
/// antecedent and the obligation that applies it.
///
/// A `T_lvar` is deliberately not one: its de Bruijn index is relative to the
/// binder depth it appears at, so the same logical variable carries different
/// indices in the two places this audit would have to compare.
fn is_variable(term: &HTerm) -> bool {
    matches!(term, HTerm::Local(_))
}

/// Applies `visit` to `assertion` and every assertion underneath it.
fn walk_assert(assertion: &HAssert, visit: &mut impl FnMut(&HAssert)) {
    visit(assertion);
    match assertion {
        HAssert::Not(inner) | HAssert::Ex(inner) | HAssert::All(inner) => {
            walk_assert(inner, visit);
        }
        HAssert::And(left, right) | HAssert::Imp(left, right) | HAssert::Or(left, right) => {
            walk_assert(left, visit);
            walk_assert(right, visit);
        }
        HAssert::True
        | HAssert::False
        | HAssert::TermEq(_, _)
        | HAssert::HasType(_, _)
        | HAssert::Defined(_)
        | HAssert::AppOk(_, _) => {}
    }
}

/// Applies `visit` to every term one assertion node holds directly, and to
/// every term underneath those.
fn walk_assert_terms(assertion: &HAssert, visit: &mut impl FnMut(&HTerm)) {
    match assertion {
        HAssert::TermEq(left, right) => {
            walk_term(left, visit);
            walk_term(right, visit);
        }
        HAssert::HasType(term, _) | HAssert::Defined(term) => walk_term(term, visit),
        HAssert::AppOk(_, args) => {
            for arg in args {
                walk_term(arg, visit);
            }
        }
        HAssert::True
        | HAssert::False
        | HAssert::Not(_)
        | HAssert::And(_, _)
        | HAssert::Imp(_, _)
        | HAssert::Or(_, _)
        | HAssert::Ex(_)
        | HAssert::All(_) => {}
    }
}

/// Applies `visit` to `term` and every term underneath it.
fn walk_term(term: &HTerm, visit: &mut impl FnMut(&HTerm)) {
    visit(term);
    match term {
        HTerm::App(_, args) => {
            for arg in args {
                walk_term(arg, visit);
            }
        }
        HTerm::Binop(_, _, left, right) | HTerm::Relop(_, _, left, right) => {
            walk_term(left, visit);
            walk_term(right, visit);
        }
        HTerm::Const(_) | HTerm::LVar(_) | HTerm::Local(_) => {}
    }
}
