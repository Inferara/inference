//! Wire codec for the `inference.hspecs` custom WASM section.
//!
//! [`encode`] serializes an [`HSpecMap`] to the section *payload* (the
//! enclosing custom-section framing is added by the emitter, mirroring how
//! `inference.spec_funcs` is wrapped downstream); [`decode`] parses that
//! payload back, fully bounds-checked so a corrupt or adversarial `.wasm`
//! surfaces a clean [`DecodeError`] rather than a panic, an unbounded
//! allocation, or a stack overflow.
//!
//! The encoding is **canonical**: [`encode`] produces the same bytes for equal
//! maps regardless of insertion order (the symbol table and spec names are
//! sorted), and [`decode`] rejects any non-canonical ordering, so the two are
//! mutual inverses on well-formed input.
//!
//! ## Payload format (LEB128 throughout; `varu32` = unsigned LEB128)
//!
//! ```text
//! version      varu32 = 2
//! sym_count    varu32
//!   repeated sym_count times, STRICTLY ASCENDING and unique:
//!     name_len   varu32
//!     name_bytes utf-8              -- a function symbol; not NUL-terminated
//! spec_count   varu32
//!   repeated spec_count times, spec names STRICTLY ASCENDING and unique:
//!     name_len    varu32
//!     name_bytes  utf-8             -- folded spec name
//!     entry_count varu32
//!     repeated entry_count times, in source order:
//!       symbol_idx varu32           -- into the symbol table: the entry's fn symbol
//!       kind       u8               -- 0x00 Forall | 0x01 Exists | 0x02 Unique
//!       reach_meta                  -- present iff kind != 0x00:
//!         entry_arity varu32
//!         locs_count  varu32        -- at most MAX_VISIBLE_LOCS
//!         loc         varu32 * locs_count
//!                                   -- STRICTLY ASCENDING and unique, each
//!                                   -- at most MAX_VISIBLE_LOCS
//!       hassert                     -- preorder, tag-prefixed (see below)
//! ```
//!
//! Both `App`/`AppOk` inside a tree and each entry's own function symbol are
//! stored as a `varu32` index into the single shared symbol table. The kind
//! byte follows [`crate::ir::SpecKind`]'s declaration order; a `Forall` entry
//! carries no reachability metadata, so the universal common case costs one
//! byte.
//!
//! ### Tree tags
//!
//! ```text
//! hassert := tag:u8, then:
//!   0x00 True
//!   0x01 False
//!   0x02 Not      hassert
//!   0x03 And      hassert hassert
//!   0x04 Imp      hassert hassert
//!   0x05 Or       hassert hassert
//!   0x06 Ex       hassert
//!   0x07 TermEq   term term
//!   0x08 HasType  term numtype
//!   0x09 Defined  term
//!   0x0A AppOk    symbol_idx:varu32  arg_count:varu32  term * arg_count
//!   0x0B All      hassert
//!
//! term := tag:u8, then:
//!   0x00 Const    hconst
//!   0x01 LVar     varu32
//!   0x02 Local    varu32
//!   0x03 App      symbol_idx:varu32  arg_count:varu32  term * arg_count
//!   0x04 Binop    numtype:u8  binop:u8  term term
//!   0x05 Relop    numtype:u8  relop:u8  term term
//!
//! hconst := tag:u8, then:
//!   0x00 I32      value: signed LEB128 (must fit i32)
//!   0x01 I64      value: signed LEB128
//!
//! numtype:u8   0x00 I32, 0x01 I64
//! binop:u8     0x00 Add  0x01 Sub  0x02 Mul  0x03 DivS 0x04 DivU 0x05 RemS 0x06 RemU
//!              0x07 And  0x08 Or   0x09 Xor  0x0A Shl  0x0B ShrS 0x0C ShrU
//! relop:u8     0x00 Eq   0x01 Ne   0x02 LtS  0x03 LtU  0x04 GtS  0x05 GtU
//!              0x06 LeS  0x07 LeU  0x08 GeS  0x09 GeU
//! ```
//!
//! The tag values are stable and part of the format: they follow each enum's
//! declaration order in [`crate::ir`]. A new constructor therefore appends both
//! a variant and a tag; it never takes a value in the middle, however closely it
//! is related to the variant it reads like.
//!
//! Adding a tag is *additive* and does not move [`HSPECS_SECTION_VERSION`]: a
//! decoder that predates the tag rejects the payload loudly with
//! [`DecodeError::UnknownHassertTag`], the section is proof-mode intermediate
//! data, and recompilation rather than migration is the compatibility story. A
//! version bump is for a change that reshapes entries the old decoder would
//! otherwise misread.

use crate::ir::{
    HAssert, HBinop, HConst, HFnRef, HNumType, HRelop, HSpecEntry, HSpecMap, HTerm, ReachMeta,
    SpecKind,
};

/// Name of the custom WASM section carrying the per-program obligation map.
///
/// Sibling of `inference.spec_funcs` in the vendor-prefixed `inference.*`
/// namespace.
pub const HSPECS_SECTION_NAME: &str = "inference.hspecs";

/// Wire-format version emitted at the head of the payload. [`decode`] rejects
/// any other value — including the superseded version 1, whose entries carried
/// no quantifier kind — so a format revision breaks compatibility loudly
/// instead of silently misparsing. The section is proof-mode intermediate
/// data; recompilation, not migration, is the compatibility story.
pub const HSPECS_SECTION_VERSION: u32 = 2;

/// Upper bound, in bytes, on a single name in the payload — a spec-name key or,
/// chiefly, a function symbol.
///
/// This is a sanity bound, not an allocation bound: the decoder already caps
/// each name at the remaining payload length, so a hand-crafted over-advertised
/// length is rejected before allocation regardless. The value is deliberately
/// **larger** than `inference.spec_funcs`' 255-byte *spec-name* cap because an
/// `inference.hspecs` function symbol is a different, longer kind of string — a
/// folded spec name (itself up to 255 bytes) joined with struct/function
/// identifiers (`{spec}.{fn}`, `{spec}.{Struct}.{method}`). A symbol built from
/// a max-length spec name necessarily exceeds 255, so a 255-byte cap would
/// reject obligations for specs that `inference.spec_funcs` accepts. The cap
/// therefore only rejects absurdly long identifiers, well past anything a real
/// program produces, while still bounding the value for the fail-closed
/// producers that mirror it.
pub const MAX_NAME_LEN: usize = 1024;

/// Maximum nesting depth of a decoded assertion or term tree.
///
/// A recursive decode of an adversarially deep tree would overflow the stack —
/// an `abort()` that bypasses every `?`. Capping the recursion turns that into
/// a recoverable [`DecodeError::TreeTooDeep`]. The bound also guards
/// [`encode`] and the derived `Drop`: no value that round-trips through this
/// codec is deeper than the cap, and the cap is far below the depth at which a
/// small (2 MiB) thread stack would be exhausted. The value matches
/// `wasm-to-v`'s `MAX_EXPRESSION_DEPTH`.
pub const MAX_TREE_DEPTH: usize = 256;

/// Sanity cap on a reachability entry's `visible_locs`: both the number of
/// listed slots and every slot index must be at most this value.
///
/// A visible slot indexes a WASM local of one function frame, and no function
/// codegen emits approaches 65 536 locals — the cap only rejects payloads no
/// producer writes, keeping an adversarial section from advertising an absurd
/// projection list while leaving every real obligation untouched.
pub const MAX_VISIBLE_LOCS: u32 = 65_536;

/// Encodes an [`HSpecMap`] into the canonical `inference.hspecs` payload bytes.
///
/// The output is deterministic: the symbol table and the spec list are sorted,
/// so two maps that are equal (regardless of how they were built) encode to
/// identical bytes.
///
/// # Contract
///
/// The map **must** satisfy [`validate`] — every name non-empty and within
/// [`MAX_NAME_LEN`], every tree within [`MAX_TREE_DEPTH`]. That is exactly the
/// input contract [`decode`] enforces, so an unvalidated map could otherwise
/// serialize into a payload the codec's own hardened decoder rejects (a corrupt
/// artifact), or overflow the stack while encoding a pathologically deep tree.
/// Callers pass either data that came from [`decode`] (which is guaranteed to
/// pass, see the crate tests) or data they have run through [`validate`]
/// themselves and turned into their own diagnostic. Code generation, the one
/// producer of fresh maps, gates on [`validate`] before reaching here.
///
/// # Panics
///
/// Panics if the map violates [`validate`] — a documented contract breach that
/// is strictly safer than emitting a decode-rejected artifact or overflowing
/// the stack. Also panics if a map holds more than `u32::MAX` symbols, specs,
/// entries, or call arguments, all unreachable for any real WASM module.
#[must_use = "the encoded payload is the return value"]
pub fn encode(map: &HSpecMap) -> Vec<u8> {
    if let Err(err) = validate(map) {
        panic!("inference.hspecs: refusing to encode a map its own decoder would reject: {err}");
    }

    // The symbol table is the sorted, de-duplicated union of every function
    // symbol referenced anywhere: each entry's own symbol plus every
    // `App`/`AppOk` symbol inside every tree.
    let mut symbols: Vec<&str> = Vec::new();
    for entries in map.values() {
        for entry in entries {
            symbols.push(entry.fn_symbol.0.as_str());
            collect_assert_symbols(&entry.hassert, &mut symbols);
        }
    }
    symbols.sort_unstable();
    symbols.dedup();

    let mut out = Vec::new();
    write_u32(&mut out, HSPECS_SECTION_VERSION);
    write_u32(&mut out, count(symbols.len()));
    for name in &symbols {
        write_str(&mut out, name);
    }

    let mut specs: Vec<(&str, &Vec<HSpecEntry>)> = map
        .iter()
        .map(|(name, entries)| (name.as_str(), entries))
        .collect();
    specs.sort_unstable_by(|a, b| a.0.cmp(b.0));

    write_u32(&mut out, count(specs.len()));
    for (name, entries) in specs {
        write_str(&mut out, name);
        write_u32(&mut out, count(entries.len()));
        for entry in entries {
            write_u32(&mut out, sym_index(&symbols, &entry.fn_symbol.0));
            encode_kind(&entry.kind, &mut out);
            encode_assert(&entry.hassert, &symbols, &mut out);
        }
    }

    out
}

