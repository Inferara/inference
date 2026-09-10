(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (export "run" (func $run))
  (func $run (;0;) (type 0) (result i32)
    i32.const 2
    i32.const 3
    call $lib.arith.add
    return
    unreachable
  )
  (func $lib.arith.add (;1;) (type 1) (param $a i32) (param $b i32) (result i32)
    (local i32 i32 i32)
    local.get $a
    local.get $b
    local.set 3
    local.tee 2
    local.get 3
    i32.add
    local.tee 4
    local.get 2
    i32.lt_s
    local.get 3
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 4
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\01\01")
)
