(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (param i32)))
  (type (;7;) (func (param i32 i32)))
  (type (;8;) (func (param i32 i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "test_get" (func $test_get))
  (export "test_increment_value_semantics" (func $test_increment_value_semantics))
  (export "test_add_value_semantics" (func $test_add_value_semantics))
  (export "test_mut_self_does_not_affect_caller" (func $test_mut_self_does_not_affect_caller))
  (export "test_multiple_increments" (func $test_multiple_increments))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $test_get (;0;) (type 0) (result i32)
    (local $c i32) (local $__frame_ptr i32)
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
    i32.const 10
    i32.store
    local.get $__frame_ptr
    local.set $c
    local.get $c
    call $Counter.get
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_increment_value_semantics (;1;) (type 1) (result i32)
    (local $c i32) (local $__frame_ptr i32)
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
    i32.const 10
    i32.store
    local.get $__frame_ptr
    local.set $c
    local.get $c
    call $Counter.increment
    local.get $c
    call $Counter.get
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_add_value_semantics (;2;) (type 2) (result i32)
    (local $c i32) (local $__frame_ptr i32)
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
    i32.const 10
    i32.store
    local.get $__frame_ptr
    local.set $c
    local.get $c
    i32.const 5
    call $Counter.add
    local.get $c
    call $Counter.get
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_mut_self_does_not_affect_caller (;3;) (type 3) (result i32)
    (local $c i32) (local $__frame_ptr i32)
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
    i32.const 42
    call $Counter.new
    local.get $__frame_ptr
    local.set $c
    local.get $c
    call $Counter.increment
    local.get $c
    i32.const 100
    call $Counter.add
    local.get $c
    call $Counter.get
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_multiple_increments (;4;) (type 4) (result i32)
    (local $c i32) (local $__frame_ptr i32)
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
    call $Counter.new
    local.get $__frame_ptr
    local.set $c
    local.get $c
    call $Counter.increment
    local.get $c
    call $Counter.increment
    local.get $c
    call $Counter.increment
    local.get $c
    call $Counter.get
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $Counter.get (;5;) (type 5) (param $self i32) (result i32)
    local.get $self
    i32.load
    return
    unreachable
  )
  (func $Counter.increment (;6;) (type 6) (param $self i32)
    (local $__frame_ptr i32)
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
    local.get $self
    i32.load align=1
    i32.store align=1
    local.get $__frame_ptr
    local.set $self
    local.get $self
    local.get $self
    i32.load
    i32.const 1
    i32.add
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
  )
  (func $Counter.add (;7;) (type 7) (param $self i32) (param $n i32)
    (local $__frame_ptr i32)
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
    local.get $self
    i32.load align=1
    i32.store align=1
    local.get $__frame_ptr
    local.set $self
    local.get $self
    local.get $self
    i32.load
    local.get $n
    i32.add
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
  )
  (func $Counter.new (;8;) (type 8) (param $sret i32) (param $v i32)
    local.get $sret
    local.get $v
    i32.store
    return
    unreachable
  )
)
