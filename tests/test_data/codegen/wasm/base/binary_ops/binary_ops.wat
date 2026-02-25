(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (param i64 i64) (result i64)))
  (type (;6;) (func (param i32 i32) (result i32)))
  (type (;7;) (func (param i32 i32) (result i32)))
  (type (;8;) (func (param i32 i32) (result i32)))
  (type (;9;) (func (param i32 i32) (result i32)))
  (type (;10;) (func (param i32 i32) (result i32)))
  (type (;11;) (func (param i32 i32) (result i32)))
  (type (;12;) (func (param i32 i32) (result i32)))
  (type (;13;) (func (param i32 i32) (result i32)))
  (type (;14;) (func (param i32 i32) (result i32)))
  (type (;15;) (func (param i32 i32) (result i32)))
  (type (;16;) (func (param i32 i32) (result i32)))
  (type (;17;) (func (param i32 i32) (result i32)))
  (type (;18;) (func (param i32 i32) (result i32)))
  (type (;19;) (func (param i32 i32) (result i32)))
  (type (;20;) (func (param i32 i32) (result i32)))
  (type (;21;) (func (param i32) (result i32)))
  (type (;22;) (func (param i32) (result i32)))
  (type (;23;) (func (param i32) (result i32)))
  (type (;24;) (func (param i32 i32) (result i32)))
  (type (;25;) (func (param i32 i32) (result i32)))
  (export "add_i32" (func $add_i32))
  (export "sub_i32" (func $sub_i32))
  (export "mul_i32" (func $mul_i32))
  (export "div_i32" (func $div_i32))
  (export "mod_i32" (func $mod_i32))
  (export "add_i64" (func $add_i64))
  (export "div_u32" (func $div_u32))
  (export "eq_i32" (func $eq_i32))
  (export "ne_i32" (func $ne_i32))
  (export "lt_i32" (func $lt_i32))
  (export "le_i32" (func $le_i32))
  (export "gt_i32" (func $gt_i32))
  (export "ge_i32" (func $ge_i32))
  (export "and_bool" (func $and_bool))
  (export "or_bool" (func $or_bool))
  (export "band_i32" (func $band_i32))
  (export "bor_i32" (func $bor_i32))
  (export "bxor_i32" (func $bxor_i32))
  (export "shl_i32" (func $shl_i32))
  (export "shr_i32" (func $shr_i32))
  (export "shr_u32" (func $shr_u32))
  (export "neg_i32" (func $neg_i32))
  (export "not_bool" (func $not_bool))
  (export "bitnot_i32" (func $bitnot_i32))
  (export "paren_add" (func $paren_add))
  (export "binop_as_let" (func $binop_as_let))
  (func $add_i32 (;0;) (type 0) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $sub_i32 (;1;) (type 1) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.sub
    return
    unreachable
  )
  (func $mul_i32 (;2;) (type 2) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.mul
    return
    unreachable
  )
  (func $div_i32 (;3;) (type 3) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.div_s
    return
    unreachable
  )
  (func $mod_i32 (;4;) (type 4) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.rem_s
    return
    unreachable
  )
  (func $add_i64 (;5;) (type 5) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.add
    return
    unreachable
  )
  (func $div_u32 (;6;) (type 6) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.div_u
    return
    unreachable
  )
  (func $eq_i32 (;7;) (type 7) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.eq
    return
    unreachable
  )
  (func $ne_i32 (;8;) (type 8) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.ne
    return
    unreachable
  )
  (func $lt_i32 (;9;) (type 9) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.lt_s
    return
    unreachable
  )
  (func $le_i32 (;10;) (type 10) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.le_s
    return
    unreachable
  )
  (func $gt_i32 (;11;) (type 11) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.gt_s
    return
    unreachable
  )
  (func $ge_i32 (;12;) (type 12) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.ge_s
    return
    unreachable
  )
  (func $and_bool (;13;) (type 13) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.and
    return
    unreachable
  )
  (func $or_bool (;14;) (type 14) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.or
    return
    unreachable
  )
  (func $band_i32 (;15;) (type 15) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.and
    return
    unreachable
  )
  (func $bor_i32 (;16;) (type 16) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.or
    return
    unreachable
  )
  (func $bxor_i32 (;17;) (type 17) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.xor
    return
    unreachable
  )
  (func $shl_i32 (;18;) (type 18) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.shl
    return
    unreachable
  )
  (func $shr_i32 (;19;) (type 19) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.shr_s
    return
    unreachable
  )
  (func $shr_u32 (;20;) (type 20) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.shr_u
    return
    unreachable
  )
  (func $neg_i32 (;21;) (type 21) (param $a i32) (result i32)
    i32.const 0
    local.get $a
    i32.sub
    return
    unreachable
  )
  (func $not_bool (;22;) (type 22) (param $a i32) (result i32)
    local.get $a
    i32.eqz
    return
    unreachable
  )
  (func $bitnot_i32 (;23;) (type 23) (param $a i32) (result i32)
    local.get $a
    i32.const -1
    i32.xor
    return
    unreachable
  )
  (func $paren_add (;24;) (type 24) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $binop_as_let (;25;) (type 25) (param $a i32) (param $b i32) (result i32)
    (local $r i32)
    local.get $a
    local.get $b
    i32.add
    local.set $r
    local.get $r
    return
    unreachable
  )
)
