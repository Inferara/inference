(module $output
  (type (;0;) (func (param i32 i32) (result i64)))
  (type (;1;) (func (param i32) (result i64)))
  (type (;2;) (func (param i32 i32) (result i64)))
  (type (;3;) (func (result i64)))
  (type (;4;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "dot" (func $dot))
  (export "sum4" (func $sum4))
  (export "pick4" (func $pick4))
  (export "call_dot" (func $call_dot))
  (export "call_sum4" (func $call_sum4))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $dot (;0;) (type 0) (param $a i32) (param $b i32) (result i64)
    local.get $a
    i64.load
    local.get $b
    i64.load
    i64.mul
    local.get $a
    i32.const 8
    i32.add
    i64.load
    local.get $b
    i32.const 8
    i32.add
    i64.load
    i64.mul
    i64.add
    local.get $a
    i32.const 16
    i32.add
    i64.load
    local.get $b
    i32.const 16
    i32.add
    i64.load
    i64.mul
    i64.add
    return
    unreachable
  )
  (func $sum4 (;1;) (type 1) (param $v i32) (result i64)
    local.get $v
    i64.load
    local.get $v
    i32.const 8
    i32.add
    i64.load
    i64.add
    local.get $v
    i32.const 16
    i32.add
    i64.load
    i64.add
    local.get $v
    i32.const 24
    i32.add
    i64.load
    i64.add
    return
    unreachable
  )
  (func $pick4 (;2;) (type 2) (param $v i32) (param $i i32) (result i64)
    (local i32)
    local.get $v
    local.get $i
    local.tee 2
    local.get 2
    i32.const 4
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    i64.load
    return
    unreachable
  )
  (func $call_dot (;3;) (type 3) (result i64)
    (local $p i32) (local $q i32) (local $__frame_ptr i32)
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
    i64.const 2
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 3
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 4
    i64.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i64.const 5
    i64.store
    local.get $__frame_ptr
    i32.const 32
    i32.add
    i64.const 7
    i64.store
    local.get $__frame_ptr
    i32.const 40
    i32.add
    i64.const 11
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.set $q
    local.get $p
    local.get $q
    call $dot
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_sum4 (;4;) (type 4) (result i64)
    (local $w i32) (local $__frame_ptr i32)
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
    i64.const 10
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 20
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 30
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i64.const 40
    i64.store
    local.get $__frame_ptr
    local.set $w
    local.get $w
    call $sum4
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
)
