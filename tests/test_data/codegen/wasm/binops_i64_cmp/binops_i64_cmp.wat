(module $output
  (type (;0;) (func (param i64 i64) (result i32)))
  (type (;1;) (func (param i64 i64) (result i32)))
  (type (;2;) (func (param i64 i64) (result i32)))
  (type (;3;) (func (param i64 i64) (result i32)))
  (type (;4;) (func (param i64 i64) (result i32)))
  (type (;5;) (func (param i64 i64) (result i32)))
  (type (;6;) (func (param i64 i64) (result i32)))
  (type (;7;) (func (param i64 i64) (result i32)))
  (type (;8;) (func (param i64 i64) (result i32)))
  (type (;9;) (func (param i64 i64) (result i32)))
  (type (;10;) (func (param i64 i64 i64) (result i32)))
  (type (;11;) (func (param i64) (result i32)))
  (export "eq_i64" (func $eq_i64))
  (export "ne_i64" (func $ne_i64))
  (export "lt_i64_signed" (func $lt_i64_signed))
  (export "lt_u64" (func $lt_u64))
  (export "le_i64_signed" (func $le_i64_signed))
  (export "le_u64" (func $le_u64))
  (export "gt_i64_signed" (func $gt_i64_signed))
  (export "gt_u64" (func $gt_u64))
  (export "ge_i64_signed" (func $ge_i64_signed))
  (export "ge_u64" (func $ge_u64))
  (export "cmp_chain_i64" (func $cmp_chain_i64))
  (export "boundary_signed_i64" (func $boundary_signed_i64))
  (func $eq_i64 (;0;) (type 0) (param $a i64) (param $b i64) (result i32)
    local.get $a
    local.get $b
    i64.eq
    return
    unreachable
  )
  (func $ne_i64 (;1;) (type 1) (param $a i64) (param $b i64) (result i32)
    local.get $a
    local.get $b
    i64.ne
    return
    unreachable
  )
  (func $lt_i64_signed (;2;) (type 2) (param $a i64) (param $b i64) (result i32)
    local.get $a
    local.get $b
    i64.lt_s
    return
    unreachable
  )
  (func $lt_u64 (;3;) (type 3) (param $a i64) (param $b i64) (result i32)
    local.get $a
    local.get $b
    i64.lt_u
    return
    unreachable
  )
  (func $le_i64_signed (;4;) (type 4) (param $a i64) (param $b i64) (result i32)
    local.get $a
    local.get $b
    i64.le_s
    return
    unreachable
  )
  (func $le_u64 (;5;) (type 5) (param $a i64) (param $b i64) (result i32)
    local.get $a
    local.get $b
    i64.le_u
    return
    unreachable
  )
  (func $gt_i64_signed (;6;) (type 6) (param $a i64) (param $b i64) (result i32)
    local.get $a
    local.get $b
    i64.gt_s
    return
    unreachable
  )
  (func $gt_u64 (;7;) (type 7) (param $a i64) (param $b i64) (result i32)
    local.get $a
    local.get $b
    i64.gt_u
    return
    unreachable
  )
  (func $ge_i64_signed (;8;) (type 8) (param $a i64) (param $b i64) (result i32)
    local.get $a
    local.get $b
    i64.ge_s
    return
    unreachable
  )
  (func $ge_u64 (;9;) (type 9) (param $a i64) (param $b i64) (result i32)
    local.get $a
    local.get $b
    i64.ge_u
    return
    unreachable
  )
  (func $cmp_chain_i64 (;10;) (type 10) (param $a i64) (param $b i64) (param $c i64) (result i32)
    local.get $a
    local.get $b
    i64.lt_s
    local.get $b
    local.get $c
    i64.lt_s
    i32.eq
    return
    unreachable
  )
  (func $boundary_signed_i64 (;11;) (type 11) (param $a i64) (result i32)
    (local $zero i64)
    i64.const 0
    local.set $zero
    local.get $a
    local.get $zero
    i64.ge_s
    return
    unreachable
  )
)
