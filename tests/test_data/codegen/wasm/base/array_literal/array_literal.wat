(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "i32_array" (func $i32_array))
  (export "bool_array" (func $bool_array))
  (export "two_arrays" (func $two_arrays))
  (export "single_element" (func $single_element))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $i32_array (;0;) (type 0) (result i32)
    (local $arr i32) (local $__frame_ptr i32)
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
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $__frame_ptr
    local.set $arr
    i32.const 0
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $bool_array (;1;) (type 1) (result i32)
    (local $flags i32) (local $__frame_ptr i32)
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
    i32.const 2
    i32.add
    i32.const 1
    i32.store8
    local.get $__frame_ptr
    local.set $flags
    i32.const 0
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $two_arrays (;2;) (type 2) (result i32)
    (local $a i32) (local $b i32) (local $__frame_ptr i32)
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
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $a
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
    i32.const 8
    i32.add
    local.set $b
    i32.const 0
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $single_element (;3;) (type 3) (result i32)
    (local $one i32) (local $__frame_ptr i32)
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
    i32.const 42
    i32.store
    local.get $__frame_ptr
    local.set $one
    i32.const 0
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
)
