(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (param i32 i32)))
  (type (;3;) (func (param i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "test_to_array_first" (func $test_to_array_first))
  (export "test_to_array_second" (func $test_to_array_second))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $test_to_array_first (;0;) (type 0) (result i32)
    (local $p i32) (local $arr i32) (local $__frame_ptr i32)
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
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    call $Pair.to_array
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $arr
    local.get $arr
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_to_array_second (;1;) (type 1) (result i32)
    (local $p i32) (local $arr i32) (local $__frame_ptr i32)
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
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    call $Pair.to_array
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $arr
    local.get $arr
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
  (func $Pair.to_array (;2;) (type 2) (param $sret i32) (param $self i32)
    (local $result i32) (local $__frame_ptr i32)
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
    local.get $self
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    local.get $self
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    local.set $result
    local.get $sret
    local.get $result
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $Pair.get_x (;3;) (type 3) (param $self i32) (result i32)
    local.get $self
    i32.load
    return
    unreachable
  )
)
