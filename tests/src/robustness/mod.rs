//! Tests for the compiler's behaviour on inputs that stress it physically
//! rather than semantically — very deep nesting, very long chains, and anything
//! else whose cost scales with the shape of the source rather than its meaning.

mod deep_syntax;
