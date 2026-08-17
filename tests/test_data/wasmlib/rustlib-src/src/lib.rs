//! The source of `../rustlib.wasm`, a stock `wasm32-unknown-unknown` artifact
//! the Inference linker merges as an external module.
//!
//! Nothing here is written for the linker's benefit. It is ordinary `#![no_std]`
//! Rust, and the point of the fixture is that ordinary Rust lands inside the
//! merge envelope: `#![no_std]` keeps `std`'s data segments and function table
//! out, which is what the envelope actually rejects, and the two exports then
//! cover its two admitted tiers. `clamp_add` touches no memory (Tier A);
//! `sum_n` reads only through the pointer its caller supplies (Tier B).
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
