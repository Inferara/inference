(module $output
  (type (;0;) (func (param i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "if_else_compound_overlap" (func $if_else_compound_overlap))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $if_else_compound_overlap (;0;) (type 0) (param $cond i32) (result i32)
    (local $a i32) (local $b i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $cond
    if ;; label = @1
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
      i32.const 12
      i32.add
      i32.const 4
      i32.store
      local.get $__frame_ptr
      local.set $a
      local.get $a
      i32.load
      local.get $__frame_ptr
      i32.const 16
      i32.add
      global.set 0
      return
    else
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
      i32.const 12
      i32.add
      i32.const 40
      i32.store
      local.get $__frame_ptr
      local.set $b
      local.get $b
      i32.const 4
      i32.add
      i32.load
      local.get $__frame_ptr
      i32.const 16
      i32.add
      global.set 0
      return
    end
    unreachable
  )
)
