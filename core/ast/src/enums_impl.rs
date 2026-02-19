//! Implementation methods for AST enum types.
//!
//! This module provides convenience methods for commonly-used type checks
//! and queries on AST enum variants.

use crate::nodes::{SimpleTypeKind, Type};

impl Type {
    /// Returns `true` if this type is the unit type `()`.
    ///
    /// Unit type is represented as `Type::Simple(SimpleTypeKind::Unit)`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use inference_ast::nodes::{Type, SimpleTypeKind};
    ///
    /// let unit_ty = Type::Simple(SimpleTypeKind::Unit);
    /// assert!(unit_ty.is_unit_type());
    ///
    /// let int_ty = Type::Simple(SimpleTypeKind::I32);
    /// assert!(!int_ty.is_unit_type());
    /// ```
    pub(crate) fn is_unit_type(&self) -> bool {
        matches!(self, Type::Simple(SimpleTypeKind::Unit))
    }
}
