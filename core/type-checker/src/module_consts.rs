//! Module-scope constants: what each one's value is, and where the compound ones
//! live in linear memory.
//!
//! A `const` declared at the top of a file has no function to run its
//! initializer in, so the compiler computes the value itself, once, after type
//! checking. A scalar's value is then emitted as an immediate at every use, and
//! costs no memory. An array's or a struct's value is laid out as bytes in the
//! **static data region**, which code generation places directly above the
//! shadow stack in one active data segment; every use reads it there.
//!
//! # Why this lives in the type checker
//!
//! Two later phases need the same answers and must not compute them twice.
//! Analysis sizes the static data region against the memory the build declares
//! (and shrinks the default stack to make room for it), and code generation
//! emits the region and addresses into it. If each laid the region out on its
//! own, a disagreement of one byte would make the stack the analysis measures
//! call chains against differ from the stack the module declares. This crate is
//! the one both depend on, so the value of every constant, and the placement of
//! every compound one, is decided here and read by both.
//!
//! # What an initializer may contain
//!
//! A *constant expression*: literals, enum variants, other module constants,
//! the arithmetic, bitwise, shift, comparison and logical operators, `checked`
//! and `wrapping`, member and index access into a constant, and array, repeated
//! array and struct literals of those. A function call, `@`, or anything else
//! whose value is only known when the program runs is refused with
//! [`TypeCheckError::NonConstantInitializer`].
//!
//! # Evaluation follows the program's own arithmetic
//!
//! Each operator computes what the emitted instruction would, at the operand's
//! type: two's-complement widths, truncating division, arithmetic shifts for
//! signed types and logical ones for unsigned. `+`, `-`, `*` and unary `-` are
//! checked unless a `wrapping(...)` encloses them, exactly as in a function
//! body. Where a run of the program would trap — an overflowing checked
//! operator, a division by zero, or an index past the end — there is no value
//! to emit, and the build is refused with
//! [`TypeCheckError::ConstEvaluationFailed`] at the failing operation.
//!
//! A shift count is the one place the evaluator is stricter than the machine:
//! the emitted shift takes its count modulo the width and does not trap, but a
//! constant count that is negative or at or past the width is always a mistake,
//! so the evaluator refuses it with the same error. Analysis rule A044 already
//! refuses such a count when it is a literal.
//!
//! # Byte layout
//!
//! A value's bytes follow the layout code generation gives the same type in a
//! frame slot (`core/wasm-codegen/src/memory.rs`): little-endian scalars at
//! their natural width (`bool` one byte, an enum its four-byte tag), arrays as
//! their elements back to back, and structs in declaration order with each
//! field at its natural alignment and the size rounded up to the largest one.
//! Padding is zero.

use inference_ast::ids::{DefId, ExprId, NodeId};
use inference_ast::nodes::{ArithMode, Def, Expr, Location, OperatorKind, UnaryOperatorKind};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::errors::{ConstEvalFailure, TypeCheckError, number_bits};
use crate::type_info::{NumberType, TypeInfoKind};
use crate::typed_context::TypedContext;

/// The value of a module-scope constant.
///
/// A scalar holds the mathematical value it denotes; a compound holds its
/// elements or fields. Every integer is within its type's range — evaluation
/// refuses a value that is not — so a consumer can emit or serialize one
/// without re-checking it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstValue {
    /// An integer at the type it was computed at.
    Int { value: i128, number: NumberType },
    /// A `bool`.
    Bool(bool),
    /// An enum variant, as the zero-based tag code generation emits for it.
    Enum { tag: u32 },
    /// An array, one value per element.
    Array(Vec<ConstValue>),
    /// `[value; count]`: an array whose every element is `value`, kept as one
    /// value so a large repeated array costs one element rather than `count`.
    Repeat { value: Box<ConstValue>, count: u32 },
    /// A struct, its fields in declaration order.
    Struct(Vec<(String, ConstValue)>),
}

