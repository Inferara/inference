(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i64)))
  (type (;3;) (func (result i64)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i64)))
  (export "i32_max_plus_one" (func $i32_max_plus_one))
  (export "i32_min_minus_one" (func $i32_min_minus_one))
  (export "i64_max_plus_one" (func $i64_max_plus_one))
  (export "i64_min_minus_one" (func $i64_min_minus_one))
  (export "u32_max_plus_one" (func $u32_max_plus_one))
  (export "i32_mul_overflow" (func $i32_mul_overflow))
  (export "i32_neg_min" (func $i32_neg_min))
  (export "i64_neg_min" (func $i64_neg_min))
  (func $i32_max_plus_one (;0;) (type 0) (result i32)
    (local $max i32)
    i32.const 2147483647
    local.set $max
    local.get $max
    i32.const 1
    i32.add
    return
    unreachable
  )
  (func $i32_min_minus_one (;1;) (type 1) (result i32)
    (local $min i32)
    i32.const -2147483648
    local.set $min
    local.get $min
    i32.const 1
    i32.sub
    return
    unreachable
  )
  (func $i64_max_plus_one (;2;) (type 2) (result i64)
    (local $max i64) (local $one i64)
    i64.const 9223372036854775807
    local.set $max
    i64.const 1
    local.set $one
    local.get $max
    local.get $one
    i64.add
    return
    unreachable
  )
  (func $i64_min_minus_one (;3;) (type 3) (result i64)
    (local $min i64) (local $one i64)
    i64.const -9223372036854775808
    local.set $min
    i64.const 1
    local.set $one
    local.get $min
    local.get $one
    i64.sub
    return
    unreachable
  )
  (func $u32_max_plus_one (;4;) (type 4) (result i32)
    (local $max i32) (local $one i32)
    i32.const -1
    local.set $max
    i32.const 1
    local.set $one
    local.get $max
    local.get $one
    i32.add
    return
    unreachable
  )
  (func $i32_mul_overflow (;5;) (type 5) (result i32)
    (local $big i32)
    i32.const 2147483647
    local.set $big
    local.get $big
    i32.const 2
    i32.mul
    return
    unreachable
  )
  (func $i32_neg_min (;6;) (type 6) (result i32)
    (local $min i32)
    i32.const -2147483648
    local.set $min
    i32.const 0
    local.get $min
    i32.sub
    return
    unreachable
  )
  (func $i64_neg_min (;7;) (type 7) (result i64)
    (local $min i64)
    i64.const -9223372036854775808
    local.set $min
    i64.const 0
    local.get $min
    i64.sub
    return
    unreachable
  )
)
