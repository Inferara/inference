(module $output
  (type (;0;) (func (param i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (import "sortlib" "sort_pair" (func (;0;) (type 0)))
  (import "sortlib" "sort_arr" (func (;1;) (type 0)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "forward_struct" (func $forward_struct))
  (export "forward_array" (func $forward_array))
  (export "no_forward" (func $no_forward))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $forward_struct (;2;) (type 1) (param $p i32) (result i32)
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
    local.get $p
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call 0
    local.get $p
    i32.load
    i32.const 10
    i32.mul
    local.get $p
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $forward_array (;3;) (type 2) (param $a i32) (result i32)
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
    i32.const 0
    i32.add
    local.get $a
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    local.get $a
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    local.set $a
    local.get $a
    call 1
    local.get $a
    i32.load
    i32.const 10
    i32.mul
    local.get $a
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $no_forward (;4;) (type 3) (param $q i32) (result i32)
    local.get $q
    i32.load
    i32.const 10
    i32.mul
    local.get $q
    i32.const 4
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
)
