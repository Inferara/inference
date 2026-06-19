(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (export "run" (func $run))
  (func $run (;0;) (type 0) (result i32)
    call $helper
    return
    unreachable
  )
  (func $helper (;1;) (type 1) (result i32)
    i32.const 7
    return
    unreachable
  )
)
