(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (param i32) (result i32)))
  (type (;7;) (func (param i32 i32 i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "test_method_calls_method" (func $test_method_calls_method))
  (export "test_method_calls_toplevel_fn" (func $test_method_calls_toplevel_fn))
  (export "test_toplevel_fn_calls_method" (func $test_toplevel_fn_calls_method))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $double (;0;) (type 0) (param $n i32) (result i32)
    local.get $n
    i32.const 2
    i32.mul
    return
    unreachable
  )
  (func $test_method_calls_method (;1;) (type 1) (result i32)
    (local $v i32) (local $__frame_ptr i32)
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
    local.set $v
    local.get $v
    call $Vec2__sum
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
  (func $test_method_calls_toplevel_fn (;2;) (type 2) (result i32)
    (local $v i32) (local $__frame_ptr i32)
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
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    local.get $__frame_ptr
    local.set $v
    local.get $v
    call $Vec2__get_x
    call $double
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
  (func $test_toplevel_fn_calls_method (;3;) (type 3) (result i32)
    (local $v i32) (local $__frame_ptr i32)
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
    i32.const 4
    i32.const 6
    call $Vec2__new
    local.get $__frame_ptr
    local.set $v
    local.get $v
    call $Vec2__get_y
    call $double
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
  (func $Vec2__get_x (;4;) (type 4) (param $self i32) (result i32)
    local.get $self
    i32.load
    return
    unreachable
  )
  (func $Vec2__get_y (;5;) (type 5) (param $self i32) (result i32)
    local.get $self
    i32.const 4
    i32.add
    i32.load
    return
    unreachable
  )
  (func $Vec2__sum (;6;) (type 6) (param $self i32) (result i32)
    local.get $self
    call $Vec2__get_x
    local.get $self
    call $Vec2__get_y
    i32.add
    return
    unreachable
  )
  (func $Vec2__new (;7;) (type 7) (param $sret i32) (param $x i32) (param $y i32)
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
