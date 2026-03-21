(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i32)))
  (type (;8;) (func (result i32)))
  (type (;9;) (func (param i32 i32 i32 i32)))
  (type (;10;) (func (param i32 i32 i32)))
  (type (;11;) (func (param i32) (result i32)))
  (type (;12;) (func (param i32) (result i32)))
  (type (;13;) (func (param i32 i32 i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "test_translate_x" (func $test_translate_x))
  (export "test_translate_y" (func $test_translate_y))
  (export "test_scale_x" (func $test_scale_x))
  (export "test_scale_y" (func $test_scale_y))
  (export "test_original_unchanged_x" (func $test_original_unchanged_x))
  (export "test_original_unchanged_y" (func $test_original_unchanged_y))
  (export "test_new_returns_struct_x" (func $test_new_returns_struct_x))
  (export "test_new_returns_struct_y" (func $test_new_returns_struct_y))
  (export "test_return_translated" (func $test_return_translated))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $test_translate_x (;0;) (type 0) (result i32)
    (local $p i32) (local $p2 i32) (local $__frame_ptr i32)
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
    i32.const 0
    i32.add
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    i32.const 5
    i32.const 3
    call $Point__translate
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $p2
    local.get $p2
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
  (func $test_translate_y (;1;) (type 1) (result i32)
    (local $p i32) (local $p2 i32) (local $__frame_ptr i32)
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
    i32.const 0
    i32.add
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    i32.const 5
    i32.const 3
    call $Point__translate
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $p2
    local.get $p2
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
  (func $test_scale_x (;2;) (type 2) (result i32)
    (local $p i32) (local $p2 i32) (local $__frame_ptr i32)
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
    i32.const 0
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    i32.const 4
    call $Point__scale
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $p2
    local.get $p2
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
  (func $test_scale_y (;3;) (type 3) (result i32)
    (local $p i32) (local $p2 i32) (local $__frame_ptr i32)
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
    i32.const 0
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    i32.const 4
    call $Point__scale
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $p2
    local.get $p2
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
  (func $test_original_unchanged_x (;4;) (type 4) (result i32)
    (local $p i32) (local $p2 i32) (local $__frame_ptr i32)
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
    i32.const 0
    i32.add
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    i32.const 5
    i32.const 3
    call $Point__translate
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $p2
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
  (func $test_original_unchanged_y (;5;) (type 5) (result i32)
    (local $p i32) (local $p2 i32) (local $__frame_ptr i32)
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
    i32.const 0
    i32.add
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    i32.const 5
    i32.const 3
    call $Point__translate
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $p2
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
  (func $test_new_returns_struct_x (;6;) (type 6) (result i32)
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
    i32.const 42
    i32.const 99
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
  (func $test_new_returns_struct_y (;7;) (type 7) (result i32)
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
    i32.const 42
    i32.const 99
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
  (func $test_return_translated (;8;) (type 8) (result i32)
    (local $p i32) (local $p2 i32) (local $__frame_ptr i32)
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
    i32.const 0
    i32.add
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    i32.const 10
    i32.const 20
    call $Point__translate
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $p2
    local.get $p2
    call $Point__get_x
    local.get $p2
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
  (func $Point__translate (;9;) (type 9) (param $sret i32) (param $self i32) (param $dx i32) (param $dy i32)
    local.get $sret
    local.get $self
    i32.load
    local.get $dx
    i32.add
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    local.get $self
    i32.const 4
    i32.add
    i32.load
    local.get $dy
    i32.add
    i32.store
    return
    unreachable
  )
  (func $Point__scale (;10;) (type 10) (param $sret i32) (param $self i32) (param $f i32)
    local.get $sret
    local.get $self
    i32.load
    local.get $f
    i32.mul
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    local.get $self
    i32.const 4
    i32.add
    i32.load
    local.get $f
    i32.mul
    i32.store
    return
    unreachable
  )
  (func $Point__get_x (;11;) (type 11) (param $self i32) (result i32)
    local.get $self
    i32.load
    return
    unreachable
  )
  (func $Point__get_y (;12;) (type 12) (param $self i32) (result i32)
    local.get $self
    i32.const 4
    i32.add
    i32.load
    return
    unreachable
  )
  (func $Point__new (;13;) (type 13) (param $sret i32) (param $x i32) (param $y i32)
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
)
