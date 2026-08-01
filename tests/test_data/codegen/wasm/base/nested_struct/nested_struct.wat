(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (param i32)))
  (type (;7;) (func (result i32)))
  (type (;8;) (func (result i32)))
  (type (;9;) (func (param i32) (result i32)))
  (type (;10;) (func (param i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "create_and_read_val" (func $create_and_read_val))
  (export "read_via_copy" (func $read_via_copy))
  (export "read_inner_y_via_copy" (func $read_inner_y_via_copy))
  (export "sum_all_fields" (func $sum_all_fields))
  (export "write_inner_field" (func $write_inner_field))
  (export "nested_struct_param" (func $nested_struct_param))
  (export "nested_struct_return" (func $nested_struct_return))
  (export "test_method_get_inner_x" (func $test_method_get_inner_x))
  (export "test_method_sum_inner" (func $test_method_sum_inner))
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
    i64.const 0
    i64.store
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=8
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
    unreachable
  )
  (func $read_via_copy (;1;) (type 1) (result i32)
    (local $o i32) (local $i i32) (local $__frame_ptr i32) (local i32 i32)
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
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
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
    unreachable
  )
  (func $read_inner_y_via_copy (;2;) (type 2) (result i32)
    (local $o i32) (local $i i32) (local $__frame_ptr i32) (local i32 i32)
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
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
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
    unreachable
  )
  (func $sum_all_fields (;3;) (type 3) (result i32)
    (local $o i32) (local $i i32) (local $sum i32) (local $__frame_ptr i32) (local i32 i32)
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
    local.set 5
    local.set 4
    local.get 4
    local.get 5
    i64.load align=1
    i64.store align=1
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
    unreachable
  )
  (func $write_inner_field (;4;) (type 4) (result i32)
    (local $o i32) (local $i i32) (local $__frame_ptr i32) (local i32 i32)
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
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
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
    unreachable
  )
  (func $nested_struct_param (;5;) (type 5) (param $o i32) (result i32)
    (local $i i32) (local $__frame_ptr i32) (local i32 i32)
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
    local.get $o
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.set $i
    local.get $i
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
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
    i64.const 0
    i64.store
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=8
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
    i64.load align=1
    i64.store align=1
    local.get $sret
    local.get $o
    i32.load offset=8 align=1
    i32.store offset=8 align=1
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_method_get_inner_x (;7;) (type 7) (result i32)
    (local $o i32) (local $__frame_ptr i32)
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
    i32.const 55
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 66
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 77
    i32.store
    local.get $__frame_ptr
    local.set $o
    local.get $o
    call $Outer.get_inner_x
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_method_sum_inner (;8;) (type 8) (result i32)
    (local $o i32) (local $__frame_ptr i32)
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
    call $Outer.sum_inner
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $Outer.get_inner_x (;9;) (type 9) (param $self i32) (result i32)
    (local $i i32) (local $__frame_ptr i32) (local i32 i32)
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
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.set $i
    local.get $i
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $Outer.sum_inner (;10;) (type 10) (param $self i32) (result i32)
    (local $i i32) (local $__frame_ptr i32) (local i32 i32)
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
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.set $i
    local.get $i
    i32.load
    local.get $i
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