/// Verifies that `map` satisfies [`decode`]'s input contract, so [`encode`] can
/// serialize it into a payload the decoder accepts.
///
/// Enforces, over the whole map, exactly the limits [`decode`] checks:
///
/// - every spec name and every function symbol (an obligation's own symbol and
///   every `App`/`AppOk` symbol inside its tree) is non-empty and at most
///   [`MAX_NAME_LEN`] bytes;
/// - every obligation's assertion/term tree nests at most [`MAX_TREE_DEPTH`]
///   deep, measured exactly as the decoder counts it;
/// - every reachability entry's `visible_locs` are strictly ascending (which
///   also rejects duplicates), with the count and every value at most
///   [`MAX_VISIBLE_LOCS`].
///
/// Specs are visited in sorted-name order, so the first reported violation is
/// deterministic. The tree walk is depth-limited — it stops descending past the
/// cap — so validating an adversarially deep map cannot itself overflow the
/// stack.
///
/// # Errors
///
/// Returns the first [`PayloadError`] found, or `Ok(())` when the map is
/// encodable.
pub fn validate(map: &HSpecMap) -> Result<(), PayloadError> {
    let mut spec_names: Vec<&String> = map.keys().collect();
    spec_names.sort_unstable();
    for spec in spec_names {
        if !name_len_ok(spec) {
            return Err(PayloadError::SpecName {
                name: spec.clone(),
                len: spec.len(),
            });
        }
        for entry in &map[spec] {
            let symbol = &entry.fn_symbol.0;
            if !name_len_ok(symbol) {
                return Err(PayloadError::FunctionSymbol {
                    spec: spec.clone(),
                    symbol: symbol.clone(),
                    len: symbol.len(),
                });
            }
            validate_kind(spec, symbol, &entry.kind)?;
            validate_assert(spec, symbol, &entry.hassert, 1)?;
        }
    }
    Ok(())
}

/// Whether `name`'s byte length is in the `1..=MAX_NAME_LEN` range the decoder
/// requires (`read_name` rejects both empty and over-cap names).
fn name_len_ok(name: &str) -> bool {
    (1..=MAX_NAME_LEN).contains(&name.len())
}

/// Validates one entry's quantifier kind: for the reachability kinds, the
/// `visible_locs` rules `decode_reach_meta` enforces — strictly ascending
/// (which also rejects duplicates), count and every value within
/// [`MAX_VISIBLE_LOCS`]. A `Forall` kind carries nothing to check.
fn validate_kind(spec: &str, function: &str, kind: &SpecKind) -> Result<(), PayloadError> {
    let (SpecKind::Exists(meta) | SpecKind::Unique(meta)) = kind else {
        return Ok(());
    };
    if meta.visible_locs.len() > MAX_VISIBLE_LOCS as usize {
        return Err(PayloadError::TooManyVisibleLocs {
            spec: spec.to_string(),
            function: function.to_string(),
            count: meta.visible_locs.len(),
        });
    }
    let mut prev: Option<u32> = None;
    for &loc in &meta.visible_locs {
        if loc > MAX_VISIBLE_LOCS {
            return Err(PayloadError::VisibleLocOutOfRange {
                spec: spec.to_string(),
                function: function.to_string(),
                loc,
            });
        }
        if prev.is_some_and(|prev| loc <= prev) {
            return Err(PayloadError::VisibleLocsNotAscending {
                spec: spec.to_string(),
                function: function.to_string(),
            });
        }
        prev = Some(loc);
    }
    Ok(())
}

/// Validates one obligation's assertion tree in a single depth-limited pass:
/// the depth cap (mirroring `decode_assert`) plus the name contract on every
/// `App`/`AppOk` symbol reached. `spec`/`function` identify the obligation for a
/// [`PayloadError::TreeTooDeep`]. The early return past the cap bounds the
/// recursion, so it cannot overflow on a deep input.
fn validate_assert(
    spec: &str,
    function: &str,
    a: &HAssert,
    depth: usize,
) -> Result<(), PayloadError> {
    if depth > MAX_TREE_DEPTH {
        return Err(PayloadError::TreeTooDeep {
            spec: spec.to_string(),
            function: function.to_string(),
        });
    }
    match a {
        HAssert::True | HAssert::False => Ok(()),
        HAssert::Not(x) | HAssert::Ex(x) | HAssert::All(x) => {
            validate_assert(spec, function, x, depth + 1)
        }
        HAssert::And(l, r) | HAssert::Imp(l, r) | HAssert::Or(l, r) => {
            validate_assert(spec, function, l, depth + 1)?;
            validate_assert(spec, function, r, depth + 1)
        }
        HAssert::TermEq(l, r) => {
            validate_term(spec, function, l, 1)?;
            validate_term(spec, function, r, 1)
        }
        HAssert::HasType(t, _) | HAssert::Defined(t) => validate_term(spec, function, t, 1),
        HAssert::AppOk(f, args) => {
            check_symbol(spec, &f.0)?;
            for arg in args {
                validate_term(spec, function, arg, 1)?;
            }
            Ok(())
        }
    }
}

/// Validates a term tree entered at `depth` 1 from its assertion position: the
/// depth cap (mirroring `decode_term`, which budgets terms from a fresh
/// counter) plus the name contract on every `App` symbol. Bounded like
/// [`validate_assert`].
fn validate_term(spec: &str, function: &str, t: &HTerm, depth: usize) -> Result<(), PayloadError> {
    if depth > MAX_TREE_DEPTH {
        return Err(PayloadError::TreeTooDeep {
            spec: spec.to_string(),
            function: function.to_string(),
        });
    }
    match t {
        HTerm::Const(_) | HTerm::LVar(_) | HTerm::Local(_) => Ok(()),
        HTerm::App(f, args) => {
            check_symbol(spec, &f.0)?;
            for arg in args {
                validate_term(spec, function, arg, depth + 1)?;
            }
            Ok(())
        }
        HTerm::Binop(_, _, l, r) | HTerm::Relop(_, _, l, r) => {
            validate_term(spec, function, l, depth + 1)?;
            validate_term(spec, function, r, depth + 1)
        }
    }
}

/// Enforces the name contract on a function symbol referenced within `spec`.
fn check_symbol(spec: &str, symbol: &str) -> Result<(), PayloadError> {
    if name_len_ok(symbol) {
        Ok(())
    } else {
        Err(PayloadError::FunctionSymbol {
            spec: spec.to_string(),
            symbol: symbol.to_string(),
            len: symbol.len(),
        })
    }
}

/// Decodes an `inference.hspecs` payload back into an [`HSpecMap`].
///
/// # Errors
///
/// Returns a [`DecodeError`] on any malformed input: an unrecognized version,
/// truncation, a bad LEB128 or over-`u32` integer, an over-advertised count,
/// an over-long or non-UTF-8 name, an empty name, a non-ascending symbol table
/// or spec list (which also rejects duplicates), an out-of-range symbol index,
/// an unknown tag (including a spec-kind tag), reachability `visible_locs`
/// that are non-ascending or past [`MAX_VISIBLE_LOCS`] in count or value, an
/// out-of-range constant, nesting past [`MAX_TREE_DEPTH`], or trailing bytes
/// after the declared payload.
pub fn decode(data: &[u8]) -> Result<HSpecMap, DecodeError> {
    let mut r = Reader::new(data);

    let version = r.read_u32()?;
    if version != HSPECS_SECTION_VERSION {
        return Err(DecodeError::UnsupportedVersion(version));
    }

    let symbols = decode_symbol_table(&mut r)?;

    let spec_count = r.read_u32()?;
    // Each spec consumes at least one payload byte; reject an advertisement
    // exceeding the remaining bytes before allocating.
    if spec_count as usize > r.remaining() {
        return Err(DecodeError::CountExceedsPayload {
            kind: "spec",
            count: spec_count,
        });
    }

    let mut map = HSpecMap::default();
    let mut prev_spec: Option<String> = None;
    for _ in 0..spec_count {
        let name = r.read_name()?;
        if !ascending(prev_spec.as_deref(), &name) {
            return Err(DecodeError::SpecNamesNotAscending);
        }

        let entry_count = r.read_u32()?;
        if entry_count as usize > r.remaining() {
            return Err(DecodeError::CountExceedsPayload {
                kind: "entry",
                count: entry_count,
            });
        }
        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let fn_symbol = resolve_symbol(&symbols, r.read_u32()?)?;
            let kind = decode_kind(&mut r)?;
            let hassert = decode_assert(&mut r, &symbols, 1)?;
            entries.push(HSpecEntry {
                fn_symbol,
                hassert,
                kind,
            });
        }

        map.insert(name.clone(), entries);
        prev_spec = Some(name);
    }

    if !r.is_empty() {
        return Err(DecodeError::TrailingBytes(r.remaining()));
    }
    Ok(map)
}

