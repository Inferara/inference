//! The overflow-guard catalogue: which source-level arithmetic operators trap
//! on a result their type cannot hold, and the instruction sequence each one
//! gets.
//!
//! WebAssembly's `add`, `sub` and `mul` wrap silently at 32 and 64 bits, and
//! there is no wider width to compute a candidate result in — an `i64` product
//! has nowhere to be checked. So a guard cannot ask the machine whether the
//! operation overflowed; it has to recompute the question from values it keeps
//! itself, which is why every row here saves its operands or its result in a
//! scratch local before testing them.
//!
//! Two things follow from that shape and are load-bearing everywhere below.
//!
//! **Every trap is `unreachable`.** The obvious multiply check, `r / b == a`,
//! reaches WebAssembly's own `integer overflow` trap at `(MIN, -1)` — where the
//! wrapped product is `MIN` and the divisor is `-1` — which is a
//! different trap kind reported to the host and a role the Rocq contract's
//! `BI_unreachable` table does not enumerate. The signed multiply row therefore
//! carries an explicit `b == -1` arm that traps before any division runs, and
//! the execution matrix asserts the trap *kind*, not merely that a trap
//! happened.
//!
//! **The narrow widths are cheap.** i8, i16, u8 and u16 compute in a promoted
//! i32 and are re-narrowed after every operation, so the un-narrowed result is
//! available and overflow is exactly "re-narrowing changes it". Those rows are
//! post-checks that run the re-narrowing themselves and leave its value behind,
//! which is why the lowering sites skip the re-narrowing they would otherwise
//! emit next.
//!
//! [`guard_kind`] is the single source of truth for whether a row applies: both
//! the emission sites and the pre-body scratch reservation ask it, so the two
//! cannot disagree about which operators are guarded.

pub(crate) use inference_ast::nodes::GuardedOp;
use inference_ast::nodes::ArithMode;
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use wasm_encoder::{BlockType as WasmBlockType, Function, Instruction, ValType};

use crate::memory;

/// The machine width a guard's arithmetic runs at.
///
/// The narrow source widths do not appear: they promote to i32 and are guarded
/// by [`GuardKind::NarrowFit`] instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GuardWidth {
    I32,
    I64,
}

/// The narrow width a promoted i32 result is checked against.
///
/// Carries the source type rather than a shift amount or a mask, so the row asks
/// [`memory::emit_sub_i32_narrowing`] for the re-narrowing that type's
/// arithmetic already ends with instead of keeping a second copy of those
/// shapes. Constructed only from the four narrow types.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct NarrowFit(NumberType);

/// One row of the catalogue: an operator at a width, with the check that
/// decides whether its result fits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum GuardKind {
    /// `a + b` signed: overflow iff `(r <s a) != (b <s 0)`.
    SignedAdd(GuardWidth),
    /// `a - b` signed: overflow iff `(r <s a) != (b >s 0)`.
    SignedSub(GuardWidth),
    /// `a * b` signed, checked by dividing the product back by `b`.
    SignedMul(GuardWidth),
    /// `-a` signed: overflow iff the result is the width's minimum, the one
    /// value whose negation is not representable.
    SignedNeg(GuardWidth),
    /// `a + b` unsigned: overflow iff `r <u b`, the sum below one operand.
    UnsignedAdd(GuardWidth),
    /// `a - b` unsigned: overflow iff `a <u b`, tested before subtracting.
    UnsignedSub(GuardWidth),
    /// `a * b` unsigned, checked by dividing the product back by `b`.
    UnsignedMul(GuardWidth),
    /// A promoted-i32 operator at a narrow width: overflow iff re-narrowing the
    /// promoted result changes it.
    NarrowFit(NarrowFit),
}

/// The guard `op` needs at `kind` under `mode`, or `None` when it needs none.
///
/// This is the whole definition of "this operator is guarded". Emission calls it
/// to decide what to emit and the pre-body scan calls it to decide what scratch
/// to reserve, so a body cannot reach a guard whose scratch was never reserved
/// and cannot reserve scratch for a guard it never emits.
///
/// `None` covers three separate cases, all of which lower exactly as they do
/// today: an operator whose effective mode is wrapping, a type that is not a
/// number (the type checker admits none, but a guard is not the place to
/// discover that), and an operator the type does not have — unsigned negation,
/// which the type checker rejects, so no unsigned row exists. That last question
/// is `NumberType::has_operator`, the one the analysis rules ask as well.
#[must_use = "the classification is the answer; this emits nothing on its own"]
pub(crate) fn guard_kind(
    op: GuardedOp,
    kind: &TypeInfoKind,
    mode: ArithMode,
) -> Option<GuardKind> {
    if mode != ArithMode::Checked {
        return None;
    }
    let TypeInfoKind::Number(number) = kind else {
        return None;
    };
    // One match answers every width, so none can be left to a default arm. The
    // narrow widths share one row per type across every operator — their
    // promoted i32 arithmetic cannot itself overflow, so "does the result fit"
    // is the only question any of them asks — and the full widths carry their
    // machine width and signedness into the per-operator table below.
    // Unsigned negation has no row at any width: the type checker refuses `-x`
    // at an unsigned type, so the operator never reaches lowering. The same
    // question decides what the rules call a governed operator.
    if !number.has_operator(op) {
        return None;
    }
    let (width, unsigned) = match number {
        NumberType::I8 | NumberType::I16 | NumberType::U8 | NumberType::U16 => {
            return Some(GuardKind::NarrowFit(NarrowFit(*number)));
        }
        NumberType::I32 => (GuardWidth::I32, false),
        NumberType::U32 => (GuardWidth::I32, true),
        NumberType::I64 => (GuardWidth::I64, false),
        NumberType::U64 => (GuardWidth::I64, true),
    };
    Some(match (op, unsigned) {
        (GuardedOp::Add, false) => GuardKind::SignedAdd(width),
        (GuardedOp::Add, true) => GuardKind::UnsignedAdd(width),
        (GuardedOp::Sub, false) => GuardKind::SignedSub(width),
        (GuardedOp::Sub, true) => GuardKind::UnsignedSub(width),
        (GuardedOp::Mul, false) => GuardKind::SignedMul(width),
        (GuardedOp::Mul, true) => GuardKind::UnsignedMul(width),
        (GuardedOp::Neg, false) => GuardKind::SignedNeg(width),
        // Refused by `has_operator` above; the arm keeps the match total.
        (GuardedOp::Neg, true) => return None,
    })
}

