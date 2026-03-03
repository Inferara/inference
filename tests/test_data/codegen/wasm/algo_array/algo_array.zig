export fn linear_search(target: i32) callconv(.c) i32 {
    const arr = [_]i32{ 3, 7, 1, 9, 4, 6, 8, 2 };
    var i: i32 = 0;
    while (i < 8) {
        if (arr[@intCast(@as(u32, @bitCast(i)))] == target) return i;
        i += 1;
    }
    return 8;
}

export fn binary_search(target: i32) callconv(.c) i32 {
    const arr = [_]i32{ 2, 5, 8, 12, 16, 23, 38, 56 };
    var low: i32 = 0;
    var high: i32 = 7;
    while (low <= high) {
        const mid = @divTrunc(low + high, 2);
        const val = arr[@intCast(@as(u32, @bitCast(mid)))];
        if (val == target) return mid;
        if (val < target) {
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }
    return 8;
}

export fn bubble_sort_element(idx: i32) callconv(.c) i32 {
    var arr = [_]i32{ 5, 3, 8, 1, 9, 2 };
    var i: i32 = 0;
    while (i < 6) {
        var j: u32 = 0;
        while (j < 5) {
            const k = j + 1;
            if (arr[j] > arr[k]) {
                const tmp = arr[j];
                arr[j] = arr[k];
                arr[k] = tmp;
            }
            j += 1;
        }
        i += 1;
    }
    return arr[@intCast(@as(u32, @bitCast(idx)))];
}

export fn dot_product() callconv(.c) i32 {
    const a = [_]i32{ 1, 2, 3, 4 };
    const b = [_]i32{ 5, 6, 7, 8 };
    var sum: i32 = 0;
    var i: u32 = 0;
    while (i < 4) {
        sum += a[i] * b[i];
        i += 1;
    }
    return sum;
}

export fn array_max(n: i32) callconv(.c) i32 {
    const arr = [_]i32{ 3, 7, 1, 9, 4, 6, 8, 2 };
    var max_val: i32 = arr[0];
    var i: i32 = 1;
    while (i < n) {
        const val = arr[@intCast(@as(u32, @bitCast(i)))];
        if (val > max_val) {
            max_val = val;
        }
        i += 1;
    }
    return max_val;
}

export fn prefix_sum_element(idx: i32) callconv(.c) i32 {
    var arr = [_]i32{ 1, 2, 3, 4, 5, 6 };
    var i: u32 = 1;
    var running: i32 = arr[0];
    while (i < 6) {
        running += arr[i];
        arr[i] = running;
        i += 1;
    }
    return arr[@intCast(@as(u32, @bitCast(idx)))];
}

export fn sum_u8_array() callconv(.c) u8 {
    const arr = [_]u8{ 1, 2, 3, 4, 5, 6, 7, 8 };
    var sum: u8 = 0;
    var i: u32 = 0;
    while (i < 8) {
        sum +%= arr[i];
        i += 1;
    }
    return sum;
}

export fn min_i8_array() callconv(.c) i8 {
    const arr = [_]i8{ 50, 30, 80, 10, 60, 40 };
    var min_val: i8 = arr[0];
    var i: u32 = 1;
    while (i < 6) {
        const val = arr[i];
        if (val < min_val) {
            min_val = val;
        }
        i += 1;
    }
    return min_val;
}

export fn max_i16_array() callconv(.c) i16 {
    const arr = [_]i16{ 300, 700, 100, 900, 400, 600 };
    var max_val: i16 = arr[0];
    var i: u32 = 1;
    while (i < 6) {
        const val = arr[i];
        if (val > max_val) {
            max_val = val;
        }
        i += 1;
    }
    return max_val;
}

export fn sum_u16_array() callconv(.c) u16 {
    const arr = [_]u16{ 1000, 2000, 3000, 4000, 5000, 6000 };
    var sum: u16 = 0;
    var i: u32 = 0;
    while (i < 6) {
        sum +%= arr[i];
        i += 1;
    }
    return sum;
}

export fn search_u32_array(target: u32) callconv(.c) i32 {
    const arr = [_]u32{ 100, 200, 300, 400, 500, 600 };
    var i: i32 = 0;
    while (i < 6) {
        if (arr[@intCast(@as(u32, @bitCast(i)))] == target) return i;
        i += 1;
    }
    return 6;
}

export fn dot_product_i64() callconv(.c) i64 {
    const a = [_]i64{ 100000, 200000, 300000, 400000 };
    const b = [_]i64{ 500000, 600000, 700000, 800000 };
    var sum: i64 = 0;
    var i: u32 = 0;
    while (i < 4) {
        sum += a[i] * b[i];
        i += 1;
    }
    return sum;
}
