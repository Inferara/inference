//! Golden coverage for the receiver copy an immutable `self` gets when it
//! escapes to an `external fn` (issue #329).
//!
//! A `mut self` receiver has always been copied into the method's own frame on
//! entry; an immutable one was passed straight through as a pointer into the
//! caller's memory. That is invisible while nothing can write through the
//! pointer — and an `external fn` with a compound parameter is exactly something
//! that can, because a compound extern argument lowers to a raw address and a
//! linked module shares the caller's linear memory. A method forwarding `self`
//! to such an external let the foreign body mutate the *caller's* struct.
//!
//! The fixtures here pin both directions of the gate that closed it. Five drive
//! the escape through a different syntactic position — whole receiver,
//! sub-object, nested block, nested expression, scalar projection. A sixth puts
//! an escaping receiver in the same frame as a named compound parameter, the one
//! arrangement where an offset collision between the new receiver slot and the
//! parameter slots could hide. One holds the `mut self` control, which must come
//! out unchanged: the two conditions that earn a slot are disjunctive over a
//! single slot, so a receiver that is both mutable and escaping is copied once,
//! not twice. One holds the negative: an immutable receiver in a module that does
//! register an import, but never hands `self` to it, still gets no slot and no
//! copy.
//!
//! Two properties make the assertions sharp rather than tautological:
//!
//! - Every fixture declares **both** `external fn …;` and `use { … } from …;`.
//!   An unbound `external fn` carries no origin, is skipped by import
//!   registration, and leaves the import map the scan consults empty — a fixture
//!   missing the `use` would golden the *unfixed* bytes and pass.
//! - The marks are checked with `check_count!` in both directions, so the family
//!   states which decision each fixture exercises. The goldens alone cannot: a
//!   gate that stopped firing and one that fired for a different reason both
//!   produce bytes that match a regenerated golden.

#[cfg(test)]
mod self_extern_escape_tests {
    use crate::utils::{
        assert_wasms_modules_equivalence, assert_wat_equivalence, get_test_file_path,
        get_test_wasm_path, wasm_codegen,
    };

