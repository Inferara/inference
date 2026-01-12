use crate::nodes::{Ast, AstNode, Definition, FunctionDefinition, SourceFile, TypeDefinition};
use rustc_hash::FxHashMap;
use std::rc::Rc;

/// Arena-based AST storage with O(1) node, parent, and children lookups.
///
/// The Arena stores all AST nodes in a hash map keyed by node ID. Parent-child
/// relationships are tracked in separate maps for efficient traversal:
/// - `parent_map`: Maps `node_id` -> `parent_id` for O(1) parent lookup
/// - `children_map`: Maps `node_id` -> `[child_ids]` for O(1) children lookup
///
/// Root nodes (`SourceFile`) are not stored in `parent_map` - their parent lookup
/// returns `None`.
#[derive(Default, Clone)]
pub struct Arena {
    pub(crate) nodes: FxHashMap<u32, AstNode>,
    pub(crate) parent_map: FxHashMap<u32, u32>,
    pub(crate) children_map: FxHashMap<u32, Vec<u32>>,
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
    #[must_use]
    pub fn functions(&self) -> Vec<Rc<FunctionDefinition>> {
        self.list_nodes_cmp(|node| {
            if let AstNode::Definition(Definition::Function(func_def)) = node {
                Some(func_def.clone())
            } else {
                None
            }
        })
        .collect()
    }
    /// Adds a node to the arena and records its parent-child relationship.
    ///
    /// Root nodes (`SourceFile`) are added with `parent_id = u32::MAX` as a sentinel.
    /// These are not stored in `parent_map`, so `find_parent_node()` returns `None` for them.
    ///
    /// # Panics
    ///
    /// Panics if `node.id()` is zero or if a node with the same ID already exists in the arena.
    pub fn add_node(&mut self, node: AstNode, parent_id: u32) {
        assert!(node.id() != 0, "Node ID must be non-zero");
        assert!(
            !self.nodes.contains_key(&node.id()),
            "Node with ID {} already exists in the arena",
            node.id()
        );
        let id = node.id();
        self.nodes.insert(id, node);

        // Root nodes (parent_id == u32::MAX) are not stored in parent_map
        if parent_id != u32::MAX {
            self.parent_map.insert(id, parent_id);
            self.children_map.entry(parent_id).or_default().push(id);
        }
    }

    #[must_use]
    pub fn find_node(&self, id: u32) -> Option<AstNode> {
        self.nodes.get(&id).cloned()
    }

    /// Returns the parent node ID for the given node, or `None` for root nodes.
    ///
    /// This is an O(1) hash map lookup.
    #[must_use]
    pub fn find_parent_node(&self, id: u32) -> Option<u32> {
        self.parent_map.get(&id).copied()
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

    /// Returns the direct children of a node as `AstNode` instances.
    ///
    /// This is an O(1) hash map lookup for the children list, plus O(c) to clone
    /// the child nodes where c is the number of children.
    fn list_nodes_children(&self, id: u32) -> Vec<AstNode> {
        self.children_map
            .get(&id)
            .map(|children| {
                children
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
