(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "write_and_read" (func $write_and_read))
  (export "write_multiple" (func $write_multiple))
  (export "swap_elements" (func $swap_elements))
  (export "write_computed_index" (func $write_computed_index))
  (export "write_bool" (func $write_bool))
  (export "reassign_zeros" (func $reassign_zeros))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $write_and_read (;0;) (type 0) (result i32)
    (local $arr i32) (local $__frame_ptr i32)
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
    local.set $arr
    local.get $arr
    i32.const 42
    i32.store
    local.get $arr
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $write_multiple (;1;) (type 1) (result i32)
    (local $arr i32) (local $__frame_ptr i32)
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
    local.set $arr
    local.get $arr
    i32.const 10
    i32.store
    local.get $arr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $arr
    i32.const 8
    i32.add
    i32.const 30
    i32.store
    local.get $arr
    i32.load
    local.get $arr
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $arr
    i32.const 8
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
  (func $swap_elements (;2;) (type 2) (result i32)
    (local $arr i32) (local $tmp i32) (local $__frame_ptr i32)
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
    local.set $arr
    local.get $arr
    i32.load
    local.set $tmp
    local.get $arr
    local.get $arr
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $arr
    i32.const 4
    i32.add
    local.get $tmp
    i32.store
    local.get $arr
    i32.load
    i32.const 10
    i32.mul
    local.get $arr
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
  (func $write_computed_index (;3;) (type 3) (param $i i32) (result i32)
    (local $arr i32) (local $__frame_ptr i32) (local i32)
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
    local.set $arr
    local.get $arr
    local.get $i
    i32.const 1
    i32.add
    local.tee 3
    local.get 3
    i32.const 3
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 4
    i32.mul
    i32.add
    i32.const 99
    i32.store
    local.get $arr
    i32.const 8
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $write_bool (;4;) (type 4) (result i32)
    (local $flags i32) (local $__frame_ptr i32)
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
    local.set $flags
    local.get $flags
    i32.const 1
    i32.store8
    local.get $flags
    i32.const 2
    i32.add
    i32.const 1
    i32.store8
    local.get $flags
    i32.load8_u
    if ;; label = @1
      local.get $flags
      i32.const 2
      i32.add
      i32.load8_u
      if ;; label = @2
        i32.const 1
        local.get $__frame_ptr
        i32.const 16
        i32.add
        global.set 0
        return
      end
    end
    i32.const 0
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $reassign_zeros (;5;) (type 5) (result i32)
    (local $arr i32) (local $__frame_ptr i32)
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
    local.set $arr
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i32.const 0
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 0
    i32.store
    local.get $__frame_ptr
    drop
    local.get $arr
    i32.load
    local.get $arr
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $arr
    i32.const 8
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