fn decode_symbol_table(r: &mut Reader) -> Result<Vec<String>, DecodeError> {
    let sym_count = r.read_u32()?;
    if sym_count as usize > r.remaining() {
        return Err(DecodeError::CountExceedsPayload {
            kind: "symbol",
            count: sym_count,
        });
    }
    let mut symbols: Vec<String> = Vec::with_capacity(sym_count as usize);
    for _ in 0..sym_count {
        let name = r.read_name()?;
        if !ascending(symbols.last().map(String::as_str), &name) {
            return Err(DecodeError::SymbolsNotAscending);
        }
        symbols.push(name);
    }
    Ok(symbols)
}

/// Decodes an entry's kind byte and, for the reachability kinds, the
/// metadata block that follows it.
fn decode_kind(r: &mut Reader) -> Result<SpecKind, DecodeError> {
    match r.read_u8()? {
        0x00 => Ok(SpecKind::Forall),
        0x01 => Ok(SpecKind::Exists(decode_reach_meta(r)?)),
        0x02 => Ok(SpecKind::Unique(decode_reach_meta(r)?)),
        other => Err(DecodeError::UnknownSpecKindTag(other)),
    }
}

/// Decodes a reachability entry's metadata, enforcing the `visible_locs`
/// rules [`validate`] mirrors: count within [`MAX_VISIBLE_LOCS`] (checked
/// before the allocation bound so an absurd advertisement is rejected either
/// way), values strictly ascending and within the cap.
fn decode_reach_meta(r: &mut Reader) -> Result<ReachMeta, DecodeError> {
    let entry_arity = r.read_u32()?;
    let locs_count = r.read_u32()?;
    if locs_count > MAX_VISIBLE_LOCS {
        return Err(DecodeError::TooManyVisibleLocs(locs_count));
    }
    // Each visible local is at least a one-byte varu32; bound before
    // allocating.
    if locs_count as usize > r.remaining() {
        return Err(DecodeError::CountExceedsPayload {
            kind: "visible-local",
            count: locs_count,
        });
    }
    let mut visible_locs = Vec::with_capacity(locs_count as usize);
    for _ in 0..locs_count {
        let loc = r.read_u32()?;
        if loc > MAX_VISIBLE_LOCS {
            return Err(DecodeError::VisibleLocOutOfRange(loc));
        }
        if visible_locs.last().is_some_and(|&prev| loc <= prev) {
            return Err(DecodeError::VisibleLocsNotAscending);
        }
        visible_locs.push(loc);
    }
    Ok(ReachMeta {
        entry_arity,
        visible_locs,
    })
}

fn decode_assert(r: &mut Reader, symbols: &[String], depth: usize) -> Result<HAssert, DecodeError> {
    if depth > MAX_TREE_DEPTH {
        return Err(DecodeError::TreeTooDeep);
    }
    let tag = r.read_u8()?;
    Ok(match tag {
        0x00 => HAssert::True,
        0x01 => HAssert::False,
        0x02 => HAssert::Not(Box::new(decode_assert(r, symbols, depth + 1)?)),
        0x03 => {
            let lhs = decode_assert(r, symbols, depth + 1)?;
            let rhs = decode_assert(r, symbols, depth + 1)?;
            HAssert::And(Box::new(lhs), Box::new(rhs))
        }
        0x04 => {
            let lhs = decode_assert(r, symbols, depth + 1)?;
            let rhs = decode_assert(r, symbols, depth + 1)?;
            HAssert::Imp(Box::new(lhs), Box::new(rhs))
        }
        0x05 => {
            let lhs = decode_assert(r, symbols, depth + 1)?;
            let rhs = decode_assert(r, symbols, depth + 1)?;
            HAssert::Or(Box::new(lhs), Box::new(rhs))
        }
        0x06 => HAssert::Ex(Box::new(decode_assert(r, symbols, depth + 1)?)),
        0x07 => {
            let lhs = decode_term(r, symbols, 1)?;
            let rhs = decode_term(r, symbols, 1)?;
            HAssert::TermEq(lhs, rhs)
        }
        0x08 => {
            let t = decode_term(r, symbols, 1)?;
            HAssert::HasType(t, decode_numtype(r)?)
        }
        0x09 => HAssert::Defined(decode_term(r, symbols, 1)?),
        0x0A => {
            let f = resolve_symbol(symbols, r.read_u32()?)?;
            HAssert::AppOk(f, decode_args(r, symbols, 1)?)
        }
        0x0B => HAssert::All(Box::new(decode_assert(r, symbols, depth + 1)?)),
        other => return Err(DecodeError::UnknownHassertTag(other)),
    })
}

fn decode_term(r: &mut Reader, symbols: &[String], depth: usize) -> Result<HTerm, DecodeError> {
    if depth > MAX_TREE_DEPTH {
        return Err(DecodeError::TreeTooDeep);
    }
    let tag = r.read_u8()?;
    Ok(match tag {
        0x00 => HTerm::Const(decode_const(r)?),
        0x01 => HTerm::LVar(r.read_u32()?),
        0x02 => HTerm::Local(r.read_u32()?),
        0x03 => {
            let f = resolve_symbol(symbols, r.read_u32()?)?;
            HTerm::App(f, decode_args(r, symbols, depth + 1)?)
        }
        0x04 => {
            let ty = decode_numtype(r)?;
            let op = decode_binop(r)?;
            let lhs = decode_term(r, symbols, depth + 1)?;
            let rhs = decode_term(r, symbols, depth + 1)?;
            HTerm::Binop(ty, op, Box::new(lhs), Box::new(rhs))
        }
        0x05 => {
            let ty = decode_numtype(r)?;
            let op = decode_relop(r)?;
            let lhs = decode_term(r, symbols, depth + 1)?;
            let rhs = decode_term(r, symbols, depth + 1)?;
            HTerm::Relop(ty, op, Box::new(lhs), Box::new(rhs))
        }
        other => return Err(DecodeError::UnknownTermTag(other)),
    })
}

fn decode_args(
    r: &mut Reader,
    symbols: &[String],
    depth: usize,
) -> Result<Vec<HTerm>, DecodeError> {
    let arg_count = r.read_u32()?;
    // Each argument term is at least a one-byte tag; bound before allocating.
    if arg_count as usize > r.remaining() {
        return Err(DecodeError::CountExceedsPayload {
            kind: "argument",
            count: arg_count,
        });
    }
    let mut args = Vec::with_capacity(arg_count as usize);
    for _ in 0..arg_count {
        args.push(decode_term(r, symbols, depth)?);
    }
    Ok(args)
}

fn decode_const(r: &mut Reader) -> Result<HConst, DecodeError> {
    match r.read_u8()? {
        0x00 => {
            let v = i32::try_from(r.read_i64()?).map_err(|_| DecodeError::ConstOutOfRange)?;
            Ok(HConst::I32(v))
        }
        0x01 => Ok(HConst::I64(r.read_i64()?)),
        other => Err(DecodeError::UnknownConstTag(other)),
    }
}

fn decode_numtype(r: &mut Reader) -> Result<HNumType, DecodeError> {
    match r.read_u8()? {
        0x00 => Ok(HNumType::I32),
        0x01 => Ok(HNumType::I64),
        other => Err(DecodeError::UnknownNumType(other)),
    }
}

fn decode_binop(r: &mut Reader) -> Result<HBinop, DecodeError> {
    Ok(match r.read_u8()? {
        0x00 => HBinop::Add,
        0x01 => HBinop::Sub,
        0x02 => HBinop::Mul,
        0x03 => HBinop::DivS,
        0x04 => HBinop::DivU,
        0x05 => HBinop::RemS,
        0x06 => HBinop::RemU,
        0x07 => HBinop::And,
        0x08 => HBinop::Or,
        0x09 => HBinop::Xor,
        0x0A => HBinop::Shl,
        0x0B => HBinop::ShrS,
        0x0C => HBinop::ShrU,
        other => return Err(DecodeError::UnknownBinop(other)),
    })
}

fn decode_relop(r: &mut Reader) -> Result<HRelop, DecodeError> {
    Ok(match r.read_u8()? {
        0x00 => HRelop::Eq,
        0x01 => HRelop::Ne,
        0x02 => HRelop::LtS,
        0x03 => HRelop::LtU,
        0x04 => HRelop::GtS,
        0x05 => HRelop::GtU,
        0x06 => HRelop::LeS,
        0x07 => HRelop::LeU,
        0x08 => HRelop::GeS,
        0x09 => HRelop::GeU,
        other => return Err(DecodeError::UnknownRelop(other)),
    })
}

fn resolve_symbol(symbols: &[String], index: u32) -> Result<HFnRef, DecodeError> {
    symbols
        .get(index as usize)
        .map(|s| HFnRef(s.clone()))
        .ok_or(DecodeError::SymbolIndexOutOfRange(index))
}

/// Whether `name` strictly follows `prev` lexicographically (with `None`
/// preceding everything), enforcing sorted order and, transitively, uniqueness.
fn ascending(prev: Option<&str>, name: &str) -> bool {
    prev.is_none_or(|prev| name > prev)
}

// -- Encoding helpers --------------------------------------------------------

