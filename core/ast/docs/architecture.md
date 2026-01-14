# AST Architecture Guide

This document explains the design principles and implementation details of the arena-based AST system in the Inference compiler.

## Table of Contents

1. [Design Philosophy](#design-philosophy)
2. [Arena-Based Storage](#arena-based-storage)
3. [Node Identification](#node-identification)
4. [Parent-Child Relationships](#parent-child-relationships)
5. [Memory Layout](#memory-layout)
6. [Tree Traversal Algorithms](#tree-traversal-algorithms)

## Design Philosophy

The AST implementation follows three core principles:

### 1. Single Source of Truth
All AST nodes are stored in a single `Arena` structure. This eliminates:
- Scattered ownership across the tree
- Complex lifetime annotations
- Borrow checker conflicts during tree manipulation

### 2. ID-Based References
Nodes reference each other by `u32` IDs rather than pointers or `Rc` references. Benefits:
- No reference cycles or memory leaks
- Trivial to serialize/deserialize
- Cache-friendly for small node graphs
- Thread-safe sharing (IDs are Copy)

### 3. Optimized for Compiler Workloads
Compilers predominantly perform:
- Downward traversal (type checking, codegen)
- Upward queries (finding enclosing scope, source file)
- Rare mutations after initial construction

The arena is optimized for these access patterns.

## Arena-Based Storage

The `Arena` struct contains three hash maps:

```rust
pub struct Arena {
    pub(crate) nodes: FxHashMap<u32, AstNode>,
    pub(crate) parent_map: FxHashMap<u32, u32>,
    pub(crate) children_map: FxHashMap<u32, Vec<u32>>,
}
```

### Node Storage

```
┌─────────────────────────────────────┐
│ nodes: FxHashMap<u32, AstNode>     │
├─────────┬───────────────────────────┤
│ ID      │ Node                      │
├─────────┼───────────────────────────┤
│ 1       │ SourceFile { ... }        │
│ 2       │ FunctionDefinition { ... }│
│ 3       │ Block { ... }             │
│ 4       │ ReturnStatement { ... }   │
│ 5       │ NumberLiteral { ... }     │
└─────────┴───────────────────────────┘
```

Every node has a unique, non-zero ID. Zero is reserved as a sentinel value meaning "no node".

### Parent Map

Maps child ID to parent ID for O(1) upward traversal:

```
┌─────────────────────────────────────┐
│ parent_map: FxHashMap<u32, u32>    │
├─────────┬───────────────────────────┤
│ Child   │ Parent                    │
├─────────┼───────────────────────────┤
│ 2       │ 1  (Function → SourceFile)│
│ 3       │ 2  (Block → Function)     │
│ 4       │ 3  (Return → Block)       │
│ 5       │ 4  (Number → Return)      │
└─────────┴───────────────────────────┘
```

Root nodes (like `SourceFile`) are not present in `parent_map`. Querying their parent returns `None`.

### Children Map

Maps parent ID to list of child IDs for O(1) children list retrieval:

```
┌──────────────────────────────────────────┐
│ children_map: FxHashMap<u32, Vec<u32>>  │
├─────────┬────────────────────────────────┤
│ Parent  │ Children                       │
├─────────┼────────────────────────────────┤
│ 1       │ [2]  (SourceFile has Function) │
│ 2       │ [3]  (Function has Block)      │
│ 3       │ [4]  (Block has Return)        │
│ 4       │ [5]  (Return has Number)       │
└─────────┴────────────────────────────────┘
```

## Node Identification

### ID Assignment

IDs are assigned sequentially during AST construction by `AstBuilder` using an atomic counter (Issue #86):

```rust
impl AstBuilder {
    /// Generate a unique node ID using an atomic counter.
    ///
    /// Uses a global atomic counter to ensure unique IDs across all AST nodes.
    /// Starting from 1 (0 is reserved as invalid/uninitialized).
    fn get_node_id() -> u32 {
        static COUNTER: AtomicU32 = AtomicU32::new(1);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }
}
```

**Why Atomic Counter (Issue #86)**:

The previous implementation used UUID-based ID generation (`uuid::Uuid::new_v4().as_u128() as u32`), which had several drawbacks:
- Non-deterministic IDs made debugging harder
- Truncating 128-bit UUIDs to 32-bit risked collisions
- Random ordering made testing and debugging less predictable

The atomic counter approach provides:
- **Deterministic ordering**: Earlier nodes have lower IDs, matching parse order
- **Sequential allocation**: IDs start at 1 and increment monotonically
- **Thread-safe**: `AtomicU32` with relaxed ordering is safe for concurrent access
- **Better debugging**: ID correlates with parse order, making AST inspection easier
- **No collisions**: Guaranteed unique IDs up to 4 billion nodes
- **Zero is reserved**: ID 0 represents invalid/uninitialized nodes

### ID Invariants

The system maintains these invariants:

1. **Non-zero IDs**: No node has ID 0
2. **Unique IDs**: Each node has a distinct ID
3. **ID stability**: Once assigned, IDs never change
4. **Sequential allocation**: IDs increase during construction

### AstNode Enum

All node types are wrapped in the `AstNode` enum:

```rust
pub enum AstNode {
    Ast(Ast),
    Directive(Directive),
    Definition(Definition),
    BlockType(BlockType),
    Statement(Statement),
    Expression(Expression),
    Literal(Literal),
    Type(Type),
    ArgumentType(ArgumentType),
    Misc(Misc),
}
```

This enum provides uniform access to `id()` and `location()` methods regardless of node type.

## Parent-Child Relationships

### Adding Nodes

When building the tree, `add_node()` records both the node and its parent-child relationship:

```rust
pub fn add_node(&mut self, node: AstNode, parent_id: u32) {
    let id = node.id();

    // Store the node itself
    self.nodes.insert(id, node);

    // Record parent-child relationship (unless it's a root)
    if parent_id != u32::MAX {
        self.parent_map.insert(id, parent_id);
        self.children_map.entry(parent_id).or_default().push(id);
    }
}
```

The sentinel value `u32::MAX` indicates a root node (no parent).

### Tree Structure Example

For this source code:

```inference
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
```

The tree structure looks like:

```
┌─────────────────────┐
│ SourceFile (ID: 1)  │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ FunctionDef (ID: 2) │
│ name: "add"         │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ Block (ID: 3)       │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ Return (ID: 4)      │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ Binary (ID: 5)      │
│ operator: Add       │
└──────────┬──────────┘
           │
      ┌────┴────┐
      ▼         ▼
┌─────────┐ ┌─────────┐
│ Ident   │ │ Ident   │
│ (ID: 6) │ │ (ID: 7) │
│ "a"     │ │ "b"     │
└─────────┘ └─────────┘
```

### Parent Queries

Finding a node's parent is O(1):

```rust
pub fn find_parent_node(&self, id: u32) -> Option<u32> {
    self.parent_map.get(&id).copied()
}
```

Walking up to the root:

```rust
let mut current_id = node_id;
while let Some(parent_id) = arena.find_parent_node(current_id) {
    println!("Parent: {}", parent_id);
    current_id = parent_id;
}
// current_id is now the root
```

### Children Queries

Finding a node's children is O(1) for the list lookup:

```rust
pub fn list_nodes_children(&self, id: u32) -> Vec<AstNode> {
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
```

## Memory Layout

### Before Optimization (Issue #69)

Each `Location` contained a full source string copy:

```rust
// Old Location (per node)
struct Location {
    source: String,          // ~24 bytes + heap allocation
    offset_start: u32,       // 4 bytes
    offset_end: u32,         // 4 bytes
    start_line: u32,         // 4 bytes
    start_column: u32,       // 4 bytes
    end_line: u32,           // 4 bytes
    end_column: u32,         // 4 bytes
}
// Total: ~52 bytes per node + N heap allocations
```

For a 1000-node AST with 10KB source:
- Memory overhead: 52 bytes × 1000 = 52KB
- Heap allocations: 1000 strings × 10KB = ~10MB
- **Total: ~10MB overhead**

### After Optimization

```rust
// New Location (per node) - Copy type
#[derive(Copy)]
struct Location {
    offset_start: u32,       // 4 bytes
    offset_end: u32,         // 4 bytes
    start_line: u32,         // 4 bytes
    start_column: u32,       // 4 bytes
    end_line: u32,           // 4 bytes
    end_column: u32,         // 4 bytes
}
// Total: 24 bytes per node (no heap allocations)

// Source stored once
struct SourceFile {
    source: String,          // ~24 bytes + 1 heap allocation
    // ... other fields
}
```

For the same 1000-node AST:
- Memory overhead: 24 bytes × 1000 = 24KB
- Heap allocations: 1 string × 10KB = 10KB
- **Total: ~34KB overhead (98% reduction)**

### Cache Efficiency

Stack-allocated `Location` (24 bytes) fits in L1 cache lines (typically 64 bytes). This means:
- 2-3 locations per cache line
- No pointer chasing to heap
- Improved CPU cache utilization during traversal

## Tree Traversal Algorithms

### Depth-First Search

Traversing all descendants of a node:

```rust
pub fn get_children_cmp<F>(&self, id: u32, comparator: F) -> Vec<AstNode>
where
    F: Fn(&AstNode) -> bool,
{
    let mut result = Vec::new();
    let mut stack: Vec<AstNode> = Vec::new();

    if let Some(root_node) = self.find_node(id) {
        stack.push(root_node);
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
```

### Finding Source File Ancestor

Walking up the tree to find the enclosing `SourceFile`:

```rust
pub fn find_source_file_for_node(&self, node_id: u32) -> Option<u32> {
    let node = self.nodes.get(&node_id)?;

    // Early return if this is already a SourceFile
    if matches!(node, AstNode::Ast(Ast::SourceFile(_))) {
        return Some(node_id);
    }

    // Walk up parent chain
    let mut current_id = node_id;
    while let Some(parent_id) = self.parent_map.get(&current_id).copied() {
        current_id = parent_id;
    }

    // Check if the root is a SourceFile
    let root_node = self.nodes.get(&current_id)?;
    if matches!(root_node, AstNode::Ast(Ast::SourceFile(_))) {
        Some(current_id)
    } else {
        None
    }
}
```

Complexity: O(d) where d is tree depth, typically < 20 for well-formed code.

### Filtered Iteration

Finding all nodes of a specific type:

```rust
pub fn list_nodes_cmp<'a, T, F>(&'a self, cmp: F) -> impl Iterator<Item = T> + 'a
where
    F: Fn(&AstNode) -> Option<T> + Clone + 'a,
    T: Clone + 'static,
{
    self.nodes
        .iter()
        .filter_map(move |(_, node)| cmp(node))
}

// Usage: find all functions
arena.list_nodes_cmp(|node| {
    if let AstNode::Definition(Definition::Function(func)) = node {
        Some(func.clone())
    } else {
        None
    }
})
```

## AST Construction Details

### Visibility Parsing (Issue #86)

The AST builder extracts visibility modifiers from the tree-sitter CST (Concrete Syntax Tree) during node construction:

```rust
/// Extracts visibility modifier from a definition CST node.
/// Returns `Visibility::Public` if a "visibility" child field is present,
/// otherwise returns `Visibility::Private` (the default).
fn get_visibility(node: &Node) -> Visibility {
    node.child_by_field_name("visibility")
        .map(|_| Visibility::Public)
        .unwrap_or_default()
}
```

**How It Works**:

1. Tree-sitter grammar defines a `visibility` field for definition nodes
2. Builder checks for presence of this field during parsing
3. If present, the definition is marked `Public`
4. If absent, defaults to `Private`

**Supported Definitions**:
- `FunctionDefinition` - `pub fn name() { ... }`
- `StructDefinition` - `pub struct Name { ... }`
- `EnumDefinition` - `pub enum Name { ... }`
- `ConstantDefinition` - `pub const NAME: Type = value;`
- `TypeDefinition` - `pub type Alias = Type;`
- `ModuleDefinition` - `pub mod name { ... }`

**Example Parsing**:

```inference
pub fn public_function() -> i32 { 42 }  // Visibility::Public
fn private_function() -> i32 { 0 }       // Visibility::Private
```

Tree-sitter produces:
```
function_definition [
  visibility: "pub"     // Visibility field present
  name: "public_function"
  ...
]

function_definition [
  // No visibility field
  name: "private_function"
  ...
]
```

The builder queries the CST node for the `visibility` field and sets the appropriate `Visibility` enum value.

**Design Rationale**:

This approach provides:
- **Simplicity**: Single function handles all definition types
- **Consistency**: All definitions use the same visibility logic
- **Default safety**: Missing visibility defaults to private (principle of least privilege)
- **Grammar alignment**: Directly maps tree-sitter fields to AST properties

## Design Trade-offs

### Pros

- **Simple ownership**: Arena owns everything, no lifetime parameters
- **Fast lookups**: O(1) node, parent, and children access
- **Memory efficient**: Compact Location, single source storage
- **Type safe**: Exhaustive enum matching catches missing cases
- **Debuggable**: Sequential IDs make debugging easier

### Cons

- **No mutations**: Changing the tree structure after construction is complex
- **Memory overhead**: Hash maps have load factor overhead (~1.5x capacity)
- **Cloning cost**: Accessing nodes requires cloning (mitigated by `Rc` wrapping)
- **No cross-arena references**: Can't easily merge or split arenas

### When This Design Works Well

- Immutable ASTs (compiler phases don't modify structure)
- Single-threaded processing (or read-only parallel access)
- Moderate tree sizes (< 1 million nodes)
- Frequent parent/child queries

### When to Consider Alternatives

- Incremental compilation (need partial tree updates)
- Large ASTs (> 10 million nodes)
- Heavy structural mutations (tree rewriting passes)
- Multi-threaded tree construction

## Future Optimizations

Potential improvements for consideration:

1. **Interned strings**: Use string interning for identifiers
2. **Bump allocator**: Replace FxHashMap with bump-allocated nodes
3. **Compressed IDs**: Use 16-bit IDs for small ASTs
4. **Node pooling**: Reuse node structures across compilations
5. **Lazy source loading**: mmap source files for large inputs

## Related Documentation

- [Arena API Guide](arena-api.md) - Comprehensive API reference
- [Location Optimization](location.md) - Details on memory-efficient locations
- [Node Types](nodes.md) - AST node type reference
