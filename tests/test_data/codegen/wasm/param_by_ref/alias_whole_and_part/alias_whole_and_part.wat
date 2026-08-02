(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "whole_and_field" (func $whole_and_field))
  (export "whole_and_element" (func $whole_and_element))
  (export "call_whole_and_field" (func $call_whole_and_field))
  (export "call_whole_and_element" (func $call_whole_and_element))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $whole_and_field (;0;) (type 0) (param $o i32) (param $i i32) (result i32)
    local.get $o
    i32.load
    i32.const 1000
    i32.mul
    local.get $o
    i32.const 4
    i32.add
    i32.load
    i32.const 100
    i32.mul
    i32.add
    local.get $i
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $i
    i32.const 4
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
  (func $whole_and_element (;1;) (type 1) (param $all i32) (param $one i32) (result i32)
    local.get $all
    i32.load
    i32.const 1000
    i32.mul
    local.get $all
    i32.const 16
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.const 100
    i32.mul
    i32.add
    local.get $one
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $one
    i32.const 4
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
  (func $call_whole_and_field (;2;) (type 2) (result i32)
    (local $s i32) (local $__frame_ptr i32)
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
    i32.const 7
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $s
    local.get $s
    i32.const 4
    i32.add
    call $whole_and_field
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_whole_and_element (;3;) (type 3) (param $i i32) (result i32)
    (local $items i32) (local $__frame_ptr i32) (local i32)
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
    local.set $items
    local.get $items
    local.get $items
    local.get $i
    local.tee 3
    local.get 3
    i32.const 3
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    call $whole_and_element
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
)
