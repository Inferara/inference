(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (param i32 i32 i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i32)))
  (type (;8;) (func (result i32)))
  (type (;9;) (func (param i32 i32 i32)))
  (type (;10;) (func (param i32)))
  (type (;11;) (func (param i32 i32) (result i32)))
  (type (;12;) (func (param i32) (result i32)))
  (type (;13;) (func (param i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "test_new" (func $test_new))
  (export "test_new_y" (func $test_new_y))
  (export "test_origin" (func $test_origin))
  (export "test_sum_of" (func $test_sum_of))
  (export "test_mixed" (func $test_mixed))
  (export "test_return_new" (func $test_return_new))
  (export "test_return_new_get_x" (func $test_return_new_get_x))
  (export "test_return_new_get_y" (func $test_return_new_get_y))
  (export "test_standalone" (func $test_standalone))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $test_new (;0;) (type 0) (result i32)
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
    i32.const 3
    i32.const 7
    call $Point__new
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $Point__get_x
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
  (func $test_new_y (;1;) (type 1) (result i32)
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
    i32.const 3
    i32.const 7
    call $Point__new
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $Point__get_y
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
  (func $test_origin (;2;) (type 2) (result i32)
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
    call $Point__origin
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $Point__get_x
    local.get $p
    call $Point__get_y
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
  (func $test_sum_of (;3;) (type 3) (result i32)
    i32.const 10
    i32.const 20
    call $Point__sum_of
    return
    unreachable
  )
  (func $test_mixed (;4;) (type 4) (result i32)
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
    i32.const 5
    i32.const 15
    call $Point__new
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $Point__get_x
    i32.const 1
    i32.const 2
    call $Point__sum_of
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
  (func $test_return_new (;5;) (type 5) (param $sret i32) (param $x i32) (param $y i32)
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
    local.get $x
    local.get $y
    call $Point__new
    local.get $__frame_ptr
    local.set $p
    local.get $sret
    local.get $p
    i32.const 8
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
  (func $test_return_new_get_x (;6;) (type 6) (result i32)
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
    i32.const 20
    call $test_return_new
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $Point__get_x
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
  (func $test_return_new_get_y (;7;) (type 7) (result i32)
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
    i32.const 20
    call $test_return_new
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $Point__get_y
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
  (func $test_standalone (;8;) (type 8) (result i32)
    i32.const 1
    i32.const 2
    call $Point__sum_of
    drop
    i32.const 42
    return
    unreachable
  )
  (func $Point__new (;9;) (type 9) (param $sret i32) (param $x i32) (param $y i32)
    local.get $sret
    local.get $x
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    local.get $y
    i32.store
    return
    unreachable
  )
  (func $Point__origin (;10;) (type 10) (param $sret i32)
    local.get $sret
    i32.const 0
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    return
    unreachable
  )
  (func $Point__sum_of (;11;) (type 11) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $Point__get_x (;12;) (type 12) (param $self i32) (result i32)
    local.get $self
    i32.load
    return
    unreachable
  )
  (func $Point__get_y (;13;) (type 13) (param $self i32) (result i32)
    local.get $self
    i32.const 4
    i32.add
    i32.load
    return
    unreachable
  )
)
