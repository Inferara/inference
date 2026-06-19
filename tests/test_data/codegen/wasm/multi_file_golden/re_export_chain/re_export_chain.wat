(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func))
  (export "run" (func $run))
  (func $run (;0;) (type 0) (result i32)
    i32.const 1
    i32.const 2
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
  (func $foo (;2;) (type 2))
)
