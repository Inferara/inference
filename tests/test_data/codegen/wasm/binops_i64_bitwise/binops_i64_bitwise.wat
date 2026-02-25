(module $output
  (type (;0;) (func (param i64 i64) (result i64)))
  (type (;1;) (func (param i64 i64) (result i64)))
  (type (;2;) (func (param i64 i64) (result i64)))
  (type (;3;) (func (param i64 i64) (result i64)))
  (type (;4;) (func (param i64 i64) (result i64)))
  (type (;5;) (func (param i64 i64) (result i64)))
  (type (;6;) (func (param i64) (result i64)))
  (type (;7;) (func (param i64) (result i64)))
  (type (;8;) (func (param i64) (result i64)))
  (type (;9;) (func (param i64) (result i64)))
  (type (;10;) (func (param i64) (result i64)))
  (type (;11;) (func (param i64) (result i64)))
  (type (;12;) (func (param i64) (result i64)))
  (type (;13;) (func (param i64 i64 i64) (result i64)))
  (type (;14;) (func (param i64) (result i64)))
  (export "bitand_i64" (func $bitand_i64))
  (export "bitor_i64" (func $bitor_i64))
  (export "bitxor_i64" (func $bitxor_i64))
  (export "shl_i64" (func $shl_i64))
  (export "shr_i64_signed" (func $shr_i64_signed))
  (export "shr_u64" (func $shr_u64))
  (export "bitnot_i64" (func $bitnot_i64))
  (export "bitand_mask_i64" (func $bitand_mask_i64))
  (export "bitor_set_bit_i64" (func $bitor_set_bit_i64))
  (export "bitxor_flip_i64" (func $bitxor_flip_i64))
  (export "shl_by_one_i64" (func $shl_by_one_i64))
  (export "shr_signed_negative_i64" (func $shr_signed_negative_i64))
  (export "shr_unsigned_high_bit_i64" (func $shr_unsigned_high_bit_i64))
  (export "bitand_bitor_chain_i64" (func $bitand_bitor_chain_i64))
  (export "mask_and_shift_i64" (func $mask_and_shift_i64))
  (func $bitand_i64 (;0;) (type 0) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.and
    return
    unreachable
  )
  (func $bitor_i64 (;1;) (type 1) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.or
    return
    unreachable
  )
  (func $bitxor_i64 (;2;) (type 2) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.xor
    return
    unreachable
  )
  (func $shl_i64 (;3;) (type 3) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.shl
    return
    unreachable
  )
  (func $shr_i64_signed (;4;) (type 4) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.shr_s
    return
    unreachable
  )
  (func $shr_u64 (;5;) (type 5) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.shr_u
    return
    unreachable
  )
  (func $bitnot_i64 (;6;) (type 6) (param $a i64) (result i64)
    local.get $a
    i64.const -1
    i64.xor
    return
    unreachable
  )
  (func $bitand_mask_i64 (;7;) (type 7) (param $a i64) (result i64)
    (local $m i64)
    i64.const 255
    local.set $m
    local.get $a
    local.get $m
    i64.and
    return
    unreachable
  )
  (func $bitor_set_bit_i64 (;8;) (type 8) (param $a i64) (result i64)
    (local $one i64)
    i64.const 1
    local.set $one
    local.get $a
    local.get $one
    i64.or
    return
    unreachable
  )
  (func $bitxor_flip_i64 (;9;) (type 9) (param $a i64) (result i64)
    (local $neg i64)
    i64.const -1
    local.set $neg
    local.get $a
    local.get $neg
    i64.xor
    return
    unreachable
  )
  (func $shl_by_one_i64 (;10;) (type 10) (param $a i64) (result i64)
    (local $one i64)
    i64.const 1
    local.set $one
    local.get $a
    local.get $one
    i64.shl
    return
    unreachable
  )
  (func $shr_signed_negative_i64 (;11;) (type 11) (param $a i64) (result i64)
    (local $one i64)
    i64.const 1
    local.set $one
    local.get $a
    local.get $one
    i64.shr_s
    return
    unreachable
  )
  (func $shr_unsigned_high_bit_i64 (;12;) (type 12) (param $a i64) (result i64)
    (local $one i64)
    i64.const 1
    local.set $one
    local.get $a
    local.get $one
    i64.shr_u
    return
    unreachable
  )
  (func $bitand_bitor_chain_i64 (;13;) (type 13) (param $a i64) (param $b i64) (param $c i64) (result i64)
    local.get $a
    local.get $b
    i64.and
    local.get $c
    i64.or
    return
    unreachable
  )
  (func $mask_and_shift_i64 (;14;) (type 14) (param $a i64) (result i64)
    (local $s i64) (local $m i64)
    i64.const 4
    local.set $s
    i64.const 15
    local.set $m
    local.get $a
    local.get $s
    i64.shr_s
    local.get $m
    i64.and
    return
    unreachable
  )
)
