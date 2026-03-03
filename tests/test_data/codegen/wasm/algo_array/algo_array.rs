#![no_std]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn linear_search(target: i32) -> i32 {
    let arr: [i32; 8] = [3, 7, 1, 9, 4, 6, 8, 2];
    let mut i: i32 = 0;
    while i < 8 {
        if arr[i as usize] == target { return i; }
        i += 1;
    }
    8
}

#[no_mangle]
pub extern "C" fn binary_search(target: i32) -> i32 {
    let arr: [i32; 8] = [2, 5, 8, 12, 16, 23, 38, 56];
    let mut low: i32 = 0;
    let mut high: i32 = 7;
    while low <= high {
        let mid = (low + high) / 2;
        let val = arr[mid as usize];
        if val == target { return mid; }
        if val < target {
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }
    8
}

#[no_mangle]
pub extern "C" fn bubble_sort_element(idx: i32) -> i32 {
    let mut arr: [i32; 6] = [5, 3, 8, 1, 9, 2];
    let mut i: i32 = 0;
    while i < 6 {
        let mut j: i32 = 0;
        while j < 5 {
            let k = (j + 1) as usize;
            if arr[j as usize] > arr[k] {
                arr.swap(j as usize, k);
            }
            j += 1;
        }
        i += 1;
    }
    arr[idx as usize]
}

#[no_mangle]
pub extern "C" fn dot_product() -> i32 {
    let a: [i32; 4] = [1, 2, 3, 4];
    let b: [i32; 4] = [5, 6, 7, 8];
    let mut sum: i32 = 0;
    let mut i: i32 = 0;
    while i < 4 {
        sum += a[i as usize] * b[i as usize];
        i += 1;
    }
    sum
}

#[no_mangle]
pub extern "C" fn array_max(n: i32) -> i32 {
    let arr: [i32; 8] = [3, 7, 1, 9, 4, 6, 8, 2];
    let mut max_val = arr[0];
    let mut i: i32 = 1;
    while i < n {
        if arr[i as usize] > max_val {
            max_val = arr[i as usize];
        }
        i += 1;
    }
    max_val
}

#[no_mangle]
pub extern "C" fn prefix_sum_element(idx: i32) -> i32 {
    let mut arr: [i32; 6] = [1, 2, 3, 4, 5, 6];
    let mut i: i32 = 1;
    let mut running = arr[0];
    while i < 6 {
        running += arr[i as usize];
        arr[i as usize] = running;
        i += 1;
    }
    arr[idx as usize]
}

#[no_mangle]
pub extern "C" fn sum_u8_array() -> u8 {
    let arr: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let mut sum: u8 = 0;
    let mut i: i32 = 0;
    while i < 8 {
        sum += arr[i as usize];
        i += 1;
    }
    sum
}

#[no_mangle]
pub extern "C" fn min_i8_array() -> i8 {
    let arr: [i8; 6] = [50, 30, 80, 10, 60, 40];
    let mut min_val: i8 = arr[0];
    let mut i: i32 = 1;
    while i < 6 {
        let val = arr[i as usize];
        if val < min_val {
            min_val = val;
        }
        i += 1;
    }
    min_val
}

#[no_mangle]
pub extern "C" fn max_i16_array() -> i16 {
    let arr: [i16; 6] = [300, 700, 100, 900, 400, 600];
    let mut max_val: i16 = arr[0];
    let mut i: i32 = 1;
    while i < 6 {
        let val = arr[i as usize];
        if val > max_val {
            max_val = val;
        }
        i += 1;
    }
    max_val
}

#[no_mangle]
pub extern "C" fn sum_u16_array() -> u16 {
    let arr: [u16; 6] = [1000, 2000, 3000, 4000, 5000, 6000];
    let mut sum: u16 = 0;
    let mut i: i32 = 0;
    while i < 6 {
        sum += arr[i as usize];
        i += 1;
    }
    sum
}

#[no_mangle]
pub extern "C" fn search_u32_array(target: u32) -> i32 {
    let arr: [u32; 6] = [100, 200, 300, 400, 500, 600];
    let mut i: i32 = 0;
    while i < 6 {
        if arr[i as usize] == target { return i; }
        i += 1;
    }
    6
}

#[no_mangle]
pub extern "C" fn dot_product_i64() -> i64 {
    let a: [i64; 4] = [100000, 200000, 300000, 400000];
    let b: [i64; 4] = [500000, 600000, 700000, 800000];
    let mut sum: i64 = 0;
    let mut i: i32 = 0;
    while i < 4 {
        sum += a[i as usize] * b[i as usize];
        i += 1;
    }
    sum
}
