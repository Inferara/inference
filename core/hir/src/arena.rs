use crate::nodes::SourceFile;

#[derive(Default, Clone)]
pub struct Arena {
    pub sources: Vec<SourceFile>,
}