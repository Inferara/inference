(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (import "sortlib" "probe" (func (;0;) (type 0)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "run" (func $run))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $double (;1;) (type 1) (param $v i32) (result i32)
    local.get $v
    local.get $v
    i32.add
    return
    unreachable
  )
  (func $run (;2;) (type 2) (result i32)
    (local $p i32) (local $__frame_ptr i32)
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
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $Pair.touch
    i32.const 100
    i32.mul
    local.get $p
    i32.load
    i32.const 10
    i32.mul
    i32.add
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
    unreachable
  )
  (func $Pair.touch (;3;) (type 3) (param $self i32) (result i32)
    (local $x i32) (local $__frame_ptr i32)
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
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.set $self
    i32.const 1
    local.get $self
    call 0
    i32.add
    local.set $x
    local.get $x
    call $double
    local.get $self
    i32.load
    i32.const 1000
    i32.mul
    i32.add
    local.get $self
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
