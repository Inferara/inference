(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "set_and_get" (func $set_and_get))
  (export "swap_fields" (func $swap_fields))
  (export "modify_bool" (func $modify_bool))
  (export "reassign_zeros" (func $reassign_zeros))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $set_and_get (;0;) (type 0) (result i32)
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
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.const 42
    i32.store
    local.get $p
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $swap_fields (;1;) (type 1) (result i32)
    (local $p i32) (local $tmp i32) (local $__frame_ptr i32)
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
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.load
    local.set $tmp
    local.get $p
    local.get $p
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $p
    i32.const 4
    i32.add
    local.get $tmp
    i32.store
    local.get $p
    i32.load
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
  (func $modify_bool (;2;) (type 2) (result i32)
    (local $f i32) (local $__frame_ptr i32)
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
    i32.add
    i32.const 100
    i32.store
    local.get $__frame_ptr
    local.set $f
    local.get $f
    i32.const 1
    i32.store8
    local.get $f
    i32.load8_u
    if ;; label = @1
      local.get $f
      i32.const 4
      i32.add
      i32.load
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
    unreachable
  )
  (func $reassign_zeros (;3;) (type 3) (result i32)
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
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $p
    local.get $__frame_ptr
    i32.const 0
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    local.get $__frame_ptr
    drop
    local.get $p
    i32.load
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
)
