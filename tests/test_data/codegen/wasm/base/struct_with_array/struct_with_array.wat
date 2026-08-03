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
  (type (;9;) (func (param i32 i32) (result i32)))
  (type (;10;) (func (param i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "create_and_read_val" (func $create_and_read_val))
  (export "read_arr_first" (func $read_arr_first))
  (export "read_arr_last" (func $read_arr_last))
  (export "write_arr_element" (func $write_arr_element))
  (export "sum_arr_and_val" (func $sum_arr_and_val))
  (export "struct_with_array_param" (func $struct_with_array_param))
  (export "struct_with_array_return" (func $struct_with_array_return))
  (export "test_method_get_arr_elem" (func $test_method_get_arr_elem))
  (export "test_method_sum_arr" (func $test_method_sum_arr))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $create_and_read_val (;0;) (type 0) (result i32)
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
    i32.const 12
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $s
    i32.const 12
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $read_arr_first (;1;) (type 1) (result i32)
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
    i32.const 12
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $s
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $read_arr_last (;2;) (type 2) (result i32)
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
    i32.const 12
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $s
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
  (func $write_arr_element (;3;) (type 3) (result i32)
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
    i32.const 12
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $s
    i32.const 4
    i32.add
    i32.const 99
    i32.store
    local.get $s
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
  (func $sum_arr_and_val (;4;) (type 4) (result i32)
    (local $s i32) (local $sum i32) (local $__frame_ptr i32)
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
    i32.const 12
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $s
    i32.load
    local.get $s
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $s
    i32.const 8
    i32.add
    i32.load
    i32.add
    local.get $s
    i32.const 12
    i32.add
    i32.load
    i32.add
    local.set $sum
    local.get $sum
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $struct_with_array_param (;5;) (type 5) (param $s i32) (result i32)
    local.get $s
    i32.load
    local.get $s
    i32.const 12
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
  (func $struct_with_array_return (;6;) (type 6) (param $sret i32)
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
    local.set $s
    local.get $sret
    local.get $s
    i64.load align=1
    i64.store align=1
    local.get $sret
    local.get $s
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_method_get_arr_elem (;7;) (type 7) (result i32)
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
    i32.const 12
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $s
    i32.const 1
    call $HasArray.get_arr_elem
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_method_sum_arr (;8;) (type 8) (result i32)
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
    i32.const 12
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $s
    call $HasArray.sum_arr
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $HasArray.get_arr_elem (;9;) (type 9) (param $self i32) (param $idx i32) (result i32)
    (local i32)
    local.get $self
    local.get $idx
    local.tee 2
    local.get 2
    i32.const 3
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
  (func $HasArray.sum_arr (;10;) (type 10) (param $self i32) (result i32)
    local.get $self
    i32.load
    local.get $self
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $self
    i32.const 8
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
)
