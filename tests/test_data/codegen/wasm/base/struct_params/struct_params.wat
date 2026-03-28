(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (param i32) (result i64)))
  (type (;5;) (func (result i64)))
  (type (;6;) (func (param i32 i32) (result i32)))
  (type (;7;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "sum_point" (func $sum_point))
  (export "call_sum" (func $call_sum))
  (export "modify_no_effect" (func $modify_no_effect))
  (export "verify_copy_semantics" (func $verify_copy_semantics))
  (export "read_mixed" (func $read_mixed))
  (export "call_read_mixed" (func $call_read_mixed))
  (export "two_struct_params" (func $two_struct_params))
  (export "call_two_params" (func $call_two_params))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $sum_point (;0;) (type 0) (param $p i32) (result i32)
    (local $__frame_ptr i32)
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
    local.get $p
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.load
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
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $call_sum (;1;) (type 1) (result i32)
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
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $sum_point
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
  (func $modify_no_effect (;2;) (type 2) (param $p i32) (result i32)
    (local $__frame_ptr i32)
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
    local.get $p
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.const 99
    i32.store
    local.get $p
    i32.load
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
  (func $verify_copy_semantics (;3;) (type 3) (result i32)
    (local $p i32) (local $ignored i32) (local $__frame_ptr i32)
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
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $modify_no_effect
    local.set $ignored
    local.get $p
    i32.load
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
  (func $read_mixed (;4;) (type 4) (param $m i32) (result i64)
    (local $__frame_ptr i32)
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
    local.get $m
    i32.const 16
    memory.copy
    local.get $__frame_ptr
    local.set $m
    local.get $m
    i32.const 8
    i32.add
    i64.load
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
  (func $call_read_mixed (;5;) (type 5) (result i64)
    (local $m i32) (local $__frame_ptr i32)
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
    i32.const 1
    i32.store8
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 42
    i64.store
    local.get $__frame_ptr
    local.set $m
    local.get $m
    call $read_mixed
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
  (func $two_struct_params (;6;) (type 6) (param $a i32) (param $b i32) (result i32)
    (local $__frame_ptr i32)
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
    local.get $a
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    local.set $a
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $b
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $b
    local.get $a
    i32.load
    local.get $a
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $b
    i32.load
    i32.add
    local.get $b
    i32.const 4
    i32.add
    i32.load
    i32.add
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
  (func $call_two_params (;7;) (type 7) (result i32)
    (local $p1 i32) (local $p2 i32) (local $__frame_ptr i32)
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
    local.set $p1
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 40
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $p2
    local.get $p1
    local.get $p2
    call $two_struct_params
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
)
