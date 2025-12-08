use crate::nodes::Type;

impl Type {
    pub(crate) fn is_unit_type(&self) -> bool {
        match self {
            Type::Simple(simple_type) => simple_type.name == "unit", //FIXME string comparison
            _ => false,
        }
    }
}
