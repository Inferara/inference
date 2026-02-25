(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (export "public_caller" (func $public_caller))
  (func $private_helper (;0;) (type 0) (result i32)
    i32.const 1
    return
    unreachable
  )
  (func $public_caller (;1;) (type 1) (result i32)
    i32.const 42
    return
    unreachable
  )
)
