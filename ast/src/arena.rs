use std::collections::HashMap;

use crate::types::AstNode;

#[derive(Default, Clone)]
pub(crate) struct Arena {
    nodes: HashMap<u32, AstNode>,
}

impl Arena {
    pub fn add_node(&mut self, node: AstNode) {
        assert!(node.id() != 0, "Node ID must be non-zero");
        assert!(
            !self.nodes.contains_key(&node.id()),
            "Node already exists in the arena"
        );
        self.nodes.insert(node.id(), node);
    }
}
