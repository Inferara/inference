use std::collections::HashMap;

use crate::types::AstNode;

#[derive(Default, Clone)]
pub(crate) struct Arena {
    nodes: HashMap<u32, AstNode>,
    node_routes: Vec<NodeRoute>,
}

impl Arena {
    pub fn add_node(&mut self, node: AstNode, parent_id: u32) {
        assert!(node.id() != 0, "Node ID must be non-zero");
        assert!(
            !self.nodes.contains_key(&node.id()),
            "Node already exists in the arena"
        );
        let id = node.id();
        self.nodes.insert(node.id(), node);
        self.add_storage_node(
            NodeRoute {
                id,
                parent: Some(parent_id),
                children: vec![],
            },
            parent_id,
        );
    }

    fn add_storage_node(&mut self, node: NodeRoute, parent: u32) {
        if let Some(parent_node) = self.node_routes.iter_mut().find(|n| n.id == parent) {
            parent_node.children.push(node.id);
        }
        self.node_routes.push(node);
    }

    #[must_use]
    pub(crate) fn find_parent_node(&self, id: u32) -> Option<u32> {
        self.node_routes
            .iter()
            .find(|n| n.id == id)
            .cloned()
            .and_then(|node| node.parent)
    }

    // pub fn check_expressions_typed(&self) {}
}

#[derive(Clone, Default)]
pub struct NodeRoute {
    pub id: u32,
    parent: Option<u32>,
    children: Vec<u32>,
}
