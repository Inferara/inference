(module $output
  (type (;0;) (func (param i32) (result i64)))
  (type (;1;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "bump" (func $bump))
  (export "call_bump" (func $call_bump))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $bump (;0;) (type 0) (param $v i32) (result i64)
    (local $__frame_ptr i32) (local i64 i64 i64)
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
    local.get $v
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.get $v
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    local.get $v
    i64.load offset=16 align=1
    i64.store offset=16 align=1
    local.get $__frame_ptr
    local.set $v
    local.get $v
    local.get $v
    i64.load
    i64.const 100
    local.set 3
    local.tee 2
    local.get 3
    i64.add
    local.tee 4
    local.get 2
    i64.lt_s
    local.get 3
    i64.const 0
    i64.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 4
    i64.store
    local.get $v
    i64.load
    local.get $v
    i32.const 8
    i32.add
    i64.load
    local.set 3
    local.tee 2
    local.get 3
    i64.add
    local.tee 4
    local.get 2
    i64.lt_s
    local.get 3
    i64.const 0
    i64.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 4
    local.get $v
    i32.const 16
    i32.add
    i64.load
    local.set 3
    local.tee 2
    local.get 3
    i64.add
    local.tee 4
    local.get 2
    i64.lt_s
    local.get 3
    i64.const 0
    i64.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 4
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_bump (;1;) (type 1) (result i64)
    (local $p i32) (local $inner i64) (local $__frame_ptr i32) (local i64 i64 i64)
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
    i64.const 1
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 2
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 3
    i64.store
    local.get $__frame_ptr
    local.set $p
    local.get $p
    call $bump
    local.set $inner
    local.get $inner
    i64.const 1000
    local.set 4
    local.tee 3
    local.get 4
    i64.mul
    local.set 5
    local.get 4
    i64.const 0
    i64.ne
    if ;; label = @1
      local.get 4
      i64.const -1
      i64.eq
      if ;; label = @2
        local.get 3
        i64.const -9223372036854775808
        i64.eq
        if ;; label = @3
          unreachable
        end
      else
        local.get 5
        local.get 4
        i64.div_s
        local.get 3
        i64.ne
        if ;; label = @3
          unreachable
        end
      end
    end
    local.get 5
    local.get $p
    i64.load
    local.set 4
    local.tee 3
    local.get 4
    i64.add
    local.tee 5
    local.get 3
    i64.lt_s
    local.get 4
    i64.const 0
    i64.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 5
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\02\00\01")
)