impl GuardKind {
    /// Whether the guard emits its operator's own instruction itself.
    ///
    /// The full-width binary rows do: their test reads operands the operation
    /// consumes, so the guard has to save them before operating. The negation
    /// and narrow rows are post-checks on a value already on the stack.
    #[must_use = "the answer decides whether the caller emits the operation itself"]
    pub(crate) fn replaces_the_operation(self) -> bool {
        match self {
            GuardKind::SignedAdd(_)
            | GuardKind::SignedSub(_)
            | GuardKind::SignedMul(_)
            | GuardKind::UnsignedAdd(_)
            | GuardKind::UnsignedSub(_)
            | GuardKind::UnsignedMul(_) => true,
            GuardKind::SignedNeg(_) | GuardKind::NarrowFit(_) => false,
        }
    }

    /// Whether the guard leaves its type's re-narrowed result on the stack.
    ///
    /// The narrow rows do, and it is not incidental: the value they compare the
    /// promoted result against *is* the re-narrowing, so they end by leaving it.
    /// The re-narrowing a lowering site emits after an unguarded narrow
    /// operation would therefore run a second time over its own output — same
    /// value, more bytes — so the sites ask this and skip it.
    #[must_use = "the answer decides whether the caller re-narrows again"]
    pub(crate) fn narrows_the_result(self) -> bool {
        matches!(self, GuardKind::NarrowFit(_))
    }

    /// The width class whose scratch slots this guard uses.
    ///
    /// The narrow rows answer i32 because their arithmetic is the promoted i32
    /// arithmetic and their scratch holds promoted values.
    #[must_use = "the width selects which half of the pool the guard reads"]
    pub(crate) fn scratch_width(self) -> GuardWidth {
        match self {
            GuardKind::SignedAdd(width)
            | GuardKind::SignedSub(width)
            | GuardKind::SignedMul(width)
            | GuardKind::SignedNeg(width)
            | GuardKind::UnsignedAdd(width)
            | GuardKind::UnsignedSub(width)
            | GuardKind::UnsignedMul(width) => width,
            GuardKind::NarrowFit(_) => GuardWidth::I32,
        }
    }
}

/// The most scratch locals of one width class any single guard needs.
///
/// Every class present in a body reserves this many rather than the maximum
/// over the rows that body actually contains. One number cannot disagree with
/// the emitter; a per-row table could, and a reservation one slot short would
/// hand a guard a local index belonging to something else.
pub(crate) const SCRATCH_SLOTS_PER_WIDTH: u32 = 3;

/// The scratch locals one guard writes.
///
/// Named for the roles the catalogue's sequences use: `a` and `b` hold the
/// operands a full-width binary row has to re-read after its operation consumed
/// them, and `r` holds the result. The narrow rows use `a` for the promoted
/// result and `b` for its re-narrowing, so `r` goes untouched there.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GuardScratch {
    a: u32,
    b: u32,
    r: u32,
}

impl GuardScratch {
    /// The three consecutive slots starting at `base`.
    #[must_use = "constructs the slot triple a guard writes"]
    pub(crate) fn at(base: u32) -> Self {
        GuardScratch {
            a: base,
            b: base + 1,
            r: base + 2,
        }
    }
}

/// The width classes a function body's guards need scratch for.
///
/// Accumulated by scanning the body with [`guard_kind`] — the same classifier
/// the emission sites ask — so a body demands scratch for exactly the classes it
/// will emit guards at. A body with no effectively-checked arithmetic demands
/// nothing, which is what keeps a program that writes no annotation
/// byte-identical to one compiled before guards existed.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct GuardScratchDemand {
    i32_class: bool,
    i64_class: bool,
}

impl GuardScratchDemand {
    /// Records that a guard of `kind` will be emitted.
    pub(crate) fn add(&mut self, kind: GuardKind) {
        match kind.scratch_width() {
            GuardWidth::I32 => self.i32_class = true,
            GuardWidth::I64 => self.i64_class = true,
        }
    }
}

