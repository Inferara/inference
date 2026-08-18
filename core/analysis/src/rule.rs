//! Rule trait and rule! macro for analysis passes.

// Re-export for use in rule! macro expansions
#[doc(hidden)]
pub use inference_type_checker::typed_context::TypedContext;

use crate::AnalysisOptions;
use crate::errors::{LabeledDiagnostic, Severity};

/// A single analysis rule that checks a semantic invariant.
///
/// Each rule is a zero-sized struct that implements this trait.
/// `Send + Sync` bounds signal that rules are stateless and safe
/// for future parallel execution.
pub trait Rule: Send + Sync {
    /// Rule identifier, e.g. "A001".
    fn id(&self) -> &'static str;
    /// Human-readable rule name, e.g. "Break outside loop".
    fn name(&self) -> &'static str;
    /// Severity level for findings produced by this rule.
    fn severity(&self) -> Severity;
    /// Runs the check against the typed context and returns the findings, each
    /// paired with the file it belongs to so the report can name the file.
    ///
    /// `options` reaches every rule, not only the ones that read it today. A
    /// rule that later needs an artifact setting therefore has it already, and
    /// cannot acquire one by reaching for a constant of its own instead.
    fn check(&self, ctx: &TypedContext, options: AnalysisOptions) -> Vec<LabeledDiagnostic>;
}

/// Declares an analysis rule struct and implements the `Rule` trait.
///
/// Each rule's `check` returns findings paired with the file they belong to (a
/// [`LabeledDiagnostic`]), so a multi-file report can name the file an imported
/// finding came from.
///
/// A rule that measures the program against the artifact it will be compiled
/// into declares an [`AnalysisOptions`] parameter as well. Most rules check the
/// source alone and use the one-parameter form, which supplies the trait's
/// second argument under an unused name — so the trait stays uniform without
/// every rule body naming an argument it ignores.
///
/// # Example
/// ```ignore
/// rule! {
///     /// Break must appear inside a loop body.
///     #[id = "A001"]
///     #[name = "Break outside loop"]
///     #[severity = error]
///     pub struct BreakOutsideLoop;
///     fn check(ctx: &TypedContext) -> Vec<LabeledDiagnostic> {
///         // implementation
///     }
/// }
/// ```
///
/// [`LabeledDiagnostic`]: crate::errors::LabeledDiagnostic
/// [`AnalysisOptions`]: crate::AnalysisOptions
#[macro_export]
macro_rules! rule {
    (
        $(#[doc = $doc:literal])*
        #[id = $id:literal]
        #[name = $name:literal]
        #[severity = $severity:ident]
        pub struct $tname:ident;
        fn check($ctx:ident : &TypedContext, $opts:ident : AnalysisOptions) -> Vec<LabeledDiagnostic> $body:block
    ) => {
        $(#[doc = $doc])*
        pub struct $tname;
        impl $crate::rule::Rule for $tname {
            fn id(&self) -> &'static str { $id }
            fn name(&self) -> &'static str { $name }
            fn severity(&self) -> $crate::errors::Severity {
                $crate::__severity!($severity)
            }
            fn check(&self, $ctx: &$crate::rule::TypedContext, $opts: $crate::AnalysisOptions) -> Vec<$crate::errors::LabeledDiagnostic> $body
        }
    };
    (
        $(#[doc = $doc:literal])*
        #[id = $id:literal]
        #[name = $name:literal]
        #[severity = $severity:ident]
        pub struct $tname:ident;
        fn check($ctx:ident : &TypedContext) -> Vec<LabeledDiagnostic> $body:block
    ) => {
        $(#[doc = $doc])*
        pub struct $tname;
        impl $crate::rule::Rule for $tname {
            fn id(&self) -> &'static str { $id }
            fn name(&self) -> &'static str { $name }
            fn severity(&self) -> $crate::errors::Severity {
                $crate::__severity!($severity)
            }
            fn check(&self, $ctx: &$crate::rule::TypedContext, _options: $crate::AnalysisOptions) -> Vec<$crate::errors::LabeledDiagnostic> $body
        }
    };
}

/// Maps severity identifier to `Severity` variant. Internal use only.
#[doc(hidden)]
#[macro_export]
macro_rules! __severity {
    (error) => { $crate::errors::Severity::Error };
    (warning) => { $crate::errors::Severity::Warning };
    (info) => { $crate::errors::Severity::Info };
    ($other:ident) => { compile_error!(concat!("invalid severity: `", stringify!($other), "`, expected `error`, `warning`, or `info`")) };
}
