# inference-target-conformance

Whether a finished `.wasm` artifact is one its target's runtime can load, and
what loading it costs.

Every other crate in this workspace answers a question about a *program*. This
one answers a question about the *bytes*, asked after linking and after every
rewrite, of whichever module is about to be written to disk. It therefore parses
rather than compiles, and its only dependencies are a WebAssembly decoder and an
error derive — which is what lets `infc`, `infs` and the test suite all link it
without dragging code generation along.

## Why stock `wasmparser`

The in-tree `inf-wasmparser` fork decodes and validates this compiler's custom
`0xfc` non-deterministic opcodes with no feature gate. That is what makes it the
right reader everywhere else in the pipeline and the wrong one here: a decoder
that accepts an instruction no standard names cannot testify that a module is
WebAssembly 1.0. The stock decoder can, and it is held at the workspace pin so
the answer does not drift with an unrelated upgrade.

## `check_wasm1`

```rust
pub fn check_wasm1(wasm: &[u8]) -> Result<(), String>
```

Validates at the WebAssembly 1.0 feature set — the MVP plus mutable globals —
and returns the validator's own message, which names the feature and the offset.
Deliberately a *feature* question rather than an instruction scan: a proposal
grows the accepted set in more places than its headline opcodes.

Two targets are held to it. It is asked of whole artifacts and of their parts:
a module linked into a program has to clear the same bar as the program, and a
caller holding the artifacts separately asks it of each one *before* merging
them, where a refusal can still name the file the offending instruction came
from. Asked only of the merged module, the answer is a byte offset into bytes no
file holds.

## `spacewasm::check`

```rust
pub fn check(wasm: &[u8]) -> Result<Report, Violations>
```

SpaceWasm is a `no_std` WebAssembly 1.0 decoder, validator and interpreter for
on-board use. It allocates nothing it did not ask an embedder for, so its
acceptance set is narrower than the standard's in ways the standard does not
describe. `check` answers both questions a build has about that envelope:

- a **verdict** — is this a module the decoder would accept, and could an
  embedder supply what it imports?
- a **measurement** — what must the embedder's two const generics be for it to
  fit? Neither number can be read off the standard, and a module that exceeds
  one fails at load time on hardware.

`check` drives the incremental validation loop rather than
`Validator::validate_all`, because the two per-function maxima are readings on a
live `FuncValidator` and there is no summary to ask for afterwards. The
structural scan runs beside it and does not stop when validation does, so a
module refused for a post-1.0 instruction still reports the over-wide parameter
list further down.

### The limits, their units and where they come from

Every constant cites the upstream file and line it was transcribed from, at the
release named by `LIMITS_FROM` (`spacewasm 0.7.1`). Units matter more than usual
here, because the two quantities that sound alike are not the same:
`MAX_STACK_DEPTH` bounds a stack with one entry per **value** whatever its
width, while a function's recorded stack usage is in **words**, where `i64` and
`f64` count two.

| Constant | Value | Unit | Upstream | Kind |
| --- | --- | --- | --- | --- |
| `MAX_PARAM_WORDS` | 255 | words | `src/module.rs:583,593`; `Func::parameter_size: u8`, `src/code.rs:72` | decode |
| `MAX_LOCAL_WORDS` | 65 535 | words | `src/code.rs:142-145`; `Func::local_size: u16`, `src/code.rs:69` | decode |
| `MAX_FRAME_WORDS` | 65 535 | words | `src/code.rs:157-160` | decode |
| `MAX_LOCALS_GROUP_COUNT` | 65 535 | locals per group | `src/code.rs:126-127` | decode |
| `MAX_IR_INDEX` | 65 535 | a value in one 16-bit IR immediate | `src/text.rs:1099-1102` (`instr_imm_8_or_16`); `src/compiler.rs:281-283` | decode |
| `MAX_BRANCH_UNWIND_WORDS` | 255 | words | `src/text.rs:741-748` | decode |
| `MAX_IMPORT_NAME_BYTES` | 31 | bytes | `src/host.rs:327,352` (`HOST_FUNCTION_NAME_CAP`, `HOST_MODULE_NAME_CAP`) | registration |
| `MAX_HOST_FUNCTION_PARAMS` | 9 | parameters | `src/host.rs:168` | registration |
| `MAX_HOST_FUNCTION_RESULTS` | 1 | results | `src/host.rs:180` | registration |
| `MAX_CUSTOM_SECTION_NAME_BYTES` | 32 | bytes | `src/module.rs:483` | decode |
| `MAX_MEMORY_PAGES` | 65 536 | 64 KiB pages | `src/types.rs:332-343` | decode |
| `REFERENCE_MAX_CONTROL_FRAMES` | 64 | frames | `spacewasm_std`'s embedding; the generic is `src/text.rs:444` | embedder |
| `REFERENCE_MAX_STACK_DEPTH` | 256 | values | `spacewasm_std`'s embedding; the generic is `src/text.rs:445` | embedder |

