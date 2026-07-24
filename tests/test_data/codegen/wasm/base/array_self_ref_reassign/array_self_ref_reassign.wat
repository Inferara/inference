(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "swap01" (func $swap01))
  (export "rotate3" (func $rotate3))
  (export "swap01_i64" (func $swap01_i64))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $swap01 (;0;) (type 0) (result i32)
    (local $a i32) (local $__frame_ptr i32)
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
    i32.const 9
    i32.store
    local.get $__frame_ptr
    local.set $a
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $a
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $a
    i32.load
    i32.store
    local.get $a
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 8
    memory.copy
    local.get $a
    i32.load
    i32.const 100
    i32.mul
    local.get $a
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
  (func $rotate3 (;1;) (type 1) (result i32)
    (local $b i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
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
    i32.const 8
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    local.set $b
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $b
    i32.const 8
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $b
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    local.get $b
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $b
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 12
    memory.copy
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
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $swap01_i64 (;2;) (type 2) (result i64)
    (local $c i32) (local $hundred i64) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
    memory.fill
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 5
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 9
    i64.store
    local.get $__frame_ptr
    local.set $c
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $c
    i32.const 8
    i32.add
    i64.load
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    local.get $c
    i64.load
    i64.store
    local.get $c
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 16
    memory.copy
    i64.const 100
    local.set $hundred
    local.get $c
    i64.load
    local.get $hundred
    i64.mul
    local.get $c
    i32.const 8
    i32.add
    i64.load
    i64.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
)