fn collect_assert_symbols<'a>(a: &'a HAssert, acc: &mut Vec<&'a str>) {
    match a {
        HAssert::True | HAssert::False => {}
        HAssert::Not(x) | HAssert::Ex(x) | HAssert::All(x) => collect_assert_symbols(x, acc),
        HAssert::And(l, r) | HAssert::Imp(l, r) | HAssert::Or(l, r) => {
            collect_assert_symbols(l, acc);
            collect_assert_symbols(r, acc);
        }
        HAssert::TermEq(l, r) => {
            collect_term_symbols(l, acc);
            collect_term_symbols(r, acc);
        }
        HAssert::HasType(t, _) | HAssert::Defined(t) => collect_term_symbols(t, acc),
        HAssert::AppOk(f, args) => {
            acc.push(f.0.as_str());
            for arg in args {
                collect_term_symbols(arg, acc);
            }
        }
    }
}

fn collect_term_symbols<'a>(t: &'a HTerm, acc: &mut Vec<&'a str>) {
    match t {
        HTerm::Const(_) | HTerm::LVar(_) | HTerm::Local(_) => {}
        HTerm::App(f, args) => {
            acc.push(f.0.as_str());
            for arg in args {
                collect_term_symbols(arg, acc);
            }
        }
        HTerm::Binop(_, _, l, r) | HTerm::Relop(_, _, l, r) => {
            collect_term_symbols(l, acc);
            collect_term_symbols(r, acc);
        }
    }
}

/// Writes an entry's kind byte (the [`SpecKind`] declaration-order tag) and,
/// for the reachability kinds, the metadata block that follows it.
fn encode_kind(kind: &SpecKind, out: &mut Vec<u8>) {
    match kind {
        SpecKind::Forall => out.push(0x00),
        SpecKind::Exists(meta) => {
            out.push(0x01);
            encode_reach_meta(meta, out);
        }
        SpecKind::Unique(meta) => {
            out.push(0x02);
            encode_reach_meta(meta, out);
        }
    }
}

fn encode_reach_meta(meta: &ReachMeta, out: &mut Vec<u8>) {
    write_u32(out, meta.entry_arity);
    write_u32(out, count(meta.visible_locs.len()));
    for &loc in &meta.visible_locs {
        write_u32(out, loc);
    }
}

fn encode_assert(a: &HAssert, symbols: &[&str], out: &mut Vec<u8>) {
    match a {
        HAssert::True => out.push(0x00),
        HAssert::False => out.push(0x01),
        HAssert::Not(x) => {
            out.push(0x02);
            encode_assert(x, symbols, out);
        }
        HAssert::And(l, r) => {
            out.push(0x03);
            encode_assert(l, symbols, out);
            encode_assert(r, symbols, out);
        }
        HAssert::Imp(l, r) => {
            out.push(0x04);
            encode_assert(l, symbols, out);
            encode_assert(r, symbols, out);
        }
        HAssert::Or(l, r) => {
            out.push(0x05);
            encode_assert(l, symbols, out);
            encode_assert(r, symbols, out);
        }
        HAssert::Ex(x) => {
            out.push(0x06);
            encode_assert(x, symbols, out);
        }
        HAssert::TermEq(l, r) => {
            out.push(0x07);
            encode_term(l, symbols, out);
            encode_term(r, symbols, out);
        }
        HAssert::HasType(t, ty) => {
            out.push(0x08);
            encode_term(t, symbols, out);
            out.push(numtype_tag(*ty));
        }
        HAssert::Defined(t) => {
            out.push(0x09);
            encode_term(t, symbols, out);
        }
        HAssert::AppOk(f, args) => {
            out.push(0x0A);
            write_u32(out, sym_index(symbols, &f.0));
            write_u32(out, count(args.len()));
            for arg in args {
                encode_term(arg, symbols, out);
            }
        }
        HAssert::All(x) => {
            out.push(0x0B);
            encode_assert(x, symbols, out);
        }
    }
}

fn encode_term(t: &HTerm, symbols: &[&str], out: &mut Vec<u8>) {
    match t {
        HTerm::Const(c) => {
            out.push(0x00);
            encode_const(*c, out);
        }
        HTerm::LVar(i) => {
            out.push(0x01);
            write_u32(out, *i);
        }
        HTerm::Local(i) => {
            out.push(0x02);
            write_u32(out, *i);
        }
        HTerm::App(f, args) => {
            out.push(0x03);
            write_u32(out, sym_index(symbols, &f.0));
            write_u32(out, count(args.len()));
            for arg in args {
                encode_term(arg, symbols, out);
            }
        }
        HTerm::Binop(ty, op, l, r) => {
            out.push(0x04);
            out.push(numtype_tag(*ty));
            out.push(binop_tag(*op));
            encode_term(l, symbols, out);
            encode_term(r, symbols, out);
        }
        HTerm::Relop(ty, op, l, r) => {
            out.push(0x05);
            out.push(numtype_tag(*ty));
            out.push(relop_tag(*op));
            encode_term(l, symbols, out);
            encode_term(r, symbols, out);
        }
    }
}

fn encode_const(c: HConst, out: &mut Vec<u8>) {
    match c {
        HConst::I32(v) => {
            out.push(0x00);
            write_i64(out, i64::from(v));
        }
        HConst::I64(v) => {
            out.push(0x01);
            write_i64(out, v);
        }
    }
}

fn numtype_tag(ty: HNumType) -> u8 {
    match ty {
        HNumType::I32 => 0x00,
        HNumType::I64 => 0x01,
    }
}

fn binop_tag(op: HBinop) -> u8 {
    match op {
        HBinop::Add => 0x00,
        HBinop::Sub => 0x01,
        HBinop::Mul => 0x02,
        HBinop::DivS => 0x03,
        HBinop::DivU => 0x04,
        HBinop::RemS => 0x05,
        HBinop::RemU => 0x06,
        HBinop::And => 0x07,
        HBinop::Or => 0x08,
        HBinop::Xor => 0x09,
        HBinop::Shl => 0x0A,
        HBinop::ShrS => 0x0B,
        HBinop::ShrU => 0x0C,
    }
}

fn relop_tag(op: HRelop) -> u8 {
    match op {
        HRelop::Eq => 0x00,
        HRelop::Ne => 0x01,
        HRelop::LtS => 0x02,
        HRelop::LtU => 0x03,
        HRelop::GtS => 0x04,
        HRelop::GtU => 0x05,
        HRelop::LeS => 0x06,
        HRelop::LeU => 0x07,
        HRelop::GeS => 0x08,
        HRelop::GeU => 0x09,
    }
}

/// Looks up a symbol's index in the sorted table. The table is the union of
/// every referenced symbol, so the lookup never misses.
fn sym_index(symbols: &[&str], name: &str) -> u32 {
    count(
        symbols
            .binary_search(&name)
            .expect("every referenced symbol is in the table"),
    )
}

/// Narrows a container length to the `u32` the wire format uses. A real module
/// never approaches `u32::MAX` symbols/specs/entries/arguments.
fn count(len: usize) -> u32 {
    u32::try_from(len).expect("count exceeds the u32 wire width")
}

fn write_u32(out: &mut Vec<u8>, v: u32) {
    leb128::write::unsigned(out, u64::from(v)).expect("writing to a Vec is infallible");
}

fn write_i64(out: &mut Vec<u8>, v: i64) {
    leb128::write::signed(out, v).expect("writing to a Vec is infallible");
}

fn write_str(out: &mut Vec<u8>, s: &str) {
    write_u32(out, count(s.len()));
    out.extend_from_slice(s.as_bytes());
}

/// Distinguishes running out of bytes (an EOF from the underlying slice
/// reader, i.e. truncation) from a genuinely malformed varint whose value
/// overflows `u64`.
fn map_leb_err(e: &leb128::read::Error) -> DecodeError {
    match e {
        leb128::read::Error::IoError(_) => DecodeError::Truncated,
        leb128::read::Error::Overflow => DecodeError::Leb128,
    }
}

/// A bounds-checked cursor over the payload. Every read either advances within
/// bounds or returns a [`DecodeError`]; no method can panic on malformed input.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    fn read_u8(&mut self) -> Result<u8, DecodeError> {
        let b = *self.data.get(self.pos).ok_or(DecodeError::Truncated)?;
        self.pos += 1;
        Ok(b)
    }

    fn read_u32(&mut self) -> Result<u32, DecodeError> {
        let mut cursor = &self.data[self.pos..];
        let before = cursor.len();
        let value = leb128::read::unsigned(&mut cursor).map_err(|e| map_leb_err(&e))?;
        self.pos += before - cursor.len();
        u32::try_from(value).map_err(|_| DecodeError::IntOverflow)
    }

    fn read_i64(&mut self) -> Result<i64, DecodeError> {
        let mut cursor = &self.data[self.pos..];
        let before = cursor.len();
        let value = leb128::read::signed(&mut cursor).map_err(|e| map_leb_err(&e))?;
        self.pos += before - cursor.len();
        Ok(value)
    }

    /// Reads a length-prefixed UTF-8 name, enforcing the length cap before the
    /// bounds check so an over-advertised length is rejected without touching
    /// memory, and requiring the name be non-empty.
    fn read_name(&mut self) -> Result<String, DecodeError> {
        let len = self.read_u32()? as usize;
        if len == 0 {
            return Err(DecodeError::EmptyName);
        }
        if len > MAX_NAME_LEN {
            return Err(DecodeError::NameTooLong(len));
        }
        if len > self.remaining() {
            return Err(DecodeError::Truncated);
        }
        let bytes = &self.data[self.pos..self.pos + len];
        let name = std::str::from_utf8(bytes)
            .map_err(|_| DecodeError::InvalidUtf8)?
            .to_string();
        self.pos += len;
        Ok(name)
    }
}