impl ConstValue {
    /// Whether the value is an array or a struct, and so lives in the static
    /// data region rather than being emitted inline.
    #[must_use = "this is a pure check with no side effects"]
    pub fn is_compound(&self) -> bool {
        matches!(
            self,
            ConstValue::Array(_) | ConstValue::Repeat { .. } | ConstValue::Struct(_)
        )
    }

    /// The number of elements of an array value; `None` for anything else.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn element_count(&self) -> Option<u32> {
        match self {
            ConstValue::Array(elements) => u32::try_from(elements.len()).ok(),
            ConstValue::Repeat { count, .. } => Some(*count),
            _ => None,
        }
    }

    /// The element at `index` of an array value; `None` past the end or for a
    /// value that is not an array.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn element(&self, index: u32) -> Option<&ConstValue> {
        match self {
            ConstValue::Array(elements) => elements.get(index as usize),
            ConstValue::Repeat { value, count } if index < *count => Some(value),
            _ => None,
        }
    }

    /// The field named `name` of a struct value; `None` for a missing field or
    /// a value that is not a struct.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn field(&self, name: &str) -> Option<&ConstValue> {
        match self {
            ConstValue::Struct(fields) => fields
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The bytes the value occupies, under code generation's layout of its
    /// type. See the module documentation for the rules.
    #[must_use = "returns the size without side effects"]
    pub fn byte_size(&self) -> u64 {
        match self {
            ConstValue::Int { number, .. } => u64::from(number_bits(*number) / 8),
            ConstValue::Bool(_) => 1,
            ConstValue::Enum { .. } => 4,
            ConstValue::Array(elements) => elements
                .first()
                .map_or(0, |first| first.byte_size() * elements.len() as u64),
            ConstValue::Repeat { value, count } => value.byte_size() * u64::from(*count),
            ConstValue::Struct(fields) => {
                let mut offset = 0u64;
                for (_, field) in fields {
                    offset = align_up(offset, field.alignment()) + field.byte_size();
                }
                align_up(offset, self.alignment())
            }
        }
    }

    /// The value's natural alignment in bytes: a scalar's width, an array's
    /// element's, a struct's largest field's. Never more than 8.
    #[must_use = "returns the alignment without side effects"]
    pub fn alignment(&self) -> u32 {
        match self {
            ConstValue::Int { number, .. } => number_bits(*number) / 8,
            ConstValue::Bool(_) => 1,
            ConstValue::Enum { .. } => 4,
            ConstValue::Array(elements) => elements.first().map_or(1, ConstValue::alignment),
            ConstValue::Repeat { value, .. } => value.alignment(),
            ConstValue::Struct(fields) => fields
                .iter()
                .map(|(_, field)| field.alignment())
                .max()
                .unwrap_or(1),
        }
    }

    /// Writes the value's bytes into the start of `out`, which must be at least
    /// [`Self::byte_size`] long and zeroed where padding falls.
    ///
    /// # Panics
    ///
    /// Panics if `out` is shorter than the value.
    pub fn write_bytes(&self, out: &mut [u8]) {
        match self {
            ConstValue::Int { value, number } => {
                let width = (number_bits(*number) / 8) as usize;
                out[..width].copy_from_slice(&value.to_le_bytes()[..width]);
            }
            ConstValue::Bool(value) => out[0] = u8::from(*value),
            ConstValue::Enum { tag } => out[..4].copy_from_slice(&tag.to_le_bytes()),
            ConstValue::Array(elements) => {
                let stride = elements.first().map_or(0, ConstValue::byte_size) as usize;
                for (index, element) in elements.iter().enumerate() {
                    element.write_bytes(&mut out[index * stride..]);
                }
            }
            ConstValue::Repeat { value, count } => {
                let stride = value.byte_size() as usize;
                for index in 0..*count as usize {
                    value.write_bytes(&mut out[index * stride..]);
                }
            }
            ConstValue::Struct(fields) => {
                let mut offset = 0u64;
                for (_, field) in fields {
                    offset = align_up(offset, field.alignment());
                    field.write_bytes(&mut out[offset as usize..]);
                    offset += field.byte_size();
                }
            }
        }
    }
}

