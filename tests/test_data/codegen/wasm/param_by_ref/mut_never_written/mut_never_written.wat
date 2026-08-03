(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "peek_struct" (func $peek_struct))
  (export "peek_array" (func $peek_array))
  (export "call_peek_struct" (func $call_peek_struct))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $peek_struct (;0;) (type 0) (param $p i32) (result i32)
    local.get $p
    i32.load
    i32.const 10
    i32.mul
    local.get $p
    i32.const 4
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
  (func $peek_array (;1;) (type 1) (param $data i32) (param $i i32) (result i32)
    (local i32)
    local.get $data
    local.get $i
    local.tee 2
    local.get 2
    i32.const 8
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 4
    i32.mul
    i32.add
    i32.load
    return
    unreachable
  )
  (func $call_peek_struct (;2;) (type 2) (result i32)
    (local $s i32) (local $inner i32) (local $__frame_ptr i32)
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
    i32.const 4
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $s
    call $peek_struct
    local.set $inner
    local.get $inner
    i32.const 100
    i32.mul
    local.get $s
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $s
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
)
