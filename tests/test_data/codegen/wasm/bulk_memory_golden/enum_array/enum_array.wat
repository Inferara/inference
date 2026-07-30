(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "second_color" (func $second_color))
  (export "third_tag" (func $third_tag))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $second_color (;0;) (type 0) (result i32)
    (local $colors i32) (local $__frame_ptr i32)
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
    i32.const 0
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $colors
    local.get $colors
    i32.const 4
    i32.add
    i32.load
    i32.const 1
    i32.eq
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
    unreachable
  )
  (func $third_tag (;1;) (type 1) (result i32)
    (local $colors i32) (local $__frame_ptr i32)
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
    i32.const 0
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $colors
    local.get $colors
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
)
