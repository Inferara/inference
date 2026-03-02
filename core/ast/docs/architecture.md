# AST Architecture Guide

This document explains the design principles and implementation details of the arena-based AST system in the Inference compiler.

## Table of Contents

1. [Design Philosophy](#design-philosophy)
2. [Arena-Based Storage](#arena-based-storage)
3. [Node Identification](#node-identification)
4. [Memory Layout](#memory-layout)
5. [Tree Traversal Algorithms](#tree-traversal-algorithms)

## Design Philosophy

The AST implementation follows three core principles:

### 1. Single Source of Truth
All AST nodes are stored in a single `AstArena` structure. This eliminates:
- Scattered ownership across the tree
- Complex lifetime annotations
- Borrow checker conflicts during tree manipulation

### 2. ID-Based References
Nodes reference each other by typed indices (`Idx<T>` from `la_arena`) rather than pointers or `Rc` references. Benefits:
- No reference cycles or memory leaks
- Trivial to serialize/deserialize
- Cache-friendly for small node graphs
- Thread-safe sharing (indices are `Copy`)

### 3. Optimized for Compiler Workloads
Compilers predominantly perform:
- Downward traversal (type checking, codegen)
- Upward queries (finding enclosing scope, source file)
- Rare mutations after initial construction

The arena is optimized for these access patterns.

## Arena-Based Storage

`AstArena` stores all nodes in seven typed `la_arena::Arena<T>` fields:

```rust
pub struct AstArena {
    pub source_files : Arena<SourceFileData>,
    pub defs         : Arena<DefData>,
    pub stmts        : Arena<StmtData>,
    pub exprs        : Arena<ExprData>,
    pub types        : Arena<TypeData>,
    pub blocks       : Arena<BlockData>,
    pub idents       : Arena<Ident>,
}
```

### Node Storage

```
┌─────────────────────────────────────────┐
│ exprs: Arena<ExprData>                  │
├─────────────────┬───────────────────────┤
│ Idx<ExprData>   │ ExprData              │
├─────────────────┼───────────────────────┤
│ idx(0)          │ ExprData { Binary }   │
│ idx(1)          │ ExprData { Literal }  │
│ idx(2)          │ ExprData { Call }     │
└─────────────────┴───────────────────────┘
```

Every allocation returns a typed `Idx<T>` index, which is the only reference to that node.

### Allocation API

```rust
// Builder-side allocation
let expr_id: ExprId = arena.exprs.alloc(ExprData { location, kind });
let stmt_id: StmtId = arena.stmts.alloc(StmtData { location, kind });

// Consumer-side access — O(1)
let expr_data: &ExprData = &arena[expr_id];
let stmt_data: &StmtData = &arena[stmt_id];
```

## Node Identification

### Typed Index Aliases

Each arena category has a corresponding type alias over `la_arena::Idx<T>`:

```rust
pub type SourceFileId = Idx<SourceFileData>;
pub type DefId        = Idx<DefData>;
pub type StmtId       = Idx<StmtData>;
pub type ExprId       = Idx<ExprData>;
pub type TypeId       = Idx<TypeData>;
pub type BlockId      = Idx<BlockData>;
pub type IdentId      = Idx<Ident>;
```

Because `Idx<T>` is parameterized over the node type, using an `ExprId` to index `arena.defs` is a compile-time type error. This eliminates a whole class of bugs present in untyped ID schemes.

### ID Invariants

1. **Type-checked**: An `Idx<ExprData>` can only index `arena.exprs`
2. **Unique per category**: Each call to `arena.exprs.alloc()` returns a distinct `ExprId`
3. **ID stability**: Once assigned, indices never change
4. **Sequential allocation**: Indices are assigned in allocation order

### NodeId Enum

The `NodeId` enum wraps any typed ID for use in heterogeneous contexts (such as storing type annotations keyed by AST node):

```rust
pub enum NodeId {
    SourceFile(SourceFileId),
    Def(DefId),
    Stmt(StmtId),
    Expr(ExprId),
    Type(TypeId),
    Block(BlockId),
    Ident(IdentId),
}
```

### Node Type System

Each arena category uses a two-level structure: a wrapper struct that holds `location` plus a flat `kind` enum:

```
ExprData  { location: Location, kind: Expr     }
StmtData  { location: Location, kind: Stmt     }
DefData   { location: Location, kind: Def      }
TypeData  { location: Location, kind: TypeNode }
```

Blocks and identifiers are simpler:

```
BlockData { location: Location, block_kind: BlockKind, stmts: Vec<StmtId> }
Ident     { location: Location, name: String }
```

The top-level source file node stores the entire source string:

```
SourceFileData { location: Location, source: String, defs: Vec<DefId>, directives: Vec<Directive> }
```

Node kinds (`Expr`, `Stmt`, `Def`, `TypeNode`) are plain enums. References between nodes use typed IDs: for example, `Expr::Binary { left: ExprId, right: ExprId, op: OperatorKind }`.

## Memory Layout

### Location (per node) — Copy type

```rust
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
```

Source text is stored once per file in `SourceFileData.source` and retrieved by byte-offset slicing. See [Location Optimization](location.md) for details.

For a 1000-node AST with 10KB source:
- Memory overhead: 24 bytes × 1000 = 24KB
- Heap allocations: 1 string × 10KB = 10KB
- **Total: ~34KB overhead**

### Cache Efficiency

Stack-allocated `Location` (24 bytes) fits in L1 cache lines (typically 64 bytes):
- 2-3 locations per cache line
- No pointer chasing to heap
- Improved CPU cache utilization during traversal

## Tree Traversal Algorithms

### Structural Traversal (Primary Pattern)

The recommended way to traverse the AST is to follow typed IDs structurally, starting from `source_files`:

```
source_files[i]              SourceFileData
  .defs[j]                   DefId → DefData
    .kind = Def::Function
      .body                  BlockId → BlockData
        .stmts[k]            StmtId → StmtData
          .kind = Stmt::Return
            .expr            ExprId → ExprData
```

```rust
use inference_ast::nodes::{Def, Stmt, Expr, OperatorKind};

for sf in arena.source_files() {
    for &def_id in &sf.defs {
        if let Def::Function { body, .. } = &arena[def_id].kind {
            for &stmt_id in &arena[*body].stmts {
                if let Stmt::Return { expr } = arena[stmt_id].kind {
                    if let Expr::Binary { op, .. } = &arena[expr].kind {
                        if *op == OperatorKind::Add {
                            println!("Found an addition at {}", arena[expr].location);
                        }
                    }
                }
            }
        }
    }
}
```

### Finding Source File Ancestor

For `Def` nodes, `find_source_file_for_def` searches the `defs` lists of all source files. For other nodes, `find_source_file_for_node` uses byte-offset matching: it checks whether the node's byte offsets fall within each `SourceFileData`'s source string.

```rust
// For any node type
let sf_id = arena.find_source_file_for_node(NodeId::Stmt(stmt_id));

// More direct path when you have a DefId
let sf_id = arena.find_source_file_for_def(def_id);
```

Complexity: O(n) where n is the number of source files (typically 1 for single-file compilation).

### Filtered Iteration

When searching across all definitions, iterate `source_files → defs`:

```rust
// Find all struct names
for sf in arena.source_files() {
    for &def_id in &sf.defs {
        if let Def::Struct { name, .. } = &arena[def_id].kind {
            println!("Struct: {}", arena.ident_name(*name));
        }
    }
}
```

## AST Construction Details

### Builder API

```rust
let mut builder = Builder::new();
builder.add_source_code(tree.root_node(), source.as_bytes());
let arena = builder.build_ast()?;
```

`Builder` walks the tree-sitter CST and allocates typed AST nodes into the arena. It returns an immutable `AstArena`, or an error if parse errors are present.

### Error Collection During Building

The Builder collects errors during AST construction:

```rust
impl Builder {
    pub fn build_ast(&mut self) -> anyhow::Result<AstArena> {
        // build nodes...

        if !self.errors.is_empty() {
            return Err(anyhow::anyhow!("AST building failed due to errors"));
        }
        Ok(self.arena.clone())
    }
}
```

Each builder method that processes CST nodes calls error collection to identify malformed syntax. If any errors are collected, `build_ast()` returns an error.

### Visibility Parsing

The AST builder extracts visibility modifiers from the tree-sitter CST during node construction:

```rust
fn get_visibility(node: &Node) -> Visibility {
    node.child_by_field_name("visibility")
        .map(|_| Visibility::Public)
        .unwrap_or_default()
}
```

Supported definitions: `FunctionDefinition`, `StructDefinition`, `EnumDefinition`, `ConstantDefinition`, `TypeDefinition`, `ModuleDefinition`.

## Design Trade-offs

### Pros

- **Simple ownership**: Arena owns everything, no lifetime parameters
- **Fast lookups**: O(1) node access via typed indices
- **Memory efficient**: Compact Location, single source storage
- **Type safe**: `Idx<T>` parameterization catches index mismatches at compile time
- **No parent map overhead**: No hash map maintenance during construction

### Cons

- **No mutations**: Changing the tree structure after construction is complex
- **No upward traversal**: There are no parent pointers; callers pass context down explicitly or use structural search
- **No cross-arena references**: Can't easily merge or split arenas

### When This Design Works Well

- Immutable ASTs (compiler phases don't modify structure)
- Single-threaded processing (or read-only parallel access)
- Moderate tree sizes (< 1 million nodes)
- Predominantly downward traversal

## Related Documentation

- [Arena API Guide](arena-api.md) - Comprehensive API reference
- [Location Optimization](location.md) - Details on memory-efficient locations
- [Node Types](nodes.md) - AST node type reference