/// Why an [`HSpecMap`] would not survive an [`encode`]/[`decode`] round-trip:
/// the ways it can violate the decoder's input contract that [`validate`]
/// checks before [`encode`] serializes it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PayloadError {
    /// A spec name (map key) is empty or exceeds [`MAX_NAME_LEN`] bytes.
    #[error(
        "spec name {name:?} has invalid length {len} \
         (inference.hspecs names must be non-empty and within the byte cap)"
    )]
    SpecName { name: String, len: usize },
    /// A function symbol — an obligation's own symbol or one referenced by an
    /// `App`/`AppOk` inside its tree — is empty or exceeds [`MAX_NAME_LEN`]
    /// bytes.
    #[error(
        "function symbol {symbol:?} in spec {spec:?} has invalid length {len} \
         (inference.hspecs names must be non-empty and within the byte cap)"
    )]
    FunctionSymbol {
        spec: String,
        symbol: String,
        len: usize,
    },
    /// An obligation's assertion/term tree nests past [`MAX_TREE_DEPTH`].
    #[error(
        "the obligation for {function:?} in spec {spec:?} nests past the inference.hspecs depth cap"
    )]
    TreeTooDeep { spec: String, function: String },
    /// A reachability entry lists more `visible_locs` than
    /// [`MAX_VISIBLE_LOCS`].
    #[error(
        "the reachability metadata for {function:?} in spec {spec:?} lists {count} visible \
         locals, past the inference.hspecs cap"
    )]
    TooManyVisibleLocs {
        spec: String,
        function: String,
        count: usize,
    },
    /// A reachability entry names a visible local past [`MAX_VISIBLE_LOCS`].
    #[error(
        "the reachability metadata for {function:?} in spec {spec:?} names visible local {loc}, \
         past the inference.hspecs cap"
    )]
    VisibleLocOutOfRange {
        spec: String,
        function: String,
        loc: u32,
    },
    /// A reachability entry's `visible_locs` are not strictly ascending
    /// (which also rejects duplicates).
    #[error(
        "the reachability metadata for {function:?} in spec {spec:?} has visible locals out of \
         strictly ascending order"
    )]
    VisibleLocsNotAscending { spec: String, function: String },
}