/// The scratch locals a function reserves for its overflow guards.
///
/// One pool serves every guard in the function, which is safe because guard live
/// ranges strictly nest and close: a guard touches its slots only after both of
/// its operands are already on the operand stack, so an operand expression's own
/// guard has finished with the pool before the enclosing guard's first
/// `local.tee` runs. That relaxes the single-owner rule the narrow division
/// guard's dedicated scratch keeps — which stays as it is, since sharing it
/// would move committed output for no gain — and the same-kind nested cases in
/// the guard fixture are what hold the argument to its word.
///
/// The pool is variable-length: zero, one or both width classes, three slots
/// each. Its slots sit after every other reservation a function makes, so
/// adding them moves no index any other obligation already depends on.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct GuardScratchPool {
    i32_base: Option<u32>,
    i64_base: Option<u32>,
}

impl GuardScratchPool {
    /// Lays the demanded slots out consecutively from `base`.
    #[must_use = "constructs the pool the function's guards read"]
    pub(crate) fn reserve(demand: GuardScratchDemand, base: u32) -> Self {
        let i32_base = demand.i32_class.then_some(base);
        let i64_base = demand
            .i64_class
            .then_some(base + u32::from(demand.i32_class) * SCRATCH_SLOTS_PER_WIDTH);
        GuardScratchPool { i32_base, i64_base }
    }

    /// The number of locals the pool declares — the amount every index after it
    /// shifts by.
    #[must_use = "the length is what every later local index shifts by"]
    pub(crate) fn len(self) -> u32 {
        (u32::from(self.i32_base.is_some()) + u32::from(self.i64_base.is_some()))
            * SCRATCH_SLOTS_PER_WIDTH
    }

    /// The pool's local declarations, in the order its indices run.
    ///
    /// One entry per width class, not one per slot: a WASM locals vector entry
    /// is a `(count, type)` pair, so the class's slots are the one run that pair
    /// describes, and spelling them separately costs bytes in every guarded
    /// function without changing a single local index.
    #[must_use = "the declarations must be appended to the function's locals vector"]
    pub(crate) fn declarations(self) -> Vec<(u32, ValType)> {
        let mut declarations = Vec::new();
        for (base, valtype) in [(self.i32_base, ValType::I32), (self.i64_base, ValType::I64)] {
            if base.is_some() {
                declarations.push((SCRATCH_SLOTS_PER_WIDTH, valtype));
            }
        }
        declarations
    }

    /// The slots a guard of `kind` writes, or `None` when the reservation did
    /// not see this guard's width class coming.
    #[must_use = "the slots are the guard's only writable state"]
    pub(crate) fn scratch(self, kind: GuardKind) -> Option<GuardScratch> {
        let base = match kind.scratch_width() {
            GuardWidth::I32 => self.i32_base,
            GuardWidth::I64 => self.i64_base,
        }?;
        Some(GuardScratch::at(base))
    }
}

/// Emits the arithmetic for one effectively-checked operator, trapping via
/// `unreachable` when the result does not fit its type.
///
/// The stack contract differs between the two row families, because what a row
/// can check depends on what its operation left behind.
///
/// - **The full-width binary rows** consume `[a, b]` and leave `[r]`. The guard
///   *replaces* the bare `T.add`/`T.sub`/`T.mul`, emitting it itself between
///   saving the operands and testing the result: the test needs operands the
///   operation consumes.
/// - **The negation and narrow rows** consume `[p]` and leave the value the
///   operator produced. Their operator's own instruction is already emitted, so
///   the guard is a pure post-check. A narrow row leaves the *re-narrowed*
///   value, because that is the value its own test computes, so the lowering
///   site that would re-narrow next skips doing so
///   ([`GuardKind::narrows_the_result`]).
///
/// `depth` is the enclosing block depth, bumped around every `if` this emits so
/// that a `break` in surrounding code still counts its levels correctly.
///
/// The narrow rows are exact only on canonical operands — an i8 holding a value
/// outside `-128..=127` would make re-narrowing lie about whether the operation
/// overflowed. That is the same invariant the narrow division guard already
/// relies on and the same one the language maintains everywhere: exported
/// parameters are normalized on entry, every narrow-producing instruction is
/// re-narrowed, and narrow loads sign- or zero-extend.
pub(crate) fn emit(func: &mut Function, depth: &mut u32, kind: GuardKind, scratch: GuardScratch) {
    match kind {
        GuardKind::SignedAdd(width) => {
            cov_mark::hit!(wasm_codegen_overflow_guard_add);
            emit_signed_addsub(func, depth, width, scratch, SignedAddSub::Add);
        }
        GuardKind::SignedSub(width) => {
            cov_mark::hit!(wasm_codegen_overflow_guard_sub);
            emit_signed_addsub(func, depth, width, scratch, SignedAddSub::Sub);
        }
        GuardKind::UnsignedAdd(width) => {
            cov_mark::hit!(wasm_codegen_overflow_guard_add);
            emit_unsigned_add(func, depth, width, scratch);
        }
        GuardKind::UnsignedSub(width) => {
            cov_mark::hit!(wasm_codegen_overflow_guard_sub);
            emit_unsigned_sub(func, depth, width, scratch);
        }
        GuardKind::SignedMul(width) => {
            cov_mark::hit!(wasm_codegen_overflow_guard_mul);
            emit_mul(func, depth, width, scratch, true);
        }
        GuardKind::UnsignedMul(width) => {
            cov_mark::hit!(wasm_codegen_overflow_guard_mul);
            emit_mul(func, depth, width, scratch, false);
        }
        GuardKind::SignedNeg(width) => {
            cov_mark::hit!(wasm_codegen_overflow_guard_neg);
            emit_neg(func, depth, width, scratch);
        }
        GuardKind::NarrowFit(fit) => {
            cov_mark::hit!(wasm_codegen_overflow_guard_narrow);
            emit_narrow_fit(func, depth, fit, scratch);
        }
    }
}

