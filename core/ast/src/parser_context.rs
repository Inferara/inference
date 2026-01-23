//! Multi-file AST parsing context.
//!
//! Manages parsing across multiple source files, handling module resolution
//! and building a unified AST with proper scope relationships.
//!
//! # Status
//!
//! Basic multi-file support is implemented by scanning for `mod` declarations and
//! parsing referenced files into a unified AST arena.
//!
//! # Planned Implementation
//!
//! The parsing context will:
//! 1. Initialize with a root file path
//! 2. Process the queue of files, building AST for each
//! 3. Handle module declarations (`mod name;` and `mod name { ... }`)
//! 4. Resolve submodule file paths following Inference conventions
//!
//! Reference implementation patterns are preserved in function doc comments.

use std::path::PathBuf;
use std::rc::Rc;

use crate::arena::Arena;
use crate::builder::{Builder, LocationBase};
use crate::nodes::{
    Ast, AstNode, Definition, Directive, Expression, Identifier, Location, ModuleDefinition,
    SourceFile, Visibility,
};
use tree_sitter::Parser;

/// Queue entry for pending file parsing.
#[allow(dead_code)]
struct ParseQueueEntry {
    /// The scope this file belongs to.
    scope_id: u32,
    /// Path to the source file.
    file_path: PathBuf,
    /// Module declaration to populate when parsing external modules.
    module: Option<Rc<ModuleDefinition>>,
}

/// Context for parsing multiple source files.
///
/// Maintains a queue of files to parse and tracks the relationships
/// between modules and their source files.
#[allow(dead_code)]
pub struct ParserContext {
    /// Current node ID counter.
    next_id: u32,
    /// Queue of files pending parsing.
    queue: Vec<ParseQueueEntry>,
    /// The arena being built.
    arena: Arena,
}

