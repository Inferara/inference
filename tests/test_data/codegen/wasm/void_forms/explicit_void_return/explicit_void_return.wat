(module $output
  (type (;0;) (func))
  (type (;1;) (func (result i32)))
  (export "main" (func $main))
  (func $helper (;0;) (type 0)
    return
  )
  (func $main (;1;) (type 1) (result i32)
    call $helper
    i32.const 7
    return
    unreachable
  )
)
