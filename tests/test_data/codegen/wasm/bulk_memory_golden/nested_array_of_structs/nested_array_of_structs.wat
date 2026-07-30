(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "grid_2d_x" (func $grid_2d_x))
  (export "grid_2d_y" (func $grid_2d_y))
  (export "cube_3d" (func $cube_3d))
  (export "grid_nonliteral" (func $grid_nonliteral))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $grid_2d_x (;0;) (type 0) (result i32)
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
    local.set $g
    local.get $g
    i32.const 16
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_2d_y (;1;) (type 1) (result i32)
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
    local.set $g
    local.get $g
    i32.const 8
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
  (func $cube_3d (;2;) (type 2) (result i32)
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
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 11
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 12
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 13
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 14
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 15
    i32.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i32.const 16
    i32.store
    local.get $__frame_ptr
    i32.const 28
    i32.add
    i32.const 17
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
  (func $grid_nonliteral (;3;) (type 3) (result i32)
    (local $p i32) (local $g i32) (local $__frame_ptr i32)
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
    i32.const 21
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 22
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $p
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.get $p
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 32
    i32.add
    local.get $p
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $g
    local.get $g
    i32.const 16
    i32.add
    i32.const 8
    i32.add
    i32.load
    local.get $g
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
)
