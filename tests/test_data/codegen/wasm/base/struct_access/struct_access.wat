(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "get_x" (func $get_x))
  (export "get_y" (func $get_y))
  (export "sum_fields" (func $sum_fields))
  (export "get_single_val" (func $get_single_val))
  (export "get_mixed_val" (func $get_mixed_val))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $get_x (;0;) (type 0) (result i32)
    (local $p i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $get_y (;1;) (type 1) (result i32)
    (local $p i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.const 4
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $sum_fields (;2;) (type 2) (result i32)
    (local $p i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.load
    local.get $p
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $get_single_val (;3;) (type 3) (result i32)
    (local $s i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 42
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $s
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $get_mixed_val (;4;) (type 4) (result i64)
    (local $m i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 1
    i32.store8
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 100
    i64.store
    local.get $__frame_ptr
    local.set $m
    local.get $m
    i32.const 8
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
)
