(module $output
  (type (;0;) (func (param i32 i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32 i32) (result i32)))
  (type (;2;) (func (param i32 i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32 i32) (result i32)))
  (type (;5;) (func (param i32 i32 i32) (result i32)))
  (type (;6;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;7;) (func (param i32 i32 i32) (result i32)))
  (type (;8;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;9;) (func (param i32 i32 i32) (result i32)))
  (type (;10;) (func (param i32 i32) (result i32)))
  (type (;11;) (func (param i32 i32) (result i32)))
  (type (;12;) (func (param i32 i32) (result i32)))
  (type (;13;) (func (param i32 i32) (result i32)))
  (type (;14;) (func (param i32) (result i32)))
  (type (;15;) (func (param i32 i32 i32) (result i32)))
  (type (;16;) (func (param i32 i32 i32) (result i32)))
  (export "paren_override_precedence" (func $paren_override_precedence))
  (export "paren_vs_no_paren_add_mul" (func $paren_vs_no_paren_add_mul))
  (export "deep_nested_3" (func $deep_nested_3))
  (export "deep_nested_4" (func $deep_nested_4))
  (export "paren_sub_chain" (func $paren_sub_chain))
  (export "paren_div_chain" (func $paren_div_chain))
  (export "paren_mixed_arith" (func $paren_mixed_arith))
  (export "paren_compare_sum" (func $paren_compare_sum))
  (export "paren_bool_complex" (func $paren_bool_complex))
  (export "paren_bitwise_chain" (func $paren_bitwise_chain))
  (export "paren_shift_arith" (func $paren_shift_arith))
  (export "paren_negated_sum" (func $paren_negated_sum))
  (export "paren_bitnot_masked" (func $paren_bitnot_masked))
  (export "paren_not_cmp" (func $paren_not_cmp))
  (export "nested_parens_deep" (func $nested_parens_deep))
  (export "paren_with_let" (func $paren_with_let))
  (export "paren_compare_complex" (func $paren_compare_complex))
  (func $paren_override_precedence (;0;) (type 0) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    local.get $c
    i32.mul
    return
    unreachable
  )
  (func $paren_vs_no_paren_add_mul (;1;) (type 1) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    local.get $b
    local.get $c
    i32.mul
    i32.add
    return
    unreachable
  )
  (func $deep_nested_3 (;2;) (type 2) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    local.get $c
    i32.add
    return
    unreachable
  )
  (func $deep_nested_4 (;3;) (type 3) (param $a i32) (param $b i32) (param $c i32) (param $d i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    local.get $c
    i32.add
    local.get $d
    i32.add
    return
    unreachable
  )
  (func $paren_sub_chain (;4;) (type 4) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    local.get $b
    local.get $c
    i32.sub
    i32.sub
    return
    unreachable
  )
  (func $paren_div_chain (;5;) (type 5) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    local.get $b
    local.get $c
    i32.div_s
    i32.div_s
    return
    unreachable
  )
  (func $paren_mixed_arith (;6;) (type 6) (param $a i32) (param $b i32) (param $c i32) (param $d i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    local.get $c
    local.get $d
    i32.sub
    i32.mul
    return
    unreachable
  )
  (func $paren_compare_sum (;7;) (type 7) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    local.get $c
    i32.gt_s
    return
    unreachable
  )
  (func $paren_bool_complex (;8;) (type 8) (param $a i32) (param $b i32) (param $c i32) (param $d i32) (result i32)
    local.get $a
    local.get $b
    i32.gt_s
    local.get $c
    local.get $d
    i32.lt_s
    i32.eq
    return
    unreachable
  )
  (func $paren_bitwise_chain (;9;) (type 9) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    local.get $b
    i32.or
    local.get $c
    i32.and
    return
    unreachable
  )
  (func $paren_shift_arith (;10;) (type 10) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const 1
    i32.add
    local.get $b
    i32.shl
    return
    unreachable
  )
  (func $paren_negated_sum (;11;) (type 11) (param $a i32) (param $b i32) (result i32)
    i32.const 0
    local.get $a
    local.get $b
    i32.add
    i32.sub
    return
    unreachable
  )
  (func $paren_bitnot_masked (;12;) (type 12) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const -1
    i32.xor
    local.get $b
    i32.and
    return
    unreachable
  )
  (func $paren_not_cmp (;13;) (type 13) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.eq
    i32.eqz
    return
    unreachable
  )
  (func $nested_parens_deep (;14;) (type 14) (param $a i32) (result i32)
    local.get $a
    i32.const 1
    i32.add
    i32.const 1
    i32.add
    i32.const 1
    i32.add
    i32.const 1
    i32.add
    return
    unreachable
  )
  (func $paren_with_let (;15;) (type 15) (param $a i32) (param $b i32) (param $c i32) (result i32)
    (local $x i32)
    local.get $a
    local.get $b
    i32.add
    local.get $c
    i32.mul
    local.set $x
    local.get $x
    return
    unreachable
  )
  (func $paren_compare_complex (;16;) (type 16) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    local.get $c
    i32.const 2
    i32.mul
    i32.ge_s
    return
    unreachable
  )
)
