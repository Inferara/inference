(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (export "reassign" (func $reassign))
  (export "assign_from_param" (func $assign_from_param))
  (func $reassign (;0;) (type 0) (result i32)
    (local $c i32)
    i32.const 0
    local.set $c
    i32.const 2
    local.set $c
    local.get $c
    return
    unreachable
  )
  (func $assign_from_param (;1;) (type 1) (param $c i32) (result i32)
    (local $result i32)
    i32.const 0
    local.set $result
    local.get $c
    local.set $result
    local.get $result
    return
    unreachable
  )
)
