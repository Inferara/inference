(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (import "helpers" "ext_double" (func (;0;) (type 0)))
  (export "helper" (func $helper))
  (export "entry" (func $entry))
  (func $helper (;1;) (type 1) (param $x i32) (result i32)
    (local i32 i32 i32)
    local.get $x
    i32.const 1
    local.set 2
    local.tee 1
    local.get 2
    i32.add
    local.tee 3
    local.get 1
    i32.lt_s
    local.get 2
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 3
    return
    unreachable
  )
  (func $entry (;2;) (type 2) (param $x i32) (result i32)
    local.get $x
    call $helper
    call 0
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\01\01")
)
