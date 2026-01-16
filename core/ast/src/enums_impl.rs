use crate::nodes::{SimpleTypeKind, Type};

impl Type {
    pub(crate) fn is_unit_type(&self) -> bool {
        matches!(self, Type::Simple(SimpleTypeKind::Unit))
    }
}
