//! Integration tests for [`inference::with_compiler_stack`], the seam every
//! in-process driver of the pipeline goes through (issue #322).
//!
//! The compiler's phases recurse once per level of the input's syntactic
//! nesting, and a stack overflow aborts the process rather than unwinding, so
//! the stack the phases get cannot be left to whatever the host thread happens
//! to have. The helper answers that by running the work on a thread reserving
//! [`inference_parser::MIN_COMPILE_STACK`].
//!
//! Four properties are pinned, each of which a call site depends on:
//!
//! 1. the value the closure computes comes back to the caller;
//! 2. the closure really does run with the reserved stack, not the caller's;
//! 3. the closure may borrow from its environment — the CLI driver's `run`
//!    takes no arguments today, but a `'static` bound would rule out every
//!    embedder that needs to hand the pipeline a borrowed source or arena;
//! 4. a panic reaches the caller carrying its original payload, which is what
//!    keeps a wrapped driver's exit code and stderr identical to what they were
//!    before the wrapping.

/// Bytes of stack each [`burn`] frame is forced to occupy.
const FRAME_BYTES: usize = 4 * 1024;

/// Number of [`burn`] frames, chosen so the recursion needs far more stack than
/// any default-sized thread has and far less than the helper reserves.
const DEPTH: usize = 4 * 1024;

/// Total stack the recursion consumes: 16 MiB.
const BURN_BYTES: usize = FRAME_BYTES * DEPTH;

// A default thread — including the one Cargo's harness runs each test on — gets
// single-digit megabytes, so this recursion could not complete on the calling
// thread; and it stays an order of magnitude inside the reservation, so the test
// asserts that the helper switched stacks rather than probing where the
// reservation itself ends.
const _: () = assert!(BURN_BYTES >= 16 * 1024 * 1024);
const _: () = assert!(BURN_BYTES * 8 <= inference_parser::MIN_COMPILE_STACK);

/// Recurses `depth + 1` times, holding a [`FRAME_BYTES`] buffer live across each
/// call so the frame cannot be elided, and returns the number of frames entered.
///
/// The buffer is written before the recursive call and read after it, and passed
/// through [`std::hint::black_box`], so the frame survives optimization and the
/// test measures the same stack in a release build as in a debug one.
#[inline(never)]
fn burn(depth: usize) -> u64 {
    let mut frame = [0u8; FRAME_BYTES];
    let slot = depth % FRAME_BYTES;
    frame[slot] = 1;
    let deeper = if depth == 0 { 0 } else { burn(depth - 1) };
    u64::from(std::hint::black_box(&frame)[slot]) + deeper
}

/// The helper is transparent to its closure's result.
#[test]
fn returns_the_value_the_closure_produces() {
    let value = inference::with_compiler_stack(|| "computed on the compiler stack".to_owned());
    assert_eq!(value, "computed on the compiler stack");
}

/// The closure runs with the reserved stack, not the caller's.
///
/// A 16 MiB recursion is several times more than a default thread — the test
/// harness's included — provides, so its completing at all is the observation:
/// were the closure run inline the process would have aborted, since a stack
/// overflow cannot be caught and reported.
#[test]
fn runs_on_a_stack_far_larger_than_the_calling_thread_has() {
    let frames = inference::with_compiler_stack(|| burn(DEPTH - 1));
    assert_eq!(frames, DEPTH as u64);
}

/// The closure may borrow from its environment, read and write.
///
/// This is the scoped-thread half of the contract. A plain `spawn` would force
/// a `'static` bound, which no driver that hands the pipeline borrowed inputs
/// could satisfy; the borrows below would not compile against one.
#[test]
fn accepts_a_closure_borrowing_its_environment() {
    let source = String::from("fn main() { return 0; }");
    let mut observed = Vec::new();

    let length = inference::with_compiler_stack(|| {
        observed.push(source.len());
        source.len()
    });

    assert_eq!(length, source.len());
    assert_eq!(observed, vec![source.len()]);
    assert!(source.starts_with("fn main"), "the borrow must not move");
}

/// Serializes replacement of the process-global panic hook.
///
/// The hook is process state shared with every concurrently running test in
/// this binary, so a test that swaps it must hold this lock for the whole
/// swap-restore window; two unserialized swappers could otherwise interleave
/// take/set and leave the wrong hook installed at the end.
static PANIC_HOOK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A panic inside the closure reaches the caller carrying its original payload.
///
/// The expected panic fires on the helper's worker thread, whose output the
/// test harness does not capture, so left alone it would spray a spurious
/// "thread 'inference-compile' panicked" over the log of a passing run. The
/// hook installed here silences exactly that thread and hands every other
/// panic to the hook that was already installed, so a concurrently failing
/// test keeps its diagnostic; the swap-restore window is serialized by
/// [`PANIC_HOOK_LOCK`]. `resume_unwind` deliberately does not run the hook a
/// second time on the caller's thread.
#[test]
fn propagates_a_panic_with_its_original_payload() {
    const MESSAGE: &str = "deep-syntax stack probe";

    let hook_guard = PANIC_HOOK_LOCK.lock().expect("panic-hook lock poisoned");
    let previous_hook = std::sync::Arc::new(std::panic::take_hook());
    let delegate = std::sync::Arc::clone(&previous_hook);
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().name() != Some("inference-compile") {
            delegate(info);
        }
    }));
    let outcome = std::panic::catch_unwind(|| {
        inference::with_compiler_stack(|| -> usize { panic!("{MESSAGE}") })
    });
    drop(std::panic::take_hook());
    std::panic::set_hook(
        std::sync::Arc::try_unwrap(previous_hook)
            .unwrap_or_else(|_| unreachable!("the filtering hook held the only other reference")),
    );
    drop(hook_guard);

    let payload = outcome.expect_err("a panic inside the closure must reach the caller");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied());
    assert_eq!(message, Some(MESSAGE));
}
