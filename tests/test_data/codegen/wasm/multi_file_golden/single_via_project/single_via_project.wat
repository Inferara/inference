(module $output
  (type (;0;) (func (result i32)))
  (export "hello_world" (func $hello_world))
  (func $hello_world (;0;) (type 0) (result i32)
    i32.const 42
    return
    unreachable
  )
)
