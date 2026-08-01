(module $output
  (type (;0;) (func (param i32) (result i64)))
  (type (;1;) (func (param i32) (result i64)))
  (type (;2;) (func (param i32 i32) (result i64)))
  (type (;3;) (func (param i32 i32) (result i64)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (result i64)))
  (type (;6;) (func (result i64)))
  (type (;7;) (func (result i32)))
  (type (;8;) (func (result i64)))
  (type (;9;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "sum_ends_17" (func $sum_ends_17))
  (export "sum_ends_20" (func $sum_ends_20))
  (export "pick_20" (func $pick_20))
  (export "clobber_20" (func $clobber_20))
  (export "sum_ends_i32_20" (func $sum_ends_i32_20))
  (export "call_sum_ends_17" (func $call_sum_ends_17))
  (export "call_sum_ends_20" (func $call_sum_ends_20))
  (export "call_sum_ends_i32_20" (func $call_sum_ends_i32_20))
  (export "value_semantics_20" (func $value_semantics_20))
  (export "copy_preserves_zero" (func $copy_preserves_zero))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $sum_ends_17 (;0;) (type 0) (param $data i32) (result i64)
    local.get $data
    i64.load
    local.get $data
    i32.const 128
    i32.add
    i64.load
    i64.add
    return
    unreachable
  )
  (func $sum_ends_20 (;1;) (type 1) (param $data i32) (result i64)
    local.get $data
    i64.load
    local.get $data
    i32.const 152
    i32.add
    i64.load
    i64.add
    return
    unreachable
  )
  (func $pick_20 (;2;) (type 2) (param $data i32) (param $i i32) (result i64)
    (local i32)
    local.get $data
    local.get $i
    local.tee 2
    local.get 2
    i32.const 20
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    i64.load
    return
    unreachable
  )
  (func $clobber_20 (;3;) (type 3) (param $data i32) (param $i i32) (result i64)
    (local $__frame_ptr i32) (local i32 i32)
    global.get 0
    i32.const 160
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    i32.const 0
    local.set 4
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 4
      i32.add
      i64.const 0
      i64.store
      local.get $__frame_ptr
      local.get 4
      i32.add
      i64.const 0
      i64.store offset=8
      local.get 4
      i32.const 16
      i32.add
      local.tee 4
      i32.const 160
      i32.ne
      br_if 0 (;@1;)
    end
    i32.const 0
    local.set 4
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 4
      i32.add
      local.get $data
      local.get 4
      i32.add
      i64.load align=1
      i64.store align=1
      local.get 4
      i32.const 8
      i32.add
      local.tee 4
      i32.const 160
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    local.set $data
    local.get $data
    local.get $i
    local.tee 3
    local.get 3
    i32.const 20
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    i64.const 999
    i64.store
    local.get $data
    local.get $i
    local.tee 3
    local.get 3
    i32.const 20
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $sum_ends_i32_20 (;4;) (type 4) (param $data i32) (result i32)
    (local $__frame_ptr i32)
    global.get 0
    i32.const 80
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
    i64.const 0
    i64.store offset=32
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=40
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=48
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=56
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=64
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=72
    local.get $__frame_ptr
    local.get $data
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.get $data
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    local.get $data
    i64.load offset=16 align=1
    i64.store offset=16 align=1
    local.get $__frame_ptr
    local.get $data
    i64.load offset=24 align=1
    i64.store offset=24 align=1
    local.get $__frame_ptr
    local.get $data
    i64.load offset=32 align=1
    i64.store offset=32 align=1
    local.get $__frame_ptr
    local.get $data
    i64.load offset=40 align=1
    i64.store offset=40 align=1
    local.get $__frame_ptr
    local.get $data
    i64.load offset=48 align=1
    i64.store offset=48 align=1
    local.get $__frame_ptr
    local.get $data
    i64.load offset=56 align=1
    i64.store offset=56 align=1
    local.get $__frame_ptr
    local.get $data
    i64.load offset=64 align=1
    i64.store offset=64 align=1
    local.get $__frame_ptr
    local.get $data
    i64.load offset=72 align=1
    i64.store offset=72 align=1
    local.get $__frame_ptr
    local.set $data
    local.get $data
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    local.get $data
    i32.load
    local.get $data
    i32.const 76
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 80
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_sum_ends_17 (;5;) (type 5) (result i64)
    (local $a i32) (local $__frame_ptr i32) (local i32)
    global.get 0
    i32.const 144
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    i32.const 0
    local.set 2
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 2
      i32.add
      i64.const 0
      i64.store
      local.get $__frame_ptr
      local.get 2
      i32.add
      i64.const 0
      i64.store offset=8
      local.get 2
      i32.const 16
      i32.add
      local.tee 2
      i32.const 144
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 11
    i64.store
    local.get $__frame_ptr
    i32.const 128
    i32.add
    i64.const 22
    i64.store
    local.get $__frame_ptr
    local.set $a
    local.get $a
    call $sum_ends_17
    local.get $__frame_ptr
    i32.const 144
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_sum_ends_20 (;6;) (type 6) (result i64)
    (local $b i32) (local $__frame_ptr i32) (local i32)
    global.get 0
    i32.const 160
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    i32.const 0
    local.set 2
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 2
      i32.add
      i64.const 0
      i64.store
      local.get $__frame_ptr
      local.get 2
      i32.add
      i64.const 0
      i64.store offset=8
      local.get 2
      i32.const 16
      i32.add
      local.tee 2
      i32.const 160
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 11
    i64.store
    local.get $__frame_ptr
    i32.const 152
    i32.add
    i64.const 29
    i64.store
    local.get $__frame_ptr
    local.set $b
    local.get $b
    call $sum_ends_20
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_sum_ends_i32_20 (;7;) (type 7) (result i32)
    (local $c i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 80
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
    i64.const 0
    i64.store offset=32
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=40
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=48
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=56
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=64
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=72
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i32.const 13
    i32.store
    local.get $__frame_ptr
    i32.const 76
    i32.add
    i32.const 31
    i32.store
    local.get $__frame_ptr
    local.set $c
    local.get $c
    call $sum_ends_i32_20
    local.get $__frame_ptr
    i32.const 80
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $value_semantics_20 (;8;) (type 8) (result i64)
    (local $d i32) (local $ignored i64) (local $__frame_ptr i32) (local i32)
    global.get 0
    i32.const 160
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    i32.const 0
    local.set 3
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 3
      i32.add
      i64.const 0
      i64.store
      local.get $__frame_ptr
      local.get 3
      i32.add
      i64.const 0
      i64.store offset=8
      local.get 3
      i32.const 16
      i32.add
      local.tee 3
      i32.const 160
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 11
    i64.store
    local.get $__frame_ptr
    i32.const 152
    i32.add
    i64.const 29
    i64.store
    local.get $__frame_ptr
    local.set $d
    local.get $d
    i32.const 0
    call $clobber_20
    local.set $ignored
    local.get $d
    i64.load
    i64.const 1000
    i64.mul
    local.get $d
    i32.const 152
    i32.add
    i64.load
    i64.add
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $copy_preserves_zero (;9;) (type 9) (result i64)
    (local $e i32) (local $__frame_ptr i32) (local i32)
    global.get 0
    i32.const 160
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    i32.const 0
    local.set 2
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 2
      i32.add
      i64.const 0
      i64.store
      local.get $__frame_ptr
      local.get 2
      i32.add
      i64.const 0
      i64.store offset=8
      local.get 2
      i32.const 16
      i32.add
      local.tee 2
      i32.const 160
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 11
    i64.store
    local.get $__frame_ptr
    i32.const 152
    i32.add
    i64.const 29
    i64.store
    local.get $__frame_ptr
    local.set $e
    local.get $e
    i32.const 10
    call $pick_20
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
)
