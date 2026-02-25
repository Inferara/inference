(module $output
  (type (;0;) (func (result i32)))
  (export "hello_const_i32" (func $hello_const_i32))
  (func $hello_const_i32 (;0;) (type 0) (result i32)
    (local $a i32)
    i32.const 42
    local.set $a
    local.get $a
    return
    unreachable
  )
)
