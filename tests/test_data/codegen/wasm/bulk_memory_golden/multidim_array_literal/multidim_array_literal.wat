(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "grid_2d" (func $grid_2d))
  (export "cube_3d" (func $cube_3d))
  (export "grid_mixed_zero" (func $grid_mixed_zero))
  (export "grid_rows" (func $grid_rows))
  (export "grid_u8" (func $grid_u8))
  (export "grid_i64" (func $grid_i64))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $grid_2d (;0;) (type 0) (result i32)
    (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
    memory.fill
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
    i32.const 8
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 6
    i32.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i32.const 12
    i32.add
    i32.const 8
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $cube_3d (;1;) (type 1) (result i32)
    (local $c i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
    memory.fill
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
    i32.const 8
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 6
    i32.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    i32.const 28
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    local.set $c
    local.get $c
    i32.const 16
    i32.add
    i32.const 4
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_mixed_zero (;2;) (type 2) (result i32)
    (local $g i32) (local $__frame_ptr i32)
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
    i32.const 4
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i32.const 4
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_rows (;3;) (type 3) (result i32)
    (local $r i32) (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 48
    memory.fill
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
    i32.const 8
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    local.set $r
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $r
    i32.const 12
    memory.copy
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.get $r
    i32.const 12
    memory.copy
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.set $g
    local.get $g
    i32.const 12
    i32.add
    i32.const 8
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_u8 (;4;) (type 4) (result i32)
    (local $r i32) (local $g i32) (local $__frame_ptr i32)
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
    i32.const 0
    i32.add
    i32.const 1
    i32.store8
    local.get $__frame_ptr
    i32.const 1
    i32.add
    i32.const 2
    i32.store8
    local.get $__frame_ptr
    i32.const 2
    i32.add
    i32.const 3
    i32.store8
    local.get $__frame_ptr
    local.set $r
    local.get $__frame_ptr
    i32.const 3
    i32.add
    local.get $r
    i32.const 3
    memory.copy
    local.get $__frame_ptr
    i32.const 6
    i32.add
    local.get $r
    i32.const 3
    memory.copy
    local.get $__frame_ptr
    i32.const 3
    i32.add
    local.set $g
    local.get $g
    i32.const 3
    i32.add
    i32.const 2
    i32.add
    i32.load8_u
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_i64 (;5;) (type 5) (result i64)
    (local $r i32) (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 48
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 48
    memory.fill
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 3
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 4
    i64.store
    local.get $__frame_ptr
    local.set $r
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $r
    i32.const 16
    memory.copy
    local.get $__frame_ptr
    i32.const 32
    i32.add
    local.get $r
    i32.const 16
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $g
    local.get $g
    i32.const 16
    i32.add
    i32.const 8
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
)
