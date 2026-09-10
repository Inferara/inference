(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "main" (func $main))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $main (;0;) (type 0) (result i32)
    (local $c i32) (local $__frame_ptr i32)
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
    i32.const 14
    i32.store
    local.get $__frame_ptr
    local.set $c
    local.get $c
    i32.const 9
    call $Counter.scaled
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $Counter.scaled (;1;) (type 1) (param $self i32) (param i32) (result i32)
    (local i32 i32 i32)
    local.get $self
    i32.load
    i32.const 3
    local.set 3
    local.tee 2
    local.get 3
    i32.mul
    local.set 4
    local.get 3
    i32.const 0
    i32.ne
    if ;; label = @1
      local.get 3
      i32.const -1
      i32.eq
      if ;; label = @2
        local.get 2
        i32.const -2147483648
        i32.eq
        if ;; label = @3
          unreachable
        end
      else
        local.get 4
        local.get 3
        i32.div_s
        local.get 2
        i32.ne
        if ;; label = @3
          unreachable
        end
      end
    end
    local.get 4
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\01\01")
)
