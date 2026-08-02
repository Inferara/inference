(module $output
  (type (;0;) (func (param i32 i32)))
  (type (;1;) (func (param i32 i32)))
  (type (;2;) (func (param i32 i32)))
  (type (;3;) (func (param i32 i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "call_swap_xy" (func $call_swap_xy))
  (export "call_copy_of" (func $call_copy_of))
  (export "call_wrap" (func $call_wrap))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $swap_xy (;0;) (type 0) (param $sret i32) (param $p i32)
    local.get $sret
    local.get $p
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    local.get $p
    i32.load
    i32.store
    local.get $sret
    i32.const 8
    i32.add
    local.get $p
    i32.const 8
    i32.add
    i32.load
    i32.store
    return
    unreachable
  )
  (func $copy_of (;1;) (type 1) (param $sret i32) (param $p i32)
    local.get $sret
    local.get $p
    i32.load
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    local.get $p
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $sret
    i32.const 8
    i32.add
    local.get $p
    i32.const 8
    i32.add
    i32.load
    i32.store
    return
    unreachable
  )
  (func $rotate (;2;) (type 2) (param $sret i32) (param $q i32)
    local.get $sret
    local.get $q
    i32.const 8
    i32.add
    i32.load
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    local.get $q
    i32.load
    i32.store
    local.get $sret
    i32.const 8
    i32.add
    local.get $q
    i32.const 4
    i32.add
    i32.load
    i32.store
    return
    unreachable
  )
  (func $wrap (;3;) (type 3) (param $sret i32) (param $q i32)
    local.get $sret
    local.get $q
    call $rotate
    return
    unreachable
  )
  (func $call_swap_xy (;4;) (type 4) (result i32)
    (local $a i32) (local $b i32) (local $__frame_ptr i32)
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
    local.set $a
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $a
    call $swap_xy
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.set $b
    local.get $b
    i32.load
    i32.const 100
    i32.mul
    local.get $b
    i32.const 4
    i32.add
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $b
    i32.const 8
    i32.add
    i32.load
    i32.add
    local.get $a
    i32.load
    i32.const 1000
    i32.mul
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_copy_of (;5;) (type 5) (result i32)
    (local $c i32) (local $d i32) (local $__frame_ptr i32)
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
    i32.const 4
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 6
    i32.store
    local.get $__frame_ptr
    local.set $c
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $c
    call $copy_of
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.set $d
    local.get $d
    i32.load
    i32.const 100
    i32.mul
    local.get $d
    i32.const 4
    i32.add
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $d
    i32.const 8
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_wrap (;6;) (type 6) (result i32)
    (local $e i32) (local $w i32) (local $__frame_ptr i32)
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
    local.set $e
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $e
    call $wrap
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.set $w
    local.get $w
    i32.load
    i32.const 100
    i32.mul
    local.get $w
    i32.const 4
    i32.add
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $w
    i32.const 8
    i32.add
    i32.load
    i32.add
    local.get $e
    i32.const 8
    i32.add
    i32.load
    i32.const 1000
    i32.mul
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
)
