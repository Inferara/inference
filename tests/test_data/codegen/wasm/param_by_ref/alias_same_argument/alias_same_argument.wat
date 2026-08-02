(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "add_xy" (func $add_xy))
  (export "write_b" (func $write_b))
  (export "same_twice" (func $same_twice))
  (export "distinct_arguments" (func $distinct_arguments))
  (export "write_b_same_variable" (func $write_b_same_variable))
  (export "write_b_distinct_variables" (func $write_b_distinct_variables))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $add_xy (;0;) (type 0) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.load
    i32.const 1000
    i32.mul
    local.get $a
    i32.const 4
    i32.add
    i32.load
    i32.const 100
    i32.mul
    i32.add
    local.get $b
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $b
    i32.const 4
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
  (func $write_b (;1;) (type 1) (param $a i32) (param $b i32) (result i32)
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
    local.get $b
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.set $b
    local.get $b
    i32.const 99
    i32.store
    local.get $a
    i32.load
    i32.const 1000
    i32.mul
    local.get $b
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $same_twice (;2;) (type 2) (result i32)
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
    local.set $s
    local.get $s
    local.get $s
    call $add_xy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $distinct_arguments (;3;) (type 3) (result i32)
    (local $m i32) (local $n i32) (local $__frame_ptr i32)
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
    local.set $m
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
    local.set $n
    local.get $m
    local.get $n
    call $add_xy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $write_b_same_variable (;4;) (type 4) (result i32)
    (local $t i32) (local $inner i32) (local $__frame_ptr i32)
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
    local.set $t
    local.get $t
    local.get $t
    call $write_b
    local.set $inner
    local.get $inner
    i32.const 100
    i32.mul
    local.get $t
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $write_b_distinct_variables (;5;) (type 5) (result i32)
    (local $u i32) (local $w i32) (local $inner i32) (local $__frame_ptr i32)
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
    local.set $u
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
    local.set $w
    local.get $u
    local.get $w
    call $write_b
    local.set $inner
    local.get $inner
    i32.const 100
    i32.mul
    local.get $u
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $w
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