impl ParserContext {
    /// Creates a new parser context starting from a root file.
    ///
    /// The root file is added to the parse queue with scope ID 0 (root scope).
    #[must_use]
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            next_id: 0,
            queue: vec![ParseQueueEntry {
                scope_id: 0,
                file_path: root_path,
                module: None,
            }],
            arena: Arena::default(),
        }
    }

    /// Pushes a new file onto the parse queue for submodule resolution.
    ///
    /// # Planned Implementation
    ///
    /// Will add the file to the queue with its parent scope ID, enabling
    /// proper scope relationships when the file is parsed.
    pub fn push_file(
        &mut self,
        scope_id: u32,
        file_path: PathBuf,
        module: Option<Rc<ModuleDefinition>>,
    ) {
        self.queue
            .push(ParseQueueEntry { scope_id, file_path, module });
    }

    /// Parses all queued files and builds the unified AST.
    ///
    /// # Planned Implementation
    ///
    /// ```text
    /// while let Some(entry) = self.queue.pop() {
    ///     let ast_file = self.parse_file(&entry.file_path);
    ///     for child in ast_file.children {
    ///         match child {
    ///             Directive::Use(u) => { /* add to scope imports */ }
    ///             Definition::Module(m) => { self.process_module(m, entry.scope_id); }
    ///             _ => { /* process other definitions */ }
    ///         }
    ///     }
    /// }
    /// ```
    pub fn parse_all(&mut self) -> anyhow::Result<Arena> {
        while let Some(entry) = self.queue.pop() {
            let store_definitions = entry.module.is_none();
            let (file_arena, definitions) =
                Self::parse_file(&entry.file_path, store_definitions)?;

            if let Some(module) = entry.module {
                *module.body.borrow_mut() = Some(definitions.clone());
            }

            for definition in &definitions {
                if let Definition::Module(module_definition) = definition {
                    self.process_module(module_definition, entry.scope_id, &entry.file_path);
                }
            }

            let Arena {
                nodes,
                parent_map,
                children_map,
            } = file_arena;
            self.arena.nodes.extend(nodes);
            self.arena.parent_map.extend(parent_map);
            for (parent_id, children) in children_map {
                self.arena
                    .children_map
                    .entry(parent_id)
                    .or_default()
                    .extend(children);
            }
        }
        Ok(std::mem::take(&mut self.arena))
    }

    /// Resolves and processes a module definition.
    ///
    /// # Planned Implementation
    ///
    /// Handles both external and inline module declarations:
    ///
    /// ```text
    /// if module.body.is_none() {
    ///     // External module: `mod name;` - find the file
    ///     let mod_path = find_submodule_path(current_file_path, &module.name);
    ///     let mod_scope = create_child_scope(parent_scope_id, &module.name);
    ///     self.push_file(mod_scope.id, mod_path);
    /// } else {
    ///     // Inline module: `mod name { ... }`
    ///     let mod_scope = create_child_scope(parent_scope_id, &module.name);
    ///     for def in &module.body {
    ///         self.process_definition(def, mod_scope.id);
    ///     }
    /// }
    /// ```
    #[allow(dead_code, clippy::unused_self)]
    fn process_module(
        &mut self,
        module: &Rc<ModuleDefinition>,
        _parent_scope_id: u32,
        current_file_path: &PathBuf,
    ) {
        let module_scope_id = self.next_node_id();

        if module.body.borrow().is_none() {
            if let Some(mod_path) = find_submodule_path(current_file_path, &module.name()) {
                self.push_file(module_scope_id, mod_path, Some(Rc::clone(module)));
            }
            return;
        }

        if let Some(body) = module.body.borrow().as_ref() {
            for definition in body {
                if let Definition::Module(child_module) = definition {
                    self.process_module(child_module, module_scope_id, current_file_path);
                }
            }
        }
    }

    /// Generates a new unique node ID.
    #[allow(dead_code)]
    fn next_node_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn parse_file(
        file_path: &PathBuf,
        store_definitions: bool,
    ) -> anyhow::Result<(Arena, Vec<Definition>)> {
        let source = std::fs::read_to_string(file_path)?;
        let line_index = LineIndex::new(&source);
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_inference::language())
            .map_err(|_| anyhow::anyhow!("Error loading Inference grammar"))?;
        let mut builder = Builder::new();
        let source_file_id = Builder::next_node_id();
        let location = location_from_offsets(&line_index, 0, source.len());
        let (definitions, directives) = parse_block_definitions(
            &mut builder,
            &mut parser,
            &line_index,
            &source,
            0,
            source_file_id,
            store_definitions,
        )?;

        let mut source_file = SourceFile::new(source_file_id, location, source);
        if store_definitions {
            source_file.directives = directives;
            source_file.definitions = definitions.clone();
        }

        builder.add_node(
            AstNode::Ast(Ast::SourceFile(Rc::new(source_file))),
            u32::MAX,
        );

        let errors = builder.take_errors();
        if !errors.is_empty() {
            for err in errors {
                eprintln!("AST Builder Error: {err}");
            }
            return Err(anyhow::anyhow!("AST building failed due to errors"));
        }

        Ok((builder.into_arena(), definitions))
    }
}

#[derive(Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

struct ModuleDecl {
    name: String,
    visibility: Visibility,
    span: Span,
    name_span: Span,
    body: Option<Span>,
}

struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(idx + 1);
            }
        }
        Self { starts }
    }

    fn line_col(&self, offset: usize) -> (u32, u32) {
        let line_idx = match self.starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        let line_start = self.starts.get(line_idx).copied().unwrap_or(0);
        let line = line_idx as u32 + 1;
        let column = (offset - line_start) as u32 + 1;
        (line, column)
    }
}

fn location_from_offsets(line_index: &LineIndex, start: usize, end: usize) -> Location {
    let (start_line, start_column) = line_index.line_col(start);
    let (end_line, end_column) = line_index.line_col(end);
    Location::new(
        start as u32,
        end as u32,
        start_line,
        start_column,
        end_line,
        end_column,
    )
}

fn location_base(line_index: &LineIndex, offset: usize) -> LocationBase {
    let (line, column) = line_index.line_col(offset);
    LocationBase {
        offset: offset as u32,
        line: line.saturating_sub(1),
        column: column.saturating_sub(1),
    }
}

