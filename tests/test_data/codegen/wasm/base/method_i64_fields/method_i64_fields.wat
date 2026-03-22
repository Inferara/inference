(module $output
  (type (;0;) (func (result i64)))
  (type (;1;) (func (result i64)))
  (type (;2;) (func (result i64)))
  (type (;3;) (func (param i32) (result i64)))
  (type (;4;) (func (param i32) (result i64)))
  (type (;5;) (func (param i32) (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "test_get_a" (func $test_get_a))
  (export "test_get_b" (func $test_get_b))
  (export "test_sum" (func $test_sum))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $test_get_a (;0;) (type 0) (result i64)
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
    i32.const 0
    i32.add
    i64.const 100
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 200
    i64.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $BigPair.get_a
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
  (func $test_get_b (;1;) (type 1) (result i64)
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
    i32.const 0
    i32.add
    i64.const 100
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 200
    i64.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $BigPair.get_b
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
  (func $test_sum (;2;) (type 2) (result i64)
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
    i32.const 0
    i32.add
    i64.const 100
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 200
    i64.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $BigPair.sum
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
  (func $BigPair.get_a (;3;) (type 3) (param $self i32) (result i64)
    local.get $self
    i64.load
    return
    unreachable
  )
  (func $BigPair.get_b (;4;) (type 4) (param $self i32) (result i64)
    local.get $self
    i32.const 8
    i32.add
    i64.load
    return
    unreachable
  )
  (func $BigPair.sum (;5;) (type 5) (param $self i32) (result i64)
    local.get $self
    i64.load
    local.get $self
    i32.const 8
    i32.add
    i64.load
    i64.add
    return
    unreachable
  )
)
