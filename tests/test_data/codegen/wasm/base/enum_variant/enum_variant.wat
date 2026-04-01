(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (export "get_red" (func $get_red))
  (export "get_green" (func $get_green))
  (export "get_blue" (func $get_blue))
  (func $get_red (;0;) (type 0) (result i32)
    (local $c i32)
    i32.const 0
    local.set $c
    local.get $c
    return
    unreachable
  )
  (func $get_green (;1;) (type 1) (result i32)
    (local $c i32)
    i32.const 1
    local.set $c
    local.get $c
    return
    unreachable
  )
  (func $get_blue (;2;) (type 2) (result i32)
    (local $c i32)
    i32.const 2
    local.set $c
    local.get $c
    return
    unreachable
  )
)