    /// Compiles a fixture, validates it, and asserts it matches its goldens.
    fn assert_golden(test_name: &str) -> Vec<u8> {
        let source_path = get_test_file_path(module_path!(), test_name);
        let source_code = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {source_path:?}"));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid for {test_name}: {e}"));
        let expected_path = get_test_wasm_path(module_path!(), test_name);
        let expected = std::fs::read(&expected_path)
            .unwrap_or_else(|_| panic!("Failed to read expected wasm file for test: {test_name}"));
        assert_wasms_modules_equivalence(&expected, &actual);
        assert_wat_equivalence(&actual, module_path!(), test_name);
        actual
    }

    /// Extracts one function's WAT text, from its `(func $name ` header to the
    /// closing paren at function indentation.
    fn function_wat(wasm: &[u8], name: &str) -> String {
        let wat = wasmprinter::print_bytes(wasm).expect("Failed to print WAT");
        let marker = format!("(func ${name} ");
        let start = wat
            .find(&marker)
            .unwrap_or_else(|| panic!("no function ${name} in the printed module:\n{wat}"));
        let rest = &wat[start..];
        let end = rest.find("\n  )").map_or(rest.len(), |offset| offset + 4);
        rest[..end].to_string()
    }

    /// Asserts that `name` allocates a frame, copies the receiver into it, and
    /// rebinds `self` to the copy before it calls anything.
    ///
    /// A region copy moves untyped bytes, so its accesses carry the conservative
    /// one-byte alignment hint that a typed field store never does — which is
    /// what tells the entry copy apart from the method body's own field stores.
    /// The width of that store follows the struct's size and is not the point,
    /// so it is left unpinned here; the goldens carry it.
    ///
    /// The rebind is the half that makes the copy matter, and its *position* is
    /// what makes it matter for this issue: rebinding after the call would leave
    /// the external holding the caller's pointer with a fully formed, entirely
    /// useless copy sitting in the frame beside it.
    fn assert_receiver_copied(wasm: &[u8], name: &str) {
        let body = function_wat(wasm, name);
        assert!(
            body.contains("__frame_ptr"),
            "{name} must allocate a frame to hold its own copy of the receiver:\n{body}"
        );
        assert!(
            body.contains("store align=1"),
            "{name} must copy the caller's struct into that frame, which a region \
             copy does through accesses carrying the one-byte alignment hint:\n{body}"
        );
        // Line positions rather than byte offsets: a call nested inside an `if`
        // is indented further than one at statement level, so matching on
        // leading whitespace would silently pass on the nested fixture.
        let line_of = |prefix: &str| {
            body.lines()
                .position(|line| line.trim_start().starts_with(prefix))
        };
        let rebind = line_of("local.set $self")
            .unwrap_or_else(|| panic!("{name} must rebind `self` to the copy:\n{body}"));
        let first_call =
            line_of("call ").unwrap_or_else(|| panic!("{name} must call something:\n{body}"));
        assert!(
            rebind < first_call,
            "{name} must rebind `self` to its own copy before handing the receiver \
             to a callee, or the copy is dead and the caller's pointer travels:\n{body}"
        );
    }

    /// Asserts that `name` reads the caller's struct in place: no frame, so no
    /// slot to copy into and nothing to rebind.
    fn assert_receiver_not_copied(wasm: &[u8], name: &str) {
        let body = function_wat(wasm, name);
        assert!(
            !body.contains("__frame_ptr"),
            "an immutable receiver nothing can write through is read in place, so \
             {name} must allocate no frame:\n{body}"
        );
        assert!(
            !body.contains("local.set $self"),
            "...and must not rebind `self`:\n{body}"
        );
    }

    /// `sort_pair(self)` — the shape from the issue. The whole receiver travels
    /// to the external as a pointer, so it must be a pointer to the method's own
    /// copy.
    #[test]
    fn escape_whole_self_test() {
        cov_mark::check_count!(wasm_codegen_self_escapes_to_extern, 1);
        cov_mark::check_count!(wasm_codegen_emit_self_copy_on_entry, 1);
        let wasm = assert_golden("escape_whole_self");
        assert_receiver_copied(&wasm, "Pair.touch");
    }

    /// `scramble(self.inner)` — a sub-object's address is an address inside the
    /// receiver, so the member access peels to `self` and the copy still lands.
    #[test]
    fn escape_sub_object_test() {
        cov_mark::check_count!(wasm_codegen_self_escapes_to_extern, 1);
        cov_mark::check_count!(wasm_codegen_emit_self_copy_on_entry, 1);
        let wasm = assert_golden("escape_sub_object");
        assert_receiver_copied(&wasm, "Outer.touch");
    }

    /// `if c { sort_pair(self); }` — the call is conditional, the copy is not.
    /// A scan that read only top-level statements would miss this and emit the
    /// pre-fix bytes.
    #[test]
    fn escape_nested_block_test() {
        cov_mark::check_count!(wasm_codegen_self_escapes_to_extern, 1);
        cov_mark::check_count!(wasm_codegen_emit_self_copy_on_entry, 1);
        let wasm = assert_golden("escape_nested_block");
        assert_receiver_copied(&wasm, "Pair.touch");
    }

    /// `let x: i32 = 1 + probe(self);` — the extern call is an operand, not a
    /// statement, and its result feeds a further native call. Pins that the
    /// expression descent reaches every position an extern call can occupy.
    #[test]
    fn escape_nested_expr_test() {
        cov_mark::check_count!(wasm_codegen_self_escapes_to_extern, 1);
        cov_mark::check_count!(wasm_codegen_emit_self_copy_on_entry, 1);
        let wasm = assert_golden("escape_nested_expr");
        assert_receiver_copied(&wasm, "Pair.touch");
    }

    /// `probe_i32(self.a)` — a scalar argument, passed by value, that cannot
    /// alias anything. The copy is emitted anyway.
    ///
    /// This is the one fixture whose expected behaviour is *deliberate waste*.
    /// The scan is type-blind because refining it would need a second predicate
    /// agreeing with argument lowering, and a disagreement drops the copy — the
    /// failure direction that is the bug. Assert the over-copy so narrowing the
    /// scan has to change a test that says why it is there.
    #[test]
    fn escape_scalar_projection_test() {
        cov_mark::check_count!(wasm_codegen_self_escapes_to_extern, 1);
        cov_mark::check_count!(wasm_codegen_emit_self_copy_on_entry, 1);
        let wasm = assert_golden("escape_scalar_projection");
        assert_receiver_copied(&wasm, "Pair.touch");
    }

    /// The frame offset a compound parameter's slot starts at, read from the
    /// four-instruction rebind that closes its entry copy:
    /// `local.get $__frame_ptr` / `i32.const N` / `i32.add` / `local.set $<param>`.
    ///
    /// The shape is matched exactly rather than scanned for the nearest preceding
    /// constant. A parameter placed at offset 0 — the collision this is here to
    /// catch — emits no `i32.const`/`i32.add` at all, and a loose scan would then
    /// walk back to the prologue's frame-size constant and report a large offset
    /// that satisfies any lower bound. Demanding the shape turns that case into a
    /// failure instead of a vacuous pass.
    fn param_slot_offset(body: &str, param: &str) -> u32 {
        let rebind_line = format!("local.set ${param}");
        let lines: Vec<&str> = body.lines().map(str::trim).collect();
        let rebind = lines
            .iter()
            .position(|line| *line == rebind_line)
            .unwrap_or_else(|| panic!("`{param}` must be rebound to its frame slot:\n{body}"));
        assert!(
            rebind >= 3
                && lines[rebind - 1] == "i32.add"
                && lines[rebind - 3] == "local.get $__frame_ptr",
            "`{param}`'s rebind must displace the frame pointer by its slot offset; \
             a rebind straight to the frame base means it collided with the receiver:\n{body}"
        );
        lines[rebind - 2]
            .strip_prefix("i32.const ")
            .and_then(|offset| offset.parse().ok())
            .unwrap_or_else(|| panic!("no frame offset before the `{param}` rebind:\n{body}"))
    }

    /// An escaping receiver sharing a frame with a named compound parameter.
    ///
    /// This is the only fixture where the receiver is not the frame's sole
    /// occupant, so it is the only one that can see an offset collision. The
    /// receiver is the first argument and so takes offset 0, pushing the
    /// parameter's slot up by its own size; a receiver that claimed no space, or
    /// a parameter still holding the offset it had while the receiver was
    /// frameless, would put the two regions on top of each other and the entry
    /// copies would overwrite one another.
    ///
    /// Disjointness is asserted rather than the exact offsets — that the
    /// receiver copy lands at the frame base and the parameter starts at or past
    /// the end of it. The goldens carry the concrete numbers (0 and 8, in a
    /// 32-byte frame).
    #[test]
    fn escape_with_param_test() {
        cov_mark::check_count!(wasm_codegen_self_escapes_to_extern, 1);
        cov_mark::check_count!(wasm_codegen_emit_self_copy_on_entry, 1);
        let wasm = assert_golden("escape_with_param");
        assert_receiver_copied(&wasm, "Pair.touch");

        let body = function_wat(&wasm, "Pair.touch");
        let receiver_copy = body
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("i64.store") && line.contains("align=1"))
            .unwrap_or_else(|| panic!("no receiver region copy in:\n{body}"));
        assert!(
            !receiver_copy.contains("offset="),
            "the receiver copy must land at the frame base, so its offset is the \
             floor the parameter slots sit above; found `{receiver_copy}`:\n{body}"
        );
        // `Pair` is two i32s, so the receiver owns bytes 0..8.
        assert!(
            param_slot_offset(&body, "extra") >= 8,
            "the parameter's slot must start past the end of the receiver's, or \
             their entry copies overwrite one another:\n{body}"
        );
    }

    /// A `mut self` receiver that also escapes gets **one** copy, not two, and
    /// does not fire the escape mark: the mutable case was already covered and
    /// the two conditions share a single slot.
    #[test]
    fn mut_self_extern_test() {
        cov_mark::check_count!(wasm_codegen_self_escapes_to_extern, 0);
        cov_mark::check_count!(wasm_codegen_emit_self_copy_on_entry, 1);
        let wasm = assert_golden("mut_self_extern");
        assert_receiver_copied(&wasm, "Pair.touch");
    }

    /// The negative: an import is registered and called, but never with an
    /// argument rooted at `self`, so the receiver keeps the by-reference
    /// treatment. Both marks stay silent — the decision is "no escape", not
    /// "no externs to check against".
    #[test]
    fn no_escape_self_test() {
        cov_mark::check_count!(wasm_codegen_self_escapes_to_extern, 0);
        cov_mark::check_count!(wasm_codegen_emit_self_copy_on_entry, 0);
        let wasm = assert_golden("no_escape_self");
        assert_receiver_not_copied(&wasm, "Pair.sum");
    }
}