/// Rounds `offset` up to the next multiple of `alignment`.
fn align_up(offset: u64, alignment: u32) -> u64 {
    let alignment = u64::from(alignment.max(1));
    offset.div_ceil(alignment) * alignment
}

/// One compound module constant's place in the static data region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticDataEntry {
    /// The constant's declaration.
    pub def_id: DefId,
    /// The constant's name as declared.
    pub name: String,
    /// The defining file's module path; empty for the entry file.
    pub module_path: Vec<String>,
    /// The declaration's location, which a diagnostic about the region points
    /// at.
    pub location: Location,
    /// Offset of the constant's first byte from the start of the region.
    pub offset: u64,
    /// The constant's size in bytes.
    pub size: u64,
}

/// The static data region's layout: every compound module constant a body
/// names, in source order, each at its natural alignment.
///
/// Offsets are relative to the start of the region. Where the region starts is
/// a property of the build's memory layout, not of the program — code
/// generation places it at the top of the shadow stack, which is aligned to 16
/// bytes, so every relative alignment here is also an absolute one.
///
/// A constant only another constant's initializer names is folded into that
/// constant's value and is not placed; neither is a scalar, which is emitted
/// inline. So the region is exactly the bytes some function reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaticData {
    entries: Vec<StaticDataEntry>,
    size: u64,
}

impl StaticData {
    /// The region's size in bytes: the end of its last constant.
    #[must_use = "returns the size without side effects"]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Whether no constant is placed, so the module needs no data segment.
    #[must_use = "this is a pure check with no side effects"]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every placed constant, in placement (source) order.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn entries(&self) -> &[StaticDataEntry] {
        &self.entries
    }

    /// Where the constant declared by `def_id` is placed, if it is.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn entry(&self, def_id: DefId) -> Option<&StaticDataEntry> {
        self.entries.iter().find(|entry| entry.def_id == def_id)
    }

    /// The region's bytes: every placed constant's value at its offset, zero
    /// everywhere else.
    ///
    /// # Panics
    ///
    /// Panics if the region does not fit the host's address space, which no
    /// region a 32-bit memory can hold reaches.
    #[must_use = "returns the image without side effects"]
    pub fn image(&self, ctx: &TypedContext) -> Vec<u8> {
        let size = usize::try_from(self.size).expect("static data larger than the address space");
        let mut bytes = vec![0u8; size];
        for entry in &self.entries {
            if let Some(value) = ctx.module_const_value(entry.def_id) {
                value.write_bytes(&mut bytes[entry.offset as usize..]);
            }
        }
        bytes
    }
}

/// Lays out the static data region from the evaluated constants. See
/// [`StaticData`].
pub(crate) fn lay_out(ctx: &TypedContext) -> StaticData {
    let mut entries = Vec::new();
    let mut size = 0u64;
    for file in ctx.source_files() {
        for &def_id in &file.defs {
            let def = &ctx.arena()[def_id];
            let Def::Constant { name, .. } = &def.kind else {
                continue;
            };
            let Some(value) = ctx.module_const_value(def_id) else {
                continue;
            };
            if !value.is_compound() || !ctx.is_const_named_by_a_body(def_id) {
                continue;
            }
            let offset = align_up(size, value.alignment());
            let bytes = value.byte_size();
            size = offset + bytes;
            entries.push(StaticDataEntry {
                def_id,
                name: ctx.arena()[*name].name.clone(),
                module_path: file.module_path.clone(),
                location: def.location,
                offset,
                size: bytes,
            });
        }
    }
    StaticData { entries, size }
}