/// Which of the two signed rows that share a sequence is being emitted.
#[derive(Clone, Copy)]
enum SignedAddSub {
    Add,
    Sub,
}

/// `[a, b] -> [r]`, trapping when the signed sum or difference does not fit.
///
/// Both operations overflow exactly when the result lands on the wrong side of
/// the left operand: a sum is below `a` iff `b` was negative, and a difference
/// is below `a` iff `b` was positive. The guard computes both sides as 0/1
/// values and traps when they disagree.
fn emit_signed_addsub(
    func: &mut Function,
    depth: &mut u32,
    width: GuardWidth,
    scratch: GuardScratch,
    which: SignedAddSub,
) {
    func.instruction(&Instruction::LocalSet(scratch.b));
    func.instruction(&Instruction::LocalTee(scratch.a));
    func.instruction(&Instruction::LocalGet(scratch.b));
    func.instruction(&match (which, width) {
        (SignedAddSub::Add, GuardWidth::I32) => Instruction::I32Add,
        (SignedAddSub::Add, GuardWidth::I64) => Instruction::I64Add,
        (SignedAddSub::Sub, GuardWidth::I32) => Instruction::I32Sub,
        (SignedAddSub::Sub, GuardWidth::I64) => Instruction::I64Sub,
    });
    func.instruction(&Instruction::LocalTee(scratch.r));
    func.instruction(&Instruction::LocalGet(scratch.a));
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32LtS,
        GuardWidth::I64 => Instruction::I64LtS,
    });
    func.instruction(&Instruction::LocalGet(scratch.b));
    emit_zero(func, width);
    func.instruction(&match (which, width) {
        (SignedAddSub::Add, GuardWidth::I32) => Instruction::I32LtS,
        (SignedAddSub::Add, GuardWidth::I64) => Instruction::I64LtS,
        (SignedAddSub::Sub, GuardWidth::I32) => Instruction::I32GtS,
        (SignedAddSub::Sub, GuardWidth::I64) => Instruction::I64GtS,
    });
    // Both comparisons produced canonical 0/1 i32 values, so inequality of the
    // two booleans is inequality of the two words.
    func.instruction(&Instruction::I32Ne);
    emit_trap_if(func, depth);
    func.instruction(&Instruction::LocalGet(scratch.r));
}

/// `[a, b] -> [r]`, trapping when the unsigned sum does not fit.
///
/// A wrapped unsigned sum is below both of its operands, so testing against the
/// operand already on top of the stack costs one scratch fewer than testing
/// against the other one.
fn emit_unsigned_add(
    func: &mut Function,
    depth: &mut u32,
    width: GuardWidth,
    scratch: GuardScratch,
) {
    func.instruction(&Instruction::LocalTee(scratch.b));
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32Add,
        GuardWidth::I64 => Instruction::I64Add,
    });
    func.instruction(&Instruction::LocalTee(scratch.r));
    func.instruction(&Instruction::LocalGet(scratch.b));
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32LtU,
        GuardWidth::I64 => Instruction::I64LtU,
    });
    emit_trap_if(func, depth);
    func.instruction(&Instruction::LocalGet(scratch.r));
}

/// `[a, b] -> [a - b]`, trapping when the difference would be negative.
///
/// Unsigned subtraction is the one row that tests before operating: `a <u b` is
/// the whole condition and needs neither the result nor a second scratch read.
fn emit_unsigned_sub(
    func: &mut Function,
    depth: &mut u32,
    width: GuardWidth,
    scratch: GuardScratch,
) {
    func.instruction(&Instruction::LocalSet(scratch.b));
    func.instruction(&Instruction::LocalTee(scratch.a));
    func.instruction(&Instruction::LocalGet(scratch.b));
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32LtU,
        GuardWidth::I64 => Instruction::I64LtU,
    });
    emit_trap_if(func, depth);
    func.instruction(&Instruction::LocalGet(scratch.a));
    func.instruction(&Instruction::LocalGet(scratch.b));
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32Sub,
        GuardWidth::I64 => Instruction::I64Sub,
    });
}

