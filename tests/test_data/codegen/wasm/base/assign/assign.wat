(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i64)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i32)))
  (type (;8;) (func (param i32) (result i32)))
  (type (;9;) (func (param i32) (result i32)))
  (export "assign_simple_i32" (func $assign_simple_i32))
  (export "assign_simple_i64" (func $assign_simple_i64))
  (export "assign_from_expr" (func $assign_from_expr))
  (export "assign_from_param" (func $assign_from_param))
  (export "assign_multiple" (func $assign_multiple))
  (export "assign_from_call" (func $assign_from_call))
  (export "assign_bool" (func $assign_bool))
  (export "assign_in_if" (func $assign_in_if))
  (export "assign_param_mut" (func $assign_param_mut))
  (func $assign_simple_i32 (;0;) (type 0) (result i32)
    (local $x i32)
    i32.const 0
    local.set $x
    i32.const 42
    local.set $x
    local.get $x
    return
    unreachable
  )
  (func $assign_simple_i64 (;1;) (type 1) (result i64)
    (local $x i64)
    i64.const 0
    local.set $x
    i64.const 42
    local.set $x
    local.get $x
    return
    unreachable
  )
  (func $assign_from_expr (;2;) (type 2) (result i32)
    (local $x i32)
    i32.const 0
    local.set $x
    i32.const 1
    i32.const 2
    i32.add
    local.set $x
    local.get $x
    return
    unreachable
  )
  (func $assign_from_param (;3;) (type 3) (param $a i32) (result i32)
    (local $x i32)
    i32.const 0
    local.set $x
    local.get $a
    local.set $x
    local.get $x
    return
    unreachable
  )
  (func $assign_multiple (;4;) (type 4) (result i32)
    (local $x i32)
    i32.const 1
    local.set $x
    i32.const 2
    local.set $x
    i32.const 3
    local.set $x
    local.get $x
    return
    unreachable
  )
  (func $get_three (;5;) (type 5) (result i32)
    i32.const 3
    return
    unreachable
  )
  (func $assign_from_call (;6;) (type 6) (result i32)
    (local $x i32)
    i32.const 0
    local.set $x
    call $get_three
    local.set $x
    local.get $x
    return
    unreachable
  )
  (func $assign_bool (;7;) (type 7) (result i32)
    (local $flag i32)
    i32.const 0
    local.set $flag
    i32.const 1
    local.set $flag
    local.get $flag
    return
    unreachable
  )
  (func $assign_in_if (;8;) (type 8) (param $x i32) (result i32)
    (local $result i32)
    i32.const 0
    local.set $result
    local.get $x
    i32.const 0
    i32.gt_s
    if ;; label = @1
      local.get $x
      local.set $result
    end
    local.get $result
    return
    unreachable
  )
  (func $assign_param_mut (;9;) (type 9) (param $a i32) (result i32)
    i32.const 99
    local.set $a
    local.get $a
    return
    unreachable
  )
)
