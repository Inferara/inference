(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "copy_and_modify" (func $copy_and_modify))
  (export "copy_values_match" (func $copy_values_match))
  (export "independent_copies" (func $independent_copies))
  (export "copy_mixed" (func $copy_mixed))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $copy_and_modify (;0;) (type 0) (result i32)
    (local $p i32) (local $q i32) (local $__frame_ptr i32)
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
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $q
    local.get $q
    i32.const 99
    i32.store
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
  (func $copy_values_match (;1;) (type 1) (result i32)
    (local $p i32) (local $q i32) (local $__frame_ptr i32)
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
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $q
    local.get $q
    i32.load
    local.get $q
    i32.const 4
    i32.add
    i32.load
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
  (func $independent_copies (;2;) (type 2) (result i32)
    (local $p i32) (local $a i32) (local $b i32) (local $__frame_ptr i32)
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
    local.set $p
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $p
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $a
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $p
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $b
    local.get $a
    i32.const 100
    i32.store
    local.get $b
    i32.const 4
    i32.add
    i32.const 200
    i32.store
    local.get $p
    i32.load
    local.get $p
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    unreachable
  )
  (func $copy_mixed (;3;) (type 3) (result i64)
    (local $m i32) (local $n i32) (local $__frame_ptr i32)
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
    i32.store8
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 42
    i64.store
    local.get $__frame_ptr
    local.set $m
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $m
    i32.const 16
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $n
    local.get $n
    i32.const 8
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    unreachable
  )
)
