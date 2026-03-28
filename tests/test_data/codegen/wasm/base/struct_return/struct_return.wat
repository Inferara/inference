(module $output
  (type (;0;) (func (param i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (param i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (param i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (param i32)))
  (type (;8;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "make_point" (func $make_point))
  (export "get_x_from_make" (func $get_x_from_make))
  (export "get_y_from_make" (func $get_y_from_make))
  (export "return_var" (func $return_var))
  (export "get_x_from_var" (func $get_x_from_var))
  (export "forward_point" (func $forward_point))
  (export "get_x_from_forward" (func $get_x_from_forward))
  (export "make_mixed" (func $make_mixed))
  (export "get_val_from_mixed" (func $get_val_from_mixed))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $make_point (;0;) (type 0) (param $sret i32)
    local.get $sret
    i32.const 10
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    return
    unreachable
  )
  (func $get_x_from_make (;1;) (type 1) (result i32)
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
    call $make_point
    local.get $__frame_ptr
    local.set $p
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
  (func $get_y_from_make (;2;) (type 2) (result i32)
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
    call $make_point
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.const 4
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
  (func $return_var (;3;) (type 3) (param $sret i32)
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
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 4
    i32.store
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
  (func $get_x_from_var (;4;) (type 4) (result i32)
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
    call $return_var
    local.get $__frame_ptr
    local.set $p
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
  (func $forward_point (;5;) (type 5) (param $sret i32)
    local.get $sret
    call $make_point
    return
    unreachable
  )
  (func $get_x_from_forward (;6;) (type 6) (result i32)
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
    call $forward_point
    local.get $__frame_ptr
    local.set $p
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
  (func $make_mixed (;7;) (type 7) (param $sret i32)
    local.get $sret
    i32.const 1
    i32.store8
    local.get $sret
    i32.const 8
    i32.add
    i64.const 99
    i64.store
    return
    unreachable
  )
  (func $get_val_from_mixed (;8;) (type 8) (result i64)
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
    call $make_mixed
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
)
