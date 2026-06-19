(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "grid_read" (func $grid_read))
  (export "grid_sum" (func $grid_sum))
  (export "grid_write" (func $grid_write))
  (export "cube_read" (func $cube_read))
  (export "aos_grid_read" (func $aos_grid_read))
  (export "mixed_grid" (func $mixed_grid))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $grid_read (;0;) (type 0) (param $i i32) (param $j i32) (result i32)
    (local $g i32) (local $__frame_ptr i32) (local i32)
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
    local.get $i
    local.tee 4
    local.get 4
    i32.const 2
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 12
    i32.mul
    i32.add
    local.get $j
    local.tee 4
    local.get 4
    i32.const 3
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 4
    i32.mul
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_sum (;1;) (type 1) (result i32)
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
    i32.load
    local.get $g
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $g
    i32.const 8
    i32.add
    i32.load
    i32.add
    local.get $g
    i32.const 12
    i32.add
    i32.load
    i32.add
    local.get $g
    i32.const 12
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $g
    i32.const 12
    i32.add
    i32.const 8
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_write (;2;) (type 2) (result i32)
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
    i32.const 99
    i32.store
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
  (func $cube_read (;3;) (type 3) (result i32)
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
  (func $aos_grid_read (;4;) (type 4) (result i32)
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
    local.get $g
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $mixed_grid (;5;) (type 5) (result i32)
    (local $m i32) (local $__frame_ptr i32)
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
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 100
    i32.store
    local.get $__frame_ptr
    local.set $m
    local.get $m
    i32.load
    local.get $m
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $m
    i32.const 4
    i32.add
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $m
    i32.const 20
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
)
