(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32 i32 i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "run" (func $run))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $run (;0;) (type 0) (result i32)
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
    i32.const 3
    i32.const 4
    call $make
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $make (;1;) (type 1) (param $sret i32) (param $a i32) (param $b i32)
    local.get $sret
    local.get $a
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    local.get $b
    i32.store
    return
    unreachable
  )
)
