//! Symbol table implementation for Inference code.
//!
//! Defines scopes, definitions, and name resolution logic, tracking symbols
//! (types, functions, variables) across modules.
// use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use inference_ast::nodes::{
    Block, Definition, Expression, FunctionDefinition, Literal, OperatorKind, Statement,
    UseDirective,
};

pub(crate) type ScopeRef = Rc<RefCell<Scope>>;

#[derive(Clone)]
pub(crate) enum DefinitionRef {
    Ref(String, Definition),
    QualifiedName(String),
}

impl DefinitionRef {
    pub(crate) fn name(&self) -> String {
        match self {
            DefinitionRef::QualifiedName(name) | DefinitionRef::Ref(name, _) => name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Scope {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) parent: Option<ScopeRef>,
    pub(crate) children: Vec<ScopeRef>,
    pub(crate) imports: Vec<Rc<UseDirective>>,
    import_aliases: HashMap<String, String>,
    pub(crate) definitions: HashMap<String, Definition>,
    variables: HashMap<String, (u32, NodeType)>,
    /// Structs or enums and their methods
    methods: HashMap<String, Vec<Rc<FunctionDefinition>>>,
}

impl Scope {
    pub(crate) fn new(id: u32, name: String, parent: Option<ScopeRef>) -> ScopeRef {
        Rc::new(RefCell::new(Scope {
            id,
            name,
            parent,
            children: Vec::new(),
            imports: Vec::new(),
            import_aliases: HashMap::new(),
            definitions: HashMap::new(),
            variables: HashMap::new(),
            methods: HashMap::new(),
        }))
    }

    pub fn visible_child(&self, ident: &str) -> Option<DefOrScopeRef> {
        if let Some(sub) = self
            .children
            .iter()
            .find(|s| s.borrow().name.rsplit("::").next() == Some(ident))
        {
            return Some(DefOrScopeRef::Module(sub.clone()));
        }

        if let Some(def) = self.definitions.get(ident) {
            return Some(DefOrScopeRef::Definition(def.clone()));
        }

        for u in &self.imports {
            for (key, def_opt) in u.target.borrow().iter() {
                let bound = if let Some((_, alias)) = key.split_once('%') {
                    alias
                } else {
                    key.rsplit("::").next().unwrap_or(key)
                };
                if bound == ident {
                    if let Some(def) = def_opt {
                        return Some(DefOrScopeRef::Definition(def.clone()));
                    }
                }
            }
        }

        None
    }

    fn get_crate_name(&self) -> String {
        if let Some(parent) = &self.parent {
            parent.borrow().get_crate_name()
        } else {
            self.name.clone()
        }
    }

    fn relative_to_absolute_path(&self, path: &str) -> String {
        if path.starts_with("crate") {
            let crate_name = self.get_crate_name();
            return path.replace("crate", &crate_name);
        }
        path.to_string()
    }

    fn resolve_self(&self) -> Option<DefinitionRef> {
        if let Some((_, ty)) = self.variables.get("self") {
            return self.lookup_def(&ty.name());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().resolve_self();
        }
        None
    }

    fn lookup_def(&self, name: &str) -> Option<DefinitionRef> {
        let mut name: &str = name;
        if name.contains("::") {
            name = name.split("::").last().unwrap_or(name);
        }
        if self.import_aliases.contains_key(name) {
            name = self.import_aliases.get(name).unwrap();
        }
        if let Some(def) = self.try_get_definition(name) {
            if self
                .definitions
                .keys()
                .find_map(|def_name| {
                    if def_name == name {
                        Some(DefinitionRef::Ref(def_name.clone(), def.clone()))
                    } else {
                        None
                    }
                })
                .is_some()
            {
                let q_name = format!("{}::{}", self.name, def.name());
                return Some(DefinitionRef::Ref(q_name, def));
            }
            return Some(DefinitionRef::Ref(name.to_string(), def.clone()));
        }
        for import in &self.imports {
            for it in &import.imported_types {
                if it == name || it.ends_with(name) {
                    if let Some(Some(def)) = import.target.borrow().get(it) {
                        return Some(DefinitionRef::Ref(it.clone(), def.clone()));
                    }
                    return Some(DefinitionRef::QualifiedName(it.clone()));
                }
            }
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().lookup_def(name);
        }
        None
    }

    fn insert_var(&mut self, name: String, id: u32, ty: NodeType) {
        self.variables.insert(name, (id, ty));
    }

    fn lookup_symbol(&self, name: &str) -> Option<NodeType> {
        if let Some(ty) = self.variables.get(name) {
            return Some(ty.1.clone());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().lookup_symbol(name);
        }
        None
    }

    pub(crate) fn try_get_definition(&self, name: &str) -> Option<Definition> {
        self.definitions.get(name).cloned()
    }

    fn qualify_definition_name(&self, name: &str) -> Option<String> {
        if let Some(res) = self.definitions.keys().find_map(|def_name| {
            if def_name == name {
                Some(def_name.clone())
            } else {
                None
            }
        }) {
            return Some(res);
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().qualify_definition_name(name);
        }
        None
    }

    pub fn crate_root_id(&self) -> u32 {
        if let Some(parent) = &self.parent {
            parent.borrow().crate_root_id()
        } else {
            self.id
        }
    }

    pub fn crate_root(&self) -> Option<ScopeRef> {
        if let Some(parent) = &self.parent {
            if parent.borrow().parent.is_none() {
                Some(parent.clone())
            } else {
                parent.borrow().crate_root()
            }
        } else {
            None
        }
    }

    fn functions(&self) -> impl Iterator<Item = &Rc<FunctionDefinition>> {
        self.definitions.values().filter_map(|def| {
            if let Definition::Function(f) = def {
                Some(f)
            } else {
                None
            }
        })
    }

    fn methods(&self) -> impl Iterator<Item = &Rc<FunctionDefinition>> {
        self.methods.values().flat_map(|methods| methods.iter())
    }
}

// #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
// struct DefinitionName {
//     name: String,
//     qualified_name: String,
// }

pub(crate) enum DefOrScopeRef {
    Module(ScopeRef),
    Definition(Definition),
}
impl DefOrScopeRef {
    pub(crate) fn as_module(&self) -> Option<Rc<RefCell<Scope>>> {
        match self {
            DefOrScopeRef::Module(scope) => Some(scope.clone()),
            DefOrScopeRef::Definition(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolTable {
    pub(crate) scopes: HashMap<u32, ScopeRef>,
    mod_scopes: HashMap<String, ScopeRef>,
    defs: HashMap<(u32, String), Definition>,
}

impl Default for SymbolTable {
    fn default() -> Self {
        SymbolTable::new()
    }
}

impl SymbolTable {
    pub(crate) fn new() -> Self {
        SymbolTable {
            scopes: HashMap::new(),
            mod_scopes: HashMap::new(),
            defs: HashMap::new(),
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn insert_scope(&mut self, scope: ScopeRef) {
        self.scopes.insert(scope.borrow().id, scope.clone());
        self.mod_scopes
            .insert(scope.borrow().name.clone(), scope.clone());
    }

    pub(crate) fn insert_def(
        &mut self,
        scope_id: u32,
        qualified_name: String,
        def: Definition,
    ) -> Option<Definition> {
        self.defs.insert((scope_id, qualified_name), def)
    }

    fn get_scope_by_def_id(&self, id: u32) -> Option<ScopeRef> {
        for ((s_id, _), v) in &self.defs {
            if v.id() == id {
                return self.scopes.get(s_id).cloned();
            }
        }
        None
    }

    pub(crate) fn build_symbol_tables(&mut self) {
        let scopes = self.scopes.values().cloned().collect::<Vec<_>>();
        for scope in scopes {
            let functions = scope
                .borrow()
                .definitions
                .iter()
                .filter_map(|(_, def)| {
                    if let Definition::Function(f) = def {
                        Some(f.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for mut function in functions {
                self.build_function_symbol_table(&scope, &mut function);
            }
            let scope_borrow = scope.borrow();
            let methods = scope_borrow.methods.values().cloned().collect::<Vec<_>>();
            drop(scope_borrow);
            for methods_group in methods {
                for method in methods_group {
                    self.build_function_symbol_table(&scope, &mut method.clone());
                }
            }
            let scope_borrow = scope.borrow();
            let functions = scope_borrow.functions().collect::<Vec<_>>();
            for function in functions {
                self.build_function_symbol_table(&scope, &mut function.clone());
            }
        }
    }

    fn build_function_symbol_table(&mut self, scope: &ScopeRef, f: &mut Rc<FunctionDefinition>) {
        let self_type = self.find_self_type_for_method(f.clone());
        let fn_scope = self
            .scopes
            .get(&f.id)
            .unwrap_or_else(|| {
                panic!(
                    "Function scope with id {} not found for function {}",
                    f.id, f.name
                )
            })
            .clone();
        for p in &f.parameters {
            let mut param_ty = NodeType::from_string(&p.type_name);
            if param_ty.is_self() {
                if let Some(self_ty_name) = &self_type {
                    param_ty.replace_path(self_ty_name.clone());
                }
            }
            let param_pure_type_name = param_ty.pure_name();

            if let Some(DefinitionRef::Ref(qname, _)) =
                scope.borrow().lookup_def(&param_pure_type_name)
            {
                param_ty.replace_path(qname.clone());
            }

            fn_scope
                .borrow_mut()
                .insert_var(p.name.clone(), p.id, param_ty);
        }
        let ty_node_name = &f.returns.borrow().pure_name();
        if let Some(def_ref) = scope.borrow().lookup_def(ty_node_name) {
            f.returns.borrow_mut().replace_path(def_ref.name());
        }
        if let Some(block) = &f.body {
            for stmt in &block.statements {
                process_statement(stmt, &fn_scope, self);
            }
        }
    }

    #[must_use]
    pub fn lookup_symbol_in_scope(&self, scope_id: u32, name: &str) -> Option<NodeType> {
        if self.scopes.contains_key(&scope_id) {
            return self
                .scopes
                .get(&scope_id)
                .unwrap()
                .borrow()
                .lookup_symbol(name);
        }
        let mut stack: Vec<ScopeRef> = self.scopes.values().cloned().collect();
        while let Some(scope) = stack.pop() {
            if scope.borrow().id == scope_id {
                return scope.borrow().lookup_symbol(name);
            }
            for child in &scope.borrow().children {
                stack.push(child.clone());
            }
        }
        None
    }

    #[must_use]
    pub fn infer_expr_type(&self, scope_id: u32, expr: &Expression) -> NodeType {
        if let Some(scope) = self.scopes.get(&scope_id) {
            return infer_expr_type(expr, scope, self);
        }
        let mut stack: Vec<ScopeRef> = self.scopes.values().cloned().collect();
        while let Some(scope) = stack.pop() {
            if scope.borrow().id == scope_id {
                return infer_expr_type(expr, &scope, self);
            }
            for child in &scope.borrow().children {
                stack.push(child.clone());
            }
        }
        NodeType::Empty
    }

    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn find_self_type_for_method(
        &self,
        function: Rc<FunctionDefinition>,
    ) -> Option<String> {
        fn find_in_scope(scope: &ScopeRef, function_name: &str) -> Option<String> {
            let scope_b = scope.borrow();
            for (type_name, methods) in &scope_b.methods {
                if methods.iter().any(|m| m.name == function_name) {
                    return Some(type_name.clone());
                }
            }
            for child in &scope_b.children {
                if let Some(name) = find_in_scope(child, function_name) {
                    return Some(name);
                }
            }
            None
        }
        for scope in self.mod_scopes.values() {
            if let Some(name) = find_in_scope(scope, &function.name) {
                return Some(name);
            }
        }
        None
    }

    pub(crate) fn get_function_by_name(&self, scope_id: u32, name: &str) -> Option<Definition> {
        if let Some(scope) = &self.scopes.get(&scope_id) {
            if let Some(def_ref) = scope.borrow().lookup_def(name) {
                match def_ref {
                    DefinitionRef::Ref(_, def) => return Some(def),
                    DefinitionRef::QualifiedName(q_name) => {
                        for d in &self.defs {
                            if d.0.1 == q_name {
                                return Some(d.1.clone());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    #[allow(unused_variables, clippy::too_many_lines)]
    pub(crate) fn lookup_symbol_origin(
        &self,
        scope_id: u32,
        symbol: &str,
    ) -> Option<crate::node_type::NodeKind> {
        fn find_stmt(block: &Block, id: u32) -> Option<Statement> {
            for stmt in &block.statements {
                if stmt.id() == id {
                    return Some(stmt.clone());
                }
                match stmt {
                    Statement::Block(inner) => {
                        if let Some(s) = find_stmt(inner, id) {
                            return Some(s);
                        }
                    }
                    Statement::Expression(expr) => match expr {
                        Expression::If(if_expr) => {
                            if let Some(s) = find_stmt(&if_expr.then_branch, id) {
                                return Some(s);
                            }
                            if let Some(Expression::EBlock(inner)) = &if_expr.else_branch {
                                if let Some(s) = find_stmt(&inner.block, id) {
                                    return Some(s);
                                }
                            }
                        }
                        Expression::ForLoop(for_loop) => {
                            if let Some(s) = find_stmt(&for_loop.block, id) {
                                return Some(s);
                            }
                        }
                        Expression::Loop(loop_stmt) => {
                            if let Some(s) = find_stmt(&loop_stmt.block, id) {
                                return Some(s);
                            }
                        }
                        Expression::TryBlock(try_block) => {
                            if let Some(s) = find_stmt(&try_block.block, id) {
                                return Some(s);
                            }
                        }
                        Expression::EBlock(inner) => {
                            if let Some(s) = find_stmt(&inner.block, id) {
                                return Some(s);
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            None
        }
        fn find_call_sites(
            block: &Block,
            name: &str,
            param_idx: usize,
            caller_id: u32,
            calls: &mut Vec<(u32, Expression)>,
        ) {
            for stmt in &block.statements {
                if let Statement::Expression(Expression::FunctionCall(fc)) = stmt {
                    if fc.function_name == name {
                        if let Some(arg) = fc.parameters.get(param_idx) {
                            calls.push((caller_id, arg.clone()));
                        }
                    }
                }
                if let Statement::Block(inner) = stmt {
                    find_call_sites(inner, name, param_idx, caller_id, calls);
                }
                if let Statement::Expression(Expression::If(if_expr)) = stmt {
                    find_call_sites(&if_expr.then_branch, name, param_idx, caller_id, calls);
                    if let Some(Expression::EBlock(inner)) = &if_expr.else_branch {
                        find_call_sites(&inner.block, name, param_idx, caller_id, calls);
                    }
                }
                if let Statement::Expression(Expression::ForLoop(for_loop)) = stmt {
                    find_call_sites(&for_loop.block, name, param_idx, caller_id, calls);
                }
                if let Statement::Expression(Expression::Loop(loop_stmt)) = stmt {
                    find_call_sites(&loop_stmt.block, name, param_idx, caller_id, calls);
                }
            }
        }
        if let Some(scope_rc) = self.scopes.get(&scope_id) {
            let scope = scope_rc.borrow();
            if let Some((var_id, _)) = scope.variables.get(symbol) {
                if let Some(parent_rc) = &scope.parent {
                    let parent = parent_rc.borrow();
                    if let Some(func_name) = scope.name.rsplit("::").next() {
                        if let Some(DefinitionRef::Ref(_, Definition::Function(func_def))) =
                            parent.lookup_def(func_name)
                        {
                            if let Some((idx, param)) = func_def
                                .parameters
                                .iter()
                                .enumerate()
                                .find(|(_, p)| p.id == *var_id)
                            {
                                let mut calls = Vec::new();
                                for def in self.defs.values() {
                                    if let Definition::Function(caller_def) = def {
                                        if caller_def.id != func_def.id {
                                            if let Some(body) = &caller_def.body {
                                                find_call_sites(
                                                    body,
                                                    func_name,
                                                    idx,
                                                    caller_def.id,
                                                    &mut calls,
                                                );
                                            }
                                        }
                                    }
                                }
                                if calls.len() == 1 {
                                    let (caller_id, arg_expr) = &calls[0];
                                    if let Expression::Identifier(id) = arg_expr {
                                        return self.lookup_symbol_origin(*caller_id, &id.name);
                                    }
                                }
                                return Some(crate::node_type::NodeKind::Misc(Misc::FnParameter(
                                    param.clone(),
                                )));
                            }
                            if let Some(body) = &func_def.body {
                                if let Some(stmt) = find_stmt(body, *var_id) {
                                    return Some(crate::node_type::NodeKind::Statement(stmt));
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut current = self.scopes.get(&scope_id).cloned();
        while let Some(scope_rc) = current {
            let scope = scope_rc.borrow();
            if let Some(def_ref) = scope.lookup_def(symbol) {
                match def_ref {
                    DefinitionRef::Ref(_, def) => {
                        return Some(crate::node_type::NodeKind::Definition(def.clone()));
                    }
                    DefinitionRef::QualifiedName(qn) => {
                        for ((_, name), def) in &self.defs {
                            if *name == qn {
                                return Some(crate::node_type::NodeKind::Definition(def.clone()));
                            }
                        }
                    }
                }
            }
            current = scope.parent.as_ref().map(Rc::clone);
        }
        None
    }
}

pub(crate) fn process_definition(
    parent_scope: &ScopeRef,
    def: Definition,
    table: &mut SymbolTable,
) {
    let parent_path = parent_scope.borrow().name.clone();
    let name = def.name();
    let qualified = format!("{parent_path}::{name}");
    parent_scope
        .borrow_mut()
        .definitions
        .insert(name.clone(), def.clone());
    table.insert_def(parent_scope.borrow().id, qualified.clone(), def.clone());
    match def {
        Definition::Struct(s) => {
            parent_scope
                .borrow_mut()
                .methods
                .insert(s.name.clone(), Vec::new());
        }
        Definition::Enum(e) => {
            parent_scope
                .borrow_mut()
                .methods
                .insert(e.name.clone(), Vec::new());
        }
        Definition::Function(f) => {
            let fn_scope = Scope::new(f.id, qualified, Some(parent_scope.clone()));
            table.insert_scope(fn_scope.clone());
        }
        Definition::Implementation(i) => {
            for constant in &i.constants {
                let constant_name = constant.name.clone();
                let constant_def = Definition::Const(constant.clone());
                parent_scope
                    .borrow_mut()
                    .definitions
                    .insert(constant_name, constant_def);
            }
            for ta in &i.type_aliases {
                let alias_name = ta.name.clone();
                let alias_def = Definition::TypeAlias(ta.clone());
                parent_scope
                    .borrow_mut()
                    .definitions
                    .insert(alias_name, alias_def);
            }
            if let Type::Typename(target) = &i.for_type {
                let def_name_op = parent_scope.borrow().qualify_definition_name(&target.name);
                if let Some(def_name) = def_name_op {
                    if let Some(method_list) = parent_scope.borrow_mut().methods.get_mut(&def_name)
                    {
                        for func in &i.functions {
                            method_list.push(func.clone());
                        }
                    }
                    for func in &i.functions {
                        process_definition(
                            &parent_scope.clone(),
                            Definition::Function(func.clone()),
                            table,
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn fixpoint_resolver(table: &mut SymbolTable, extern_prelude: &mut ExternPrelude) {
    loop {
        let mut progress = false;
        for scope in table.scopes.values() {
            for rc_use in &scope.borrow().imports {
                if rc_use.is_resolved() {
                    continue;
                }
                let imported_types = rc_use.imported_types.clone();
                for import_path in imported_types {
                    let (orig_path, _) = import_path
                        .split_once('%')
                        .map(|(orig, alias)| (orig.to_string(), Some(alias.to_string())))
                        .unwrap_or((import_path.clone(), None));
                    let (head, tail) = match orig_path.split_once("::") {
                        Some((h, t)) => (h.to_string(), t.to_string()),
                        None => (orig_path.clone(), String::new()),
                    };
                    let start_scope = match head.as_str() {
                        "crate" => scope.borrow().crate_root(),
                        "self" => Some(scope.clone()),
                        "super" => scope.borrow().parent.clone(),
                        other => {
                            if let Some(ext) = extern_prelude.get(other) {
                                Some(ext.root_scope.clone())
                            } else {
                                scope
                                    .borrow()
                                    .visible_child(other)
                                    .and_then(|d| d.as_module())
                                    .or_else(|| {
                                        scope.borrow().crate_root().and_then(|root| {
                                            root.borrow()
                                                .visible_child(other)
                                                .and_then(|d| d.as_module())
                                        })
                                    })
                            }
                        }
                    };
                    if let Some(start) = start_scope {
                        if let Some(def) = walk_segments(start, &tail, scope) {
                            if rc_use.target.borrow().contains_key(&import_path)
                                && rc_use.target.borrow().get(&import_path).unwrap().is_some()
                            {
                                continue;
                            }
                            rc_use.insert_target(import_path, Some(def));
                            progress = true;
                        }
                    }
                }
            }
        }
        if !progress {
            break;
        }
    }
    table.build_symbol_tables();
}

#[allow(clippy::assigning_clones)]
fn scope_path(scope: &ScopeRef) -> Vec<String> {
    let mut segments = Vec::new();
    let mut cur = Some(scope.clone());
    while let Some(sc) = cur {
        segments.push(sc.borrow().name.clone());
        cur = sc.borrow().parent.clone();
    }
    segments.reverse();
    segments
}

impl Visibility {
    pub(crate) fn is_visible_from(&self, from_scope: &ScopeRef, owner_scope: &ScopeRef) -> bool {
        if *self == Visibility::Public {
            return true;
        }

        let same_crate =
            from_scope.borrow().crate_root_id() == owner_scope.borrow().crate_root_id();

        match self {
            Visibility::Private | Visibility::Inherited => {
                let owner_path = scope_path(owner_scope);
                let from_path = scope_path(from_scope);
                from_path.starts_with(&owner_path)
            }
            Visibility::PubCrate => same_crate,
            Visibility::PubSuper => {
                if let Some(parent) = owner_scope.borrow().parent.clone() {
                    let super_path = scope_path(&parent);
                    let from_path = scope_path(from_scope);
                    from_path.starts_with(&super_path)
                } else {
                    // owner is crate root -> `pub(super)` degenerates to private
                    false
                }
            }
            Visibility::PubIn(path) => {
                let target_mod = path
                    .split("::")
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                let from_path = scope_path(from_scope);
                from_path.starts_with(&target_mod)
            }

            Visibility::Public => unreachable!("handled above {:?}", self),
        }
    }
}

fn walk_segments(mut scope: ScopeRef, path: &str, from_crate: &ScopeRef) -> Option<Definition> {
    if path.is_empty() {
        return None;
    }
    let mut segments = path.split("::").filter(|s| !s.is_empty()).peekable();
    while let Some(seg) = segments.next() {
        let is_last = segments.peek().is_none();
        let child = scope.borrow().visible_child(seg)?;
        match child {
            DefOrScopeRef::Module(sub_scope) => {
                if is_last {
                    return None;
                }
                scope = sub_scope;
            }
            DefOrScopeRef::Definition(def) => {
                if !def.visibility().is_visible_from(from_crate, &scope) {
                    return None;
                }
                if is_last {
                    return Some(def);
                }
                return None;
            }
        }
    }
    None
}

#[allow(clippy::too_many_lines)]
fn process_statement(stmt: &Statement, scope: &ScopeRef, table: &mut SymbolTable) {
    match stmt {
        Statement::VariableDefinition(let_stmt) => {
            let mut vty = if let Some(init) = &let_stmt.initial_value {
                let ty = infer_expr_type(init, scope, table);
                if ty != NodeType::Empty {
                    ty
                } else if let Some((_, ty_str)) = let_stmt.pattern.kind.split_once(':') {
                    parse_str::<syn::Type>(ty_str.trim())
                        .map(|ty| NodeType::from_syn_item(&ty))
                        .unwrap_or(NodeType::Empty)
                } else {
                    NodeType::Empty
                }
            } else {
                NodeType::Empty
            };
            if let Some(def) = scope.borrow().lookup_def(&vty.name()) {
                match def {
                    DefinitionRef::Ref(_, _) => {}
                    DefinitionRef::QualifiedName(qualified_name) => {
                        vty = NodeType::Path(qualified_name);
                    }
                }
            }
            scope
                .borrow_mut()
                .insert_var(let_stmt.name.clone(), let_stmt.id, vty);
        }
        Statement::Expression(Expression::If(if_expr)) => {
            for stmt in &if_expr.then_branch.statements {
                process_statement(stmt, scope, table);
            }
        }
        Statement::Block(block) => {
            for stmt in &block.statements {
                process_statement(stmt, scope, table);
            }
        }
        Statement::Expression(Expression::EBlock(e_block)) => {
            for stmt in &e_block.block.statements {
                process_statement(stmt, scope, table);
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_lines)]
fn infer_expr_type(expr: &Expression, scope: &ScopeRef, table: &SymbolTable) -> NodeType {
    match expr {
        Expression::Identifier(id) => {
            if id.name.contains("::") {
                let (module, rest) = id.name.split_once("::").unwrap();
                if let Some(mod_scope) = table.mod_scopes.get(module) {
                    if let Some(def) = mod_scope.borrow().lookup_def(rest) {
                        let ty = match def {
                            DefinitionRef::Ref(_, Definition::Const(c)) => c.type_.to_type_node(),
                            DefinitionRef::Ref(_, Definition::Static(s)) => s.ty.to_type_node(),
                            DefinitionRef::Ref(_, Definition::Function(f)) => {
                                f.returns.borrow().clone()
                            }
                            DefinitionRef::Ref(_, Definition::AssocType(t)) => t.ty.to_type_node(),
                            DefinitionRef::Ref(
                                _,
                                Definition::Struct(s) | Definition::Contract(s),
                            ) => NodeType::Path(s.name.clone()),
                            DefinitionRef::Ref(_, Definition::Enum(e)) => {
                                NodeType::Path(e.name.clone())
                            }
                            DefinitionRef::Ref(_, Definition::Union(u)) => {
                                NodeType::Path(u.name.clone())
                            }
                            DefinitionRef::Ref(_, Definition::Module(m)) => {
                                NodeType::Path(m.name.clone())
                            }
                            DefinitionRef::Ref(_, Definition::TraitAlias(ta)) => {
                                NodeType::Path(ta.name.clone())
                            }
                            _ => return NodeType::Empty,
                        };
                        return ty;
                    }
                }
            }
            if let Some(v) = scope.borrow().lookup_symbol(&id.name) {
                return v;
            }
            if let Some(def) = scope.borrow().lookup_def(&id.name) {
                let ty_node = match def {
                    DefinitionRef::Ref(_, Definition::Const(c)) => c.type_.to_type_node(),
                    DefinitionRef::Ref(_, Definition::Static(s)) => s.ty.to_type_node(),
                    DefinitionRef::Ref(_, Definition::Function(f)) => f.returns.borrow().clone(),
                    DefinitionRef::Ref(_, Definition::AssocType(t)) => t.ty.to_type_node(),
                    DefinitionRef::Ref(_, Definition::Struct(s) | Definition::Contract(s)) => {
                        NodeType::Path(s.name.clone())
                    }
                    DefinitionRef::Ref(_, Definition::Enum(e)) => NodeType::Path(e.name.clone()),
                    DefinitionRef::Ref(_, Definition::Union(u)) => NodeType::Path(u.name.clone()),
                    DefinitionRef::Ref(_, Definition::Module(m)) => NodeType::Path(m.name.clone()),
                    DefinitionRef::Ref(_, Definition::TraitAlias(ta)) => {
                        NodeType::Path(ta.name.clone())
                    }
                    _ => return NodeType::Empty,
                };
                return ty_node;
            }
            NodeType::Empty
        }
        Expression::Literal(lit_expr) => match &lit_expr.value {
            Literal::Bool(_) => NodeType::Path("bool".to_string()),
            Literal::Byte(_) => NodeType::Path("u8".to_string()),
            Literal::Char(_) => NodeType::Path("char".to_string()),
            Literal::Int(_) => NodeType::Path("i32".to_string()),
            Literal::Float(_) => NodeType::Path("f64".to_string()),
            Literal::String(_) => NodeType::Reference {
                inner: Box::new(NodeType::Path("str".to_string())),
                mutable: false,
                is_explicit_reference: true,
            },
            Literal::BString(_) => NodeType::Reference {
                inner: Box::new(NodeType::Path("[u8]".to_string())),
                mutable: false,
                is_explicit_reference: true,
            },
            Literal::CString(cs) => NodeType::Ptr {
                inner: Box::new(NodeType::Path("c_char".to_string())),
                mutable: cs.value.starts_with("*const"),
            },
        },
        Expression::Binary(bin) => {
            match bin.operator {
                OperatorKind::Add
                | OperatorKind::Sub
                | OperatorKind::Mul
                | OperatorKind::Div
                | OperatorKind::Mod
                | OperatorKind::BitXor
                | OperatorKind::BitAnd
                | OperatorKind::BitOr
                | OperatorKind::Shl
                | OperatorKind::Shr => {
                    let left_ty = infer_expr_type(&bin.left, scope, table);
                    let right_ty = infer_expr_type(&bin.right, scope, table);
                    if left_ty == right_ty {
                        return left_ty;
                    }
                    right_ty //Won't fix, fallback
                }
                OperatorKind::And
                | OperatorKind::Or
                | OperatorKind::Eq
                | OperatorKind::Ne
                | OperatorKind::Lt
                | OperatorKind::Le
                | OperatorKind::Ge
                | OperatorKind::Gt => NodeType::Path("bool".to_string()),
                OperatorKind::AddAssign
                | OperatorKind::SubAssign
                | OperatorKind::MulAssign
                | OperatorKind::DivAssign
                | OperatorKind::ModAssign
                | OperatorKind::BitXorAssign
                | OperatorKind::BitAndAssign
                | OperatorKind::BitOrAssign
                | OperatorKind::ShlAssign
                | OperatorKind::ShrAssign => infer_expr_type(&bin.left, scope, table),
            }
        }
        Expression::PrefixUnary(u) => infer_expr_type(&u.expression, scope, table),
        Expression::FunctionCall(fc) => {
            for def in scope.borrow().functions().chain(scope.borrow().methods()) {
                if def.name == fc.function_name {
                    return def.returns.borrow().clone();
                }
            }
            if let Expression::Identifier(base) = &fc.expression {
                let name = base.name.clone();
                if name == "Some" {
                    if let Some(arg) = &fc.parameters.first() {
                        let ty = infer_expr_type(arg, scope, table);
                        return NodeType::Path(format!("Option<{}>", ty.name()));
                    }
                    return NodeType::Path("Option<_>".to_string());
                }
                if name == "None" {
                    return NodeType::Path("Option<_>".to_string());
                }
                if name == "Ok" {
                    if let Some(arg) = &fc.parameters.first() {
                        let ty = infer_expr_type(arg, scope, table);
                        return NodeType::Path(format!("Result<{}, _>", ty.name()));
                    }
                    return NodeType::Path("Result<_, _>".to_string());
                }
                if name.contains("::") {
                    let (module, rest) = name.split_once("::").unwrap();
                    let module = module.trim();
                    let rest = rest.trim();
                    if let Some(mod_scope) = table.mod_scopes.get(module) {
                        for def in mod_scope
                            .borrow()
                            .functions()
                            .chain(mod_scope.borrow().methods())
                        {
                            if def.name == rest {
                                return def.returns.borrow().clone();
                            }
                        }
                    }
                    if ["Vec", "HashMap"].contains(&module) {
                        return NodeType::Reference {
                            inner: Box::new(NodeType::Path(module.to_string())),
                            mutable: false,
                            is_explicit_reference: false,
                        };
                    }
                } else if let Some(v) = scope.borrow().lookup_symbol(&name) {
                    return v;
                }
            }
            NodeType::Empty
        }
        Expression::MethodCall(mc) => {
            let base_ty = infer_expr_type(&mc.base, scope, table);
            if base_ty == NodeType::Empty {
                return NodeType::Empty;
            }
            let type_name_owned = base_ty.pure_name().clone();
            let type_name = type_name_owned.as_str();
            let base_def = if let Some(base_def_ref) = scope.borrow().lookup_def(type_name) {
                match base_def_ref {
                    DefinitionRef::Ref(_, def) => Some(def),
                    DefinitionRef::QualifiedName(_) => None,
                }
            } else if let Some(d) = table.defs.iter().find(|((_, n), _)| n == type_name) {
                Some(d.1.clone())
            } else {
                return NodeType::Empty;
            };
            if let Some(def) = base_def {
                if let Some(def_scope) = table.get_scope_by_def_id(def.id()) {
                    if let Some(method) =
                        def_scope
                            .borrow()
                            .methods
                            .get(&def.name())
                            .and_then(|methods| {
                                methods.iter().find(|f| f.name == mc.method_name).cloned()
                            })
                    {
                        let ty_node_name = method.returns.borrow().pure_name();
                        if let Some(def) = def_scope.borrow().lookup_def(&ty_node_name) {
                            match def {
                                DefinitionRef::Ref(qualified_name, _) => {
                                    *method.returns.borrow_mut() = NodeType::Path(
                                        def_scope
                                            .borrow()
                                            .relative_to_absolute_path(&qualified_name),
                                    );
                                }
                                DefinitionRef::QualifiedName(qualified_name) => {
                                    *method.returns.borrow_mut() = NodeType::Path(qualified_name);
                                }
                            }
                        }
                        return method.returns.borrow().clone();
                    }
                }
            }
            NodeType::Empty
        }
        Expression::MemberAccess(ma) => {
            let base_ty = infer_expr_type(&ma.base, scope, table);
            if let Some(def_ref) = scope.borrow().lookup_def(&base_ty.pure_name()) {
                match def_ref {
                    DefinitionRef::Ref(_, Definition::Struct(s) | Definition::Contract(s)) => {
                        for (field, fty) in &s.fields {
                            if &ma.member_name == field {
                                return fty.to_type_node();
                            }
                        }
                    }
                    DefinitionRef::Ref(_, Definition::Enum(e)) => {
                        for variant in &e.variants {
                            if &ma.member_name == variant {
                                return NodeType::Path(base_ty.name().clone());
                            }
                        }
                    }
                    DefinitionRef::QualifiedName(qn) => {
                        let def_ref_o = scope.borrow().lookup_def(&qn);
                        if let Some(def) = def_ref_o {
                            if let DefinitionRef::Ref(
                                _,
                                Definition::Struct(s) | Definition::Contract(s),
                            ) = &def
                            {
                                for (field, fty) in &s.fields {
                                    if &ma.member_name == field {
                                        return fty.to_type_node();
                                    }
                                }
                            }
                            if let DefinitionRef::Ref(_, Definition::Enum(e)) = def {
                                for variant in &e.variants {
                                    if &ma.member_name == variant {
                                        return NodeType::Path(base_ty.name().clone());
                                    }
                                }
                            }
                        } else {
                            return NodeType::Path(qn.clone());
                        }
                    }
                    DefinitionRef::Ref(_, _) => {}
                }
            }

            NodeType::Empty
        }
        Expression::Return(r) => {
            if let Some(e) = &r.expression {
                infer_expr_type(e, scope, table)
            } else {
                NodeType::Tuple(Vec::new())
            }
        }
        Expression::Cast(c) => {
            let mut ty_node = c.target_type.to_type_node();
            if ty_node.is_self() {
                if let Some(self_ty) = scope.borrow().resolve_self() {
                    ty_node = match ty_node {
                        NodeType::Path(ref name) if name == "Self" => {
                            NodeType::Path(self_ty.name())
                        }
                        NodeType::Reference {
                            inner: old_inner,
                            mutable,
                            is_explicit_reference,
                        } if old_inner.name() == "Self" => NodeType::Reference {
                            inner: Box::new(NodeType::Path(self_ty.name())),
                            mutable,
                            is_explicit_reference,
                        },
                        NodeType::Ptr { inner, mutable } if inner.name() == "Self" => {
                            NodeType::Ptr {
                                inner: Box::new(NodeType::Path(self_ty.name())),
                                mutable,
                            }
                        }
                        other => other,
                    };
                }
            }
            ty_node
        }
        Expression::Tuple(t) => {
            let mut types = Vec::new();
            for e in &t.elements {
                types.push(infer_expr_type(e, scope, table));
            }
            NodeType::Tuple(types)
        }
        _ => NodeType::Empty,
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::{Codebase, OpenState, directive::Directive, node_type::NodeKind};

//     #[must_use]
//     #[allow(clippy::missing_panics_doc)]
//     pub(crate) fn resolve_path(
//         table: &SymbolTable,
//         scope_id: u32,
//         path: &str,
//     ) -> Option<DefinitionRef> {
//         let mut parts = path
//             .split("::")
//             .filter(|s| !s.is_empty())
//             .collect::<Vec<_>>();
//         if parts.is_empty() {
//             return None;
//         }
//         let name = parts.pop().unwrap();
//         if parts.is_empty() {
//             if let Some(scope) = table
//                 .mod_scopes
//                 .iter()
//                 .find(|(_, v)| v.borrow().id == scope_id)
//                 .map(|(_, v)| v)
//             {
//                 return scope.borrow().lookup_def(name);
//             }
//         }
//         let module_path = parts.join("::");
//         for (mod_scope_name, mod_scope) in &table.mod_scopes {
//             if *mod_scope_name == module_path {
//                 for (def_name, def) in &mod_scope.borrow().definitions {
//                     if def_name == name {
//                         return Some(DefinitionRef::Ref(def_name.clone(), def.clone()));
//                     }
//                 }
//             }
//         }
//         None
//     }

//     fn build_table_and_uses(src: &str) -> (SymbolTable, Vec<Rc<Use>>) {
//         let mut cb = Codebase::<OpenState>::default();
//         let content = src.to_string();
//         let mut data = HashMap::new();
//         data.insert(
//             format!("test{0}test.rs", std::path::MAIN_SEPARATOR),
//             content,
//         );
//         let sealed = cb.build_api(&data).unwrap();
//         let table = sealed.symbol_table.clone();
//         let file = sealed.files().next().unwrap().clone();
//         let uses = file
//             .children
//             .borrow()
//             .iter()
//             .filter_map(|node| {
//                 if let NodeKind::Directive(Directive::Use(u)) = node {
//                     Some(u.clone())
//                 } else {
//                     None
//                 }
//             })
//             .collect::<Vec<_>>();
//         (table, uses)
//     }

//     #[test]
//     fn test_resolve_path_basic() {
//         let src = r"
//             mod a {
//                 pub struct S;
//                 pub mod b {
//                     pub enum E { A }
//                 }
//             }
//         ";
//         let (table, _) = build_table_and_uses(src);
//         let scope_id = table.mod_scopes.iter().next().unwrap().1.borrow().id;
//         assert!(
//             resolve_path(
//                 &table,
//                 scope_id,
//                 "soroban_security_detectors_sdk::test::a::S"
//             )
//             .is_some()
//         );
//         assert!(
//             resolve_path(
//                 &table,
//                 scope_id,
//                 "soroban_security_detectors_sdk::test::a::b::E"
//             )
//             .is_some()
//         );
//         assert!(
//             resolve_path(
//                 &table,
//                 scope_id,
//                 "soroban_security_detectors_sdk::test::a::X"
//             )
//             .is_none()
//         );
//         assert!(resolve_path(&table, scope_id, "").is_none());
//     }

//     #[test]
//     fn test_import_target_resolution() {
//         let mut cb = Codebase::<OpenState>::default();
//         let file1 = r"
//         pub type MyType = u8;
//         pub mod sub {
//             pub struct SubType;
//         }
//         "
//         .to_string();

//         let file2 = r"
//         use file1::MyType;
//         use file1::sub::SubType as Renamed;
//         "
//         .to_string();
//         let mut data = HashMap::new();
//         data.insert(format!("test{0}file1.rs", std::path::MAIN_SEPARATOR), file1);
//         data.insert(format!("test{0}file2.rs", std::path::MAIN_SEPARATOR), file2);
//         let sealed = cb.build_api(&data).unwrap();
//         let table = sealed.symbol_table.clone();

//         let file2 = sealed.files().find(|f| f.name == "file2.rs").unwrap();
//         let uses = file2
//             .children
//             .borrow()
//             .iter()
//             .filter_map(|node| {
//                 if let NodeKind::Directive(Directive::Use(u)) = node {
//                     Some(u.clone())
//                 } else {
//                     None
//                 }
//             })
//             .collect::<Vec<_>>();
//         assert_eq!(uses.len(), 2);

//         let full1 = "file1::MyType".to_string();
//         let u1 = uses
//             .iter()
//             .find(|u| u.imported_types == vec![full1.clone()])
//             .unwrap();
//         let DefinitionRef::Ref(_, my_def) = resolve_path(
//             &table,
//             table
//                 .mod_scopes
//                 .iter()
//                 .find(|s| !s.1.borrow().definitions.is_empty())
//                 .unwrap()
//                 .1
//                 .borrow()
//                 .id,
//             "soroban_security_detectors_sdk::file1::MyType",
//         )
//         .unwrap() else {
//             panic!("Expected a reference to a definition");
//         };
//         let binding = u1.target.borrow();
//         println!("Use target: {binding:?}, checking key {full1:?}");
//         let found = binding.get(&full1);
//         assert_eq!(found.unwrap().as_ref().unwrap().id(), my_def.id());

//         let full2 = "file1::sub::SubType%Renamed".to_string();
//         let u2 = uses
//             .iter()
//             .find(|u| u.imported_types == vec![full2.clone()])
//             .unwrap();
//         let DefinitionRef::Ref(_, sub) = resolve_path(
//             &table,
//             table.mod_scopes.iter().next().unwrap().1.borrow().id,
//             "soroban_security_detectors_sdk::file1::sub::SubType",
//         )
//         .unwrap() else {
//             panic!("Expected a reference to a definition");
//         };
//         assert_eq!(u2.target.borrow().get(&full2), Some(&Some(sub)));
//     }

//     #[test]
//     fn test_lookup_symbol_origin_basic() {
//         let src = r"
//         pub const C: u32 = 5;

//         fn f() {
//             let x = C;
//             let y = x;
//         }
//         ";
//         let (table, _) = build_table_and_uses(src);
//         let root_scope = table
//             .mod_scopes
//             .get("soroban_security_detectors_sdk::test")
//             .unwrap()
//             .borrow()
//             .id;
//         let origin_c_root = table.lookup_symbol_origin(root_scope, "C").unwrap();
//         if let crate::node_type::NodeKind::Definition(ref def) = origin_c_root {
//             assert_eq!(def.name(), "C");
//         } else {
//             panic!("Expected Definition for C");
//         }
//         let f_def = table.get_function_by_name(root_scope, "f").unwrap();
//         let f_scope = f_def.id();
//         let origin_c = table.lookup_symbol_origin(f_scope, "C").unwrap();
//         assert_eq!(origin_c.id(), origin_c_root.id());
//         let origin_x = table.lookup_symbol_origin(f_scope, "x").unwrap();
//         if let crate::node_type::NodeKind::Statement(stmt) = origin_x.clone() {
//             if let Statement::Let(let_stmt) = stmt {
//                 assert_eq!(let_stmt.name, "x");
//             } else {
//                 panic!("Expected Let statement for x");
//             }
//         } else {
//             panic!("Expected Statement variant for x origin");
//         }
//         let origin_y = table.lookup_symbol_origin(f_scope, "y").unwrap();
//         if let crate::node_type::NodeKind::Statement(stmt) = origin_y.clone() {
//             if let Statement::Let(let_stmt) = stmt {
//                 assert_eq!(let_stmt.name, "y");
//             } else {
//                 panic!("Expected Let statement for y");
//             }
//         } else {
//             panic!("Expected Statement variant for y origin");
//         }
//         let origin_x_again = table.lookup_symbol_origin(f_scope, "x").unwrap();
//         assert_eq!(origin_x_again.id(), origin_x.id());
//         assert!(table.lookup_symbol_origin(f_scope, "z").is_none());
//     }

//     #[test]
//     fn test_lookup_symbol_origin_parameter() {
//         let src = r"
//         fn g(p: u32) {
//             let q = p;
//         }
//         ";
//         let (table, _) = build_table_and_uses(src);
//         let root_scope = table
//             .mod_scopes
//             .get("soroban_security_detectors_sdk::test")
//             .unwrap()
//             .borrow()
//             .id;
//         let g_def = table.get_function_by_name(root_scope, "g").unwrap();
//         let g_scope = g_def.id();
//         let origin_p = table.lookup_symbol_origin(g_scope, "p").unwrap();
//         if let crate::node_type::NodeKind::Misc(Misc::FnParameter(param)) = origin_p {
//             assert_eq!(param.name, "p");
//         } else {
//             panic!("Expected FnParameter origin for p");
//         }
//     }

//     #[test]
//     fn test_lookup_symbol_origin_parameter_single_call_constant() {
//         let src = r"
//         const C: u32 = 5;

//         fn g(p: u32) {
//             let q = p;
//         }

//         fn f() {
//             g(C);
//         }
//         ";
//         let (table, _) = build_table_and_uses(src);
//         let root_scope = table
//             .mod_scopes
//             .get("soroban_security_detectors_sdk::test")
//             .unwrap()
//             .borrow()
//             .id;
//         let g_scope = table.get_function_by_name(root_scope, "g").unwrap().id();
//         let origin_c = table.lookup_symbol_origin(root_scope, "C").unwrap();
//         let origin_p = table.lookup_symbol_origin(g_scope, "p").unwrap();
//         assert_eq!(origin_p.id(), origin_c.id());
//     }

//     #[test]
//     fn test_lookup_symbol_origin_parameter_single_call_variable() {
//         let src = r"
//         fn g(p: u32) {
//             let q = p;
//         }

//         fn f() {
//             let x = 42;
//             g(x);
//         }
//         ";
//         let (table, _) = build_table_and_uses(src);
//         let root_scope = table
//             .mod_scopes
//             .get("soroban_security_detectors_sdk::test")
//             .unwrap()
//             .borrow()
//             .id;
//         let g_scope = table.get_function_by_name(root_scope, "g").unwrap().id();
//         let f_scope = table.get_function_by_name(root_scope, "f").unwrap().id();
//         let origin_x = table.lookup_symbol_origin(f_scope, "x").unwrap();
//         let origin_p = table.lookup_symbol_origin(g_scope, "p").unwrap();
//         assert_eq!(origin_p.id(), origin_x.id());
//     }

//     #[test]
//     fn test_lookup_symbol_origin_parameter_multiple_calls() {
//         let src = r"
//         fn g(p: u32) {
//             let q = p;
//         }

//         fn a() { g(1); }
//         fn b() { g(2); }
//         ";
//         let (table, _) = build_table_and_uses(src);
//         let root_scope = table
//             .mod_scopes
//             .get("soroban_security_detectors_sdk::test")
//             .unwrap()
//             .borrow()
//             .id;
//         let g_scope = table.get_function_by_name(root_scope, "g").unwrap().id();
//         let origin_p = table.lookup_symbol_origin(g_scope, "p").unwrap();
//         if let crate::node_type::NodeKind::Misc(Misc::FnParameter(param)) = origin_p {
//             assert_eq!(param.name, "p");
//         } else {
//             panic!("Expected FnParameter for multiple calls");
//         }
//     }

//     #[test]
//     fn test_lookup_symbol_origin_parameter_single_call_nested_block() {
//         let src = r"
//         const C: u32 = 5;

//         fn h(p: u32) {
//             let q = p;
//         }

//         fn f() {
//             if true {
//                 let x = C;
//                 h(x);
//             }
//         }
//         ";
//         let (table, _) = build_table_and_uses(src);
//         let root_scope = table
//             .mod_scopes
//             .get("soroban_security_detectors_sdk::test")
//             .unwrap()
//             .borrow()
//             .id;
//         let h_scope = table.get_function_by_name(root_scope, "h").unwrap().id();
//         let f_scope = table.get_function_by_name(root_scope, "f").unwrap().id();
//         let origin_x = table.lookup_symbol_origin(f_scope, "x").unwrap();
//         let origin_p = table.lookup_symbol_origin(h_scope, "p").unwrap();
//         assert_eq!(origin_p.id(), origin_x.id());
//     }

//     #[test]
//     fn test_import_across_file_scopes() {
//         let mut cb = Codebase::<OpenState>::default();
//         let file1 = r"
//             pub struct AStruct;
//         "
//         .to_string();

//         let file2 = r"
//             use file1::AStruct;
//         "
//         .to_string();
//         let mut data = HashMap::new();
//         data.insert(format!("test{0}file1.rs", std::path::MAIN_SEPARATOR), file1);
//         data.insert(format!("test{0}file2.rs", std::path::MAIN_SEPARATOR), file2);

//         let sealed = cb.build_api(&data).unwrap();
//         let table = sealed.symbol_table.clone();

//         let file2 = sealed.files().find(|f| f.name == "file2.rs").unwrap();
//         let uses = file2
//             .children
//             .borrow()
//             .iter()
//             .filter_map(|node| {
//                 if let NodeKind::Directive(Directive::Use(u)) = node {
//                     Some(u.clone())
//                 } else {
//                     None
//                 }
//             })
//             .collect::<Vec<_>>();
//         assert_eq!(uses.len(), 1);

//         let u = &uses[0];
//         let scope = table
//             .mod_scopes
//             .get("soroban_security_detectors_sdk::file2")
//             .expect("Module scope for file2 not found");
//         let DefinitionRef::Ref(_, expected) = resolve_path(
//             &table,
//             scope.borrow().id,
//             "soroban_security_detectors_sdk::file1::AStruct",
//         )
//         .unwrap() else {
//             panic!("Expected a reference to a definition");
//         };
//         let key = &u.imported_types[0];
//         assert_eq!(u.target.borrow().get(key), Some(&Some(expected)));
//     }
// }
