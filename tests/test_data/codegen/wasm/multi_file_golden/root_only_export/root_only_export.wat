(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (export "run" (func $run))
  (func $run (;0;) (type 0) (result i32)
    i32.const 2
    i32.const 3
    call $add
    return
    unreachable
  )
  (func $add (;1;) (type 1) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
)
