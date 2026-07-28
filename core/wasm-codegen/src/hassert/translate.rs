//! The specification-body-to-`hassert` translation itself.
//!
//! One [`SpecFnTranslator`] per specification function walks its typed AST and
//! produces a single [`HAssert`] obligation. The scheme is a right-folded
//! statement translator with two polarities ([`Mode::Univ`]/[`Mode::Exist`]) and
//! a small term translator that mirrors the WASM operators code generation emits
//! for the same expressions, so the obligation speaks the same numeric language
//! as the compiled body it constrains.
//!
//! ## Logical variables carry levels, not indices, until the end
//!
//! While a tree is under construction every [`HTerm::LVar`] stores an *absolute
//! binder level* (counted from the outside), not a de Bruijn index. A single
//! [`SpecFnTranslator::finalize`] pass then rewrites each level to the index it
//! has at its own depth. Levels are position-independent, so a pure `let` that
//! captures an existential variable and is used further inside more binders needs
//! no re-indexing at its use site — the final pass alone resolves it. This is
//! what keeps `exists { let a = @; let t = a + 1; let b = @; assert(b > t); }`
//! correct without shifting already-built subterms.

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, IdentId, StmtId, TypeId};
use inference_ast::nodes::{
    ArgKind, BlockKind, Def, Expr, Location, OperatorKind, Stmt, UnaryOperatorKind,
};
use inference_fn_key::FnKey;
use inference_hassert::{HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HTerm};
use inference_type_checker::type_info::{NumberType, TypeInfo, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashMap;

use super::CalleeIndex;
use super::diag::{HassertDiagnostic, PCode};

/// Polarity of the surrounding quantification.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Universal context: `assume` filters (antecedent), `if` is a
    /// conjunction of guarded implications, `@` takes a `T_local` slot.
    Univ,
    /// Existential context: `assume` constrains the witness (conjunct), `if` is
    /// a strict disjunction of guarded conjunctions, `@` binds an `HA_ex`
    /// logical variable.
    Exist,
}

/// What an identifier resolves to inside a specification body.
#[derive(Clone)]
enum Binding {
    /// A universally-quantified slot (`T_local n`).
    Slot(u32),
    /// An existential logical variable, stored as an absolute binder *level*.
    Level(u32),
    /// A pure `let`, inlined as its translated term. Any [`HTerm::LVar`] inside
    /// keeps its absolute level, so the term re-indexes correctly wherever it is
    /// later embedded.
    Term(HTerm),
}

/// The classification of a call's result, deciding whether it can be a term.
enum ResultClass {
    /// A single scalar (bool, integer, or enum) — a valid `T_app` term.
    Scalar,
    /// No result (`unit`) — only realizable as an `HA_app_ok` statement.
    Void,
    /// A compound result (array or struct) — memory-backed, not a term.
    Compound,
}

pub(super) struct SpecFnTranslator<'a> {
    arena: &'a AstArena,
    ctx: &'a TypedContext,
    module_path: &'a [String],
    spec_name: &'a str,
    callee: &'a CalleeIndex,
    /// Next universal slot number. Parameters take `0..P-1`; each universal `@`
    /// takes the next in encounter order. Never rewound — slots are global to
    /// the function.
    slots: u32,
    /// Number of committed existential binders enclosing the current point.
    depth: u32,
    /// Existential binders introduced by call-argument `@`s within the statement
    /// currently being translated, not yet wrapped around its atom.
    pending: u32,
    env: FxHashMap<String, Binding>,
    diags: Vec<HassertDiagnostic>,
}

impl<'a> SpecFnTranslator<'a> {
    pub(super) fn new(
        ctx: &'a TypedContext,
        module_path: &'a [String],
        spec_name: &'a str,
        callee: &'a CalleeIndex,
    ) -> Self {
        Self {
            arena: ctx.arena(),
            ctx,
            module_path,
            spec_name,
            callee,
            slots: 0,
            depth: 0,
            pending: 0,
            env: FxHashMap::default(),
            diags: Vec::new(),
        }
    }

    /// Removes and returns the diagnostics gathered so far.
    pub(super) fn take_diagnostics(&mut self) -> Vec<HassertDiagnostic> {
        std::mem::take(&mut self.diags)
    }

    /// Translates one specification free function into its obligation.
    ///
    /// A `forall`-quantified or plain (`Regular`) body is translated in
    /// universal mode. An `exists`/`unique`/`assume`-quantified body has no
    /// milestone-1 encoding and yields [`PCode::P001`] plus a trivial `⊤`
    /// obligation (discarded, since any diagnostic aborts code generation).
    pub(super) fn translate_fn(&mut self, def_id: DefId) -> HAssert {
        let (args, body) = match &self.arena[def_id].kind {
            Def::Function { args, body, .. } => (args.clone(), *body),
            _ => return HAssert::True,
        };

        let body_kind = self.arena[body].block_kind;
        match body_kind {
            BlockKind::Forall | BlockKind::Regular => {}
            BlockKind::Exists | BlockKind::Assume | BlockKind::Unique => {
                let name = self.arena.def_name(def_id).to_string();
                self.error(
                    PCode::P001,
                    self.arena[def_id].location,
                    format!(
                        "spec function `{name}` is `{}`-quantified; only `forall`-quantified \
                         (or plain) spec functions can be translated to a verification assertion \
                         yet — restructure the property as a `forall` function with a nested \
                         `exists` block",
                        block_kind_word(body_kind)
                    ),
                );
                return HAssert::True;
            }
        }

        self.bind_parameters(&args);

        let stmts = self.arena[body].stmts.clone();
        let raw = self.t_stmts(&stmts, Mode::Univ);
        // Rewrite every logical-variable level to the de Bruijn index it has at
        // its own binder depth, now that the whole tree (and thus every binder)
        // is known.
        lower_assert(&raw, 0)
    }

    /// Binds each parameter to a universal slot in declaration order. A
    /// non-scalar parameter type is [`PCode::P004`]; the slot is still consumed
    /// so later slot numbers stay aligned with the source.
    fn bind_parameters(&mut self, args: &[inference_ast::nodes::ArgData]) {
        for arg in args {
            match &arg.kind {
                ArgKind::Named { name, ty, .. } => {
                    if !self.type_is_scalar(*ty) {
                        self.error(PCode::P004, arg.location, self.non_scalar_message(*ty));
                    }
                    let slot = self.next_slot();
                    self.env
                        .insert(self.arena[*name].name.clone(), Binding::Slot(slot));
                }
                ArgKind::Ignored { ty } => {
                    if !self.type_is_scalar(*ty) {
                        self.error(PCode::P004, arg.location, self.non_scalar_message(*ty));
                    }
                    let _ = self.next_slot();
                }
                ArgKind::SelfRef { .. } | ArgKind::TypeOnly(_) => {}
            }
        }
    }

