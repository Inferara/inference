(module $output
  (type (;0;) (func (result i32)))
  (export "main" (func $main))
  (func $main (;0;) (type 0) (result i32)
    (local $x i32)
    i32.const 40
    local.set $x
    local.get $x
    i32.const 2
    i32.add
    return
    unreachable
  )
)
