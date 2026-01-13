use crate::nodes::{Ast, AstNode, Definition, FunctionDefinition, SourceFile, TypeDefinition};
use rustc_hash::FxHashMap;
use std::rc::Rc;

/// Arena-based AST storage with O(1) node and parent lookups.
///
/// The Arena stores all AST nodes in a hash map keyed by node ID. Parent-child
/// relationships are tracked in separate maps for efficient traversal:
/// - `parent_map`: Maps `node_id` -> `parent_id` for O(1) parent lookup
/// - `children_map`: Maps `node_id` -> `[child_ids]` for O(1) lookup of the children list,
///   plus O(c) to access child nodes where c is the number of children
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
    /// Panics if the node ID is zero or if a node with the same ID already exists.
    /// These conditions indicate bugs in the builder, not recoverable runtime errors.
    pub fn add_node(&mut self, node: AstNode, parent_id: u32) {
        assert!(node.id() != 0, "node ID must be non-zero");
        assert!(
            !self.nodes.contains_key(&node.id()),
            "node with ID {} already exists in the arena",
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

    /// Finds the root `SourceFile` ancestor for the given node.
    ///
    /// Traverses the parent chain from `node_id` to find the root ancestor.
    /// Returns `Some(source_file_id)` if found, `None` if the node doesn't exist
    /// or has no `SourceFile` ancestor.
    ///
    /// # Complexity
    ///
    /// `O(tree_depth)`, typically < 20 levels for well-formed ASTs.
    /// Each parent lookup is `O(1)` using `parent_map`.
    #[must_use]
    pub fn find_source_file_for_node(&self, node_id: u32) -> Option<u32> {
        let node = self.nodes.get(&node_id)?;

        if matches!(node, AstNode::Ast(Ast::SourceFile(_))) {
            return Some(node_id);
        }

        let mut current_id = node_id;
        while let Some(parent_id) = self.parent_map.get(&current_id).copied() {
            current_id = parent_id;
        }

        let root_node = self.nodes.get(&current_id)?;
        if matches!(root_node, AstNode::Ast(Ast::SourceFile(_))) {
            Some(current_id)
        } else {
            None
        }
    }

    /// Returns the source text for a node using its byte offset range.
    ///
    /// Retrieves the source text by slicing `SourceFile.source[offset_start..offset_end]`.
    /// Returns `None` if:
    /// - The node ID doesn't exist
    /// - No `SourceFile` ancestor exists
    /// - The byte offsets are out of bounds
    ///
    /// # Complexity
    ///
    /// `O(tree_depth)` for finding the source file, plus `O(1)` for the string slice.
    /// Tree depth is typically < 20 levels for well-formed ASTs.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let source = arena.get_node_source(function_id);
    /// assert_eq!(source, Some("fn add(a: i32) -> i32 { return a; }"));
    /// ```
    #[must_use]
    pub fn get_node_source(&self, node_id: u32) -> Option<&str> {
        let source_file_id = self.find_source_file_for_node(node_id)?;
        let node = self.nodes.get(&node_id)?;
        let location = node.location();

        let source_file_node = self.nodes.get(&source_file_id)?;
        let source = match source_file_node {
            AstNode::Ast(Ast::SourceFile(sf)) => &sf.source,
            _ => return None,
        };

        let start = location.offset_start as usize;
        let end = location.offset_end as usize;

        if start <= end && end <= source.len() {
            source.get(start..end)
        } else {
            None
        }
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
