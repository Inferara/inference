//! The source of `../rustlib.wasm`, a stock `wasm32-unknown-unknown` artifact
//! the Inference linker merges as an external module.
//!
//! Nothing here is written for the linker's benefit. It is ordinary `#![no_std]`
//! Rust, and the point of the fixture is that ordinary Rust lands inside the
//! merge envelope: `#![no_std]` keeps `std`'s data segments and function table
//! out, which is what the envelope actually rejects, and the exports then cover
//! its two admitted tiers. `clamp_add` and `mulhi` touch no memory (Tier A);
//! `sum_n` reads only through the pointer its caller supplies (Tier B).
//!
//! `mulhi` additionally carries the width-changing operators. It is the one
//! function here whose shape was chosen against the optimizer rather than only
//! for what it computes: an intermediate that merely *could* be 64-bit gets
//! narrowed away — `clamp_add` is written over `i64` and reaches the artifact as
//! branchless `i32` — so covering `i64.extend_i32_s` and `i32.wrap_i64` with a
//! real artifact needs a result that 32 bits cannot hold on the way to one they
//! can.
//!
//! See `../README.md` for the toolchain that produced the committed bytes and
//! how to regenerate them.

#![no_std]

use core::panic::PanicInfo;

/// Unreachable: `panic = "abort"` and neither export can panic. Required all the
/// same, because `#![no_std]` leaves the crate without one.
#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

/// Adds two 32-bit integers with 64-bit intermediate precision, saturating at
/// the `i32` bounds instead of wrapping.
#[unsafe(no_mangle)]
pub extern "C" fn clamp_add(a: i32, b: i32) -> i32 {
    let wide = a as i64 + b as i64;
    if wide > i32::MAX as i64 {
        i32::MAX
    } else if wide < i32::MIN as i64 {
        i32::MIN
    } else {
        wide as i32
    }
}

/// The high 32 bits of the full 64-bit product of two 32-bit integers.
///
/// No 32-bit-only lowering computes this, so the widening and the narrowing both
/// survive optimization.
#[unsafe(no_mangle)]
pub extern "C" fn mulhi(a: i32, b: i32) -> i32 {
    (((a as i64) * (b as i64)) >> 32) as i32
}

/// Sums the `n` 32-bit integers starting at `p`, wrapping on overflow.
///
/// # Safety
///
/// `p` must point at `n` readable, aligned, initialized `i32`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sum_n(p: *const i32, n: i32) -> i32 {
    let mut total: i32 = 0;
    let mut i: i32 = 0;
    while i < n {
        total = total.wrapping_add(unsafe { *p.offset(i as isize) });
        i += 1;
    }
    total
}
