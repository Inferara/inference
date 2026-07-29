(module $output
  (type (;0;) (func (result i64)))
  (type (;1;) (func (result i64)))
  (type (;2;) (func (result i64)))
  (type (;3;) (func (result i64)))
  (type (;4;) (func (result i64)))
  (type (;5;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "grid_first" (func $grid_first))
  (export "grid_last" (func $grid_last))
  (export "grid_sum" (func $grid_sum))
  (export "grid_of_expressions" (func $grid_of_expressions))
  (export "grid_complement_element" (func $grid_complement_element))
  (export "grid_unsigned_max" (func $grid_unsigned_max))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $grid_first (;0;) (type 0) (result i64)
    (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
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
    i64.const 0
    i64.store offset=16
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=24
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 1099511627776
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 2199023255552
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 3298534883328
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i64.const 4398046511104
    i64.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i64.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_last (;1;) (type 1) (result i64)
    (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
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
    i64.const 0
    i64.store offset=16
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=24
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 1099511627776
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 2199023255552
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 3298534883328
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i64.const 4398046511104
    i64.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i32.const 16
    i32.add
    i32.const 8
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_sum (;2;) (type 2) (result i64)
    (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
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
    i64.const 0
    i64.store offset=16
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=24
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 1
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 2
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 3
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i64.const 4
    i64.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i64.load
    local.get $g
    i32.const 8
    i32.add
    i64.load
    i64.add
    local.get $g
    i32.const 16
    i32.add
    i64.load
    i64.add
    local.get $g
    i32.const 16
    i32.add
    i32.const 8
    i32.add
    i64.load
    i64.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_of_expressions (;3;) (type 3) (result i64)
    (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
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
    i64.const 0
    i64.store offset=16
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=24
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 1
    i64.const 40
    i64.shl
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 1
    i64.const 41
    i64.shl
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 1
    i64.const 40
    i64.shl
    i64.const 1
    i64.add
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i64.const 0
    i64.const -1
    i64.xor
    i64.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i32.const 16
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_complement_element (;4;) (type 4) (result i64)
    (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
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
    i64.const 0
    i64.store offset=16
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=24
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 1
    i64.const 40
    i64.shl
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 1
    i64.const 41
    i64.shl
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 1
    i64.const 40
    i64.shl
    i64.const 1
    i64.add
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i64.const 0
    i64.const -1
    i64.xor
    i64.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i32.const 16
    i32.add
    i32.const 8
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_unsigned_max (;5;) (type 5) (result i64)
    (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
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
    i64.const 0
    i64.store offset=16
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=24
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 1
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 2
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i64.const -1
    i64.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i32.const 16
    i32.add
    i32.const 8
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
)