Three kinds of limit, and a refusal always says which it met:

- **decode** — the decoder refuses the module outright.
- **registration** — the module decodes, and then no embedder can supply what it
  imports, so it can never be instantiated. A user told "the decoder rejects 32
  bytes" would shorten a name to 32 and meet the same wall.
- **embedder** — not a limit at all, but the configuration `spacewasm_std`
  ships. A module above it is conformant and needs a bigger const generic.

Two of the decode limits interact, and which one a function reaches first is
worth knowing: `MAX_FRAME_WORDS` bounds two words of header plus the locals plus
the operand peak, so a function with an empty operand stack may declare 65 533
local words and no more — two fewer than `MAX_LOCAL_WORDS` alone would allow.

The last two are of a different kind again. Every other row bounds something a
section *declares*, and the decoder meets it while reading that section. These
two bound the bytecode the interpreter compiles the module into, and it meets
them while walking a function body:

- `MAX_IR_INDEX` is one 16-bit immediate. Two operators can present a value over
  it — a `call_indirect`'s type index and a `br_table`'s target count — so one
  violation covers both and says which. They meet the cap in two different
  places, which is worth knowing when reading upstream: the target count goes
  through the shared emitter `instr_imm_8_or_16`, and the type index is checked
  by hand before its `write_16`. Nothing else reaches the refusal — a global and
  a function index arrive already narrowed to `u16`, which is a defect rather
  than a safeguard and is what the residue section below is about; a local index
  is a frame offset bounded by `MAX_FRAME_WORDS`; and a label index is refused
  as `InvalidLabelIndex` before it can be one.
- `MAX_BRANCH_UNWIND_WORDS` is the operands one branch discards, counted in
  words from the bottom of the live stack up to the target frame's own height —
  the same word convention and the same stop-at-the-first-untracked-slot rule
  the operand peak uses, and for the same reason. It is measured at `br`, at
  `br_if` after its condition comes off, and at every target of a `br_table`
  including the default, each against the frame that target leaves. A branch to
  the function's own outermost frame is not measured at all: the interpreter
  compiles it as an early return, which discards the frame wholesale, and so
  does `return`. Those four are four of the five branches the interpreter
  measures; the fifth is the implicit one an `else` emits over its own arm, and
  it is exempt because it cannot reach the bound at WebAssembly 1.0 — what
  stands above the `if` frame's floor there is the block's result, and 1.0
  admits at most one, so the unwind is at most two words.

### What the report carries

`Report` holds one `FunctionMetrics` per defined function. That list is the
whole of the record: the three module-wide maxima are methods over it rather
than fields beside it, each returning the number and the function that attains
it, so a report cannot state a maximum no function in it reaches.

- `deepest()` — control frames, **including** the implicit function-body frame,
  which both this crate and the interpreter push before reading an operator, so
  the number is comparable with an embedder's `MAX_CONTROL_FRAMES` with no
  adjustment.
- `tallest()` — operand stack height in **values**, the unit of
  `MAX_STACK_DEPTH`.
- `widest()` — the same peak weighted by width, in **words**, where `i64` and
  `f64` count two (`src/code.rs:29-32`). Summed from the bottom of the stack up
  to the first operand the validator no longer tracks — every slot after an
  `unreachable`, until the enclosing block ends — which contributes nothing, and
  neither does anything above it. That is the interpreter's own rule
  (`src/text.rs:789-802`) and not a choice: the figure it produces is the one the
  interpreter checks `MAX_FRAME_WORDS` against, so rounding an untracked slot up
  would refuse functions the decoder loads.

