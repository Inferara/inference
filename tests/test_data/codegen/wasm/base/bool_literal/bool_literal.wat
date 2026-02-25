(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (export "get_true" (func $get_true))
  (export "get_false" (func $get_false))
  (func $get_true (;0;) (type 0) (result i32)
    i32.const 1
    return
    unreachable
  )
  (func $get_false (;1;) (type 1) (result i32)
    i32.const 0
    return
    unreachable
  )
)