/// Evaluates every module-scope constant's initializer.
///
/// Returns the value of each constant that has one, and a diagnostic for each
/// initializer that is not a constant expression or would trap. A constant
/// whose initializer failed to type-check is skipped without a diagnostic of
/// its own: the type error already explains it, and a second message about the
/// same expression would only repeat it.
pub(crate) fn evaluate(ctx: &TypedContext) -> (FxHashMap<DefId, ConstValue>, Vec<LabeledError>) {
    let mut evaluator = Evaluator {
        ctx,
        files: FxHashMap::default(),
        results: FxHashMap::default(),
        in_progress: FxHashSet::default(),
        errors: Vec::new(),
    };
    for file in ctx.source_files() {
        for &def_id in &file.defs {
            if matches!(ctx.arena()[def_id].kind, Def::Constant { .. }) {
                evaluator.files.insert(def_id, file.module_path.clone());
            }
        }
    }
    let def_ids: Vec<DefId> = ctx
        .source_files()
        .flat_map(|file| file.defs.iter().copied())
        .filter(|def_id| evaluator.files.contains_key(def_id))
        .collect();
    for def_id in def_ids {
        evaluator.constant(def_id);
    }
    let values = evaluator
        .results
        .into_iter()
        .filter_map(|(def_id, value)| value.map(|value| (def_id, value)))
        .collect();
    (values, evaluator.errors)
}

/// A type error with the file label of the file it belongs to: the
/// `::`-joined module path, or `None` for the entry file.
type LabeledError = (Option<String>, TypeCheckError);

/// Why evaluation of one initializer stopped. The diagnostic, when there is
/// one, has already been recorded; the caller only unwinds.
struct Stop;

/// Per-initializer state: the constant being computed and the arithmetic mode
/// the operators at the current position have.
#[derive(Clone)]
struct Frame<'n> {
    name: &'n str,
    file_label: Option<String>,
    mode: ArithMode,
}

struct Evaluator<'a> {
    ctx: &'a TypedContext,
    /// The defining file of every module constant.
    files: FxHashMap<DefId, Vec<String>>,
    /// Every constant computed so far; `None` for one that has no value.
    results: FxHashMap<DefId, Option<ConstValue>>,
    /// Constants whose initializer is being computed, so a reference back into
    /// one stops instead of recursing. The type checker has already reported
    /// the value cycle that makes such a reference possible.
    in_progress: FxHashSet<DefId>,
    errors: Vec<LabeledError>,
}

