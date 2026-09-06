//! The shared vocabulary of *value positions* a rule can name.
//!
//! Several rules reject a type or a literal wherever a value of it could be
//! introduced or consumed, and each renders one message parameterized by the
//! position that offended. The phrases are the same across those rules, and the
//! tests assert on them by value — so they live here once rather than being
//! copied per rule, where they could drift into naming the same position two
//! ways.
//!
//! Each constant is a noun phrase that reads directly after "cannot be used
//! as": the message supplies the surrounding sentence, so a phrase must never
//! carry punctuation or a leading article of its own beyond the one written
//! here.
//!
//! Not every rule covers every position. A rule's own module doc states which
//! ones it takes and, where the omission is load-bearing, why the others are
//! left alone.

/// A struct-literal expression, in any expression position.
pub(crate) const STRUCT_LITERAL: &str = "a struct literal";
/// The type of a string-literal expression, in any expression position. The
/// phrase names the *type*, not the literal, because the subject of the
/// sentence it lands in is a type name.
pub(crate) const STRING_LITERAL: &str = "the type of a string literal";
/// A `let` binding, or a `const` declaration at function or module scope.
pub(crate) const VARIABLE_TYPE: &str = "the declared type of a variable";
/// A function, method, or `external fn` parameter (the receiver has its own).
pub(crate) const PARAMETER_TYPE: &str = "the type of a parameter";
/// A function, method, or `external fn` return type.
pub(crate) const RETURN_TYPE: &str = "the return type of a function";
/// A struct field declaration.
pub(crate) const STRUCT_FIELD_TYPE: &str = "the type of a struct field";
/// A `self` / `mut self` receiver declared on the offending struct.
pub(crate) const SELF_RECEIVER_TYPE: &str = "the type of a `self` receiver";
