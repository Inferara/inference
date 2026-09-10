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
    (local $a i32) (local $__frame_ptr i32) (local i32 i32 i32 i32 i32)
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
    local.set 6
    local.set 5
    local.get 5
    local.get 6
    i64.load align=1
    i64.store align=1
    local.get $a
    i32.load
    i32.const 100
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
    local.get $a
    i32.const 4
    i32.add
    i32.load
    local.set 3
    local.tee 2
    local.get 3
    i32.add
    local.tee 4
    local.get 2
    i32.lt_s
    local.get 3
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 4
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $rotate3 (;1;) (type 1) (result i32)
    (local $b i32) (local $__frame_ptr i32) (local i32 i32 i32 i32 i32)
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
    local.set 6
    local.set 5
    local.get 5
    local.get 6
    i64.load align=1
    i64.store align=1
    local.get 5
    local.get 6
    i32.load offset=8 align=1
    i32.store offset=8 align=1
    local.get $b
    i32.load
    i32.const 100
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
    local.get $b
    i32.const 4
    i32.add
    i32.load
    i32.const 10
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
    local.set 3
    local.tee 2
    local.get 3
    i32.add
    local.tee 4
    local.get 2
    i32.lt_s
    local.get 3
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 4
    local.get $b
    i32.const 8
    i32.add
    i32.load
    local.set 3
    local.tee 2
    local.get 3
    i32.add
    local.tee 4
    local.get 2
    i32.lt_s
    local.get 3
    i32.const 0
    i32.lt_s
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
  (func $swap01_i64 (;2;) (type 2) (result i64)
    (local $c i32) (local $hundred i64) (local $__frame_ptr i32) (local i64 i64 i64 i32 i32)
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
    local.set 7
    local.set 6
    local.get 6
    local.get 7
    i64.load align=1
    i64.store align=1
    local.get 6
    local.get 7
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    i64.const 100
    local.set $hundred
    local.get $c
    i64.load
    local.get $hundred
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
    local.get $c
    i32.const 8
    i32.add
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
  (@custom "inference.checked" (after code) "\01\03\00\01\02")
)
