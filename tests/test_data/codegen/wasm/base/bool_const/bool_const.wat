(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (export "get_const_true" (func $get_const_true))
  (export "get_const_false" (func $get_const_false))
  (func $get_const_true (;0;) (type 0) (result i32)
    (local $b i32)
    i32.const 1
    local.set $b
    local.get $b
    return
    unreachable
  )
  (func $get_const_false (;1;) (type 1) (result i32)
    (local $b i32)
    i32.const 0
    local.set $b
    local.get $b
    return
    unreachable
  )
)