/// `[a, b] -> [r]`, trapping when the product does not fit.
///
/// There is no wider width to compute the true product in, so the check divides
/// the wrapped product back by `b`: a product that fits divides back to `a`, and
/// a product that wrapped cannot, because the wrap moved it by a multiple of
/// 2^N and 2^N exceeds anything `b` could carry back.
///
/// `b == 0` is excluded first, since a product by zero always fits and dividing
/// by zero traps with the wrong trap kind. The signed row then excludes
/// `b == -1` before dividing, for two reasons at once: `MIN / -1` is the one
/// division WebAssembly traps on natively, and `(MIN, -1)` is exactly the
/// overflowing pair, which the arm reports as `unreachable` like every other
/// guard.
fn emit_mul(
    func: &mut Function,
    depth: &mut u32,
    width: GuardWidth,
    scratch: GuardScratch,
    signed: bool,
) {
    func.instruction(&Instruction::LocalSet(scratch.b));
    func.instruction(&Instruction::LocalTee(scratch.a));
    func.instruction(&Instruction::LocalGet(scratch.b));
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32Mul,
        GuardWidth::I64 => Instruction::I64Mul,
    });
    func.instruction(&Instruction::LocalSet(scratch.r));

    func.instruction(&Instruction::LocalGet(scratch.b));
    emit_zero(func, width);
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32Ne,
        GuardWidth::I64 => Instruction::I64Ne,
    });
    func.instruction(&Instruction::If(WasmBlockType::Empty));
    *depth += 1;

    if signed {
        func.instruction(&Instruction::LocalGet(scratch.b));
        func.instruction(&match width {
            GuardWidth::I32 => Instruction::I32Const(-1),
            GuardWidth::I64 => Instruction::I64Const(-1),
        });
        func.instruction(&match width {
            GuardWidth::I32 => Instruction::I32Eq,
            GuardWidth::I64 => Instruction::I64Eq,
        });
        func.instruction(&Instruction::If(WasmBlockType::Empty));
        *depth += 1;
        func.instruction(&Instruction::LocalGet(scratch.a));
        emit_min(func, width);
        func.instruction(&match width {
            GuardWidth::I32 => Instruction::I32Eq,
            GuardWidth::I64 => Instruction::I64Eq,
        });
        emit_trap_if(func, depth);
        func.instruction(&Instruction::Else);
        emit_round_trip_check(func, depth, width, scratch, signed);
        *depth -= 1;
        func.instruction(&Instruction::End);
    } else {
        emit_round_trip_check(func, depth, width, scratch, signed);
    }

    *depth -= 1;
    func.instruction(&Instruction::End);
    func.instruction(&Instruction::LocalGet(scratch.r));
}

/// `r / b != a`, trapping when it holds. Emitted where `b` is known to be a
/// divisor the division cannot trap on.
fn emit_round_trip_check(
    func: &mut Function,
    depth: &mut u32,
    width: GuardWidth,
    scratch: GuardScratch,
    signed: bool,
) {
    func.instruction(&Instruction::LocalGet(scratch.r));
    func.instruction(&Instruction::LocalGet(scratch.b));
    func.instruction(&match (width, signed) {
        (GuardWidth::I32, true) => Instruction::I32DivS,
        (GuardWidth::I32, false) => Instruction::I32DivU,
        (GuardWidth::I64, true) => Instruction::I64DivS,
        (GuardWidth::I64, false) => Instruction::I64DivU,
    });
    func.instruction(&Instruction::LocalGet(scratch.a));
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32Ne,
        GuardWidth::I64 => Instruction::I64Ne,
    });
    emit_trap_if(func, depth);
}

/// `[r] -> [r]`, trapping when a negation landed on the width's minimum.
///
/// `-MIN` is the only signed negation without a result, and it is also the only
/// input whose wrapped negation is itself, so testing the result is exhaustive.
fn emit_neg(func: &mut Function, depth: &mut u32, width: GuardWidth, scratch: GuardScratch) {
    func.instruction(&Instruction::LocalTee(scratch.r));
    emit_min(func, width);
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32Eq,
        GuardWidth::I64 => Instruction::I64Eq,
    });
    emit_trap_if(func, depth);
    func.instruction(&Instruction::LocalGet(scratch.r));
}

/// `[p] -> [n]`, trapping when the promoted result does not fit its narrow
/// width, and otherwise leaving that width's re-narrowing of it.
///
/// The re-narrowing is the test: a promoted value fits iff re-narrowing leaves
/// it alone. It is emitted by the same function every narrow operation's own
/// re-narrowing comes from, so there is one copy of those shapes rather than
/// two that could drift, and its result is what the row leaves on the stack —
/// which is why the lowering site does not emit the re-narrowing again.
fn emit_narrow_fit(func: &mut Function, depth: &mut u32, fit: NarrowFit, scratch: GuardScratch) {
    func.instruction(&Instruction::LocalTee(scratch.a));
    let re_narrowed = memory::emit_sub_i32_narrowing(func, &TypeInfoKind::Number(fit.0));
    debug_assert!(
        re_narrowed,
        "a narrow-fit row is built from the four narrow widths alone, each of which re-narrows"
    );
    func.instruction(&Instruction::LocalTee(scratch.b));
    func.instruction(&Instruction::LocalGet(scratch.a));
    func.instruction(&Instruction::I32Ne);
    emit_trap_if(func, depth);
    func.instruction(&Instruction::LocalGet(scratch.b));
}

/// `if <cond on the stack>; unreachable; end`.
fn emit_trap_if(func: &mut Function, depth: &mut u32) {
    func.instruction(&Instruction::If(WasmBlockType::Empty));
    *depth += 1;
    func.instruction(&Instruction::Unreachable);
    *depth -= 1;
    func.instruction(&Instruction::End);
}

/// The zero of `width`.
fn emit_zero(func: &mut Function, width: GuardWidth) {
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32Const(0),
        GuardWidth::I64 => Instruction::I64Const(0),
    });
}