    // ----- statement-list translation -----------------------------------

    /// The right-folded statement translator. `⊤` for the empty list; each
    /// statement contributes a conjunct (or an implication antecedent, for a
    /// universal `assume`) over the translation of the rest.
    fn t_stmts(&mut self, stmts: &[StmtId], mode: Mode) -> HAssert {
        let Some((first, rest)) = stmts.split_first() else {
            return HAssert::True;
        };
        let stmt_id = *first;
        match &self.arena[stmt_id].kind {
            Stmt::VarDef {
                name, ty, value, ..
            } => {
                let (name, ty, value) = (*name, *ty, *value);
                self.t_var_def(name, ty, value, rest, mode)
            }
            Stmt::Assert { expr } => {
                let expr = *expr;
                let atom = self.eval_atom(mode, |s| s.p_expr(expr, mode));
                HAssert::and(atom, self.t_stmts(rest, mode))
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                let (condition, then_block, else_block) = (*condition, *then_block, *else_block);
                self.t_if(condition, then_block, else_block, rest, mode)
            }
            Stmt::Block(block_id) => {
                let block_id = *block_id;
                self.t_block(block_id, rest, mode)
            }
            Stmt::Return { expr } => {
                let expr = *expr;
                // A returned expression is validated (it may surface a
                // diagnostic) but contributes nothing; `return` is reachable
                // only in a `Regular`-kind body (analysis bans it under any
                // non-deterministic block).
                let _ = self.term(expr, mode);
                self.t_stmts(rest, mode)
            }
            Stmt::Expr(expr) => {
                let expr = *expr;
                self.t_expr_stmt(expr, rest, mode)
            }
            Stmt::ConstDef(def_id) => {
                let def_id = *def_id;
                self.bind_const(def_id, mode);
                self.t_stmts(rest, mode)
            }
            Stmt::TypeDef { .. } => self.t_stmts(rest, mode),
            Stmt::Assign { .. } => {
                self.error(
                    PCode::P003,
                    self.arena[stmt_id].location,
                    "reassignment is not supported in specification bodies; bind a new `let` \
                     instead"
                        .to_string(),
                );
                self.t_stmts(rest, mode)
            }
            Stmt::Loop { .. } => {
                self.error_no_encoding(self.arena[stmt_id].location, "`loop`");
                self.t_stmts(rest, mode)
            }
            Stmt::Break => {
                self.error_no_encoding(self.arena[stmt_id].location, "`break`");
                self.t_stmts(rest, mode)
            }
        }
    }

    /// `let` translation. A bare `@` right-hand side binds a slot (universal) or
    /// an existential binder; any other right-hand side is a pure `let`, inlined
    /// as a term.
    fn t_var_def(
        &mut self,
        name_id: IdentId,
        ty: TypeId,
        value: Option<ExprId>,
        rest: &[StmtId],
        mode: Mode,
    ) -> HAssert {
        let name = self.arena[name_id].name.clone();
        let Some(value_expr) = value else {
            // `let x: T;` without an initializer binds nothing to translate.
            return self.t_stmts(rest, mode);
        };

        if matches!(self.arena[value_expr].kind, Expr::Uzumaki) {
            if !self.type_is_scalar(ty) {
                self.emit_non_scalar_uzumaki(ty, self.arena[value_expr].location);
            }
            return match mode {
                Mode::Univ => {
                    let slot = self.next_slot();
                    self.env.insert(name, Binding::Slot(slot));
                    self.t_stmts(rest, Mode::Univ)
                }
                Mode::Exist => {
                    let level = self.depth;
                    self.env.insert(name, Binding::Level(level));
                    self.depth += 1;
                    let body = self.t_stmts(rest, Mode::Exist);
                    self.depth -= 1;
                    HAssert::ex(body)
                }
            };
        }

        // Pure `let`: translate the right-hand side once, then inline it. In
        // existential mode a call-argument `@` in the right-hand side introduces
        // binders that must scope over the rest of the block.
        let base = self.pending;
        let term = self.term(value_expr, mode);
        let introduced = self.pending - base;
        self.pending = base;
        self.env.insert(name, Binding::Term(term));

        if mode == Mode::Exist && introduced > 0 {
            self.depth += introduced;
            let body = self.t_stmts(rest, Mode::Exist);
            self.depth -= introduced;
            wrap_existentials(body, introduced)
        } else {
            self.t_stmts(rest, mode)
        }
    }

    /// A bare `if`. Universal mode is a conjunction of guarded implications
    /// (`nz`/`eqz` guards); existential mode is a strict disjunction of guarded
    /// conjunctions, so a non-denoting condition cannot fabricate a witness.
    fn t_if(
        &mut self,
        condition: ExprId,
        then_block: BlockId,
        else_block: Option<BlockId>,
        rest: &[StmtId],
        mode: Mode,
    ) -> HAssert {
        let guarded = match mode {
            Mode::Univ => {
                let cond = self.term(condition, Mode::Univ);
                let then_h =
                    self.scoped_block(then_block, self.branch_mode(then_block, Mode::Univ));
                if let Some(else_id) = else_block {
                    let else_h = self.scoped_block(else_id, self.branch_mode(else_id, Mode::Univ));
                    HAssert::and(
                        HAssert::imp(HAssert::nz(cond.clone()), then_h),
                        HAssert::imp(HAssert::eqz(cond), else_h),
                    )
                } else {
                    HAssert::imp(HAssert::nz(cond), then_h)
                }
            }
            Mode::Exist => {
                let base = self.pending;
                let cond = self.term(condition, Mode::Exist);
                let introduced = self.pending - base;
                self.pending = base;
                self.depth += introduced;
                self.check_branch_forall(then_block);
                let then_h = self.scoped_block(then_block, Mode::Exist);
                let disjunction = if let Some(else_id) = else_block {
                    self.check_branch_forall(else_id);
                    let else_h = self.scoped_block(else_id, Mode::Exist);
                    HAssert::or(
                        HAssert::and(HAssert::nz(cond.clone()), then_h),
                        HAssert::and(HAssert::eqz(cond), else_h),
                    )
                } else {
                    // No `else`: the else path is trivially satisfiable, so a
                    // determinate false condition alone discharges it.
                    HAssert::or(
                        HAssert::and(HAssert::nz(cond.clone()), then_h),
                        HAssert::eqz(cond),
                    )
                };
                self.depth -= introduced;
                wrap_existentials(disjunction, introduced)
            }
        };
        HAssert::and(guarded, self.t_stmts(rest, mode))
    }

