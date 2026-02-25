(module $output
  (type (;0;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (param i32 i32) (result i32)))
  (type (;6;) (func (param i32) (result i32)))
  (type (;7;) (func (param i32) (result i32)))
  (type (;8;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;9;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;10;) (func (param i32) (result i32)))
  (type (;11;) (func (param i32 i32) (result i32)))
  (type (;12;) (func (param i32 i32) (result i32)))
  (type (;13;) (func (param i32 i32 i32) (result i32)))
  (type (;14;) (func (param i64 i64 i64) (result i64)))
  (type (;15;) (func (param i32 i32 i32) (result i32)))
  (export "quadratic" (func $quadratic))
  (export "manhattan_distance" (func $manhattan_distance))
  (export "bitwise_avg" (func $bitwise_avg))
  (export "swap_nibbles" (func $swap_nibbles))
  (export "count_high_bits" (func $count_high_bits))
  (export "pack_bytes" (func $pack_bytes))
  (export "unpack_hi" (func $unpack_hi))
  (export "unpack_lo" (func $unpack_lo))
  (export "dot_product" (func $dot_product))
  (export "cross_product" (func $cross_product))
  (export "bit_reverse_nibble" (func $bit_reverse_nibble))
  (export "fibonacci_step" (func $fibonacci_step))
  (export "compound_arith_chain" (func $compound_arith_chain))
  (export "multi_let_chain" (func $multi_let_chain))
  (export "i64_compound" (func $i64_compound))
  (export "mixed_cmp_arith" (func $mixed_cmp_arith))
  (func $quadratic (;0;) (type 0) (param $a i32) (param $b i32) (param $c i32) (param $x i32) (result i32)
    local.get $a
    local.get $x
    i32.mul
    local.get $x
    i32.mul
    local.get $b
    local.get $x
    i32.mul
    i32.add
    local.get $c
    i32.add
    return
    unreachable
  )
  (func $manhattan_distance (;1;) (type 1) (param $x1 i32) (param $y1 i32) (param $x2 i32) (param $y2 i32) (result i32)
    (local $dx i32) (local $ax i32) (local $dy i32) (local $ay i32)
    local.get $x2
    local.get $x1
    i32.sub
    local.set $dx
    local.get $dx
    local.get $dx
    i32.const 31
    i32.shr_s
    i32.add
    local.get $dx
    i32.const 31
    i32.shr_s
    i32.xor
    local.set $ax
    local.get $y2
    local.get $y1
    i32.sub
    local.set $dy
    local.get $dy
    local.get $dy
    i32.const 31
    i32.shr_s
    i32.add
    local.get $dy
    i32.const 31
    i32.shr_s
    i32.xor
    local.set $ay
    local.get $ax
    local.get $ay
    i32.add
    return
    unreachable
  )
  (func $bitwise_avg (;2;) (type 2) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.and
    local.get $a
    local.get $b
    i32.xor
    i32.const 1
    i32.shr_s
    i32.add
    return
    unreachable
  )
  (func $swap_nibbles (;3;) (type 3) (param $x i32) (result i32)
    local.get $x
    i32.const 15
    i32.and
    i32.const 4
    i32.shl
    local.get $x
    i32.const 4
    i32.shr_s
    i32.const 15
    i32.and
    i32.or
    return
    unreachable
  )
  (func $count_high_bits (;4;) (type 4) (param $x i32) (result i32)
    local.get $x
    i32.const 16
    i32.shr_s
    i32.const 65535
    i32.and
    return
    unreachable
  )
  (func $pack_bytes (;5;) (type 5) (param $hi i32) (param $lo i32) (result i32)
    local.get $hi
    i32.const 255
    i32.and
    i32.const 8
    i32.shl
    local.get $lo
    i32.const 255
    i32.and
    i32.or
    return
    unreachable
  )
  (func $unpack_hi (;6;) (type 6) (param $packed i32) (result i32)
    local.get $packed
    i32.const 8
    i32.shr_s
    i32.const 255
    i32.and
    return
    unreachable
  )
  (func $unpack_lo (;7;) (type 7) (param $packed i32) (result i32)
    local.get $packed
    i32.const 255
    i32.and
    return
    unreachable
  )
  (func $dot_product (;8;) (type 8) (param $vax i32) (param $vay i32) (param $vbx i32) (param $vby i32) (result i32)
    local.get $vax
    local.get $vbx
    i32.mul
    local.get $vay
    local.get $vby
    i32.mul
    i32.add
    return
    unreachable
  )
  (func $cross_product (;9;) (type 9) (param $vax i32) (param $vay i32) (param $vbx i32) (param $vby i32) (result i32)
    local.get $vax
    local.get $vby
    i32.mul
    local.get $vay
    local.get $vbx
    i32.mul
    i32.sub
    return
    unreachable
  )
  (func $bit_reverse_nibble (;10;) (type 10) (param $x i32) (result i32)
    local.get $x
    i32.const 1
    i32.and
    i32.const 3
    i32.shl
    local.get $x
    i32.const 2
    i32.and
    i32.const 1
    i32.shl
    i32.or
    local.get $x
    i32.const 4
    i32.and
    i32.const 1
    i32.shr_s
    i32.or
    local.get $x
    i32.const 8
    i32.and
    i32.const 3
    i32.shr_s
    i32.or
    return
    unreachable
  )
  (func $fibonacci_step (;11;) (type 11) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $compound_arith_chain (;12;) (type 12) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    local.get $a
    local.get $b
    i32.sub
    i32.mul
    local.get $a
    local.get $a
    i32.mul
    local.get $b
    local.get $b
    i32.mul
    i32.sub
    i32.add
    return
    unreachable
  )
  (func $multi_let_chain (;13;) (type 13) (param $a i32) (param $b i32) (param $c i32) (result i32)
    (local $x i32) (local $y i32) (local $z i32)
    local.get $a
    local.get $b
    i32.add
    local.set $x
    local.get $x
    local.get $c
    i32.mul
    local.set $y
    local.get $y
    local.get $a
    i32.sub
    local.set $z
    local.get $z
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $i64_compound (;14;) (type 14) (param $a i64) (param $b i64) (param $c i64) (result i64)
    local.get $a
    local.get $b
    i64.add
    local.get $c
    i64.mul
    local.get $a
    local.get $b
    i64.mul
    local.get $c
    i64.add
    i64.sub
    return
    unreachable
  )
  (func $mixed_cmp_arith (;15;) (type 15) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    local.get $b
    i32.mul
    local.get $c
    i32.add
    local.get $a
    local.get $b
    local.get $c
    i32.mul
    i32.add
    i32.gt_s
    return
    unreachable
  )
)
