(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (import "arith" "sum" (func (;0;) (type 0)))
  (import "arith" "neg" (func (;1;) (type 1)))
  (export "compute" (func $compute))
  (func $compute (;2;) (type 2) (param $x i32) (result i32)
    local.get $x
    call 1
    i32.const 3
    call 0
    return
    unreachable
  )
)