fn parse_block_definitions(
    builder: &mut Builder,
    parser: &mut Parser,
    line_index: &LineIndex,
    source: &str,
    base_offset: usize,
    parent_id: u32,
    include_directives: bool,
) -> anyhow::Result<(Vec<Definition>, Vec<Directive>)> {
    let (modules, sanitized_source) = scan_modules(source);
    let tree = parser
        .parse(&sanitized_source, None)
        .ok_or_else(|| anyhow::anyhow!("Parse error"))?;
    let root = tree.root_node();

    let base = location_base(line_index, base_offset);
    builder.set_location_base(base);

    let mut directives: Vec<Directive> = Vec::new();
    let mut definitions = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "use_directive" if include_directives => {
                directives.push(Directive::Use(builder.build_use_directive(
                    parent_id,
                    &child,
                    sanitized_source.as_bytes(),
                )));
            }
            "use_directive" => {}
            _ => {
                let definition = builder.build_definition(
                    parent_id,
                    &child,
                    sanitized_source.as_bytes(),
                );
                definitions.push((definition.location().offset_start, definition));
            }
        }
    }

    builder.reset_location_base();

    for module in modules {
        let module_def = build_module_definition(
            builder,
            parser,
            line_index,
            source,
            base_offset,
            parent_id,
            module,
        )?;
        let offset = module_def.location.offset_start;
        definitions.push((offset, Definition::Module(module_def)));
    }

    definitions.sort_by_key(|(offset, _)| *offset);
    let definitions = definitions
        .into_iter()
        .map(|(_, definition)| definition)
        .collect();

    Ok((definitions, directives))
}

fn build_module_definition(
    builder: &mut Builder,
    parser: &mut Parser,
    line_index: &LineIndex,
    source: &str,
    base_offset: usize,
    parent_id: u32,
    module: ModuleDecl,
) -> anyhow::Result<Rc<ModuleDefinition>> {
    let ModuleDecl {
        name,
        visibility,
        span,
        name_span,
        body,
    } = module;

    let module_id = Builder::next_node_id();
    let name_id = Builder::next_node_id();

    let name_start = base_offset + name_span.start;
    let name_end = base_offset + name_span.end;
    let name_location = location_from_offsets(line_index, name_start, name_end);
    let name_node = Rc::new(Identifier::new(name_id, name, name_location));
    builder.add_node(
        AstNode::Expression(Expression::Identifier(name_node.clone())),
        module_id,
    );

    let module_start = base_offset + span.start;
    let module_end = base_offset + span.end;
    let module_location = location_from_offsets(line_index, module_start, module_end);

    let body = if let Some(body_span) = body {
        let body_source = &source[body_span.start..body_span.end];
        let (body_defs, _) = parse_block_definitions(
            builder,
            parser,
            line_index,
            body_source,
            base_offset + body_span.start,
            module_id,
            false,
        )?;
        Some(body_defs)
    } else {
        None
    };

    let module_def = Rc::new(ModuleDefinition::new(
        module_id,
        visibility,
        name_node,
        body,
        module_location,
    ));

    builder.add_node(
        AstNode::Definition(Definition::Module(module_def.clone())),
        parent_id,
    );

    Ok(module_def)
}

fn scan_modules(source: &str) -> (Vec<ModuleDecl>, String) {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut modules = Vec::new();
    let mut i = 0;
    let mut depth = 0u32;

    while i < len {
        if bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            i = skip_line_comment(bytes, i + 2);
            continue;
        }
        if bytes[i] == b'"' {
            i = skip_string(bytes, i + 1);
            continue;
        }
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
                continue;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
                continue;
            }
            _ => {}
        }

        if depth == 0 && is_ident_start(bytes[i]) {
            let (ident, ident_start, ident_end) = parse_ident(bytes, i);
            if ident == "pub" {
                let j = skip_ws_and_comments(bytes, ident_end);
                if j < len && is_ident_start(bytes[j]) {
                    let (next_ident, _mod_start, mod_end) = parse_ident(bytes, j);
                    if next_ident == "mod" {
                        if let Some((module, next_idx)) =
                            parse_module_decl(bytes, ident_start, mod_end, Visibility::Public)
                        {
                            modules.push(module);
                            i = next_idx;
                            continue;
                        }
                    }
                }
            } else if ident == "mod" {
                if let Some((module, next_idx)) =
                    parse_module_decl(bytes, ident_start, ident_end, Visibility::Private)
                {
                    modules.push(module);
                    i = next_idx;
                    continue;
                }
            }
            i = ident_end;
            continue;
        }

        i += 1;
    }

    let mut sanitized = bytes.to_vec();
    for module in &modules {
        for idx in module.span.start..module.span.end {
            let byte = sanitized[idx];
            if byte != b'\n' && byte != b'\r' {
                sanitized[idx] = b' ';
            }
        }
    }

    let sanitized = String::from_utf8_lossy(&sanitized).into_owned();
    (modules, sanitized)
}

