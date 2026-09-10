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
    (local i32 i32 i32)
    i32.const 40
    i32.const 2
    call $taking
    i32.const 2
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\01\01")
)
