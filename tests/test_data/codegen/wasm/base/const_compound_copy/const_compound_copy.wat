(module $output
  (type (;0;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "copy_x" (func $copy_x))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $copy_x (;0;) (type 0) (result i32)
    (local $base i32) (local $P i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 7
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    local.set $base
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $base
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $P
    local.get $P
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
)
