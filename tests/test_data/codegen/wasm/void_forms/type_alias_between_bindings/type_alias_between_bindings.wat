(module $output
  (type (;0;) (func (result i32)))
  (export "main" (func $main))
  (func $main (;0;) (type 0) (result i32)
    (local $a i32) (local $b i32)
    i32.const 20
    local.set $a
    i32.const 22
    local.set $b
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
)
