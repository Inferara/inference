(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (result i32)))
  (export "main" (func $main))
  (func $taking (;0;) (type 0) (param $a i32) (param i32) (result i32)
    local.get $a
    return
    unreachable
  )
  (func $main (;1;) (type 1) (result i32)
    i32.const 40
    i32.const 2
    call $taking
    i32.const 2
    i32.add
    return
    unreachable
  )
)
