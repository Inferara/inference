use crate::nodes::{Ast, AstNode, Definition, SourceFile, TypeDefinition};
use rustc_hash::FxHashMap;
use std::rc::Rc;

#[derive(Default, Clone)]
pub struct Arena {
    pub(crate) nodes: FxHashMap<u32, AstNode>,
    pub(crate) node_routes: Vec<NodeRoute>,
}

impl Arena {
    #[must_use]
    pub fn source_files(&self) -> Vec<Rc<SourceFile>> {
        self.list_nodes_cmp(|node| {
            if let AstNode::Ast(Ast::SourceFile(source_file)) = node {
                Some(source_file.clone())
            } else {
                None
            }
        })
        .collect()
    }
    /// Adds a node to the arena and records its parent-child relationship.
    ///
    /// # Panics
    ///
    /// Panics if `node.id()` is zero or if a node with the same ID already exists in the arena.
    pub fn add_node(&mut self, node: AstNode, parent_id: u32) {
        // println!("Adding node with ID: {node:?}");
        assert!(node.id() != 0, "Node ID must be non-zero");
        assert!(
            !self.nodes.contains_key(&node.id()),
            "Node with ID {} already exists in the arena",
            node.id()
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

    #[must_use]
    pub fn find_node(&self, id: u32) -> Option<AstNode> {
        self.nodes.get(&id).cloned()
    }

    #[must_use]
    pub fn find_parent_node(&self, id: u32) -> Option<u32> {
        self.node_routes
            .iter()
            .find(|n| n.id == id)
            .cloned()
            .and_then(|node| node.parent)
    }

    pub fn get_children_cmp<F>(&self, id: u32, comparator: F) -> Vec<AstNode>
    where
        F: Fn(&AstNode) -> bool,
    {
        let mut result = Vec::new();
        let mut stack: Vec<AstNode> = Vec::new();

        if let Some(root_node) = self.find_node(id) {
            stack.push(root_node.clone());
        }

        while let Some(current_node) = stack.pop() {
            if comparator(&current_node) {
                result.push(current_node.clone());
            }
            stack.extend(
                self.list_nodes_children(current_node.id())
                    .into_iter()
                    .filter(|child| comparator(child)),
            );
        }

        result
    }

    #[must_use]
    pub fn list_type_definitions(&self) -> Vec<Rc<TypeDefinition>> {
        self.list_nodes_cmp(|node| {
            if let AstNode::Definition(Definition::Type(type_def)) = node {
                Some(type_def.clone())
            } else {
                None
            }
        })
        .collect()
    }

    pub fn filter_nodes<T: Fn(&AstNode) -> bool>(&self, fn_predicate: T) -> Vec<AstNode> {
        self.nodes
            .values()
            .filter(|node| fn_predicate(node))
            .cloned()
            .collect()
    }

    fn add_storage_node(&mut self, node: NodeRoute, parent: u32) {
        if let Some(parent_node) = self.node_routes.iter_mut().find(|n| n.id == parent) {
            parent_node.children.push(node.id);
        }
        self.node_routes.push(node);
    }

    fn list_nodes_children(&self, id: u32) -> Vec<AstNode> {
        self.node_routes
            .iter()
            .find(|n| n.id == id)
            .map(|node| {
                node.children
                    .iter()
                    .filter_map(|child_id| self.nodes.get(child_id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn list_nodes_cmp<'a, T, F>(&'a self, cmp: F) -> impl Iterator<Item = T> + 'a
    where
        F: Fn(&AstNode) -> Option<T> + Clone + 'a,
        T: Clone + 'static,
    {
        let cmp = cmp.clone();
        self.nodes.iter().filter_map(move |(_, node)| cmp(node))
    }
}

#[derive(Clone, Default)]
pub struct NodeRoute {
    pub id: u32,
    parent: Option<u32>,
    children: Vec<u32>,
}
