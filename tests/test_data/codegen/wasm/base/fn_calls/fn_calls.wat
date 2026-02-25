(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (param i32 i32) (result i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i32)))
  (type (;8;) (func (result i32)))
  (export "call_zero" (func $call_zero))
  (export "call_identity" (func $call_identity))
  (export "call_first" (func $call_first))
  (export "let_from_call" (func $let_from_call))
  (export "forward_call" (func $forward_call))
  (func $get_zero (;0;) (type 0) (result i32)
    i32.const 0
    return
    unreachable
  )
  (func $identity_i32 (;1;) (type 1) (param $x i32) (result i32)
    local.get $x
    return
    unreachable
  )
  (func $first_i32 (;2;) (type 2) (param $a i32) (param $b i32) (result i32)
    local.get $a
    return
    unreachable
  )
  (func $call_zero (;3;) (type 3) (result i32)
    call $get_zero
    return
    unreachable
  )
  (func $call_identity (;4;) (type 4) (param $x i32) (result i32)
    local.get $x
    call $identity_i32
    return
    unreachable
  )
  (func $call_first (;5;) (type 5) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    call $first_i32
    return
    unreachable
  )
  (func $let_from_call (;6;) (type 6) (result i32)
    (local $x i32)
    call $get_zero
    local.set $x
    local.get $x
    return
    unreachable
  )
  (func $forward_call (;7;) (type 7) (result i32)
    call $forward_helper
    return
    unreachable
  )
  (func $forward_helper (;8;) (type 8) (result i32)
    i32.const 99
    return
    unreachable
  )
)
