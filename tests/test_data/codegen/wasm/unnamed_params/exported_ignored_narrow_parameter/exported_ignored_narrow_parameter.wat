(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (result i32)))
  (export "f" (func $f))
  (export "main" (func $main))
  (func $f (;0;) (type 0) (param i32) (param $b i32) (result i32)
    local.get 0
    i32.const 255
    i32.and
    local.set 0
    local.get $b
    return
    unreachable
  )
  (func $main (;1;) (type 1) (result i32)
    i32.const 3
    i32.const 7
    call $f
    return
    unreachable
  )
)