/// The signed minimum of `width`.
fn emit_min(func: &mut Function, width: GuardWidth) {
    func.instruction(&match width {
        GuardWidth::I32 => Instruction::I32Const(i32::MIN),
        GuardWidth::I64 => Instruction::I64Const(i64::MIN),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_ast::nodes::{OperatorKind, UnaryOperatorKind};

    /// Every width and operator, classified. The table is written out rather
    /// than computed so that a change to the classifier has to be restated here
    /// as a deliberate answer.
    #[test]
    fn the_catalogue_classifies_every_width_and_operator() {
        use GuardKind::{
            NarrowFit as Narrow, SignedAdd, SignedMul, SignedNeg, SignedSub, UnsignedAdd,
            UnsignedMul, UnsignedSub,
        };
        use GuardWidth::{I32, I64};
        let i8_fit = NarrowFit(NumberType::I8);
        let i16_fit = NarrowFit(NumberType::I16);
        let u8_fit = NarrowFit(NumberType::U8);
        let u16_fit = NarrowFit(NumberType::U16);
        let expected: &[(NumberType, GuardedOp, Option<GuardKind>)] = &[
            (NumberType::I8, GuardedOp::Add, Some(Narrow(i8_fit))),
            (NumberType::I8, GuardedOp::Sub, Some(Narrow(i8_fit))),
            (NumberType::I8, GuardedOp::Mul, Some(Narrow(i8_fit))),
            (NumberType::I8, GuardedOp::Neg, Some(Narrow(i8_fit))),
            (NumberType::I16, GuardedOp::Add, Some(Narrow(i16_fit))),
            (NumberType::I16, GuardedOp::Sub, Some(Narrow(i16_fit))),
            (NumberType::I16, GuardedOp::Mul, Some(Narrow(i16_fit))),
            (NumberType::I16, GuardedOp::Neg, Some(Narrow(i16_fit))),
            (NumberType::U8, GuardedOp::Add, Some(Narrow(u8_fit))),
            (NumberType::U8, GuardedOp::Sub, Some(Narrow(u8_fit))),
            (NumberType::U8, GuardedOp::Mul, Some(Narrow(u8_fit))),
            (NumberType::U8, GuardedOp::Neg, None),
            (NumberType::U16, GuardedOp::Add, Some(Narrow(u16_fit))),
            (NumberType::U16, GuardedOp::Sub, Some(Narrow(u16_fit))),
            (NumberType::U16, GuardedOp::Mul, Some(Narrow(u16_fit))),
            (NumberType::U16, GuardedOp::Neg, None),
            (NumberType::I32, GuardedOp::Add, Some(SignedAdd(I32))),
            (NumberType::I32, GuardedOp::Sub, Some(SignedSub(I32))),
            (NumberType::I32, GuardedOp::Mul, Some(SignedMul(I32))),
            (NumberType::I32, GuardedOp::Neg, Some(SignedNeg(I32))),
            (NumberType::I64, GuardedOp::Add, Some(SignedAdd(I64))),
            (NumberType::I64, GuardedOp::Sub, Some(SignedSub(I64))),
            (NumberType::I64, GuardedOp::Mul, Some(SignedMul(I64))),
            (NumberType::I64, GuardedOp::Neg, Some(SignedNeg(I64))),
            (NumberType::U32, GuardedOp::Add, Some(UnsignedAdd(I32))),
            (NumberType::U32, GuardedOp::Sub, Some(UnsignedSub(I32))),
            (NumberType::U32, GuardedOp::Mul, Some(UnsignedMul(I32))),
            (NumberType::U32, GuardedOp::Neg, None),
            (NumberType::U64, GuardedOp::Add, Some(UnsignedAdd(I64))),
            (NumberType::U64, GuardedOp::Sub, Some(UnsignedSub(I64))),
            (NumberType::U64, GuardedOp::Mul, Some(UnsignedMul(I64))),
            (NumberType::U64, GuardedOp::Neg, None),
        ];
        assert_eq!(
            expected.len(),
            NumberType::ALL.len() * 4,
            "every width needs a row for each of the four governed operators"
        );
        for &(number, op, want) in expected {
            let kind = TypeInfoKind::Number(number);
            assert_eq!(
                guard_kind(op, &kind, ArithMode::Checked),
                want,
                "{number:?} under {op:?}"
            );
        }
    }

    /// A guard exists exactly where the type has the operator.
    ///
    /// The analysis rule that reports an annotation with nothing to govern and
    /// this classifier both ask `NumberType::has_operator`, so they cannot
    /// disagree about which operators an annotation reaches. What this pins is
    /// that the classifier really is that predicate and nothing narrower: an
    /// annotation called meaningful over arithmetic the emitter leaves bare
    /// would be a rule speaking about a guard the bytes do not carry.
    #[test]
    fn a_guard_exists_exactly_where_the_type_has_the_operator() {
        for &number in NumberType::ALL {
            for op in [
                GuardedOp::Add,
                GuardedOp::Sub,
                GuardedOp::Mul,
                GuardedOp::Neg,
            ] {
                assert_eq!(
                    guard_kind(op, &TypeInfoKind::Number(number), ArithMode::Checked).is_some(),
                    number.has_operator(op),
                    "{number:?} under {op:?}"
                );
            }
        }
    }

    /// Wrapping is the absence of a guard, at every width and operator.
    #[test]
    fn nothing_is_guarded_in_wrapping_mode() {
        for &number in NumberType::ALL {
            for op in [
                GuardedOp::Add,
                GuardedOp::Sub,
                GuardedOp::Mul,
                GuardedOp::Neg,
            ] {
                assert_eq!(
                    guard_kind(op, &TypeInfoKind::Number(number), ArithMode::Wrapping),
                    None,
                    "{number:?} under {op:?} must not be guarded in wrapping mode"
                );
            }
        }
    }

    /// The ungoverned operators have no `GuardedOp` at all, so no annotation can
    /// reach them however the mode is set.
    #[test]
    fn only_the_four_governed_operators_map_to_a_guarded_op() {
        let governed = [
            (OperatorKind::Add, GuardedOp::Add),
            (OperatorKind::Sub, GuardedOp::Sub),
            (OperatorKind::Mul, GuardedOp::Mul),
        ];
        for (op, want) in governed {
            assert_eq!(GuardedOp::from_binary(&op), Some(want));
        }
        for op in [
            OperatorKind::Div,
            OperatorKind::Mod,
            OperatorKind::Pow,
            OperatorKind::And,
            OperatorKind::Or,
            OperatorKind::Eq,
            OperatorKind::Ne,
            OperatorKind::Lt,
            OperatorKind::Le,
            OperatorKind::Gt,
            OperatorKind::Ge,
            OperatorKind::BitAnd,
            OperatorKind::BitOr,
            OperatorKind::BitXor,
            OperatorKind::Shl,
            OperatorKind::Shr,
        ] {
            assert_eq!(GuardedOp::from_binary(&op), None, "{op:?} is not governed");
        }
        assert_eq!(
            GuardedOp::from_unary(&UnaryOperatorKind::Neg),
            Some(GuardedOp::Neg)
        );
        assert_eq!(GuardedOp::from_unary(&UnaryOperatorKind::Not), None);
        assert_eq!(GuardedOp::from_unary(&UnaryOperatorKind::BitNot), None);
    }

    /// A type that is not a number is not guarded, whatever the mode.
    #[test]
    fn a_non_numeric_type_asks_for_no_guard() {
        for kind in [
            TypeInfoKind::Bool,
            TypeInfoKind::Unit,
            TypeInfoKind::String,
        ] {
            assert_eq!(guard_kind(GuardedOp::Add, &kind, ArithMode::Checked), None);
        }
    }

    /// The scratch a row uses is the class its arithmetic runs at, which for the
    /// narrow rows is the promoted i32 rather than their declared width.
    #[test]
    fn every_row_takes_its_scratch_from_the_width_it_computes_at() {
        for &number in NumberType::ALL {
            for op in [
                GuardedOp::Add,
                GuardedOp::Sub,
                GuardedOp::Mul,
                GuardedOp::Neg,
            ] {
                let Some(kind) = guard_kind(op, &TypeInfoKind::Number(number), ArithMode::Checked)
                else {
                    continue;
                };
                let want = match number {
                    NumberType::I64 | NumberType::U64 => GuardWidth::I64,
                    _ => GuardWidth::I32,
                };
                assert_eq!(kind.scratch_width(), want, "{number:?} under {op:?}");
            }
        }
    }

    /// Every `if` a row opens is closed before the row ends.
    ///
    /// The block depth the compiler carries is what a `break` counts levels
    /// against, so a row that bumped it without restoring it would misdirect
    /// every branch emitted after it in the same function. The signed multiply
    /// row nests three of them.
    #[test]
    fn every_row_leaves_the_block_depth_where_it_found_it() {
        for &number in NumberType::ALL {
            for op in [
                GuardedOp::Add,
                GuardedOp::Sub,
                GuardedOp::Mul,
                GuardedOp::Neg,
            ] {
                let Some(kind) = guard_kind(op, &TypeInfoKind::Number(number), ArithMode::Checked)
                else {
                    continue;
                };
                let mut func = Function::new([]);
                let mut depth = 7;
                emit(&mut func, &mut depth, kind, GuardScratch::at(0));
                assert_eq!(depth, 7, "{kind:?} left the block depth unbalanced");
            }
        }
    }

    /// The pool reserves exactly as many slots as a guard has roles for.
    ///
    /// A fourth role added to [`GuardScratch`] without widening the pool would
    /// hand that role a local index belonging to whatever the reservation put
    /// next, which is the one failure mode of this design that no module-level
    /// test can see.
    #[test]
    fn the_pool_is_as_wide_as_one_guard_s_scratch() {
        // Measured off the type rather than off a hand-written slot list: a
        // fourth role added to `GuardScratch` would satisfy a list that still
        // named three and then be handed a local index belonging to whatever
        // the reservation put after the pool.
        assert_eq!(
            size_of::<GuardScratch>(),
            SCRATCH_SLOTS_PER_WIDTH as usize * size_of::<u32>(),
            "every slot of a guard's scratch must be one the pool reserves"
        );
        let GuardScratch { a, b, r } = GuardScratch::at(0);
        assert_eq!(
            [a, b, r].iter().copied().max(),
            Some(SCRATCH_SLOTS_PER_WIDTH - 1),
            "the slots must run from the pool's base to its last reserved index"
        );
    }

    /// A width class declares its slots as one locals-vector entry.
    ///
    /// The count is what the reservation and every later local index agree on,
    /// so a run split across entries would be the same indices at more bytes —
    /// and a run of the wrong length would move every index after the pool.
    #[test]
    fn each_width_class_declares_one_run_of_its_slots() {
        let i32_only = GuardScratchPool::reserve(
            GuardScratchDemand {
                i32_class: true,
                i64_class: false,
            },
            0,
        );
        assert_eq!(
            i32_only.declarations(),
            vec![(SCRATCH_SLOTS_PER_WIDTH, ValType::I32)]
        );
        let both = GuardScratchPool::reserve(
            GuardScratchDemand {
                i32_class: true,
                i64_class: true,
            },
            0,
        );
        assert_eq!(
            both.declarations(),
            vec![
                (SCRATCH_SLOTS_PER_WIDTH, ValType::I32),
                (SCRATCH_SLOTS_PER_WIDTH, ValType::I64),
            ],
            "the i64 half follows the i32 half, in the order the indices run"
        );
        for pool in [i32_only, both] {
            let declared: u32 = pool.declarations().iter().map(|(n, _)| *n).sum();
            assert_eq!(
                declared,
                pool.len(),
                "the declared slot count must be the shift every later index takes"
            );
        }
    }

    /// A guard's scratch reads the class it was reserved for and no other.
    ///
    /// The i64 half of the pool starts where the i32 half ends, so a row that
    /// took its base from the wrong class would silently alias three slots of
    /// the other one.
    #[test]
    fn each_class_reads_its_own_half_of_the_pool() {
        let both = GuardScratchPool::reserve(
            GuardScratchDemand {
                i32_class: true,
                i64_class: true,
            },
            10,
        );
        let narrow = both
            .scratch(GuardKind::NarrowFit(NarrowFit(NumberType::I8)))
            .expect("the i32 class is reserved");
        let wide = both
            .scratch(GuardKind::SignedAdd(GuardWidth::I64))
            .expect("the i64 class is reserved");
        assert_eq!((narrow.a, narrow.b, narrow.r), (10, 11, 12));
        assert_eq!((wide.a, wide.b, wide.r), (13, 14, 15));

        let i64_only = GuardScratchPool::reserve(
            GuardScratchDemand {
                i32_class: false,
                i64_class: true,
            },
            10,
        );
        assert_eq!(
            i64_only
                .scratch(GuardKind::SignedAdd(GuardWidth::I64))
                .map(|s| s.a),
            Some(10),
            "an unreserved class must not push the reserved one along"
        );
        assert!(
            i64_only
                .scratch(GuardKind::NarrowFit(NarrowFit(NumberType::I8)))
                .is_none(),
            "a class the reservation did not see must have no slots to hand out"
        );
    }

    /// Every row's stack contract, validated by the WebAssembly validator.
    ///
    /// `replaces_the_operation` is what the two lowering sites branch on to
    /// decide whether they emit the bare instruction themselves, so it is a
    /// claim about operand arity: a row that answers `true` consumes two
    /// operands and one that answers `false` consumes one. A row whose emission
    /// disagreed with its own answer would leave the operand stack wrong, and no
    /// golden comparison would say so — the bytes would simply be a module no
    /// runtime accepts.
    #[test]
    fn every_row_validates_at_the_operand_arity_it_claims() {
        for &number in NumberType::ALL {
            for op in [
                GuardedOp::Add,
                GuardedOp::Sub,
                GuardedOp::Mul,
                GuardedOp::Neg,
            ] {
                let Some(kind) = guard_kind(op, &TypeInfoKind::Number(number), ArithMode::Checked)
                else {
                    continue;
                };
                let wasm = one_guard_module(kind);
                inf_wasmparser::validate(&wasm).unwrap_or_else(|e| {
                    panic!("{number:?} under {op:?} ({kind:?}) does not validate: {e}")
                });
            }
        }
    }

    /// A module whose single exported function feeds `kind` the operands it
    /// claims to consume and returns the value it leaves.
    ///
    /// The pool is declared at the base the reservation would give it in a
    /// function with no other locals, so the guard writes the slots it was
    /// handed rather than parameters.
    fn one_guard_module(kind: GuardKind) -> Vec<u8> {
        use wasm_encoder::{
            CodeSection, ExportKind, ExportSection, FunctionSection, Module, TypeSection,
        };

        let valtype = match kind.scratch_width() {
            GuardWidth::I32 => ValType::I32,
            GuardWidth::I64 => ValType::I64,
        };
        let arity = if kind.replaces_the_operation() { 2 } else { 1 };
        let params = vec![valtype; arity];

        let mut types = TypeSection::new();
        types.ty().function(params.clone(), [valtype]);
        let mut functions = FunctionSection::new();
        functions.function(0);
        let mut exports = ExportSection::new();
        exports.export("row", ExportKind::Func, 0);

        let base = u32::try_from(arity).expect("at most two parameters");
        let mut func = Function::new([(SCRATCH_SLOTS_PER_WIDTH, valtype)]);
        for slot in 0..base {
            func.instruction(&Instruction::LocalGet(slot));
        }
        let mut depth = 0;
        emit(&mut func, &mut depth, kind, GuardScratch::at(base));
        func.instruction(&Instruction::End);
        let mut code = CodeSection::new();
        code.function(&func);

        let mut module = Module::new();
        module.section(&types);
        module.section(&functions);
        module.section(&exports);
        module.section(&code);
        module.finish()
    }
}