fn parse_module_decl(
    bytes: &[u8],
    decl_start: usize,
    mod_end: usize,
    visibility: Visibility,
) -> Option<(ModuleDecl, usize)> {
    let len = bytes.len();
    let mut i = skip_ws_and_comments(bytes, mod_end);
    if i >= len || !is_ident_start(bytes[i]) {
        return None;
    }
    let (name, name_start, name_end) = parse_ident(bytes, i);
    i = skip_ws_and_comments(bytes, name_end);
    if i >= len {
        return None;
    }
    if bytes[i] == b';' {
        let span = Span {
            start: decl_start,
            end: i + 1,
        };
        let module = ModuleDecl {
            name,
            visibility,
            span,
            name_span: Span {
                start: name_start,
                end: name_end,
            },
            body: None,
        };
        return Some((module, i + 1));
    }
    if bytes[i] == b'{' {
        let body_start = i + 1;
        let body_end = find_matching_brace(bytes, body_start)?;
        let span = Span {
            start: decl_start,
            end: body_end + 1,
        };
        let module = ModuleDecl {
            name,
            visibility,
            span,
            name_span: Span {
                start: name_start,
                end: name_end,
            },
            body: Some(Span {
                start: body_start,
                end: body_end,
            }),
        };
        return Some((module, body_end + 1));
    }
    None
}

fn find_matching_brace(bytes: &[u8], mut i: usize) -> Option<usize> {
    let len = bytes.len();
    let mut depth = 1u32;
    while i < len {
        if bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            i = skip_line_comment(bytes, i + 2);
            continue;
        }
        if bytes[i] == b'"' {
            i = skip_string(bytes, i + 1);
            continue;
        }
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_line_comment(bytes: &[u8], mut i: usize) -> usize {
    let len = bytes.len();
    while i < len && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_string(bytes: &[u8], mut i: usize) -> usize {
    let len = bytes.len();
    while i < len {
        match bytes[i] {
            b'\\' if i + 1 < len => {
                i += 2;
            }
            b'"' => {
                return i + 1;
            }
            _ => i += 1,
        }
    }
    i
}

fn skip_ws_and_comments(bytes: &[u8], mut i: usize) -> usize {
    let len = bytes.len();
    while i < len {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                i = skip_line_comment(bytes, i + 2);
            }
            _ => break,
        }
    }
    i
}

fn parse_ident(bytes: &[u8], start: usize) -> (String, usize, usize) {
    let len = bytes.len();
    let mut i = start;
    while i < len && is_ident_continue(bytes[i]) {
        i += 1;
    }
    let name = String::from_utf8_lossy(&bytes[start..i]).into_owned();
    (name, start, i)
}

fn is_ident_start(byte: u8) -> bool {
    byte == b'_' || (byte as char).is_ascii_alphabetic()
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || (byte as char).is_ascii_digit()
}

/// Finds the path to a submodule file.
///
/// # Planned Implementation
///
/// Searches for submodule files in the following order:
/// 1. `{current_dir}/{module_name}.inf`
/// 2. `{current_dir}/{module_name}/mod.inf`
///
/// Returns the first path that exists, or `None` if no candidate is found.
#[must_use]
pub fn find_submodule_path(current_file: &PathBuf, module_name: &str) -> Option<PathBuf> {
    let current_dir = current_file.parent()?;
    let file_candidate = current_dir.join(format!("{module_name}.inf"));
    if file_candidate.exists() {
        return Some(file_candidate);
    }
    let mod_candidate = current_dir.join(module_name).join("mod.inf");
    if mod_candidate.exists() {
        return Some(mod_candidate);
    }
    None
}