`FunctionMetrics::frame_words()` is `2 + local_words + max_operand_words`: the
call frame the engine's stack must hold, which is a third quantity again and the
one `Engine::new`'s budget is sized from. It is computed for the same reason —
a stored sum can disagree with the two fields it is the sum of.

### What a passing build says

`Report::summary_line()` is the one line a build owes an embedder, and
`Report::budget_warnings()` the warnings owed when a maximum exceeds the
`spacewasm_std` reference configuration (64 control frames, 256 operand values),
one per axis that is over, each naming the three functions that reach highest:

```
spacewasm: conformant with WebAssembly 1.0; deepest control nesting 7 in `render_row`, tallest operand stack 6 values in `blend` (peak 8 stack words in `blend`). Build the embedder with MAX_CONTROL_FRAMES >= 7 and MAX_STACK_DEPTH >= 6 (spacewasm_std uses 64 and 256); limits from spacewasm 0.7.1.
```

Both live here rather than in each caller: `infc` prints them about the module
it wrote and `infs` about the module an optimizer produced, and a build log whose
two lines named their units differently would be worse than one line.

### What a refusal looks like

`Violations::render(subject, consequence)` produces a header naming what was
checked and what the caller did about it, then one block per finding. Each block
gives the finding with both numbers, the sentence naming the authority that
imposes the cap, and one thing to change:

```
SpaceWasm conformance failed: out/main.wasm cannot be loaded by a SpaceWasm embedder.
No file was written.

  parameter words exceeded: function `mix_channels` declares 260 parameter words; SpaceWasm accepts at most 255.
    SpaceWasm stores a function's parameter size in a single byte. An i64 or f64 parameter counts as 2 words, every other parameter as 1.
    Pass fewer parameters, or collect them into a struct: a struct or array parameter is one i32 pointer, which is 1 word.
```

The two strings are the caller's because this crate is handed bytes and nothing
else: only `infc` knows it was about to write `out/main.wasm`, and only `infs`
knows the original artifact is still there. An empty `consequence` omits that
line, which is what `Display` passes — a reader quoting the violations out of a
build has no build to report the consequence of.

The violation types live in the crate's `errors` module and are re-exported from
`spacewasm`, which is the path to use. They are that target's alone — every
variant, the maxima each quotes, the two sentences under it and the header above
them all name one runtime — and the module is split out for length rather than
because anything in it is target-neutral.

Five of the violations describe module shapes this compiler cannot produce. Four
of them share one sentence — `MemoryTooLarge`, `LocalsGroupTooLarge`,
`CustomSectionNameTooLong` and `ImportMultipleResults`. "Shorten the custom
section name" would be advice about a file the user did not write, so their
remedy splits on provenance instead: report a compiler bug, or rebuild the
external module, because only the person holding the build knows which half
applies.

`IndexTooLarge` is the fifth — code generation emits no `call_indirect` and no
`br_table` at any target — and its remedy splits the same way while still naming
the edit, because there is one and it differs by instruction: fewer function
types, or a narrower jump table. `BranchUnwindTooDeep` is not in that family: a
branch over a deep stack is a shape a body can be written into, so its remedy is
the source edit that shortens it.

### What `check` does not refuse, and why

The verdict is a claim about the limits in the table above, not a proof that the
interpreter will decode the module. `spacewasm` 0.7.1 refuses through one enum,
`ValidationError` (`src/error.rs:37-129`), and every variant of it is classified
below: an exemption has to name the reason it is out of reach, because the one
kind of residue that never turns red is the kind nobody wrote down.

**Modelled here, each as a `Violation`.** `FunctionParametersTooLarge`
(`src/module.rs:594`), `TooManyLocals` (`src/code.rs:127,140,145`),
`StackTooLarge` (`src/code.rs:46,160`), `MemoryTooLarge`
(`src/types.rs:329-343`), `IdxTooLarge` (`src/text.rs:1099-1102`,
`src/compiler.rs:281-283`), `LabelStackJumpTooDeep` (`src/text.rs:744-748`), and
the `VecTooLong` (`src/reader.rs:478-480`) raised by the two fixed 32-byte reads
an import's names and a custom section's name go through.
`FunctionReturnsTooLarge` (`src/types.rs:196`, `src/module.rs:586`) is modelled
for an import; on a defined function it is the standard's own multi-value rule
and stock validation reaches it first. `LabelStackJumpTooDeep` is modelled at
four of the five branches the interpreter measures — the fifth, an `else`'s
implicit branch over its own arm (`src/compiler.rs:129-134`), is exempt for the
reason given with `MAX_BRANCH_UNWIND_WORDS` above: at WebAssembly 1.0 what
stands above the `if` frame there is one result at most, so that unwind is at
most two words and cannot reach 255. Two of this crate's refusals are not
`ValidationError`s at all — the 31-byte registration cap and the nine-parameter
host list are `src/host.rs`'s, met after the module has already decoded.

