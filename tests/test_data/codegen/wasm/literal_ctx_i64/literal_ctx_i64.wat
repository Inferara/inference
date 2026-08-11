(module $output
  (type (;0;) (func (param i64 i64) (result i64)))
  (type (;1;) (func (param i64) (result i64)))
  (type (;2;) (func (param i64) (result i64)))
  (type (;3;) (func (param i64) (result i64)))
  (type (;4;) (func (param i64) (result i64)))
  (type (;5;) (func (param i64) (result i64)))
  (type (;6;) (func (result i64)))
  (type (;7;) (func (result i64)))
  (type (;8;) (func (result i64)))
  (type (;9;) (func (result i64)))
  (type (;10;) (func (result i64)))
  (type (;11;) (func (result i64)))
  (type (;12;) (func (result i64)))
  (type (;13;) (func (result i64)))
  (type (;14;) (func (param i32) (result i32)))
  (type (;15;) (func (result i64)))
  (type (;16;) (func (param i64) (result i64)))
  (type (;17;) (func (param i64 i64) (result i64)))
  (type (;18;) (func (param i64 i64) (result i64)))
  (type (;19;) (func (param i64) (result i64)))
  (type (;20;) (func (param i64) (result i64)))
  (type (;21;) (func (param i64) (result i64)))
  (type (;22;) (func (param i64) (result i32)))
  (type (;23;) (func (param i64) (result i64)))
  (type (;24;) (func (param i32) (result i32)))
  (type (;25;) (func (result i32)))
  (export "shift_by_literal" (func $shift_by_literal))
  (export "add_literal" (func $add_literal))
  (export "compare_with_literal" (func $compare_with_literal))
  (export "call_with_literal" (func $call_with_literal))
  (export "return_literal" (func $return_literal))
  (export "return_glued_negative" (func $return_glued_negative))
  (export "return_parenthesized_negation" (func $return_parenthesized_negation))
  (export "parenthesized_literal" (func $parenthesized_literal))
  (export "complement_literal" (func $complement_literal))
  (export "shift_of_two_literals" (func $shift_of_two_literals))
  (export "nested_literal_expression" (func $nested_literal_expression))
  (export "max_u64_argument" (func $max_u64_argument))
  (export "narrow_peer" (func $narrow_peer))
  (export "fixed_one" (func $fixed_one))
  (export "fixed_from_int" (func $fixed_from_int))
  (export "fixed_mul" (func $fixed_mul))
  (export "fixed_div" (func $fixed_div))
  (export "fixed_round_to_int" (func $fixed_round_to_int))
  (export "udiv_right" (func $udiv_right))
  (export "udiv_left" (func $udiv_left))
  (export "ucmp_left" (func $ucmp_left))
  (export "ushr_max" (func $ushr_max))
  (export "narrow_div_left" (func $narrow_div_left))
  (export "narrow_wrap_const" (func $narrow_wrap_const))
  (func $scale (;0;) (type 0) (param $v i64) (param $factor i64) (result i64)
    local.get $v
    local.get $factor
    i64.mul
    return
    unreachable
  )
  (func $take_u64 (;1;) (type 1) (param $v i64) (result i64)
    local.get $v
    return
    unreachable
  )
  (func $shift_by_literal (;2;) (type 2) (param $a i64) (result i64)
    local.get $a
    i64.const 16
    i64.shl
    return
    unreachable
  )
  (func $add_literal (;3;) (type 3) (param $a i64) (result i64)
    local.get $a
    i64.const 65536
    i64.add
    return
    unreachable
  )
  (func $compare_with_literal (;4;) (type 4) (param $a i64) (result i64)
    local.get $a
    i64.const 65536
    i64.lt_s
    if ;; label = @1
      i64.const 1
      return
    end
    i64.const 0
    return
    unreachable
  )
  (func $call_with_literal (;5;) (type 5) (param $a i64) (result i64)
    local.get $a
    i64.const 65536
    call $scale
    return
    unreachable
  )
  (func $return_literal (;6;) (type 6) (result i64)
    i64.const 65536
    return
    unreachable
  )
  (func $return_glued_negative (;7;) (type 7) (result i64)
    i64.const -42
    return
    unreachable
  )
  (func $return_parenthesized_negation (;8;) (type 8) (result i64)
    i64.const 0
    i64.const 42
    i64.sub
    return
    unreachable
  )
  (func $parenthesized_literal (;9;) (type 9) (result i64)
    i64.const 65536
    return
    unreachable
  )
  (func $complement_literal (;10;) (type 10) (result i64)
    i64.const 0
    i64.const -1
    i64.xor
    return
    unreachable
  )
  (func $shift_of_two_literals (;11;) (type 11) (result i64)
    i64.const 1
    i64.const 40
    i64.shl
    return
    unreachable
  )
  (func $nested_literal_expression (;12;) (type 12) (result i64)
    i64.const 0
    i64.const 65536
    i64.const 1
    i64.const 40
    i64.shl
    i64.add
    i64.sub
    return
    unreachable
  )
  (func $max_u64_argument (;13;) (type 13) (result i64)
    i64.const -1
    call $take_u64
    return
    unreachable
  )
  (func $narrow_peer (;14;) (type 14) (param $x i32) (result i32)
    local.get $x
    i32.const 255
    i32.and
    local.set $x
    local.get $x
    i32.const 1
    i32.add
    i32.const 255
    i32.and
    return
    unreachable
  )
  (func $fixed_one (;15;) (type 15) (result i64)
    i64.const 1
    i64.const 16
    i64.shl
    return
    unreachable
  )
  (func $fixed_from_int (;16;) (type 16) (param $n i64) (result i64)
    local.get $n
    i64.const 16
    i64.shl
    return
    unreachable
  )
  (func $fixed_mul (;17;) (type 17) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.mul
    i64.const 16
    i64.shr_s
    return
    unreachable
  )
  (func $fixed_div (;18;) (type 18) (param $a i64) (param $b i64) (result i64)
    local.get $a
    i64.const 16
    i64.shl
    local.get $b
    i64.div_s
    return
    unreachable
  )
  (func $fixed_round_to_int (;19;) (type 19) (param $x i64) (result i64)
    local.get $x
    i64.const 32768
    i64.add
    i64.const 16
    i64.shr_s
    return
    unreachable
  )
  (func $udiv_right (;20;) (type 20) (param $a i64) (result i64)
    local.get $a
    i64.const 3
    i64.div_u
    return
    unreachable
  )
  (func $udiv_left (;21;) (type 21) (param $a i64) (result i64)
    i64.const 1000
    local.get $a
    i64.div_u
    return
    unreachable
  )
  (func $ucmp_left (;22;) (type 22) (param $a i64) (result i32)
    i64.const 1000
    local.get $a
    i64.gt_u
    return
    unreachable
  )
  (func $ushr_max (;23;) (type 23) (param $a i64) (result i64)
    i64.const -1
    local.get $a
    i64.shr_u
    return
    unreachable
  )
  (func $narrow_div_left (;24;) (type 24) (param $x i32) (result i32)
    local.get $x
    i32.const 255
    i32.and
    local.set $x
    i32.const 200
    local.get $x
    i32.div_u
    i32.const 255
    i32.and
    return
    unreachable
  )
  (func $narrow_wrap_const (;25;) (type 25) (result i32)
    (local $x i32)
    i32.const 200
    i32.const 100
    i32.add
    i32.const 255
    i32.and
    local.set $x
    local.get $x
    return
    unreachable
  )
)
