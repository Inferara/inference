(module $output
  (type (;0;) (func (result i32)))
  (export "main" (func $main))
  (func $main (;0;) (type 0) (result i32)
    i32.const 7
    return
    unreachable
  )
)
