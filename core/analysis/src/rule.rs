//! Rule trait and rule! macro for analysis passes.

// Re-export for use in rule! macro expansions
#[doc(hidden)]
pub use inference_type_checker::typed_context::TypedContext;

use crate::errors::{AnalysisError, Severity};

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
    /// Run the check against the typed context and return errors found.
    fn check(&self, ctx: &TypedContext) -> Vec<AnalysisError>;
}

/// Declares an analysis rule struct and implements the `Rule` trait.
///
/// # Example
/// ```ignore
/// rule! {
///     /// Break must appear inside a loop body.
///     #[id = "A001"]
///     #[name = "Break outside loop"]
///     #[severity = error]
///     pub struct BreakOutsideLoop;
///     fn check(ctx: &TypedContext) -> Vec<AnalysisError> {
///         // implementation
///     }
/// }
/// ```
#[macro_export]
macro_rules! rule {
    (
        $(#[doc = $doc:literal])*
        #[id = $id:literal]
        #[name = $name:literal]
        #[severity = $severity:ident]
        pub struct $tname:ident;
        fn check($ctx:ident : &TypedContext) -> Vec<AnalysisError> $body:block
    ) => {
        $(#[doc = $doc])*
        pub struct $tname;
        impl $crate::rule::Rule for $tname {
            fn id(&self) -> &'static str { $id }
            fn name(&self) -> &'static str { $name }
            fn severity(&self) -> $crate::errors::Severity {
                $crate::__severity!($severity)
            }
            fn check(&self, $ctx: &$crate::rule::TypedContext) -> Vec<$crate::errors::AnalysisError> $body
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
}
