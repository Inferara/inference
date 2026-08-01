(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (param i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "test_deep_inner_arr_access" (func $test_deep_inner_arr_access))
  (export "test_deep_inner_val" (func $test_deep_inner_val))
  (export "test_deep_tag" (func $test_deep_tag))
  (export "test_deep_inner_arr_sum" (func $test_deep_inner_arr_sum))
  (export "deep_param" (func $deep_param))
  (export "deep_return" (func $deep_return))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $test_deep_inner_arr_access (;0;) (type 0) (result i32)
    (local $ha i32) (local $d i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 99
    i32.store
    local.get $__frame_ptr
    local.set $ha
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $ha
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get 3
    local.get 4
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    i32.const 32
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $d
    local.get $d
    i32.const 4
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_deep_inner_val (;1;) (type 1) (result i32)
    (local $ha i32) (local $d i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 99
    i32.store
    local.get $__frame_ptr
    local.set $ha
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $ha
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get 3
    local.get 4
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    i32.const 32
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $d
    local.get $d
    i32.const 12
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_deep_tag (;2;) (type 2) (result i32)
    (local $ha i32) (local $d i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 99
    i32.store
    local.get $__frame_ptr
    local.set $ha
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $ha
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get 3
    local.get 4
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    i32.const 32
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $d
    local.get $d
    i32.const 16
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $test_deep_inner_arr_sum (;3;) (type 3) (result i32)
    (local $ha i32) (local $d i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 99
    i32.store
    local.get $__frame_ptr
    local.set $ha
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $ha
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get 3
    local.get 4
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    i32.const 32
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $d
    local.get $d
    i32.load
    local.get $d
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $d
    i32.const 8
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $deep_param (;4;) (type 4) (param $d i32) (result i32)
    local.get $d
    i32.load
    local.get $d
    i32.const 16
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
  (func $deep_return (;5;) (type 5) (param $sret i32)
    (local $ha i32) (local $d i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 99
    i32.store
    local.get $__frame_ptr
    local.set $ha
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $ha
    local.set 5
    local.set 4
    local.get 4
    local.get 5
    i64.load align=1
    i64.store align=1
    local.get 4
    local.get 5
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    i32.const 32
    i32.add
    i32.const 42
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $d
    local.get $sret
    local.get $d
    i64.load align=1
    i64.store align=1
    local.get $sret
    local.get $d
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $sret
    local.get $d
    i32.load offset=16 align=1
    i32.store offset=16 align=1
    local.get $__frame_ptr
    i32.const 48
    i32.add
    global.set 0
    return
    unreachable
  )
)
