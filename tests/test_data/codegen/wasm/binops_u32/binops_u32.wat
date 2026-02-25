(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (param i32 i32) (result i32)))
  (type (;6;) (func (param i32 i32) (result i32)))
  (type (;7;) (func (param i32 i32) (result i32)))
  (type (;8;) (func (param i32 i32) (result i32)))
  (type (;9;) (func (param i32 i32) (result i32)))
  (type (;10;) (func (param i32 i32) (result i32)))
  (type (;11;) (func (param i32 i32) (result i32)))
  (export "div_u32" (func $div_u32))
  (export "mod_u32" (func $mod_u32))
  (export "lt_u32" (func $lt_u32))
  (export "le_u32" (func $le_u32))
  (export "gt_u32" (func $gt_u32))
  (export "ge_u32" (func $ge_u32))
  (export "shr_u32" (func $shr_u32))
  (export "add_u32" (func $add_u32))
  (export "mul_u32" (func $mul_u32))
  (export "eq_u32" (func $eq_u32))
  (export "high_bit_div_u32" (func $high_bit_div_u32))
  (export "high_bit_lt_u32" (func $high_bit_lt_u32))
  (func $div_u32 (;0;) (type 0) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.div_u
    return
    unreachable
  )
  (func $mod_u32 (;1;) (type 1) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.rem_u
    return
    unreachable
  )
  (func $lt_u32 (;2;) (type 2) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.lt_u
    return
    unreachable
  )
  (func $le_u32 (;3;) (type 3) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.le_u
    return
    unreachable
  )
  (func $gt_u32 (;4;) (type 4) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.gt_u
    return
    unreachable
  )
  (func $ge_u32 (;5;) (type 5) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.ge_u
    return
    unreachable
  )
  (func $shr_u32 (;6;) (type 6) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.shr_u
    return
    unreachable
  )
  (func $add_u32 (;7;) (type 7) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $mul_u32 (;8;) (type 8) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.mul
    return
    unreachable
  )
  (func $eq_u32 (;9;) (type 9) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.eq
    return
    unreachable
  )
  (func $high_bit_div_u32 (;10;) (type 10) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.div_u
    return
    unreachable
  )
  (func $high_bit_lt_u32 (;11;) (type 11) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.gt_u
    return
    unreachable
  )
)