**Refused by the WebAssembly 1.0 validation that runs first**, so they arrive as
`OutsideWasm1` carrying the stock validator's wording rather than the
interpreter's: `Eof`, `MalformedInteger`, `MalformedMagic`, `MalformedVersion`,
`MalformedUtf8`, `MalformedSectionId`, `MalformedSectionSize`,
`MalformedValueType`, `MalformedFunction`, `MalformedLimit`, `MalformedElemType`,
`MalformedMemType`, `MalformedCodeSize`, `MalformedImportExportDesc`,
`InvalidPageSize`, `InvalidMaxLimit`, `InvalidSectionOrdering`,
`DuplicateSection`, `DuplicateExportName`, `InvalidOpcode`,
`InvalidCodeSectionFunctionCount`, `ExpectedConstOrVar`, `ExpectedTerminal`,
`StackUnderflow`, `TypeMismatch`, `BlockResultTypeMismatch`,
`BrTableResultTypeMismatch`, `FunctionResultTypeMismatch`, `MemAlignTooLarge`,
`AlignmentLargerThanType`, `InvalidMemIndex`, `InvalidMemOffsetType`,
`InvalidNegativeMemOffset`, `InvalidMemOffset`, `MemoryNotDefined`,
`InvalidTableIndex`, `TableNotDefined`, `InvalidElementCount`,
`InvalidElementOffset`, `InvalidElementOutOfBounds`, `InvalidLabelIndex`,
`InvalidElseBlock`, `InvalidEndBlock`, `MultipleMemories`, `MultipleTables`,
`InstructionOutsideOfFunction`, `LocalIdxOutOfRange`, `FunctionIdxOutOfRange`,
`TypeIdxOutOfRange`, `GlobalIdxOutOfRange`, `GlobalTypeMismatch`,
`GlobalNotMutable`, `InvalidConstInstruction`, `InvalidConstantExpr`,
`InvalidStartFunctionSignature`.

**Not a property of the bytes: the host set decides them.** An import is bound
against the modules an embedder registered, and this crate is handed bytes and
no embedder — which is why it holds an import only to the two caps *no* host
could ever satisfy. `FunctionImportNotFound`, `GlobalImportNotFound`,
`MemoryImportNotFound`, `TableImportNotFound`, `FunctionImportOutOfRange`,
`FunctionImportTypeMismatch`, `GlobalImportTypeMismatch`,
`MemoryImportTypeMismatch`, `TableImportTypeMismatch`,
`TableImportIncompatibleSize`, `MemoryImportTooLarge`, `TableRefNotUnique`,
`DuplicateModuleName` (`src/module.rs:113,117`: a name already in the store),
and `InvalidHostStartFunction` (`src/module.rs:317`: a start section naming a
host function).

**Embedder-configured, which is why they are measured and not refused.** The
verifier's two stacks are const generics, and overflowing either is a
`StaticVec::push` failure (`src/util/static_vec.rs:78`) that surfaces as
`AllocError` — `ControlFlowTooDeep` is in the enum and is constructed nowhere in
0.7.1. There is no fixed number to refuse against, so `Report` measures both
axes and the build prints them. `IllegalMemoryGrow` (`src/compiler.rs:443`) is a
`CompilerOptions` choice in the same way; `GuestMemoryAllocationFailure` and
`MemoryError` are the embedder's own allocation at load. So is the code-page
budget `CompilerOptions::max_code_pages` sets, and it is worth naming because it
bites *before* one of the modelled limits: a `br_table` of 65,535 targets is
inside `MAX_IR_INDEX` and costs 131,070 IR words, which the reference
configuration cannot hold — so the accepting side of that particular boundary is
out of reach of any plausible embedder, and the oracle says so rather than
pretending otherwise.

