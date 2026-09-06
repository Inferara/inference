(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (result i32)))
  (export "taking" (func $taking))
  (export "main" (func $main))
  (func $taking (;0;) (type 0) (param i32) (param $b i32) (result i32)
    local.get $b
    return
    unreachable
  )
  (func $main (;1;) (type 1) (result i32)
    i32.const 5
    return
    unreachable
  )
)
