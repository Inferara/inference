#![no_std]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn fibonacci_iter(n: i32) -> i32 {
    if n <= 0 { return 0; }
    if n == 1 { return 1; }
    let mut a: i32 = 0;
    let mut b: i32 = 1;
    let mut i: i32 = 2;
    while i <= n {
        let next = a + b;
        a = b;
        b = next;
        i += 1;
    }
    b
}

#[no_mangle]
pub extern "C" fn gcd_iter(a: i32, b: i32) -> i32 {
    let mut x = a;
    let mut y = b;
    if x < 0 { x = 0 - x; }
    if y < 0 { y = 0 - y; }
    while y > 0 {
        let t = y;
        y = x % y;
        x = t;
    }
    x
}

#[no_mangle]
pub extern "C" fn is_prime_iter(n: i32) -> i32 {
    if n <= 1 { return 0; }
    if n <= 3 { return 1; }
    if (n % 2) == 0 { return 0; }
    let mut d: i32 = 3;
    while d * d <= n {
        if (n % d) == 0 { return 0; }
        d += 2;
    }
    1
}

#[no_mangle]
pub extern "C" fn isqrt(n: i32) -> i32 {
    if n <= 0 { return 0; }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[no_mangle]
pub extern "C" fn pow_iter(base: i32, exp: i32) -> i32 {
    let mut result: i32 = 1;
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if (e & 1) == 1 {
            result = result.wrapping_mul(b);
        }
        b = b.wrapping_mul(b);
        e >>= 1;
    }
    result
}

#[no_mangle]
pub extern "C" fn fibonacci_iter_i64(n: i64) -> i64 {
    if n <= 0 { return 0; }
    if n == 1 { return 1; }
    let mut a: i64 = 0;
    let mut b: i64 = 1;
    let mut i: i64 = 2;
    while i <= n {
        let next = a + b;
        a = b;
        b = next;
        i += 1;
    }
    b
}

#[no_mangle]
pub extern "C" fn gcd_iter_i64(a: i64, b: i64) -> i64 {
    let mut x = a;
    let mut y = b;
    if x < 0 { x = 0 - x; }
    if y < 0 { y = 0 - y; }
    while y > 0 {
        let t = y;
        y = x % y;
        x = t;
    }
    x
}

#[no_mangle]
pub extern "C" fn pow_iter_i64(base: i64, exp: i64) -> i64 {
    let mut result: i64 = 1;
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if (e & 1) == 1 {
            result = result.wrapping_mul(b);
        }
        b = b.wrapping_mul(b);
        e >>= 1;
    }
    result
}

#[no_mangle]
pub extern "C" fn gcd_u8(a: u8, b: u8) -> u8 {
    let mut x = a;
    let mut y = b;
    while y > 0 {
        let t = y;
        y = x % y;
        x = t;
    }
    x
}

#[no_mangle]
pub extern "C" fn fibonacci_i16(n: i16) -> i16 {
    if n <= 0 { return 0; }
    if n == 1 { return 1; }
    let mut a: i16 = 0;
    let mut b: i16 = 1;
    let mut i: i16 = 2;
    while i <= n {
        let next = a + b;
        a = b;
        b = next;
        i += 1;
    }
    b
}

#[no_mangle]
pub extern "C" fn pow_u16(base: u16, exp: u16) -> u16 {
    let mut result: u16 = 1;
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if (e & 1) == 1 {
            result = result.wrapping_mul(b);
        }
        b = b.wrapping_mul(b);
        e >>= 1;
    }
    result
}

#[no_mangle]
pub extern "C" fn is_prime_bool(n: i32) -> bool {
    if n <= 1 { return false; }
    if n <= 3 { return true; }
    if (n % 2) == 0 { return false; }
    let mut d: i32 = 3;
    while d * d <= n {
        if (n % d) == 0 { return false; }
        d += 2;
    }
    true
}
