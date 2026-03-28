(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (param i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "create_and_read_val" (func $create_and_read_val))
  (export "read_via_copy" (func $read_via_copy))
  (export "read_inner_y_via_copy" (func $read_inner_y_via_copy))
  (export "sum_all_fields" (func $sum_all_fields))
  (export "write_inner_field" (func $write_inner_field))
  (export "nested_struct_param" (func $nested_struct_param))
  (export "nested_struct_return" (func $nested_struct_return))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $create_and_read_val (;0;) (type 0) (result i32)
    (local $o i32) (local $__frame_ptr i32)
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
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $__frame_ptr
    local.set $o
    local.get $o
    i32.const 8
    i32.add
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
  (func $read_via_copy (;1;) (type 1) (result i32)
    (local $o i32) (local $i i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
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
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $__frame_ptr
    local.set $o
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $o
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.set $i
    local.get $i
    i32.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    unreachable
  )
  (func $read_inner_y_via_copy (;2;) (type 2) (result i32)
    (local $o i32) (local $i i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
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
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $__frame_ptr
    local.set $o
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $o
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.set $i
    local.get $i
    i32.const 4
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    unreachable
  )
  (func $sum_all_fields (;3;) (type 3) (result i32)
    (local $o i32) (local $i i32) (local $sum i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
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
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $__frame_ptr
    local.set $o
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $o
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.set $i
    local.get $i
    i32.load
    local.get $i
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $o
    i32.const 8
    i32.add
    i32.load
    i32.add
    local.set $sum
    local.get $sum
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    unreachable
  )
  (func $write_inner_field (;4;) (type 4) (result i32)
    (local $o i32) (local $i i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
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
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $__frame_ptr
    local.set $o
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $o
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.set $i
    local.get $i
    i32.const 99
    i32.store
    local.get $i
    i32.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    unreachable
  )
  (func $nested_struct_param (;5;) (type 5) (param $o i32) (result i32)
    (local $i i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
    memory.fill
    local.get $__frame_ptr
    local.get $o
    i32.const 12
    memory.copy
    local.get $__frame_ptr
    local.set $o
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $o
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.set $i
    local.get $i
    i32.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    unreachable
  )
  (func $nested_struct_return (;6;) (type 6) (param $sret i32)
    (local $o i32) (local $__frame_ptr i32)
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
    i32.const 42
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 84
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 126
    i32.store
    local.get $__frame_ptr
    local.set $o
    local.get $sret
    local.get $o
    i32.const 12
    memory.copy
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
