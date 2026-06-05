(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (import "arith" "inc" (func (;0;) (type 0)))
  (import "arith" "dec" (func (;1;) (type 0)))
  (export "run" (func $run))
  (func $run (;2;) (type 1) (param $x i32) (result i32)
    local.get $x
    call 1
    call 0
    return
    unreachable
  )
)