/// Every way an `inference.hspecs` payload can be malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("unsupported inference.hspecs section version {0}")]
    UnsupportedVersion(u32),
    #[error("inference.hspecs payload is truncated")]
    Truncated,
    #[error("malformed LEB128 in inference.hspecs payload")]
    Leb128,
    #[error("integer in inference.hspecs payload exceeds the u32 wire width")]
    IntOverflow,
    #[error("declared {kind} count {count} exceeds the remaining payload")]
    CountExceedsPayload { kind: &'static str, count: u32 },
    #[error("name length {0} exceeds the inference.hspecs name cap")]
    NameTooLong(usize),
    #[error("empty name in inference.hspecs payload")]
    EmptyName,
    #[error("invalid UTF-8 in an inference.hspecs name")]
    InvalidUtf8,
    #[error("inference.hspecs symbol table is not strictly ascending")]
    SymbolsNotAscending,
    #[error("inference.hspecs spec names are not strictly ascending")]
    SpecNamesNotAscending,
    #[error("symbol index {0} is out of range of the symbol table")]
    SymbolIndexOutOfRange(u32),
    #[error("unknown spec-kind tag {0:#04x}")]
    UnknownSpecKindTag(u8),
    #[error("reachability metadata lists {0} visible locals, past the inference.hspecs cap")]
    TooManyVisibleLocs(u32),
    #[error("reachability metadata names visible local {0}, past the inference.hspecs cap")]
    VisibleLocOutOfRange(u32),
    #[error("reachability visible locals are not strictly ascending")]
    VisibleLocsNotAscending,
    #[error("unknown hassert tag {0:#04x}")]
    UnknownHassertTag(u8),
    #[error("unknown term tag {0:#04x}")]
    UnknownTermTag(u8),
    #[error("unknown const tag {0:#04x}")]
    UnknownConstTag(u8),
    #[error("unknown number-type tag {0:#04x}")]
    UnknownNumType(u8),
    #[error("unknown binop tag {0:#04x}")]
    UnknownBinop(u8),
    #[error("unknown relop tag {0:#04x}")]
    UnknownRelop(u8),
    #[error("i32 constant value out of range")]
    ConstOutOfRange,
    #[error("assertion or term nesting exceeds the inference.hspecs depth cap")]
    TreeTooDeep,
    #[error("{0} trailing byte(s) after the declared payload")]
    TrailingBytes(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn href(name: &str) -> HFnRef {
        HFnRef(name.to_string())
    }

    fn map_of(entries: Vec<(&str, Vec<HSpecEntry>)>) -> HSpecMap {
        entries
            .into_iter()
            .map(|(name, es)| (name.to_string(), es))
            .collect()
    }

    /// A universally quantified entry — the common case throughout the suite.
    fn forall(name: &str, hassert: HAssert) -> HSpecEntry {
        HSpecEntry::new(href(name), hassert, SpecKind::Forall)
    }

    fn reach(entry_arity: u32, visible_locs: &[u32]) -> ReachMeta {
        ReachMeta {
            entry_arity,
            visible_locs: visible_locs.to_vec(),
        }
    }

    /// A tree exercising every `HAssert`, `HTerm`, `HConst`, binop, relop, and
    /// number type, plus `App`/`AppOk` symbol references. Both binders appear,
    /// nested one inside the other, so their tags cannot be swapped without
    /// this round trip noticing.
    fn kitchen_sink() -> HAssert {
        let call = HTerm::App(href("callee"), vec![HTerm::Local(0), HTerm::LVar(1)]);
        HAssert::And(
            Box::new(HAssert::Imp(
                Box::new(HAssert::Or(
                    Box::new(HAssert::Not(Box::new(HAssert::False))),
                    Box::new(HAssert::All(Box::new(HAssert::Ex(Box::new(
                        HAssert::Defined(HTerm::Const(HConst::I64(-9))),
                    ))))),
                )),
                Box::new(HAssert::HasType(
                    HTerm::Const(HConst::I32(-1)),
                    HNumType::I64,
                )),
            )),
            Box::new(HAssert::And(
                Box::new(HAssert::TermEq(
                    HTerm::Binop(
                        HNumType::I32,
                        HBinop::ShrU,
                        Box::new(HTerm::Local(2)),
                        Box::new(call.clone()),
                    ),
                    HTerm::Relop(
                        HNumType::I64,
                        HRelop::GeU,
                        Box::new(HTerm::Const(HConst::I64(7))),
                        Box::new(HTerm::LVar(0)),
                    ),
                )),
                Box::new(HAssert::AppOk(
                    href("sink"),
                    vec![HTerm::Const(HConst::I32(0)), call],
                )),
            )),
        )
    }

    /// A term spanning every binop and relop so the tag tables round-trip in
    /// full.
    fn every_operator() -> HAssert {
        let binops = [
            HBinop::Add,
            HBinop::Sub,
            HBinop::Mul,
            HBinop::DivS,
            HBinop::DivU,
            HBinop::RemS,
            HBinop::RemU,
            HBinop::And,
            HBinop::Or,
            HBinop::Xor,
            HBinop::Shl,
            HBinop::ShrS,
            HBinop::ShrU,
        ];
        let relops = [
            HRelop::Eq,
            HRelop::Ne,
            HRelop::LtS,
            HRelop::LtU,
            HRelop::GtS,
            HRelop::GtU,
            HRelop::LeS,
            HRelop::LeU,
            HRelop::GeS,
            HRelop::GeU,
        ];
        let mut a = HAssert::True;
        for op in binops {
            let t = HTerm::Binop(
                HNumType::I64,
                op,
                Box::new(HTerm::Local(0)),
                Box::new(HTerm::Local(1)),
            );
            a = HAssert::and(a, HAssert::eqz(t));
        }
        for op in relops {
            let t = HTerm::Relop(
                HNumType::I32,
                op,
                Box::new(HTerm::Local(0)),
                Box::new(HTerm::Local(1)),
            );
            a = HAssert::and(a, HAssert::nz(t));
        }
        a
    }

    fn nest(n: usize) -> HAssert {
        let mut a = HAssert::True;
        for _ in 1..n {
            a = HAssert::Not(Box::new(a));
        }
        a
    }

    #[test]
    fn empty_map_round_trips_to_a_minimal_payload() {
        let map = HSpecMap::default();
        let bytes = encode(&map);
        // version=2, sym_count=0, spec_count=0
        assert_eq!(bytes, vec![2, 0, 0]);
        assert_eq!(decode(&bytes).unwrap(), map);
    }

    #[test]
    fn empty_spec_list_round_trips() {
        let map = map_of(vec![("S", vec![])]);
        let bytes = encode(&map);
        // version=2, sym_count=0, spec_count=1, name_len=1 'S', entry_count=0
        assert_eq!(bytes, vec![2, 0, 1, 1, b'S', 0]);
        assert_eq!(decode(&bytes).unwrap(), map);
    }

    #[test]
    fn kitchen_sink_round_trips() {
        // The two exhaustive trees under all three quantifier kinds, so every
        // tag table round-trips under every kind byte.
        let map = map_of(vec![(
            "props",
            vec![
                forall("first", kitchen_sink()),
                HSpecEntry::new(
                    href("second"),
                    every_operator(),
                    SpecKind::Exists(reach(0, &[])),
                ),
                HSpecEntry::new(
                    href("third"),
                    kitchen_sink(),
                    SpecKind::Unique(reach(2, &[0, 1, 5])),
                ),
            ],
        )]);
        assert_eq!(decode(&encode(&map)).unwrap(), map);
    }

    #[test]
    fn multi_spec_multi_entry_round_trips() {
        let map = map_of(vec![
            (
                "alpha",
                vec![
                    forall(
                        "a_fn",
                        HAssert::nz(HTerm::App(href("z_fn"), vec![HTerm::Local(0)])),
                    ),
                    forall("b_fn", HAssert::True),
                ],
            ),
            (
                "beta",
                vec![HSpecEntry::new(
                    href("c_fn"),
                    HAssert::AppOk(href("a_fn"), vec![]),
                    SpecKind::Exists(reach(1, &[0])),
                )],
            ),
        ]);
        assert_eq!(decode(&encode(&map)).unwrap(), map);
    }

    #[test]
    fn encoding_is_deterministic_across_insertion_order() {
        // A reachability kind, so canonicality covers the metadata block too.
        let entry = |s: &str| {
            HSpecEntry::new(
                href(s),
                HAssert::nz(HTerm::App(href("shared"), vec![])),
                SpecKind::Unique(reach(1, &[0, 3])),
            )
        };
        let mut forward = HSpecMap::default();
        forward.insert("aaa".to_string(), vec![entry("m1")]);
        forward.insert("zzz".to_string(), vec![entry("m2")]);

        let mut backward = HSpecMap::default();
        backward.insert("zzz".to_string(), vec![entry("m2")]);
        backward.insert("aaa".to_string(), vec![entry("m1")]);

        assert_eq!(encode(&forward), encode(&backward));
    }

    #[test]
    fn symbol_table_is_sorted_and_deduplicated() {
        // `beta` is referenced by an entry symbol and inside a tree; `alpha`
        // only inside a tree. The table must be sorted and hold each once.
        let map = map_of(vec![(
            "s",
            vec![forall(
                "beta",
                HAssert::And(
                    Box::new(HAssert::AppOk(href("alpha"), vec![])),
                    Box::new(HAssert::AppOk(href("beta"), vec![])),
                ),
            )],
        )]);
        let bytes = encode(&map);
        // version=2, sym_count=2, len=5 "alpha", len=4 "beta", ...
        let mut expected = vec![2u8, 2, 5];
        expected.extend_from_slice(b"alpha");
        expected.push(4);
        expected.extend_from_slice(b"beta");
        assert_eq!(&bytes[..expected.len()], &expected[..]);
        assert_eq!(decode(&bytes).unwrap(), map);
    }

    #[test]
    fn accepts_a_tree_at_the_depth_cap() {
        let map = map_of(vec![("s", vec![forall("f", nest(MAX_TREE_DEPTH))])]);
        assert_eq!(decode(&encode(&map)).unwrap(), map);
    }

    #[test]
    fn rejects_a_tree_beyond_the_depth_cap() {
        // Hand-built payload: spec "s", entry symbol "f" (table index 0), the
        // Forall kind byte, then a `Not` spine one level past the cap. Built
        // directly rather than via `encode`, which now refuses an over-deep
        // tree by contract (see `encode_panics_on_a_tree_beyond_the_depth_cap`).
        let mut bytes = vec![2, 1, 1, b'f', 1, 1, b's', 1, 0, 0x00];
        bytes.resize(bytes.len() + MAX_TREE_DEPTH, 0x02); // MAX_TREE_DEPTH `Not` tags
        bytes.push(0x00); // a `True` leaf: total depth MAX_TREE_DEPTH + 1
        assert_eq!(decode(&bytes), Err(DecodeError::TreeTooDeep));
    }

    /// The universal binder costs a level exactly as every other one-child node
    /// does, on both the decode and the `validate` side. Built as a spine of
    /// `All`s so a node the depth walkers forgot to count would let a tree past
    /// the cap through.
    #[test]
    fn the_universal_binder_counts_a_depth_level() {
        let all_spine = |n: usize| (1..n).fold(HAssert::True, |acc, _| HAssert::All(Box::new(acc)));
        let at_cap = map_of(vec![("s", vec![forall("f", all_spine(MAX_TREE_DEPTH))])]);
        assert_eq!(validate(&at_cap), Ok(()));
        assert_eq!(decode(&encode(&at_cap)).unwrap(), at_cap);

        let past_cap = map_of(vec![(
            "s",
            vec![forall("f", all_spine(MAX_TREE_DEPTH + 1))],
        )]);
        assert_eq!(
            validate(&past_cap),
            Err(PayloadError::TreeTooDeep {
                spec: "s".to_string(),
                function: "f".to_string(),
            })
        );

        // Decode past the cap is reached through a hand-built spine, since
        // `encode` refuses an over-deep tree by contract. `validate` is an
        // independent walker, so only this half can catch a decode arm that
        // recurses without counting its level — the case that turns a crafted
        // payload into unbounded recursion instead of `TreeTooDeep`.
        let mut bytes = vec![2, 1, 1, b'f', 1, 1, b's', 1, 0, 0x00];
        bytes.resize(bytes.len() + MAX_TREE_DEPTH, 0x0B);
        bytes.push(0x00);
        assert_eq!(decode(&bytes), Err(DecodeError::TreeTooDeep));
    }

    /// The assertion tag table is closed at the universal binder: the first
    /// unassigned value must still be a clean rejection, so a payload written by
    /// a *newer* producer fails loudly here instead of being misparsed.
    #[test]
    fn rejects_the_first_unassigned_hassert_tag() {
        assert_eq!(
            decode(&[2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x00, 0x0C]),
            Err(DecodeError::UnknownHassertTag(0x0C))
        );
    }

    #[test]
    fn rejects_an_unsupported_version() {
        // The sentinel must stay one past the current version: when the format
        // is bumped again, a stale sentinel equal to the new version would
        // invert this test into a false green.
        assert_eq!(decode(&[3, 0, 0]), Err(DecodeError::UnsupportedVersion(3)));
    }

    #[test]
    fn rejects_the_superseded_version_one() {
        // A v1 payload (no per-entry kind byte) must be rejected loudly, not
        // misparsed: strict version equality is the compatibility story.
        assert_eq!(decode(&[1, 0, 0]), Err(DecodeError::UnsupportedVersion(1)));
    }

    #[test]
    fn rejects_a_truncated_version() {
        assert_eq!(decode(&[]), Err(DecodeError::Truncated));
    }

    #[test]
    fn rejects_truncation_mid_tree() {
        // A valid prefix whose final entry's tree tag is missing.
        let map = map_of(vec![("s", vec![forall("f", HAssert::True)])]);
        let mut bytes = encode(&map);
        bytes.pop(); // drop the `True` tag byte
        assert_eq!(decode(&bytes), Err(DecodeError::Truncated));
    }

    #[test]
    fn rejects_an_over_advertised_symbol_count() {
        // version=2, sym_count=255 in a payload with far fewer bytes.
        assert_eq!(
            decode(&[2, 255, 1]),
            Err(DecodeError::CountExceedsPayload {
                kind: "symbol",
                count: 255
            })
        );
    }

    #[test]
    fn rejects_an_over_advertised_spec_count() {
        // version=2, sym_count=0, spec_count=255, then nothing.
        assert_eq!(
            decode(&[2, 0, 255, 1]),
            Err(DecodeError::CountExceedsPayload {
                kind: "spec",
                count: 255
            })
        );
    }

    #[test]
    fn rejects_an_over_advertised_entry_count() {
        // version=2, sym_count=0, spec_count=1, name "S", entry_count=255.
        assert_eq!(
            decode(&[2, 0, 1, 1, b'S', 255, 1]),
            Err(DecodeError::CountExceedsPayload {
                kind: "entry",
                count: 255
            })
        );
    }

    #[test]
    fn rejects_an_over_advertised_arg_count() {
        // A well-formed entry whose AppOk arg count is then overwritten to a
        // value larger than the remaining payload.
        let map = map_of(vec![(
            "s",
            vec![forall(
                "f",
                HAssert::AppOk(href("g"), vec![HTerm::Local(0)]),
            )],
        )]);
        let bytes = encode(&map);
        // The last two bytes are: arg_count(=1), then Local tag(0x02)+idx. Find
        // the arg_count byte (the `1` before the trailing `0x02 0x00`) and bump
        // it far past the remaining bytes.
        let arg_count_pos = bytes.len() - 3;
        assert_eq!(bytes[arg_count_pos], 1, "arg_count byte located");
        let mut corrupt = bytes.clone();
        corrupt[arg_count_pos] = 255;
        assert!(matches!(
            decode(&corrupt),
            Err(DecodeError::CountExceedsPayload {
                kind: "argument",
                ..
            })
        ));
    }

    #[test]
    fn rejects_invalid_utf8_in_a_name() {
        // version=2, sym_count=1, name_len=1, 0xFF (not valid UTF-8).
        assert_eq!(decode(&[2, 1, 1, 0xFF]), Err(DecodeError::InvalidUtf8));
    }

    #[test]
    fn rejects_an_over_long_name() {
        // version=2, sym_count=1, then an advertised name length one past the
        // cap (LEB-encoded, so the test tracks the constant) with no bytes: the
        // cap is checked before the payload-length bound, so no name body is
        // needed.
        let mut bytes = vec![2, 1];
        leb128::write::unsigned(&mut bytes, (MAX_NAME_LEN + 1) as u64)
            .expect("writing to a Vec is infallible");
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::NameTooLong(MAX_NAME_LEN + 1))
        );
    }

    #[test]
    fn rejects_an_empty_name() {
        // version=2, sym_count=1, name_len=0.
        assert_eq!(decode(&[2, 1, 0]), Err(DecodeError::EmptyName));
    }

    #[test]
    fn rejects_an_unsorted_symbol_table() {
        // version=2, sym_count=2, "b" then "a" — descending.
        assert_eq!(
            decode(&[2, 2, 1, b'b', 1, b'a']),
            Err(DecodeError::SymbolsNotAscending)
        );
    }

    #[test]
    fn rejects_a_duplicate_symbol() {
        // version=2, sym_count=2, "a" then "a" — not strictly ascending.
        assert_eq!(
            decode(&[2, 2, 1, b'a', 1, b'a']),
            Err(DecodeError::SymbolsNotAscending)
        );
    }

    #[test]
    fn rejects_unsorted_spec_names() {
        // sym_count=0, spec_count=2, "b"/0 entries then "a"/0 entries.
        assert_eq!(
            decode(&[2, 0, 2, 1, b'b', 0, 1, b'a', 0]),
            Err(DecodeError::SpecNamesNotAscending)
        );
    }

    #[test]
    fn rejects_a_duplicate_spec_name() {
        // sym_count=0, spec_count=2, "a"/0 entries twice.
        assert_eq!(
            decode(&[2, 0, 2, 1, b'a', 0, 1, b'a', 0]),
            Err(DecodeError::SpecNamesNotAscending)
        );
    }

    #[test]
    fn rejects_an_out_of_range_symbol_index() {
        // sym_count=0, spec_count=1, "S", entry_count=1, symbol_idx=0 (empty
        // table); the index is read before the kind byte, so none is needed.
        assert_eq!(
            decode(&[2, 0, 1, 1, b'S', 1, 0]),
            Err(DecodeError::SymbolIndexOutOfRange(0))
        );
    }

    #[test]
    fn rejects_an_unknown_hassert_tag() {
        // one symbol "f", one spec "S", one entry -> symbol_idx=0, kind=Forall,
        // tree tag=0x7F.
        assert_eq!(
            decode(&[2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x00, 0x7F]),
            Err(DecodeError::UnknownHassertTag(0x7F))
        );
    }

    #[test]
    fn rejects_an_unknown_term_tag() {
        // ... entry tree = Defined(<term tag 0x7F>).
        assert_eq!(
            decode(&[2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x00, 0x09, 0x7F]),
            Err(DecodeError::UnknownTermTag(0x7F))
        );
    }

    #[test]
    fn rejects_an_unknown_const_tag() {
        // ... tree = Defined(Const(<const tag 0x7F>)).
        assert_eq!(
            decode(&[2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x00, 0x09, 0x00, 0x7F]),
            Err(DecodeError::UnknownConstTag(0x7F))
        );
    }

    #[test]
    fn rejects_an_unknown_binop_tag() {
        // ... tree = Defined(Binop(I32, <binop 0x7F>, ...)).
        assert_eq!(
            decode(&[
                2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x00, 0x09, 0x04, 0x00, 0x7F
            ]),
            Err(DecodeError::UnknownBinop(0x7F))
        );
    }

    #[test]
    fn rejects_an_unknown_numtype_tag() {
        // ... tree = HasType(Local 0, <numtype 0x7F>).
        assert_eq!(
            decode(&[
                2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x00, 0x08, 0x02, 0x00, 0x7F
            ]),
            Err(DecodeError::UnknownNumType(0x7F))
        );
    }

    #[test]
    fn rejects_an_unknown_spec_kind_tag() {
        // ... symbol_idx=0, then a kind byte past the known range.
        assert_eq!(
            decode(&[2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x7F]),
            Err(DecodeError::UnknownSpecKindTag(0x7F))
        );
    }

    #[test]
    fn rejects_trailing_bytes() {
        let map = map_of(vec![("s", vec![forall("f", HAssert::True)])]);
        let mut bytes = encode(&map);
        bytes.extend_from_slice(&[0xAA, 0xBB]);
        assert_eq!(decode(&bytes), Err(DecodeError::TrailingBytes(2)));
    }

    #[test]
    fn rejects_a_u32_that_overflows() {
        // version encoded as a 5-byte LEB above u32::MAX.
        assert_eq!(
            decode(&[0x80, 0x80, 0x80, 0x80, 0x10]),
            Err(DecodeError::IntOverflow)
        );
    }

    #[test]
    fn i32_and_i64_const_extremes_round_trip() {
        let map = map_of(vec![(
            "s",
            vec![
                forall("f", HAssert::eqz(HTerm::Const(HConst::I32(i32::MIN)))),
                forall("g", HAssert::Defined(HTerm::Const(HConst::I64(i64::MAX)))),
                forall("h", HAssert::Defined(HTerm::Const(HConst::I64(i64::MIN)))),
            ],
        )]);
        assert_eq!(decode(&encode(&map)).unwrap(), map);
    }

    // -- reachability kinds: wire shape, round-trips, rejections -----------

    /// Pins the exact byte layout of a reachability entry: kind byte between
    /// the symbol index and the tree, then `entry_arity`, `locs_count`, and
    /// the ascending locs.
    #[test]
    fn reach_entry_wire_shape_is_exact() {
        let map = map_of(vec![(
            "S",
            vec![HSpecEntry::new(
                href("f"),
                HAssert::True,
                SpecKind::Exists(reach(2, &[0, 1, 5])),
            )],
        )]);
        let bytes = encode(&map);
        // version=2, sym_count=1, "f", spec_count=1, "S", entry_count=1,
        // symbol_idx=0, kind=Exists, entry_arity=2, locs_count=3, locs 0 1 5,
        // tree=True.
        assert_eq!(
            bytes,
            vec![2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x01, 2, 3, 0, 1, 5, 0x00]
        );
        assert_eq!(decode(&bytes).unwrap(), map);
    }

    /// The metadata matrix: `entry_arity` 0 and the full `u32` range (it is
    /// carried, never derived, so the wire must not narrow it), empty and
    /// non-empty `visible_locs`, and a loc at the cap — under both
    /// reachability kinds.
    #[test]
    fn reachability_metadata_round_trips_across_the_matrix() {
        let map = map_of(vec![(
            "s",
            vec![
                HSpecEntry::new(href("a"), HAssert::False, SpecKind::Exists(reach(0, &[]))),
                HSpecEntry::new(
                    href("b"),
                    HAssert::Defined(HTerm::Local(0)),
                    SpecKind::Exists(reach(u32::MAX, &[MAX_VISIBLE_LOCS])),
                ),
                HSpecEntry::new(
                    href("c"),
                    HAssert::nz(HTerm::Local(2)),
                    SpecKind::Unique(reach(3, &[0, 1, 2, 7])),
                ),
                HSpecEntry::new(href("d"), HAssert::False, SpecKind::Unique(reach(1, &[]))),
            ],
        )]);
        assert_eq!(decode(&encode(&map)).unwrap(), map);
    }

    /// The linker's carry path decodes the main module's payload and re-encodes
    /// it; canonical encoding makes that byte-identical, and the kind byte and
    /// metadata block must round-trip inside that identity.
    #[test]
    fn reencoding_a_decoded_kind_bearing_payload_is_byte_identical() {
        let map = map_of(vec![(
            "s",
            vec![
                forall("f", HAssert::nz(HTerm::Local(0))),
                HSpecEntry::new(
                    href("g"),
                    HAssert::eqz(HTerm::Local(1)),
                    SpecKind::Unique(reach(2, &[0, 1])),
                ),
            ],
        )]);
        let bytes = encode(&map);
        let reencoded = encode(&decode(&bytes).expect("canonical payload decodes"));
        assert_eq!(reencoded, bytes);
    }

    #[test]
    fn rejects_unsorted_visible_locs() {
        // ... kind=Exists, arity=0, locs_count=2, locs 5 then 3 — descending.
        assert_eq!(
            decode(&[2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x01, 0, 2, 5, 3]),
            Err(DecodeError::VisibleLocsNotAscending)
        );
    }

    #[test]
    fn rejects_duplicate_visible_locs() {
        // ... kind=Unique, arity=0, locs_count=2, locs 4 4 — not strictly
        // ascending.
        assert_eq!(
            decode(&[2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x02, 0, 2, 4, 4]),
            Err(DecodeError::VisibleLocsNotAscending)
        );
    }

    #[test]
    fn rejects_a_visible_locs_count_past_the_cap() {
        // ... kind=Unique, arity=0, then a locs count one past the cap
        // (LEB-encoded so the test tracks the constant). The cap is checked
        // before the payload-length bound, so no locs bytes are needed.
        let mut bytes = vec![2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x02, 0];
        leb128::write::unsigned(&mut bytes, u64::from(MAX_VISIBLE_LOCS) + 1)
            .expect("writing to a Vec is infallible");
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::TooManyVisibleLocs(MAX_VISIBLE_LOCS + 1))
        );
    }

    #[test]
    fn rejects_a_visible_loc_past_the_cap() {
        // ... kind=Exists, arity=0, locs_count=1, then a loc one past the cap.
        let mut bytes = vec![2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x01, 0, 1];
        leb128::write::unsigned(&mut bytes, u64::from(MAX_VISIBLE_LOCS) + 1)
            .expect("writing to a Vec is infallible");
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::VisibleLocOutOfRange(MAX_VISIBLE_LOCS + 1))
        );
    }

    #[test]
    fn rejects_an_over_advertised_visible_locs_count() {
        // ... kind=Exists, arity=0, locs_count=255 (two LEB bytes) with no
        // bytes left: within the cap, but past the remaining payload.
        assert_eq!(
            decode(&[2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x01, 0, 255, 1]),
            Err(DecodeError::CountExceedsPayload {
                kind: "visible-local",
                count: 255
            })
        );
    }

    #[test]
    fn rejects_reach_metadata_truncated_after_the_kind_byte() {
        // The payload ends where `entry_arity` should begin.
        assert_eq!(
            decode(&[2, 1, 1, b'f', 1, 1, b'S', 1, 0, 0x01]),
            Err(DecodeError::Truncated)
        );
    }

    // -- validate: the encode-side contract -------------------------------

    #[test]
    fn validate_accepts_a_well_formed_map() {
        let map = map_of(vec![(
            "props",
            vec![
                forall("first", kitchen_sink()),
                HSpecEntry::new(
                    href("second"),
                    every_operator(),
                    SpecKind::Exists(reach(2, &[0, 1])),
                ),
            ],
        )]);
        assert_eq!(validate(&map), Ok(()));
    }

    #[test]
    fn validate_accepts_names_at_the_length_cap() {
        let name = "a".repeat(MAX_NAME_LEN);
        let mut map = HSpecMap::default();
        map.insert(
            name.clone(),
            vec![HSpecEntry::new(
                HFnRef(name.clone()),
                HAssert::AppOk(HFnRef(name), vec![]),
                SpecKind::Forall,
            )],
        );
        assert_eq!(validate(&map), Ok(()));
        // ...and the whole thing round-trips, so the cap is truly the boundary.
        assert_eq!(decode(&encode(&map)).unwrap(), map);
    }

    #[test]
    fn validate_rejects_an_over_long_spec_name() {
        let name = "a".repeat(MAX_NAME_LEN + 1);
        let map = map_of(vec![(name.as_str(), vec![forall("f", HAssert::True)])]);
        assert_eq!(
            validate(&map),
            Err(PayloadError::SpecName {
                name,
                len: MAX_NAME_LEN + 1,
            })
        );
    }

    #[test]
    fn validate_rejects_an_empty_spec_name() {
        let map = map_of(vec![("", vec![forall("f", HAssert::True)])]);
        assert_eq!(
            validate(&map),
            Err(PayloadError::SpecName {
                name: String::new(),
                len: 0,
            })
        );
    }

    #[test]
    fn validate_rejects_an_over_long_entry_symbol() {
        let symbol = "z".repeat(MAX_NAME_LEN + 1);
        let map = map_of(vec![(
            "s",
            vec![HSpecEntry::new(
                HFnRef(symbol.clone()),
                HAssert::True,
                SpecKind::Forall,
            )],
        )]);
        assert_eq!(
            validate(&map),
            Err(PayloadError::FunctionSymbol {
                spec: "s".to_string(),
                symbol,
                len: MAX_NAME_LEN + 1,
            })
        );
    }

    #[test]
    fn validate_rejects_an_empty_entry_symbol() {
        let map = map_of(vec![("s", vec![forall("", HAssert::True)])]);
        assert_eq!(
            validate(&map),
            Err(PayloadError::FunctionSymbol {
                spec: "s".to_string(),
                symbol: String::new(),
                len: 0,
            })
        );
    }

    #[test]
    fn validate_rejects_an_over_long_symbol_referenced_in_a_tree() {
        // The entry's own symbol is fine, but an `App` inside its tree names an
        // over-long symbol — which the decoder would also reject, since it
        // enters the symbol table.
        let callee = "c".repeat(MAX_NAME_LEN + 1);
        let map = map_of(vec![(
            "s",
            vec![forall(
                "f",
                HAssert::nz(HTerm::App(HFnRef(callee.clone()), vec![HTerm::Local(0)])),
            )],
        )]);
        assert_eq!(
            validate(&map),
            Err(PayloadError::FunctionSymbol {
                spec: "s".to_string(),
                symbol: callee,
                len: MAX_NAME_LEN + 1,
            })
        );
    }

    #[test]
    fn validate_accepts_a_tree_at_the_depth_cap() {
        let map = map_of(vec![("s", vec![forall("f", nest(MAX_TREE_DEPTH))])]);
        assert_eq!(validate(&map), Ok(()));
    }

    #[test]
    fn validate_rejects_a_tree_beyond_the_depth_cap() {
        let map = map_of(vec![("s", vec![forall("f", nest(MAX_TREE_DEPTH + 1))])]);
        assert_eq!(
            validate(&map),
            Err(PayloadError::TreeTooDeep {
                spec: "s".to_string(),
                function: "f".to_string(),
            })
        );
    }

    /// Both caps are inclusive boundaries: a locs list at the count cap (whose
    /// values sit below it) and a single loc at the value cap must validate
    /// and round-trip.
    #[test]
    fn validate_accepts_reach_metadata_at_the_caps() {
        let full: Vec<u32> = (0..MAX_VISIBLE_LOCS).collect();
        let map = map_of(vec![(
            "s",
            vec![
                HSpecEntry::new(href("f"), HAssert::False, SpecKind::Exists(reach(0, &full))),
                HSpecEntry::new(
                    href("g"),
                    HAssert::False,
                    SpecKind::Unique(reach(0, &[MAX_VISIBLE_LOCS])),
                ),
            ],
        )]);
        assert_eq!(validate(&map), Ok(()));
        assert_eq!(decode(&encode(&map)).unwrap(), map);
    }

    #[test]
    fn validate_rejects_unsorted_visible_locs() {
        let map = map_of(vec![(
            "s",
            vec![HSpecEntry::new(
                href("f"),
                HAssert::False,
                SpecKind::Exists(reach(1, &[5, 3])),
            )],
        )]);
        assert_eq!(
            validate(&map),
            Err(PayloadError::VisibleLocsNotAscending {
                spec: "s".to_string(),
                function: "f".to_string(),
            })
        );
    }

    #[test]
    fn validate_rejects_duplicate_visible_locs() {
        let map = map_of(vec![(
            "s",
            vec![HSpecEntry::new(
                href("f"),
                HAssert::False,
                SpecKind::Unique(reach(1, &[4, 4])),
            )],
        )]);
        assert_eq!(
            validate(&map),
            Err(PayloadError::VisibleLocsNotAscending {
                spec: "s".to_string(),
                function: "f".to_string(),
            })
        );
    }

    #[test]
    fn validate_rejects_a_visible_locs_count_past_the_cap() {
        let over: Vec<u32> = (0..=MAX_VISIBLE_LOCS).collect();
        let map = map_of(vec![(
            "s",
            vec![HSpecEntry::new(
                href("f"),
                HAssert::False,
                SpecKind::Unique(reach(0, &over)),
            )],
        )]);
        assert_eq!(
            validate(&map),
            Err(PayloadError::TooManyVisibleLocs {
                spec: "s".to_string(),
                function: "f".to_string(),
                count: MAX_VISIBLE_LOCS as usize + 1,
            })
        );
    }

    #[test]
    fn validate_rejects_a_visible_loc_past_the_cap() {
        let map = map_of(vec![(
            "s",
            vec![HSpecEntry::new(
                href("f"),
                HAssert::False,
                SpecKind::Exists(reach(0, &[MAX_VISIBLE_LOCS + 1])),
            )],
        )]);
        assert_eq!(
            validate(&map),
            Err(PayloadError::VisibleLocOutOfRange {
                spec: "s".to_string(),
                function: "f".to_string(),
                loc: MAX_VISIBLE_LOCS + 1,
            })
        );
    }

    #[test]
    #[should_panic(expected = "depth cap")]
    fn encode_panics_on_a_tree_beyond_the_depth_cap() {
        let map = map_of(vec![("s", vec![forall("f", nest(MAX_TREE_DEPTH + 1))])]);
        let _ = encode(&map);
    }

    #[test]
    #[should_panic(expected = "invalid length")]
    fn encode_panics_on_an_over_long_name() {
        let name = "a".repeat(MAX_NAME_LEN + 1);
        let map = map_of(vec![(name.as_str(), vec![forall("f", HAssert::True)])]);
        let _ = encode(&map);
    }

    /// The load-bearing invariant for the linker's decode-then-re-encode path
    /// (`merge.rs`): anything `decode` accepts satisfies `validate`, so the
    /// re-encode can never trip `encode`'s contract panic. Exercised across the
    /// representative round-trip corpus.
    #[test]
    fn every_decoded_map_satisfies_validate() {
        let name_at_cap = "a".repeat(MAX_NAME_LEN);
        let corpus = vec![
            HSpecMap::default(),
            map_of(vec![("s", vec![])]),
            map_of(vec![(
                "props",
                vec![
                    forall("first", kitchen_sink()),
                    forall("second", every_operator()),
                ],
            )]),
            map_of(vec![("s", vec![forall("f", nest(MAX_TREE_DEPTH))])]),
            map_of(vec![(
                name_at_cap.as_str(),
                vec![HSpecEntry::new(
                    HFnRef(name_at_cap.clone()),
                    HAssert::True,
                    SpecKind::Forall,
                )],
            )]),
            map_of(vec![(
                "kinds",
                vec![
                    HSpecEntry::new(href("e"), kitchen_sink(), SpecKind::Exists(reach(0, &[]))),
                    HSpecEntry::new(
                        href("u"),
                        every_operator(),
                        SpecKind::Unique(reach(2, &[0, 1, MAX_VISIBLE_LOCS])),
                    ),
                ],
            )]),
        ];
        for map in corpus {
            let decoded = decode(&encode(&map)).expect("valid map round-trips");
            assert_eq!(validate(&decoded), Ok(()), "decoded map must validate");
        }
    }
}
