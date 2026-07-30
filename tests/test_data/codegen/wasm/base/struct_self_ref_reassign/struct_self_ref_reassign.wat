(module $output
  (type (;0;) (func (result i64)))
  (type (;1;) (func (result i64)))
  (type (;2;) (func (result i64)))
  (type (;3;) (func (result i64)))
  (type (;4;) (func (result i64)))
  (type (;5;) (func (result i64)))
  (type (;6;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "swap_y" (func $swap_y))
  (export "expr_x" (func $expr_x))
  (export "expr_y" (func $expr_y))
  (export "rotate_a" (func $rotate_a))
  (export "rotate_b" (func $rotate_b))
  (export "rotate_c" (func $rotate_c))
  (export "nested_swap" (func $nested_swap))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $swap_y (;0;) (type 0) (result i64)
    (local $p i32) (local $__frame_ptr i32) (local i32 i32)
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
    i64.const 111
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 222
    i64.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $p
    i32.const 8
    i32.add
    i64.load
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.get $p
    i64.load
    i64.store
    local.get $p
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    local.get 2
    local.get 3
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $p
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
  (func $expr_x (;1;) (type 1) (result i64)
    (local $p i32) (local $__frame_ptr i32) (local i32 i32)
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
    i64.const 100
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 7
    i64.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $p
    i64.load
    local.get $p
    i32.const 8
    i32.add
    i64.load
    i64.add
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.get $p
    i64.load
    local.get $p
    i32.const 8
    i32.add
    i64.load
    i64.sub
    i64.store
    local.get $p
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    local.get 2
    local.get 3
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $p
    i64.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $expr_y (;2;) (type 2) (result i64)
    (local $p i32) (local $__frame_ptr i32) (local i32 i32)
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
    i64.const 100
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 7
    i64.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $p
    i64.load
    local.get $p
    i32.const 8
    i32.add
    i64.load
    i64.add
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.get $p
    i64.load
    local.get $p
    i32.const 8
    i32.add
    i64.load
    i64.sub
    i64.store
    local.get $p
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    local.get 2
    local.get 3
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $p
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
  (func $rotate_a (;3;) (type 3) (result i64)
    (local $q i32) (local $__frame_ptr i32) (local i32 i32)
    global.get 0
    i32.const 48
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
    i64.const 0
    i64.store offset=32
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=40
    local.get $__frame_ptr
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
    local.set $q
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.get $q
    i32.const 16
    i32.add
    i64.load
    i64.store
    local.get $__frame_ptr
    i32.const 32
    i32.add
    local.get $q
    i64.load
    i64.store
    local.get $__frame_ptr
    i32.const 40
    i32.add
    local.get $q
    i32.const 8
    i32.add
    i64.load
    i64.store
    local.get $q
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    local.get 2
    local.get 3
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get 2
    local.get 3
    i64.load offset=16 align=1
    i64.store offset=16 align=1
    local.get $q
    i64.load
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $rotate_b (;4;) (type 4) (result i64)
    (local $q i32) (local $__frame_ptr i32) (local i32 i32)
    global.get 0
    i32.const 48
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
    i64.const 0
    i64.store offset=32
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=40
    local.get $__frame_ptr
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
    local.set $q
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.get $q
    i32.const 16
    i32.add
    i64.load
    i64.store
    local.get $__frame_ptr
    i32.const 32
    i32.add
    local.get $q
    i64.load
    i64.store
    local.get $__frame_ptr
    i32.const 40
    i32.add
    local.get $q
    i32.const 8
    i32.add
    i64.load
    i64.store
    local.get $q
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    local.get 2
    local.get 3
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get 2
    local.get 3
    i64.load offset=16 align=1
    i64.store offset=16 align=1
    local.get $q
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
  (func $rotate_c (;5;) (type 5) (result i64)
    (local $q i32) (local $__frame_ptr i32) (local i32 i32)
    global.get 0
    i32.const 48
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
    i64.const 0
    i64.store offset=32
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=40
    local.get $__frame_ptr
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
    local.set $q
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.get $q
    i32.const 16
    i32.add
    i64.load
    i64.store
    local.get $__frame_ptr
    i32.const 32
    i32.add
    local.get $q
    i64.load
    i64.store
    local.get $__frame_ptr
    i32.const 40
    i32.add
    local.get $q
    i32.const 8
    i32.add
    i64.load
    i64.store
    local.get $q
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    local.get 2
    local.get 3
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get 2
    local.get 3
    i64.load offset=16 align=1
    i64.store offset=16 align=1
    local.get $q
    i32.const 16
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $nested_swap (;6;) (type 6) (result i64)
    (local $o i32) (local $__frame_ptr i32) (local i32 i32)
    global.get 0
    i32.const 64
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
    i64.const 0
    i64.store offset=32
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=40
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=48
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=56
    local.get $__frame_ptr
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
    local.set $o
    local.get $__frame_ptr
    i32.const 32
    i32.add
    local.get $o
    i32.const 16
    i32.add
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    local.get 2
    local.get 3
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    i32.const 48
    i32.add
    local.get $o
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    local.get 2
    local.get 3
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $o
    local.get $__frame_ptr
    i32.const 32
    i32.add
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    local.get 2
    local.get 3
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get 2
    local.get 3
    i64.load offset=16 align=1
    i64.store offset=16 align=1
    local.get 2
    local.get 3
    i64.load offset=24 align=1
    i64.store offset=24 align=1
    local.get $o
    i32.const 16
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 64
    i32.add
    global.set 0
    return
    unreachable
  )
)
