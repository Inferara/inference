(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (result i32)))
  (export "pick" (func $pick))
  (export "main" (func $main))
  (func $pick (;0;) (type 0) (param i32) (param $n i32) (result i32)
    local.get 0
    i32.const 3
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    local.get $n
    return
    unreachable
  )
  (func $main (;1;) (type 1) (result i32)
    i32.const 1
    i32.const 11
    call $pick
    return
    unreachable
  )
)