    /// A block statement, dispatched on its kind. `assume` bodies always
    /// translate existentially (their `@`s read as "some choice satisfies the
    /// filter"); `assume` flips between implication (universal) and conjunction
    /// (existential).
    fn t_block(&mut self, block_id: BlockId, rest: &[StmtId], mode: Mode) -> HAssert {
        let kind = self.arena[block_id].block_kind;
        match kind {
            BlockKind::Assume => {
                let body = self.scoped_block(block_id, Mode::Exist);
                match mode {
                    Mode::Univ => HAssert::imp(body, self.t_stmts(rest, Mode::Univ)),
                    Mode::Exist => HAssert::and(body, self.t_stmts(rest, Mode::Exist)),
                }
            }
            BlockKind::Regular => {
                let body = self.scoped_block(block_id, mode);
                HAssert::and(body, self.t_stmts(rest, mode))
            }
            BlockKind::Forall => {
                if mode == Mode::Exist {
                    self.error(
                        PCode::P007,
                        self.arena[block_id].location,
                        "a `forall` block inside an `exists` block is not yet supported in \
                         assertion emission"
                            .to_string(),
                    );
                }
                let body = self.scoped_block(block_id, mode);
                HAssert::and(body, self.t_stmts(rest, mode))
            }
            BlockKind::Exists => {
                let body = self.scoped_block(block_id, Mode::Exist);
                HAssert::and(body, self.t_stmts(rest, mode))
            }
            BlockKind::Unique => {
                self.error_no_encoding(self.arena[block_id].location, "`unique` block");
                self.t_stmts(rest, mode)
            }
        }
    }

    /// A bare expression statement. A call becomes an `HA_app_ok` obligation at
    /// any result arity; any other expression is validated and dropped.
    fn t_expr_stmt(&mut self, expr: ExprId, rest: &[StmtId], mode: Mode) -> HAssert {
        if matches!(self.arena[expr].kind, Expr::FunctionCall { .. }) {
            let atom = self.eval_atom(mode, |s| s.app_ok(expr, mode));
            HAssert::and(atom, self.t_stmts(rest, mode))
        } else {
            let _ = self.term(expr, mode);
            self.t_stmts(rest, mode)
        }
    }

    /// Binds a block-local `const` as a pure term, exactly like a pure `let`.
    fn bind_const(&mut self, def_id: DefId, mode: Mode) {
        if let Def::Constant { name, value, .. } = &self.arena[def_id].kind {
            let (name, value) = (*name, *value);
            let term = self.term(value, mode);
            self.env
                .insert(self.arena[name].name.clone(), Binding::Term(term));
        }
    }

    // ----- assertion-position translators -------------------------------

