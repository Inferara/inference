(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;2;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (result i32)))
  (export "nest_8_add" (func $nest_8_add))
  (export "nest_mixed_ops" (func $nest_mixed_ops))
  (export "nest_comparison" (func $nest_comparison))
  (export "nest_call_in_expr" (func $nest_call_in_expr))
  (export "nest_paren_deep" (func $nest_paren_deep))
  (func $nest_8_add (;0;) (type 0) (result i32)
    i32.const 1
    i32.const 2
    i32.add
    i32.const 3
    i32.add
    i32.const 4
    i32.add
    i32.const 5
    i32.add
    i32.const 6
    i32.add
    i32.const 7
    i32.add
    i32.const 8
    i32.add
    i32.const 9
    i32.add
    return
    unreachable
  )
  (func $nest_mixed_ops (;1;) (type 1) (param $a i32) (param $b i32) (param $c i32) (param $d i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    local.get $c
    local.get $d
    i32.sub
    i32.mul
    local.get $a
    local.get $b
    i32.sub
    local.get $c
    local.get $d
    i32.add
    i32.mul
    i32.add
    return
    unreachable
  )
  (func $nest_comparison (;2;) (type 2) (param $a i32) (param $b i32) (param $c i32) (param $d i32) (result i32)
    local.get $a
    local.get $b
    i32.gt_s
    if (result i32) ;; label = @1
      local.get $c
      local.get $d
      i32.lt_s
      if (result i32) ;; label = @2
        i32.const 1
      else
        local.get $a
        local.get $c
        i32.eq
      end
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $helper_square (;3;) (type 3) (param $x i32) (result i32)
    local.get $x
    local.get $x
    i32.mul
    return
    unreachable
  )
  (func $nest_call_in_expr (;4;) (type 4) (param $x i32) (result i32)
    local.get $x
    call $helper_square
    local.get $x
    i32.const 1
    i32.add
    call $helper_square
    i32.add
    i32.const 2
    i32.mul
    return
    unreachable
  )
  (func $nest_paren_deep (;5;) (type 5) (result i32)
    i32.const 1
    i32.const 2
    i32.add
    i32.const 3
    i32.add
    i32.const 4
    i32.add
    i32.const 5
    i32.add
    return
    unreachable
  )
)