/// Regeneration helpers for the goldens in this file, gated behind `#[ignore]`.
///
/// ```bash
/// cargo test -p inference-tests codegen::wasm::self_extern_escape::regenerate -- --ignored
/// ```
#[cfg(test)]
mod regenerate {
    use crate::utils::{get_test_data_path, regenerate_wat, wasm_codegen};

    fn fixture_dir(test_name: &str) -> std::path::PathBuf {
        get_test_data_path()
            .join("codegen")
            .join("wasm")
            .join("self_extern_escape")
            .join(test_name)
    }

    fn regenerate(test_name: &str) {
        let dir = fixture_dir(test_name);
        let source_path = dir.join(format!("{test_name}.inf"));
        let source_code = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", source_path.display()));
        let actual = wasm_codegen(&source_code);
        inf_wasmparser::validate(&actual)
            .unwrap_or_else(|e| panic!("Generated Wasm module is invalid for {test_name}: {e}"));
        let wasm_path = dir.join(format!("{test_name}.wasm"));
        std::fs::write(&wasm_path, &actual)
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", wasm_path.display()));
        println!(
            "Regenerated: {} ({} bytes)",
            wasm_path.display(),
            actual.len()
        );
        regenerate_wat(&actual, &dir, test_name);
    }

    #[test]
    #[ignore]
    fn regenerate_escape_whole_self_wasm() {
        regenerate("escape_whole_self");
    }

    #[test]
    #[ignore]
    fn regenerate_escape_sub_object_wasm() {
        regenerate("escape_sub_object");
    }

    #[test]
    #[ignore]
    fn regenerate_escape_nested_block_wasm() {
        regenerate("escape_nested_block");
    }

    #[test]
    #[ignore]
    fn regenerate_escape_nested_expr_wasm() {
        regenerate("escape_nested_expr");
    }

    #[test]
    #[ignore]
    fn regenerate_escape_scalar_projection_wasm() {
        regenerate("escape_scalar_projection");
    }

    #[test]
    #[ignore]
    fn regenerate_escape_with_param_wasm() {
        regenerate("escape_with_param");
    }

    #[test]
    #[ignore]
    fn regenerate_mut_self_extern_wasm() {
        regenerate("mut_self_extern");
    }

    #[test]
    #[ignore]
    fn regenerate_no_escape_self_wasm() {
        regenerate("no_escape_self");
    }
}
