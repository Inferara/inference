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
  (type (;12;) (func (param i32 i32) (result i32)))
  (type (;13;) (func (param i64 i64) (result i64)))
  (type (;14;) (func (param i64 i64) (result i64)))
  (type (;15;) (func (param i32) (result i32)))
  (type (;16;) (func (param i32) (result i32)))
  (type (;17;) (func (param i32) (result i32)))
  (type (;18;) (func (param i32 i32) (result i32)))
  (type (;19;) (func (param i32 i32) (result i32)))
  (type (;20;) (func (param i64) (result i64)))
  (export "neg_add" (func $neg_add))
  (export "neg_sub" (func $neg_sub))
  (export "neg_mul" (func $neg_mul))
  (export "add_neg" (func $add_neg))
  (export "bitnot_and" (func $bitnot_and))
  (export "bitnot_or" (func $bitnot_or))
  (export "and_bitnot" (func $and_bitnot))
  (export "xor_bitnot" (func $xor_bitnot))
  (export "not_and" (func $not_and))
  (export "not_or" (func $not_or))
  (export "and_not" (func $and_not))
  (export "not_eq" (func $not_eq))
  (export "not_lt" (func $not_lt))
  (export "neg_i64_add" (func $neg_i64_add))
  (export "bitnot_i64_and" (func $bitnot_i64_and))
  (export "double_neg" (func $double_neg))
  (export "neg_bitnot" (func $neg_bitnot))
  (export "bitnot_neg" (func $bitnot_neg))
  (export "neg_shift" (func $neg_shift))
  (export "bitnot_shift" (func $bitnot_shift))
  (export "neg_i64" (func $neg_i64))
  (func $neg_add (;0;) (type 0) (param $a i32) (param $b i32) (result i32)
    i32.const 0
    local.get $a
    i32.sub
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $neg_sub (;1;) (type 1) (param $a i32) (param $b i32) (result i32)
    i32.const 0
    local.get $a
    i32.sub
    local.get $b
    i32.sub
    return
    unreachable
  )
  (func $neg_mul (;2;) (type 2) (param $a i32) (param $b i32) (result i32)
    i32.const 0
    local.get $a
    local.get $b
    i32.mul
    i32.sub
    return
    unreachable
  )
  (func $add_neg (;3;) (type 3) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const 0
    local.get $b
    i32.sub
    i32.add
    return
    unreachable
  )
  (func $bitnot_and (;4;) (type 4) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const -1
    i32.xor
    local.get $b
    i32.and
    return
    unreachable
  )
  (func $bitnot_or (;5;) (type 5) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const -1
    i32.xor
    local.get $b
    i32.or
    return
    unreachable
  )
  (func $and_bitnot (;6;) (type 6) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.const -1
    i32.xor
    i32.and
    return
    unreachable
  )
  (func $xor_bitnot (;7;) (type 7) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.const -1
    i32.xor
    i32.xor
    return
    unreachable
  )
  (func $not_and (;8;) (type 8) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.eqz
    local.get $b
    i32.and
    return
    unreachable
  )
  (func $not_or (;9;) (type 9) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.eqz
    local.get $b
    i32.or
    return
    unreachable
  )
  (func $and_not (;10;) (type 10) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.eqz
    i32.and
    return
    unreachable
  )
  (func $not_eq (;11;) (type 11) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.eq
    i32.eqz
    return
    unreachable
  )
  (func $not_lt (;12;) (type 12) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.lt_s
    i32.eqz
    return
    unreachable
  )
  (func $neg_i64_add (;13;) (type 13) (param $a i64) (param $b i64) (result i64)
    i64.const 0
    local.get $a
    i64.sub
    local.get $b
    i64.add
    return
    unreachable
  )
  (func $bitnot_i64_and (;14;) (type 14) (param $a i64) (param $b i64) (result i64)
    local.get $a
    i64.const -1
    i64.xor
    local.get $b
    i64.and
    return
    unreachable
  )
  (func $double_neg (;15;) (type 15) (param $a i32) (result i32)
    i32.const 0
    i32.const 0
    local.get $a
    i32.sub
    i32.sub
    return
    unreachable
  )
  (func $neg_bitnot (;16;) (type 16) (param $a i32) (result i32)
    i32.const 0
    local.get $a
    i32.const -1
    i32.xor
    i32.sub
    return
    unreachable
  )
  (func $bitnot_neg (;17;) (type 17) (param $a i32) (result i32)
    i32.const 0
    local.get $a
    i32.sub
    i32.const -1
    i32.xor
    return
    unreachable
  )
  (func $neg_shift (;18;) (type 18) (param $a i32) (param $b i32) (result i32)
    i32.const 0
    local.get $a
    i32.sub
    local.get $b
    i32.shl
    return
    unreachable
  )
  (func $bitnot_shift (;19;) (type 19) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const -1
    i32.xor
    local.get $b
    i32.shr_s
    return
    unreachable
  )
  (func $neg_i64 (;20;) (type 20) (param $a i64) (result i64)
    i64.const 0
    local.get $a
    i64.sub
    return
    unreachable
  )
)
