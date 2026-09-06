(module $output
  (type (;0;) (func (param i32)))
  (type (;1;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "main" (func $main))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $helper (;0;) (type 0) (param $early i32)
    (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i64.const 0
    i64.store
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=8
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $a
    local.get $early
    if ;; label = @1
      local.get $__frame_ptr
      i32.const 16
      i32.add
      global.set 0
      return
    end
    local.get $a
    i32.const 4
    i32.add
    local.get $a
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
  )
  (func $main (;1;) (type 1) (result i32)
    i32.const 1
    call $helper
    i32.const 0
    call $helper
    i32.const 3
    return
    unreachable
  )
)