impl Evaluator<'_> {
    /// The value of the module constant `def_id`, computing it on first use.
    fn constant(&mut self, def_id: DefId) -> Option<ConstValue> {
        if let Some(result) = self.results.get(&def_id) {
            return result.clone();
        }
        let module_path = self.files.get(&def_id)?.clone();
        if !self.in_progress.insert(def_id) {
            return None;
        }
        let Def::Constant { name, value, .. } = &self.ctx.arena()[def_id].kind else {
            return None;
        };
        let name = self.ctx.arena()[*name].name.clone();
        let frame = Frame {
            name: &name,
            file_label: (!module_path.is_empty()).then(|| module_path.join("::")),
            mode: ArithMode::DEFAULT,
        };
        let result = self.expr(*value, &frame).ok();
        self.in_progress.remove(&def_id);
        self.results.insert(def_id, result.clone());
        result
    }

    fn refuse(&mut self, frame: &Frame<'_>, construct: &'static str, expr: ExprId) -> Stop {
        self.errors.push((
            frame.file_label.clone(),
            TypeCheckError::NonConstantInitializer {
                name: frame.name.to_string(),
                construct,
                location: self.ctx.arena()[expr].location,
            },
        ));
        Stop
    }

    fn fail(&mut self, frame: &Frame<'_>, reason: ConstEvalFailure, expr: ExprId) -> Stop {
        self.errors.push((
            frame.file_label.clone(),
            TypeCheckError::ConstEvaluationFailed {
                name: frame.name.to_string(),
                reason,
                location: self.ctx.arena()[expr].location,
            },
        ));
        Stop
    }

    /// The type recorded on an expression, or a silent stop when inference
    /// could not type it (its own diagnostic explains why).
    fn kind_of(&self, expr: ExprId) -> Result<TypeInfoKind, Stop> {
        self.ctx
            .get_node_typeinfo(NodeId::Expr(expr))
            .map(|info| info.kind)
            .ok_or(Stop)
    }

    fn number_of(&self, expr: ExprId) -> Result<NumberType, Stop> {
        match self.kind_of(expr)? {
            TypeInfoKind::Number(number) => Ok(number),
            _ => Err(Stop),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn expr(&mut self, expr: ExprId, frame: &Frame<'_>) -> Result<ConstValue, Stop> {
        // A module constant read by name, bare or qualified, is its value. This
        // is asked first because a qualified constant is a `TypeMemberAccess`,
        // the same shape as an enum variant.
        if let Some(def_id) = self.ctx.module_const_ref(expr) {
            return self.constant(def_id).ok_or(Stop);
        }
        match &self.ctx.arena()[expr].kind {
            Expr::Parenthesized { expr: inner } => self.expr(*inner, frame),
            Expr::ArithMode { mode, expr: inner } => {
                let inner_frame = Frame {
                    mode: *mode,
                    ..frame.clone()
                };
                self.expr(*inner, &inner_frame)
            }
            Expr::NumberLiteral { value } => {
                let number = self.number_of(expr)?;
                match value.parse::<i128>() {
                    Ok(parsed) if number.range().contains(&parsed) => Ok(ConstValue::Int {
                        value: parsed,
                        number,
                    }),
                    _ => Err(self.fail(
                        frame,
                        ConstEvalFailure::LiteralOutOfRange {
                            literal: value.clone(),
                            number,
                        },
                        expr,
                    )),
                }
            }
            Expr::BoolLiteral { value } => Ok(ConstValue::Bool(*value)),
            Expr::TypeMemberAccess { name, .. } => {
                let variant = self.ctx.arena()[*name].name.clone();
                let info = match self.kind_of(expr)? {
                    TypeInfoKind::Enum(_, key) => self.ctx.lookup_enum(&key),
                    _ => None,
                };
                let tag = info
                    .and_then(|info| info.variant_index(&variant))
                    .and_then(|index| u32::try_from(index).ok())
                    .ok_or(Stop)?;
                Ok(ConstValue::Enum { tag })
            }
            Expr::PrefixUnary { expr: operand, op } => self.unary(expr, *operand, op, frame),
            Expr::Binary { left, right, op } => self.binary(expr, *left, *right, op, frame),
            Expr::ArrayLiteral { elements } => {
                let elements = elements.clone();
                let values = elements
                    .into_iter()
                    .map(|element| self.expr(element, frame))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ConstValue::Array(values))
            }
            Expr::ArrayRepeat { value, .. } => {
                let TypeInfoKind::Array(_, count) = self.kind_of(expr)? else {
                    return Err(Stop);
                };
                let value = self.expr(*value, frame)?;
                Ok(ConstValue::Repeat {
                    value: Box::new(value),
                    count,
                })
            }
            Expr::StructLiteral { fields, .. } => {
                let TypeInfoKind::Struct(_, key) = self.kind_of(expr)? else {
                    return Err(Stop);
                };
                let info = self.ctx.lookup_struct(&key).ok_or(Stop)?;
                let written = fields.clone();
                let mut values = Vec::with_capacity(info.fields.len());
                for declared in &info.fields {
                    let (_, field_expr) = written
                        .iter()
                        .find(|(ident, _)| self.ctx.arena()[*ident].name == declared.name)
                        .ok_or(Stop)?;
                    values.push((declared.name.clone(), self.expr(*field_expr, frame)?));
                }
                Ok(ConstValue::Struct(values))
            }
            Expr::MemberAccess { expr: base, name } => {
                let field = self.ctx.arena()[*name].name.clone();
                let base = self.expr(*base, frame)?;
                base.field(&field).cloned().ok_or(Stop)
            }
            Expr::ArrayIndexAccess { array, index } => {
                let (array, index_expr) = (*array, *index);
                let base = self.expr(array, frame)?;
                let ConstValue::Int { value: index, .. } = self.expr(index_expr, frame)? else {
                    return Err(Stop);
                };
                let length = base.element_count().ok_or(Stop)?;
                match u32::try_from(index).ok().and_then(|i| base.element(i)) {
                    Some(element) => Ok(element.clone()),
                    None => Err(self.fail(
                        frame,
                        ConstEvalFailure::IndexOutOfBounds { index, length },
                        index_expr,
                    )),
                }
            }
            Expr::FunctionCall { .. } => Err(self.refuse(frame, "a function call", expr)),
            Expr::Uzumaki => Err(self.refuse(frame, "`@`, a non-deterministic value,", expr)),
            // An identifier that names no module constant names nothing a
            // module scope can see, which inference has already reported.
            // Strings and the unit value are refused by their own analysis
            // rules (A048, A049) wherever they appear, constants included, so a
            // second message here would only repeat them.
            Expr::Identifier(_)
            | Expr::StringLiteral { .. }
            | Expr::UnitLiteral
            | Expr::Type(_) => Err(Stop),
        }
    }

    fn unary(
        &mut self,
        expr: ExprId,
        operand: ExprId,
        op: &UnaryOperatorKind,
        frame: &Frame<'_>,
    ) -> Result<ConstValue, Stop> {
        let value = self.expr(operand, frame)?;
        match (op, value) {
            (UnaryOperatorKind::Not, ConstValue::Bool(value)) => Ok(ConstValue::Bool(!value)),
            // Negation at an unsigned type is a type error, reported there.
            (UnaryOperatorKind::Neg, ConstValue::Int { value, number }) if number.is_signed() => {
                self.governed(expr, "-", -value, number, frame)
            }
            (UnaryOperatorKind::BitNot, ConstValue::Int { value, number }) => Ok(ConstValue::Int {
                value: wrap(!value, number),
                number,
            }),
            _ => Err(Stop),
        }
    }

    /// The result of one of the operators an arithmetic mode governs: the exact
    /// value when it fits, its wrap under `wrapping`, and a refusal under
    /// `checked`, where the program would trap.
    fn governed(
        &mut self,
        expr: ExprId,
        op: &'static str,
        exact: i128,
        number: NumberType,
        frame: &Frame<'_>,
    ) -> Result<ConstValue, Stop> {
        if number.range().contains(&exact) {
            return Ok(ConstValue::Int {
                value: exact,
                number,
            });
        }
        match frame.mode {
            ArithMode::Wrapping => Ok(ConstValue::Int {
                value: wrap(exact, number),
                number,
            }),
            ArithMode::Checked => Err(self.fail(
                frame,
                ConstEvalFailure::Overflow { op, exact, number },
                expr,
            )),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn binary(
        &mut self,
        expr: ExprId,
        left: ExprId,
        right: ExprId,
        op: &OperatorKind,
        frame: &Frame<'_>,
    ) -> Result<ConstValue, Stop> {
        // `&&` and `||` evaluate their right operand only when the left one does
        // not decide the result, as the emitted code does, so a right operand
        // that would trap is never reached when it is not needed.
        if matches!(op, OperatorKind::And | OperatorKind::Or) {
            let ConstValue::Bool(lhs) = self.expr(left, frame)? else {
                return Err(Stop);
            };
            if lhs == matches!(op, OperatorKind::Or) {
                return Ok(ConstValue::Bool(lhs));
            }
            return match self.expr(right, frame)? {
                ConstValue::Bool(rhs) => Ok(ConstValue::Bool(rhs)),
                _ => Err(Stop),
            };
        }
        let lhs = self.expr(left, frame)?;
        let rhs = self.expr(right, frame)?;
        let comparison = |ordering: std::cmp::Ordering| match op {
            OperatorKind::Eq => Some(ordering.is_eq()),
            OperatorKind::Ne => Some(ordering.is_ne()),
            OperatorKind::Lt => Some(ordering.is_lt()),
            OperatorKind::Le => Some(ordering.is_le()),
            OperatorKind::Gt => Some(ordering.is_gt()),
            OperatorKind::Ge => Some(ordering.is_ge()),
            _ => None,
        };
        let (a, b, number) = match (lhs, rhs) {
            (ConstValue::Int { value: a, number }, ConstValue::Int { value: b, .. }) => {
                (a, b, number)
            }
            (ConstValue::Bool(a), ConstValue::Bool(b)) => {
                return comparison(a.cmp(&b))
                    .map(ConstValue::Bool)
                    .or(match op {
                        OperatorKind::BitAnd => Some(ConstValue::Bool(a & b)),
                        OperatorKind::BitOr => Some(ConstValue::Bool(a | b)),
                        OperatorKind::BitXor => Some(ConstValue::Bool(a ^ b)),
                        _ => None,
                    })
                    .ok_or(Stop);
            }
            (ConstValue::Enum { tag: a }, ConstValue::Enum { tag: b }) => {
                return comparison(a.cmp(&b)).map(ConstValue::Bool).ok_or(Stop);
            }
            _ => return Err(Stop),
        };
        if let Some(result) = comparison(a.cmp(&b)) {
            return Ok(ConstValue::Bool(result));
        }
        let int = |value: i128| ConstValue::Int { value, number };
        match op {
            OperatorKind::Add => self.governed(expr, "+", a + b, number, frame),
            OperatorKind::Sub => self.governed(expr, "-", a - b, number, frame),
            OperatorKind::Mul => self.governed(expr, "*", a * b, number, frame),
            OperatorKind::Div | OperatorKind::Mod => {
                let symbol = if matches!(op, OperatorKind::Div) {
                    "/"
                } else {
                    "%"
                };
                if b == 0 {
                    // A literal zero divisor is already a type error.
                    if matches!(&self.ctx.arena()[right].kind, Expr::NumberLiteral { .. }) {
                        return Err(Stop);
                    }
                    return Err(self.fail(
                        frame,
                        ConstEvalFailure::DivisionByZero { op: symbol },
                        expr,
                    ));
                }
                if matches!(op, OperatorKind::Mod) {
                    // Rust's `%` truncates toward zero, as `rem_s` does, and
                    // `MIN % -1` is 0 at every width.
                    return Ok(int(a % b));
                }
                let quotient = a / b;
                if number.range().contains(&quotient) {
                    Ok(int(quotient))
                } else {
                    Err(self.fail(frame, ConstEvalFailure::DivisionOverflow { number }, expr))
                }
            }
            // Two in-range operands give an in-range result for each of these at
            // either signedness, so no wrap is needed.
            OperatorKind::BitAnd => Ok(int(a & b)),
            OperatorKind::BitOr => Ok(int(a | b)),
            OperatorKind::BitXor => Ok(int(a ^ b)),
            OperatorKind::Shl | OperatorKind::Shr => {
                let symbol = if matches!(op, OperatorKind::Shl) {
                    "<<"
                } else {
                    ">>"
                };
                let bits = number_bits(number);
                let Some(count) = u32::try_from(b).ok().filter(|count| *count < bits) else {
                    return Err(self.fail(
                        frame,
                        ConstEvalFailure::ShiftCountOutOfRange {
                            op: symbol,
                            count: b,
                            number,
                        },
                        right,
                    ));
                };
                if matches!(op, OperatorKind::Shl) {
                    Ok(int(wrap(a << count, number)))
                } else {
                    // An in-range signed value shifts arithmetically and an
                    // unsigned one, being non-negative, logically: both are
                    // `i128`'s `>>` on the exact value.
                    Ok(int(a >> count))
                }
            }
            // `**` is refused by the type checker wherever it appears.
            _ => Err(Stop),
        }
    }
}

/// `value` reduced to `number`'s range by two's-complement wrapping, as the
/// machine instruction leaves it.
fn wrap(value: i128, number: NumberType) -> i128 {
    let bits = number_bits(number);
    let modulus = 1i128 << bits;
    let reduced = value.rem_euclid(modulus);
    if number.is_signed() && reduced >= modulus / 2 {
        reduced - modulus
    } else {
        reduced
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_reduces_into_every_width() {
        assert_eq!(wrap(128, NumberType::I8), -128);
        assert_eq!(wrap(-129, NumberType::I8), 127);
        assert_eq!(wrap(256, NumberType::U8), 0);
        assert_eq!(wrap(-1, NumberType::U32), i128::from(u32::MAX));
        assert_eq!(
            wrap(i128::from(i64::MAX) + 1, NumberType::I64),
            i128::from(i64::MIN)
        );
        assert_eq!(wrap(-1, NumberType::U64), i128::from(u64::MAX));
    }

    #[test]
    fn struct_layout_pads_each_field_to_its_alignment() {
        // { a: u8, b: i32, c: u16 } lays out as a@0, b@4, c@8, size 12 (rounded
        // to the struct's 4-byte alignment), as codegen's frame layout does.
        let value = ConstValue::Struct(vec![
            (
                "a".into(),
                ConstValue::Int {
                    value: 1,
                    number: NumberType::U8,
                },
            ),
            (
                "b".into(),
                ConstValue::Int {
                    value: -2,
                    number: NumberType::I32,
                },
            ),
            (
                "c".into(),
                ConstValue::Int {
                    value: 3,
                    number: NumberType::U16,
                },
            ),
        ]);
        assert_eq!(value.byte_size(), 12);
        assert_eq!(value.alignment(), 4);
        let mut bytes = vec![0u8; 12];
        value.write_bytes(&mut bytes);
        assert_eq!(bytes, [1, 0, 0, 0, 0xfe, 0xff, 0xff, 0xff, 3, 0, 0, 0]);
    }

    #[test]
    fn a_repeated_array_serializes_every_element() {
        let value = ConstValue::Repeat {
            value: Box::new(ConstValue::Int {
                value: 7,
                number: NumberType::I16,
            }),
            count: 3,
        };
        assert_eq!(value.byte_size(), 6);
        assert_eq!(value.element_count(), Some(3));
        assert_eq!(
            value.element(2),
            Some(&ConstValue::Int {
                value: 7,
                number: NumberType::I16
            })
        );
        assert_eq!(value.element(3), None);
        let mut bytes = vec![0u8; 6];
        value.write_bytes(&mut bytes);
        assert_eq!(bytes, [7, 0, 7, 0, 7, 0]);
    }

    #[test]
    fn an_array_of_structs_strides_by_the_struct_size() {
        let element = ConstValue::Struct(vec![
            (
                "x".into(),
                ConstValue::Int {
                    value: 1,
                    number: NumberType::I64,
                },
            ),
            ("ok".into(), ConstValue::Bool(true)),
        ]);
        let value = ConstValue::Array(vec![element.clone(), element]);
        // Each element is 9 bytes of fields rounded up to 16 by the i64.
        assert_eq!(value.byte_size(), 32);
        assert_eq!(value.alignment(), 8);
        let mut bytes = vec![0u8; 32];
        value.write_bytes(&mut bytes);
        assert_eq!(bytes[0], 1);
        assert_eq!(bytes[8], 1);
        assert_eq!(bytes[16], 1);
        assert_eq!(bytes[24], 1);
    }
}