**Properties of the interpreter's compiled IR that a description cannot
reproduce.** Two IR properties *are* modelled above, because both are decided by
reading one instruction. These three are not, because reproducing them means
reproducing the code builder, which is the interpreter itself: `LabelJumpTooLarge`
(`src/text.rs:53-60`, a 22-bit jump immediate into emitted code),
`PageFault` (`src/text.rs:390-415`, its code pages) and `PossibleBackpatchCycle`
(`src/text.rs:321`, an embedder-set iteration budget).

**Constructed nowhere in 0.7.1**, so no module can earn them:
`ModuleIdxTooLarge`, `I33IsNegative`, `FunctionTextOutOfRange`, and
`ControlFlowTooDeep` above. `ReaderError` is
not the decoder's finding at all — it carries back a code from the embedder's
own `WasmStream`.

**Unreachable on this compiler's path.** `TableTooLarge` (`src/types.rs:421`,
`429-433`: a table above 10,000,000 elements) is the one upstream size limit
this crate could compute and does not, and it is exempt because no artifact
reaching the check can carry a table section: code generation emits none, the
linker refuses a main-side table outright and writes no table section of its
own, and an external whose closure names the table space is refused before the
merge. Stock validation at WebAssembly 1.0 admits a table up to `u32::MAX`, so
nothing else would catch it — if any of those three facts changes, this is the
refusal to add.

**Not modelled.** None. Every variant of `ValidationError` is in one of the
classes above.

**The residue that is left, and it is not a refusal.** Two index spaces
`spacewasm` 0.7.1 **truncates** rather than refusing. `Module::get_func_ref`
(`src/module.rs:383-389`) and `Module::get_global_ref` (`src/module.rs:423-429`)
narrow a function index and a global index to `u16` with an `as` cast, and the
IR emitter is then handed a value that cannot be over the cap — which is why
`IdxTooLarge` is unreachable through `call`, `global.get` and `global.set`, and
why the modelled arms are the two operators that reach the emitter with an
un-narrowed value.

What the cast costs is measured rather than argued, and the measurement is
committed: a module declaring 65,537 globals whose exported function returns
`global.get 65536` decodes, loads and returns global **0**'s value — pinned by
`a_global_index_over_the_ir_immediate_is_truncated_rather_than_refused` in the
oracle suite, which runs the module and reads the answer, against the 65,535
neighbour as its control. `check` does not refuse it. The question this crate
answers is what the decoder's verdict would be, and the decoder's verdict is
"accepted" — a refusal here would be this crate disagreeing with the runtime it
describes, which is a policy change and not a transcription, and every row of the
oracle suite is built on the two agreeing. Whether a build ought nonetheless to be
stricter than the decoder here — refusing a module the interpreter would load, on
the grounds that it would load it and call the wrong function — is a policy
question this crate deliberately leaves undecided. It is written down because a
residue nobody wrote down is one nothing turns red about, and because the shape is
reachable in principle: a linked program declaring more than 65,536 globals. The
function-index half needs an embedder whose code-page budget survives 65,536
function bodies as well; the reference configuration does not, and refuses such a
module with `AllocError(OutOfMemory)` before the truncation can matter.

## How it is held to being right

Transcribed numbers rot, and a checker that is wrong in the permissive direction
is worse than none: it turns a build-time refusal into a load failure on flight
hardware. So the numbers are not trusted — `tests/tests/spacewasm/conformance_oracle.rs`
puts a hand-written module on *both* sides of every boundary, in front of this
crate and in front of the real interpreter, and requires the two verdicts to be
the same. `MAX_FRAME_WORDS` is in the table above because that oracle found it:
the crate accepted a 65 535-word local declaration the decoder refused.

The two IR limits are pinned the same way, and their rows assert the decoder's
*reason* rather than only its verdict, since every module that reaches them is
either large or deep and a refusal for a page budget would otherwise satisfy the
row while saying nothing. Three of those rows exist because a boundary alone
would not have caught the model being wrong in the permissive direction: a `br`
carries its condition-free stack and a `br_if` does not, so only the conditional
row separates a model that forgets the selector from one that does not; a
`br_table`'s default target is the one a model measuring "the targets" would
skip; and a branch to the function's own frame is compiled as an early return, so
a model measuring every branch alike would refuse a module the decoder runs.
