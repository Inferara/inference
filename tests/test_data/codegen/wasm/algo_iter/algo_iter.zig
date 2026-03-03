export fn fibonacci_iter(n: i32) callconv(.c) i32 {
    if (n <= 0) return 0;
    if (n == 1) return 1;
    var a: i32 = 0;
    var b: i32 = 1;
    var i: i32 = 2;
    while (i <= n) {
        const next = a +% b;
        a = b;
        b = next;
        i += 1;
    }
    return b;
}

export fn gcd_iter(a_param: i32, b_param: i32) callconv(.c) i32 {
    var x: i32 = a_param;
    var y: i32 = b_param;
    if (x < 0) {
        x = 0 - x;
    }
    if (y < 0) {
        y = 0 - y;
    }
    while (y > 0) {
        const t = y;
        y = @rem(x, y);
        x = t;
    }
    return x;
}

export fn is_prime_iter(n: i32) callconv(.c) i32 {
    if (n <= 1) return 0;
    if (n <= 3) return 1;
    if (@rem(n, 2) == 0) return 0;
    var d: i32 = 3;
    while (d * d <= n) {
        if (@rem(n, d) == 0) return 0;
        d += 2;
    }
    return 1;
}

export fn isqrt(n: i32) callconv(.c) i32 {
    if (n <= 0) return 0;
    var x: i32 = n;
    var y: i32 = @divTrunc(x + 1, 2);
    while (y < x) {
        x = y;
        y = @divTrunc(x + @divTrunc(n, x), 2);
    }
    return x;
}

export fn pow_iter(base: i32, exp: i32) callconv(.c) i32 {
    var result: i32 = 1;
    var b: i32 = base;
    var e: i32 = exp;
    while (e > 0) {
        if ((e & 1) == 1) {
            result = result *% b;
        }
        b = b *% b;
        e = e >> 1;
    }
    return result;
}

export fn fibonacci_iter_i64(n: i64) callconv(.c) i64 {
    if (n <= 0) return 0;
    if (n == 1) return 1;
    var a: i64 = 0;
    var b: i64 = 1;
    var i: i64 = 2;
    while (i <= n) {
        const next = a +% b;
        a = b;
        b = next;
        i += 1;
    }
    return b;
}

export fn gcd_iter_i64(a_param: i64, b_param: i64) callconv(.c) i64 {
    var x: i64 = a_param;
    var y: i64 = b_param;
    if (x < 0) {
        x = 0 - x;
    }
    if (y < 0) {
        y = 0 - y;
    }
    while (y > 0) {
        const t = y;
        y = @rem(x, y);
        x = t;
    }
    return x;
}

export fn pow_iter_i64(base: i64, exp: i64) callconv(.c) i64 {
    var result: i64 = 1;
    var b: i64 = base;
    var e: i64 = exp;
    while (e > 0) {
        if ((e & 1) == 1) {
            result = result *% b;
        }
        b = b *% b;
        e = e >> 1;
    }
    return result;
}

export fn gcd_u8(a: u8, b: u8) callconv(.c) u8 {
    var x: u8 = a;
    var y: u8 = b;
    while (y > 0) {
        const t = y;
        y = x % y;
        x = t;
    }
    return x;
}

export fn fibonacci_i16(n: i16) callconv(.c) i16 {
    if (n <= 0) return 0;
    if (n == 1) return 1;
    var a: i16 = 0;
    var b: i16 = 1;
    var i: i16 = 2;
    while (i <= n) {
        const next = a +% b;
        a = b;
        b = next;
        i += 1;
    }
    return b;
}

export fn pow_u16(base: u16, exp: u16) callconv(.c) u16 {
    var result: u16 = 1;
    var b: u16 = base;
    var e: u16 = exp;
    while (e > 0) {
        if ((e & 1) == 1) {
            result = result *% b;
        }
        b = b *% b;
        e = e >> 1;
    }
    return result;
}

export fn is_prime_bool(n: i32) callconv(.c) bool {
    if (n <= 1) return false;
    if (n <= 3) return true;
    if (@rem(n, 2) == 0) return false;
    var d: i32 = 3;
    while (d * d <= n) {
        if (@rem(n, d) == 0) return false;
        d += 2;
    }
    return true;
}
