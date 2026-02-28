(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "read_first" (func $read_first))
  (export "read_last" (func $read_last))
  (export "read_middle" (func $read_middle))
  (export "read_with_variable" (func $read_with_variable))
  (export "read_bool_true" (func $read_bool_true))
  (export "read_bool_false" (func $read_bool_false))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $read_first (;0;) (type 0) (result i32)
    (local $arr i32) (local $__frame_ptr i32)
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
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $__frame_ptr
    local.set $arr
    local.get $arr
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
  (func $read_last (;1;) (type 1) (result i32)
    (local $arr i32) (local $__frame_ptr i32)
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
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $__frame_ptr
    local.set $arr
    local.get $arr
    i32.const 8
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
  (func $read_middle (;2;) (type 2) (result i32)
    (local $arr i32) (local $__frame_ptr i32)
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
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $__frame_ptr
    local.set $arr
    local.get $arr
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
  (func $read_with_variable (;3;) (type 3) (param $i i32) (result i32)
    (local $arr i32) (local $__frame_ptr i32)
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
    i32.const 100
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 200
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 300
    i32.store
    local.get $__frame_ptr
    local.set $arr
    local.get $arr
    local.get $i
    i32.const 4
    i32.mul
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
  (func $read_bool_true (;4;) (type 4) (result i32)
    (local $flags i32) (local $__frame_ptr i32)
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
    i32.store8
    local.get $__frame_ptr
    i32.const 1
    i32.add
    i32.const 0
    i32.store8
    local.get $__frame_ptr
    i32.const 2
    i32.add
    i32.const 1
    i32.store8
    local.get $__frame_ptr
    local.set $flags
    local.get $flags
    i32.load8_u
    if ;; label = @1
      i32.const 1
      local.get $__frame_ptr
      i32.const 16
      i32.add
      global.set 0
      return
    end
    i32.const 0
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
  (func $read_bool_false (;5;) (type 5) (result i32)
    (local $flags i32) (local $__frame_ptr i32)
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
    i32.store8
    local.get $__frame_ptr
    i32.const 1
    i32.add
    i32.const 0
    i32.store8
    local.get $__frame_ptr
    i32.const 2
    i32.add
    i32.const 1
    i32.store8
    local.get $__frame_ptr
    local.set $flags
    local.get $flags
    i32.const 1
    i32.add
    i32.load8_u
    if ;; label = @1
      i32.const 1
      local.get $__frame_ptr
      i32.const 16
      i32.add
      global.set 0
      return
    end
    i32.const 0
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
