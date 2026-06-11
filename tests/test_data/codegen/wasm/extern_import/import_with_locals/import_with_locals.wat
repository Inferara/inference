(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (import "helpers" "ext_double" (func (;0;) (type 0)))
  (export "helper" (func $helper))
  (export "entry" (func $entry))
  (func $helper (;1;) (type 1) (param $x i32) (result i32)
    local.get $x
    i32.const 1
    i32.add
    return
    unreachable
  )
  (func $entry (;2;) (type 2) (param $x i32) (result i32)
    local.get $x
    call $helper
    call 0
    return
    unreachable
  )
)