    /// Truthiness of an assertion expression.
    fn p_expr(&mut self, expr: ExprId, mode: Mode) -> HAssert {
        match &self.arena[expr].kind {
            Expr::Parenthesized { expr } => {
                let expr = *expr;
                return self.p_expr(expr, mode);
            }
            Expr::PrefixUnary {
                expr,
                op: UnaryOperatorKind::Not,
            } => {
                let expr = *expr;
                return self.n_expr(expr, mode);
            }
            Expr::Binary { left, right, op } => {
                let (left, right, op) = (*left, *right, op.clone());
                match op {
                    OperatorKind::And => {
                        return HAssert::and(self.p_expr(left, mode), self.p_expr(right, mode));
                    }
                    OperatorKind::Or => {
                        return HAssert::or(self.p_expr(left, mode), self.p_expr(right, mode));
                    }
                    OperatorKind::Eq
                    | OperatorKind::Ne
                    | OperatorKind::Lt
                    | OperatorKind::Le
                    | OperatorKind::Gt
                    | OperatorKind::Ge => {
                        return self.p_comparison(left, right, &op, mode);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        // Atom: a bool variable, a call, a literal, …
        HAssert::nz(self.term(expr, mode))
    }

    /// Falsiness of an assertion expression (the De Morgan dual of [`Self::p_expr`]).
    fn n_expr(&mut self, expr: ExprId, mode: Mode) -> HAssert {
        match &self.arena[expr].kind {
            Expr::Parenthesized { expr } => {
                let expr = *expr;
                return self.n_expr(expr, mode);
            }
            Expr::PrefixUnary {
                expr,
                op: UnaryOperatorKind::Not,
            } => {
                let expr = *expr;
                return self.p_expr(expr, mode);
            }
            Expr::Binary {
                left,
                right,
                op: OperatorKind::And,
            } => {
                let (left, right) = (*left, *right);
                return HAssert::or(self.n_expr(left, mode), self.n_expr(right, mode));
            }
            Expr::Binary {
                left,
                right,
                op: OperatorKind::Or,
            } => {
                let (left, right) = (*left, *right);
                return HAssert::and(self.n_expr(left, mode), self.n_expr(right, mode));
            }
            _ => {}
        }
        // Atom: the strict positive zero-equality.
        HAssert::eqz(self.term(expr, mode))
    }

    /// A comparison in assertion position. `==` is the one operator whose
    /// encoding depends on the mode: strict `term_eq` on a witness path, the
    /// non-strict `nz(relop)` under universal quantification (so junk valuations
    /// discharge vacuously). `!=` conjoins `HA_defined` for each side that bears
    /// a `T_app`, matching the verifier's disequality discipline.
    fn p_comparison(
        &mut self,
        left: ExprId,
        right: ExprId,
        op: &OperatorKind,
        mode: Mode,
    ) -> HAssert {
        let (num_ty, unsigned) = self.operand_class(left);
        let ta = self.term(left, mode);
        let tb = self.term(right, mode);
        match op {
            OperatorKind::Eq => match mode {
                Mode::Univ => HAssert::nz(relop(num_ty, HRelop::Eq, ta, tb)),
                Mode::Exist => HAssert::TermEq(ta, tb),
            },
            OperatorKind::Ne => {
                let mut assertion = HAssert::nz(relop(num_ty, HRelop::Ne, ta.clone(), tb.clone()));
                if term_bears_app(&ta) {
                    assertion = HAssert::and(assertion, HAssert::Defined(ta));
                }
                if term_bears_app(&tb) {
                    assertion = HAssert::and(assertion, HAssert::Defined(tb));
                }
                assertion
            }
            OperatorKind::Lt => HAssert::nz(relop(num_ty, signed_relop(unsigned, Lt), ta, tb)),
            OperatorKind::Le => HAssert::nz(relop(num_ty, signed_relop(unsigned, Le), ta, tb)),
            OperatorKind::Gt => HAssert::nz(relop(num_ty, signed_relop(unsigned, Gt), ta, tb)),
            OperatorKind::Ge => HAssert::nz(relop(num_ty, signed_relop(unsigned, Ge), ta, tb)),
            _ => unreachable!("p_comparison only handles comparison operators"),
        }
    }

    // ----- term translation ---------------------------------------------

    /// Translates an expression to a term. On an untranslatable expression a
    /// diagnostic is recorded and a zero sentinel returned, so a single pass
    /// still collects every problem before the build is discarded.
    fn term(&mut self, expr: ExprId, mode: Mode) -> HTerm {
        match &self.arena[expr].kind {
            Expr::Parenthesized { expr } => {
                let expr = *expr;
                self.term(expr, mode)
            }
            Expr::NumberLiteral { value } => {
                let value = value.clone();
                self.number_literal(expr, &value)
            }
            Expr::BoolLiteral { value } => HTerm::Const(HConst::I32(i32::from(*value))),
            Expr::TypeMemberAccess {
                expr: type_expr,
                name,
            } => {
                let (type_expr, name) = (*type_expr, *name);
                self.enum_variant(expr, type_expr, name)
            }
            Expr::Identifier(ident_id) => {
                let ident_id = *ident_id;
                self.identifier(expr, ident_id)
            }
            Expr::Binary { left, right, op } => {
                let (left, right, op) = (*left, *right, op.clone());
                self.binary(expr, left, right, &op, mode)
            }
            Expr::PrefixUnary { expr: inner, op } => {
                let (inner, op) = (*inner, op.clone());
                self.unary(expr, inner, &op, mode)
            }
            Expr::FunctionCall { .. } => self.call_term(expr, mode),
            Expr::Uzumaki => {
                self.error(
                    PCode::P006,
                    self.arena[expr].location,
                    "uzumaki (@) can only be bound by a `let` or passed as a call argument \
                     inside a translated spec body"
                        .to_string(),
                );
                zero_sentinel()
            }
            Expr::ArrayIndexAccess { .. } => {
                self.error_no_encoding(self.arena[expr].location, "array indexing");
                zero_sentinel()
            }
            Expr::MemberAccess { .. } => {
                self.error_no_encoding(self.arena[expr].location, "struct field access");
                zero_sentinel()
            }
            Expr::StructLiteral { .. } => {
                self.error_no_encoding(self.arena[expr].location, "a struct literal");
                zero_sentinel()
            }
            Expr::ArrayLiteral { .. } => {
                self.error_no_encoding(self.arena[expr].location, "an array literal");
                zero_sentinel()
            }
            Expr::StringLiteral { .. } => {
                self.error_no_encoding(self.arena[expr].location, "a string literal");
                zero_sentinel()
            }
            Expr::UnitLiteral | Expr::Type(_) => {
                self.error(
                    PCode::P004,
                    self.arena[expr].location,
                    "type `unit` cannot appear in a specification term; only bool, integer, \
                     and enum values can"
                        .to_string(),
                );
                zero_sentinel()
            }
        }
    }

    /// A number literal, parsed and widened exactly as code generation lowers it.
    fn number_literal(&mut self, expr: ExprId, value: &str) -> HTerm {
        let kind = self.ctx.get_node_typeinfo(node_expr(expr)).map(|t| t.kind);
        match kind {
            Some(TypeInfoKind::Number(NumberType::I8 | NumberType::I16 | NumberType::I32))
            | None => HTerm::Const(HConst::I32(value.parse::<i32>().unwrap_or(0))),
            Some(TypeInfoKind::Number(NumberType::U8)) => {
                HTerm::Const(HConst::I32(i32::from(value.parse::<u8>().unwrap_or(0))))
            }
            Some(TypeInfoKind::Number(NumberType::U16)) => {
                HTerm::Const(HConst::I32(i32::from(value.parse::<u16>().unwrap_or(0))))
            }
            Some(TypeInfoKind::Number(NumberType::U32)) => {
                HTerm::Const(HConst::I32(value.parse::<u32>().unwrap_or(0).cast_signed()))
            }
            Some(TypeInfoKind::Number(NumberType::I64)) => {
                HTerm::Const(HConst::I64(value.parse::<i64>().unwrap_or(0)))
            }
            Some(TypeInfoKind::Number(NumberType::U64)) => {
                HTerm::Const(HConst::I64(value.parse::<u64>().unwrap_or(0).cast_signed()))
            }
            Some(other) => {
                self.error(
                    PCode::P004,
                    self.arena[expr].location,
                    format!(
                        "type `{other}` cannot appear in a specification term; only bool, \
                         integer, and enum values can"
                    ),
                );
                zero_sentinel()
            }
        }
    }

    /// An enum variant reference, lowered to its zero-based tag constant.
    fn enum_variant(&mut self, expr: ExprId, type_expr: ExprId, name: IdentId) -> HTerm {
        let variant = self.arena[name].name.clone();
        let enum_info = match self.ctx.get_node_typeinfo(node_expr(expr)).map(|t| t.kind) {
            Some(TypeInfoKind::Enum(_, key)) => self.ctx.lookup_enum(&key),
            _ => None,
        }
        .or_else(|| {
            let type_name = self.type_expr_name(type_expr)?;
            self.ctx.lookup_enum_in(&type_name, self.module_path)
        });

        if let Some(info) = enum_info {
            let tag = i32::try_from(info.variant_index(&variant).unwrap_or(0)).unwrap_or(0);
            HTerm::Const(HConst::I32(tag))
        } else {
            self.error(
                PCode::P004,
                self.arena[expr].location,
                "type of this value cannot appear in a specification term; only bool, integer, \
                 and enum values can"
                    .to_string(),
            );
            zero_sentinel()
        }
    }

    /// An identifier, resolved through the environment. A universal slot becomes
    /// `T_local`; an existential variable becomes `T_lvar` at its level (finalized
    /// later); a pure `let` inlines its stored term.
    fn identifier(&mut self, expr: ExprId, ident_id: IdentId) -> HTerm {
        let name = &self.arena[ident_id].name;
        match self.env.get(name) {
            Some(Binding::Slot(n)) => HTerm::Local(*n),
            Some(Binding::Level(level)) => HTerm::LVar(*level),
            Some(Binding::Term(term)) => term.clone(),
            None => {
                let rendered = self
                    .ctx
                    .get_node_typeinfo(node_expr(expr))
                    .map_or_else(|| "this value".to_string(), |t| t.to_string());
                self.error(
                    PCode::P004,
                    self.arena[expr].location,
                    format!(
                        "type `{rendered}` cannot appear in a specification term; only bool, \
                         integer, and enum values can"
                    ),
                );
                zero_sentinel()
            }
        }
    }

    /// A binary expression as a term, mirroring `lower_binary_expression`: the
    /// number class and signedness come from the left operand, sub-word results
    /// are narrowed after the arithmetic and bitwise operators, and `**` has no
    /// encoding.
    fn binary(
        &mut self,
        expr: ExprId,
        left: ExprId,
        right: ExprId,
        op: &OperatorKind,
        mode: Mode,
    ) -> HTerm {
        if matches!(op, OperatorKind::Pow) {
            self.error_no_encoding(self.arena[expr].location, "`**` (the power operator)");
            return zero_sentinel();
        }
        let (num_ty, unsigned) = self.operand_class(left);
        let l = self.term(left, mode);
        let r = self.term(right, mode);
        let term = match op {
            OperatorKind::Add => binop(num_ty, HBinop::Add, l, r),
            OperatorKind::Sub => binop(num_ty, HBinop::Sub, l, r),
            OperatorKind::Mul => binop(num_ty, HBinop::Mul, l, r),
            OperatorKind::Div => binop(
                num_ty,
                if unsigned { HBinop::DivU } else { HBinop::DivS },
                l,
                r,
            ),
            OperatorKind::Mod => binop(
                num_ty,
                if unsigned { HBinop::RemU } else { HBinop::RemS },
                l,
                r,
            ),
            OperatorKind::BitAnd => binop(num_ty, HBinop::And, l, r),
            OperatorKind::BitOr => binop(num_ty, HBinop::Or, l, r),
            OperatorKind::BitXor => binop(num_ty, HBinop::Xor, l, r),
            OperatorKind::Shl => binop(num_ty, HBinop::Shl, l, r),
            OperatorKind::Shr => binop(
                num_ty,
                if unsigned { HBinop::ShrU } else { HBinop::ShrS },
                l,
                r,
            ),
            // `&&`/`||` are non-short-circuit i32 bit operations as terms.
            OperatorKind::And => binop(HNumType::I32, HBinop::And, l, r),
            OperatorKind::Or => binop(HNumType::I32, HBinop::Or, l, r),
            OperatorKind::Eq => relop(num_ty, HRelop::Eq, l, r),
            OperatorKind::Ne => relop(num_ty, HRelop::Ne, l, r),
            OperatorKind::Lt => relop(num_ty, signed_relop(unsigned, Lt), l, r),
            OperatorKind::Le => relop(num_ty, signed_relop(unsigned, Le), l, r),
            OperatorKind::Gt => relop(num_ty, signed_relop(unsigned, Gt), l, r),
            OperatorKind::Ge => relop(num_ty, signed_relop(unsigned, Ge), l, r),
            OperatorKind::Pow => unreachable!("`**` handled above"),
        };
        if narrows(op) {
            let left_kind = self.ctx.get_node_typeinfo(node_expr(left)).map(|t| t.kind);
            narrow(term, left_kind.as_ref())
        } else {
            term
        }
    }

    /// A prefix-unary expression as a term, mirroring code generation: negation
    /// is `0 - x` (narrowed), logical `!` is the term-level `i32.eqz`
    /// (`relop Eq 0`), bitwise `~` is `xor -1` (narrowed).
    fn unary(&mut self, expr: ExprId, inner: ExprId, op: &UnaryOperatorKind, mode: Mode) -> HTerm {
        let kind = self.ctx.get_node_typeinfo(node_expr(expr)).map(|t| t.kind);
        let is_i64 = matches!(
            kind,
            Some(TypeInfoKind::Number(NumberType::I64 | NumberType::U64))
        );
        let num_ty = if is_i64 { HNumType::I64 } else { HNumType::I32 };
        let inner_term = self.term(inner, mode);
        match op {
            UnaryOperatorKind::Neg => {
                let zero = if is_i64 {
                    HTerm::Const(HConst::I64(0))
                } else {
                    HTerm::Const(HConst::I32(0))
                };
                let sub = binop(num_ty, HBinop::Sub, zero, inner_term);
                if is_i64 {
                    sub
                } else {
                    narrow(sub, kind.as_ref())
                }
            }
            UnaryOperatorKind::Not => relop(
                HNumType::I32,
                HRelop::Eq,
                inner_term,
                HTerm::Const(HConst::I32(0)),
            ),
            UnaryOperatorKind::BitNot => {
                let minus_one = if is_i64 {
                    HTerm::Const(HConst::I64(-1))
                } else {
                    HTerm::Const(HConst::I32(-1))
                };
                let xor = binop(num_ty, HBinop::Xor, inner_term, minus_one);
                if is_i64 {
                    xor
                } else {
                    narrow(xor, kind.as_ref())
                }
            }
        }
    }

    /// A call in term position: a single scalar result becomes a `T_app` term,
    /// anything else is [`PCode::P005`].
    fn call_term(&mut self, call_expr: ExprId, mode: Mode) -> HTerm {
        let (function, args) = self.call_parts(call_expr);
        match self.resolve_callee(function) {
            Ok((key, def_id)) => match self.result_class(call_expr, def_id) {
                ResultClass::Scalar => {
                    let arg_terms = self.arg_terms(&args, mode);
                    HTerm::App(HFnRef(key.to_string()), arg_terms)
                }
                ResultClass::Void | ResultClass::Compound => {
                    self.error_call(function, "its result is not a single scalar");
                    zero_sentinel()
                }
            },
            Err(reason) => {
                self.error_call(function, reason);
                zero_sentinel()
            }
        }
    }

    /// A bare statement call, realized as `HA_app_ok` at any result arity.
    fn app_ok(&mut self, call_expr: ExprId, mode: Mode) -> HAssert {
        let (function, args) = self.call_parts(call_expr);
        match self.resolve_callee(function) {
            Ok((key, _)) => {
                let arg_terms = self.arg_terms(&args, mode);
                HAssert::AppOk(HFnRef(key.to_string()), arg_terms)
            }
            Err(reason) => {
                self.error_call(function, reason);
                HAssert::True
            }
        }
    }

    /// Translates a call's arguments, with `@` in argument position taking a
    /// fresh slot (universal) or an existential binder (existential).
    fn arg_terms(&mut self, args: &[(Option<IdentId>, ExprId)], mode: Mode) -> Vec<HTerm> {
        args.iter()
            .map(|(_, arg)| {
                if matches!(self.arena[*arg].kind, Expr::Uzumaki) {
                    self.uzumaki_argument(mode)
                } else {
                    self.term(*arg, mode)
                }
            })
            .collect()
    }

    /// A `@` in call-argument position: an anonymous universal slot, or a
    /// pending existential binder to be wrapped around the enclosing statement.
    fn uzumaki_argument(&mut self, mode: Mode) -> HTerm {
        match mode {
            Mode::Univ => HTerm::Local(self.next_slot()),
            Mode::Exist => {
                let level = self.depth + self.pending;
                self.pending += 1;
                HTerm::LVar(level)
            }
        }
    }

    /// Resolves a call's callee to a `(FnKey, DefId)` for a module-defined,
    /// deterministic function, or an [`PCode::P005`] reason.
    ///
    /// Mirrors code generation's resolution: a bare same-file call (including a
    /// spec-sibling helper) is resolved spec-first then by the current file's
    /// free key; a cross-file item import, a `::`-qualified free function, and an
    /// associated function use the type-checker-recorded target; an instance
    /// method has no term encoding.
    fn resolve_callee(&self, function: ExprId) -> Result<(FnKey, DefId), &'static str> {
        match &self.arena[function].kind {
            Expr::Identifier(ident_id) => {
                let name = self.arena[*ident_id].name.clone();
                if self.ctx.is_extern_function(&name) {
                    return Err("external functions carry no verified body");
                }
                // A cross-file item import (`use lib::arith::{add}; add()`)
                // resolves to its defining file via the recorded target.
                if let Some(target) = self.ctx.call_target(function)
                    && target.receiver_struct.is_none()
                    && target.module_path != self.module_path
                {
                    return self.validate_defined(FnKey::free_in(
                        target.module_path.clone(),
                        target.name.clone(),
                    ));
                }
                // Same-file: the spec-mangled sibling key first, then the file's
                // free key — exactly `resolve_free_callee_idx`.
                let spec_key =
                    FnKey::spec_free_folded(self.module_path, self.spec_name, name.clone());
                if let Some(def_id) = self.callee.get(&spec_key) {
                    return self.validate_body(spec_key, def_id);
                }
                let free_key = FnKey::free_in(self.module_path.to_vec(), name);
                if let Some(def_id) = self.callee.get(&free_key) {
                    return self.validate_body(free_key, def_id);
                }
                Err("external functions carry no verified body")
            }
            // `Point::new()` / `math::arith::add()`: the recorded target names the
            // struct's or free function's defining file.
            Expr::TypeMemberAccess { .. } => {
                let Some(target) = self.ctx.call_target(function) else {
                    return Err("it does not resolve to a module-defined function");
                };
                let key = match &target.receiver_struct {
                    Some(struct_name) => FnKey::method_in(
                        target.module_path.clone(),
                        struct_name.clone(),
                        target.name.clone(),
                    ),
                    None => FnKey::free_in(target.module_path.clone(), target.name.clone()),
                };
                self.validate_defined(key)
            }
            Expr::MemberAccess { .. } => Err("instance methods operate on memory"),
            _ => Err("it does not resolve to a module-defined function"),
        }
    }

    /// Confirms a `FnKey` names a module-defined function (not an import) and
    /// validates its body.
    fn validate_defined(&self, key: FnKey) -> Result<(FnKey, DefId), &'static str> {
        match self.callee.get(&key) {
            Some(def_id) => self.validate_body(key, def_id),
            None => Err("external functions carry no verified body"),
        }
    }

    /// Rejects a callee whose body contains non-deterministic constructs — it can
    /// carry no realized claim.
    fn validate_body(&self, key: FnKey, def_id: DefId) -> Result<(FnKey, DefId), &'static str> {
        if self.arena.def_is_non_det(def_id) {
            return Err("its body is non-deterministic and has no executable meaning");
        }
        Ok((key, def_id))
    }

    /// Classifies a call's result. The recorded result type is preferred; a
    /// missing one falls back to the callee's declared return type.
    fn result_class(&self, call_expr: ExprId, def_id: DefId) -> ResultClass {
        if let Some(kind) = self
            .ctx
            .get_node_typeinfo(node_expr(call_expr))
            .map(|t| t.kind)
        {
            return self.classify_type_kind(&kind);
        }
        match &self.arena[def_id].kind {
            Def::Function { returns, .. } => match returns {
                None => ResultClass::Void,
                Some(ty) => self.classify_type_kind(&TypeInfo::from_type_id(self.arena, *ty).kind),
            },
            _ => ResultClass::Compound,
        }
    }

    fn classify_type_kind(&self, kind: &TypeInfoKind) -> ResultClass {
        match kind {
            TypeInfoKind::Bool | TypeInfoKind::Number(_) | TypeInfoKind::Enum(_, _) => {
                ResultClass::Scalar
            }
            TypeInfoKind::Unit => ResultClass::Void,
            TypeInfoKind::Custom(name) => {
                if self.ctx.lookup_enum_in(name, self.module_path).is_some() {
                    ResultClass::Scalar
                } else {
                    ResultClass::Compound
                }
            }
            _ => ResultClass::Compound,
        }
    }

    // ----- helpers -------------------------------------------------------

    /// Splits a `FunctionCall` into its callee expression and owned argument list.
    fn call_parts(&self, call_expr: ExprId) -> (ExprId, Vec<(Option<IdentId>, ExprId)>) {
        match &self.arena[call_expr].kind {
            Expr::FunctionCall { function, args, .. } => (*function, args.clone()),
            _ => unreachable!("call_parts called on a non-call expression"),
        }
    }

    /// Runs `f` at existential depth, wrapping its atom in one `HA_ex` per
    /// call-argument `@` that `f` introduced. A no-op under universal mode.
    fn eval_atom<F>(&mut self, mode: Mode, f: F) -> HAssert
    where
        F: FnOnce(&mut Self) -> HAssert,
    {
        match mode {
            Mode::Univ => f(self),
            Mode::Exist => {
                let base = self.pending;
                let atom = f(self);
                let introduced = self.pending - base;
                self.pending = base;
                wrap_existentials(atom, introduced)
            }
        }
    }

    /// Translates a block's statements as a fresh environment scope, so a
    /// branch-local `let` does not leak to the rest of the enclosing block.
    fn scoped_block(&mut self, block_id: BlockId, mode: Mode) -> HAssert {
        let stmts = self.arena[block_id].stmts.clone();
        let saved = self.env.clone();
        let result = self.t_stmts(&stmts, mode);
        self.env = saved;
        result
    }

    /// The translation mode of an `if` branch: existential when the branch block
    /// is an `exists` block, otherwise the enclosing mode.
    fn branch_mode(&self, block_id: BlockId, outer: Mode) -> Mode {
        match self.arena[block_id].block_kind {
            BlockKind::Exists => Mode::Exist,
            _ => outer,
        }
    }

    /// Records [`PCode::P007`] when an existential `if` branch is a `forall`
    /// block, which needs a `Hall` over logical variables (deferred).
    fn check_branch_forall(&mut self, block_id: BlockId) {
        if self.arena[block_id].block_kind == BlockKind::Forall {
            self.error(
                PCode::P007,
                self.arena[block_id].location,
                "a `forall` block inside an `exists` block is not yet supported in assertion \
                 emission"
                    .to_string(),
            );
        }
    }

    /// The number class and signedness of an operand, read from its type exactly
    /// as `lower_binary_expression` reads the left operand's.
    fn operand_class(&self, expr: ExprId) -> (HNumType, bool) {
        let kind = self.ctx.get_node_typeinfo(node_expr(expr)).map(|t| t.kind);
        let is_i64 = matches!(
            kind,
            Some(TypeInfoKind::Number(NumberType::I64 | NumberType::U64))
        );
        let unsigned = matches!(
            kind,
            Some(TypeInfoKind::Number(
                NumberType::U8 | NumberType::U16 | NumberType::U32 | NumberType::U64
            ))
        );
        (if is_i64 { HNumType::I64 } else { HNumType::I32 }, unsigned)
    }

    /// Whether a declared type is a scalar the term language can represent (a
    /// bool, an integer, or an enum).
    fn type_is_scalar(&self, ty: TypeId) -> bool {
        match TypeInfo::from_type_id(self.arena, ty).kind {
            TypeInfoKind::Bool | TypeInfoKind::Number(_) => true,
            TypeInfoKind::Custom(name) => {
                self.ctx.lookup_enum_in(&name, self.module_path).is_some()
            }
            TypeInfoKind::Qualified(path) => {
                let segments: Vec<String> = path.split("::").map(str::to_string).collect();
                self.ctx.qualified_path_is_enum(&segments, self.module_path)
            }
            _ => false,
        }
    }

    /// Renders the diagnostic message for a non-scalar type in term/parameter
    /// position.
    fn non_scalar_message(&self, ty: TypeId) -> String {
        format!(
            "type `{}` cannot appear in a specification term; only bool, integer, and enum \
             values can",
            TypeInfo::from_type_id(self.arena, ty)
        )
    }

    /// Emits the right diagnostic for a `@` at a non-scalar type: [`PCode::P008`]
    /// for a compound (array/struct) type, [`PCode::P004`] otherwise.
    fn emit_non_scalar_uzumaki(&mut self, ty: TypeId, location: Location) {
        let type_info = TypeInfo::from_type_id(self.arena, ty);
        let compound = match &type_info.kind {
            TypeInfoKind::Array(_, _) => true,
            TypeInfoKind::Custom(name) => {
                self.ctx.lookup_struct_in(name, self.module_path).is_some()
            }
            _ => false,
        };
        if compound {
            self.error(
                PCode::P008,
                location,
                format!(
                    "uzumaki (@) over compound type `{type_info}` has no assertion encoding; \
                     quantify scalar components individually"
                ),
            );
        } else {
            self.error(
                PCode::P004,
                location,
                format!(
                    "type `{type_info}` cannot appear in a specification term; only bool, \
                     integer, and enum values can"
                ),
            );
        }
    }

    /// The bare type name of a type expression, for enum resolution fallback.
    fn type_expr_name(&self, type_expr: ExprId) -> Option<String> {
        match &self.arena[type_expr].kind {
            Expr::Identifier(id) => Some(self.arena[*id].name.clone()),
            Expr::Type(ty_id) => match &self.arena[*ty_id].kind {
                inference_ast::nodes::TypeNode::Custom(id) => Some(self.arena[*id].name.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    fn next_slot(&mut self) -> u32 {
        let slot = self.slots;
        self.slots += 1;
        slot
    }

    fn error(&mut self, code: PCode, location: Location, message: String) {
        self.diags.push(HassertDiagnostic::new(
            code,
            location,
            self.module_path.to_vec(),
            message,
        ));
    }

    fn error_no_encoding(&mut self, location: Location, construct: &str) {
        self.error(
            PCode::P002,
            location,
            format!(
                "{construct} has no encoding in the verification assertion language; remove it \
                 from the spec body or move the logic into an executable helper function"
            ),
        );
    }

    fn error_call(&mut self, function: ExprId, reason: &str) {
        let name = self.call_display_name(function);
        self.error(
            PCode::P005,
            self.arena[function].location,
            format!("call to `{name}` cannot be used in a specification term ({reason})"),
        );
    }

    /// A readable callee name for a diagnostic (bare name, or `Type::method`).
    fn call_display_name(&self, function: ExprId) -> String {
        match &self.arena[function].kind {
            Expr::Identifier(id) => self.arena[*id].name.clone(),
            Expr::MemberAccess { name, .. } | Expr::TypeMemberAccess { name, .. } => {
                self.arena[*name].name.clone()
            }
            _ => "<anonymous>".to_string(),
        }
    }
}

// ----- free helpers -----------------------------------------------------

/// A distinct signed-relop selector, so [`signed_relop`] reads clearly.
#[derive(Clone, Copy)]
enum Ordered {
    Lt,
    Le,
    Gt,
    Ge,
}
use Ordered::{Ge, Gt, Le, Lt};

fn signed_relop(unsigned: bool, op: Ordered) -> HRelop {
    match (op, unsigned) {
        (Lt, false) => HRelop::LtS,
        (Lt, true) => HRelop::LtU,
        (Le, false) => HRelop::LeS,
        (Le, true) => HRelop::LeU,
        (Gt, false) => HRelop::GtS,
        (Gt, true) => HRelop::GtU,
        (Ge, false) => HRelop::GeS,
        (Ge, true) => HRelop::GeU,
    }
}

fn binop(ty: HNumType, op: HBinop, l: HTerm, r: HTerm) -> HTerm {
    HTerm::Binop(ty, op, Box::new(l), Box::new(r))
}

fn relop(ty: HNumType, op: HRelop, l: HTerm, r: HTerm) -> HTerm {
    HTerm::Relop(ty, op, Box::new(l), Box::new(r))
}

fn zero_sentinel() -> HTerm {
    HTerm::Const(HConst::I32(0))
}

/// The `emit_sub_i32_narrowing` mirror: truncates an i32 result to a sub-word
/// width. Signed widths sign-extend by `shl`/`shr_s`; unsigned widths mask.
fn narrow(term: HTerm, kind: Option<&TypeInfoKind>) -> HTerm {
    match kind {
        Some(TypeInfoKind::Number(NumberType::I8)) => sign_extend(term, 24),
        Some(TypeInfoKind::Number(NumberType::I16)) => sign_extend(term, 16),
        Some(TypeInfoKind::Number(NumberType::U8)) => mask(term, 0xFF),
        Some(TypeInfoKind::Number(NumberType::U16)) => mask(term, 0xFFFF),
        _ => term,
    }
}

fn sign_extend(term: HTerm, shift: i32) -> HTerm {
    binop(
        HNumType::I32,
        HBinop::ShrS,
        binop(
            HNumType::I32,
            HBinop::Shl,
            term,
            HTerm::Const(HConst::I32(shift)),
        ),
        HTerm::Const(HConst::I32(shift)),
    )
}

fn mask(term: HTerm, bits: i32) -> HTerm {
    binop(
        HNumType::I32,
        HBinop::And,
        term,
        HTerm::Const(HConst::I32(bits)),
    )
}

/// Whether the operator narrows a sub-word result, matching the exclusion list
/// in `lower_binary_expression` (relations, `%`, `&&`/`||`, `>>` do not).
fn narrows(op: &OperatorKind) -> bool {
    matches!(
        op,
        OperatorKind::Add
            | OperatorKind::Sub
            | OperatorKind::Mul
            | OperatorKind::Div
            | OperatorKind::BitAnd
            | OperatorKind::BitOr
            | OperatorKind::BitXor
            | OperatorKind::Shl
    )
}

/// Whether a term contains a `T_app` anywhere, deciding whether a generated
/// disequality must conjoin `HA_defined` for that side.
fn term_bears_app(term: &HTerm) -> bool {
    match term {
        HTerm::App(_, _) => true,
        HTerm::Binop(_, _, l, r) | HTerm::Relop(_, _, l, r) => {
            term_bears_app(l) || term_bears_app(r)
        }
        HTerm::Const(_) | HTerm::LVar(_) | HTerm::Local(_) => false,
    }
}

/// Wraps an assertion in `count` existential binders.
fn wrap_existentials(mut assertion: HAssert, count: u32) -> HAssert {
    for _ in 0..count {
        assertion = HAssert::ex(assertion);
    }
    assertion
}

fn block_kind_word(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Exists => "exists",
        BlockKind::Assume => "assume",
        BlockKind::Unique => "unique",
        BlockKind::Forall => "forall",
        BlockKind::Regular => "regular",
    }
}

fn node_expr(expr: ExprId) -> inference_ast::ids::NodeId {
    inference_ast::ids::NodeId::Expr(expr)
}

/// Rewrites logical-variable levels to de Bruijn indices, descending under each
/// `HA_ex` binder.
fn lower_assert(assertion: &HAssert, depth: u32) -> HAssert {
    match assertion {
        HAssert::True => HAssert::True,
        HAssert::False => HAssert::False,
        HAssert::Not(inner) => HAssert::Not(Box::new(lower_assert(inner, depth))),
        HAssert::And(l, r) => HAssert::And(
            Box::new(lower_assert(l, depth)),
            Box::new(lower_assert(r, depth)),
        ),
        HAssert::Imp(l, r) => HAssert::Imp(
            Box::new(lower_assert(l, depth)),
            Box::new(lower_assert(r, depth)),
        ),
        HAssert::Or(l, r) => HAssert::Or(
            Box::new(lower_assert(l, depth)),
            Box::new(lower_assert(r, depth)),
        ),
        HAssert::Ex(body) => HAssert::Ex(Box::new(lower_assert(body, depth + 1))),
        HAssert::TermEq(a, b) => HAssert::TermEq(lower_term(a, depth), lower_term(b, depth)),
        HAssert::HasType(t, ty) => HAssert::HasType(lower_term(t, depth), *ty),
        HAssert::Defined(t) => HAssert::Defined(lower_term(t, depth)),
        HAssert::AppOk(f, args) => HAssert::AppOk(
            f.clone(),
            args.iter().map(|t| lower_term(t, depth)).collect(),
        ),
    }
}

fn lower_term(term: &HTerm, depth: u32) -> HTerm {
    match term {
        HTerm::LVar(level) => {
            debug_assert!(
                *level < depth,
                "logical variable level {level} is not bound at depth {depth}"
            );
            HTerm::LVar(depth - 1 - level)
        }
        HTerm::Const(_) | HTerm::Local(_) => term.clone(),
        HTerm::App(f, args) => HTerm::App(
            f.clone(),
            args.iter().map(|t| lower_term(t, depth)).collect(),
        ),
        HTerm::Binop(ty, op, l, r) => binop(*ty, *op, lower_term(l, depth), lower_term(r, depth)),
        HTerm::Relop(ty, op, l, r) => relop(*ty, *op, lower_term(l, depth), lower_term(r, depth)),
    }
}
