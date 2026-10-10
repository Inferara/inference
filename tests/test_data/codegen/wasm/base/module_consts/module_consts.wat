(module $output
  (type (;0;) (func (param i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i64)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i32)))
  (type (;8;) (func (result i32)))
  (type (;9;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65344)
  (export "prime_sum" (func $prime_sum))
  (export "step" (func $step))
  (export "corner_x" (func $corner_x))
  (export "heading" (func $heading))
  (export "scale" (func $scale))
  (export "ones" (func $ones))
  (export "derived" (func $derived))
  (export "copy_then_write" (func $copy_then_write))
  (export "returned" (func $returned))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $primes (;0;) (type 0) (param $sret i32)
    (local i32 i32)
    local.get $sret
    i32.const 65344
    local.set 2
    local.set 1
    local.get 1
    local.get 2
    i64.load align=1
    i64.store align=1
    local.get 1
    local.get 2
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    return
    unreachable
  )
  (func $prime_sum (;1;) (type 1) (result i32)
    (local $i i32) (local $sum i32) (local i32 i32 i32 i32)
    i32.const 0
    local.set $i
    i32.const 0
    local.set $sum
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 4
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $sum
        i32.const 65344
        local.get $i
        local.tee 2
        local.get 2
        i32.const 4
        i32.ge_u
        if ;; label = @3
          unreachable
        end
        i32.const 4
        i32.mul
        i32.add
        i32.load
        local.set 4
        local.tee 3
        local.get 4
        i32.add
        local.tee 5
        local.get 3
        i32.lt_s
        local.get 4
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 5
        local.set $sum
        local.get $i
        i32.const 1
        local.set 4
        local.tee 3
        local.get 4
        i32.add
        local.tee 5
        local.get 3
        i32.lt_s
        local.get 4
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 5
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $sum
    return
    unreachable
  )
  (func $step (;2;) (type 2) (param $i i32) (result i32)
    (local i32)
    local.get $i
    i32.const 0
    i32.ge_s
    if (result i32) ;; label = @1
      local.get $i
      i32.const 3
      i32.lt_s
    else
      i32.const 0
    end
    if ;; label = @1
      i32.const 65360
      local.get $i
      local.tee 1
      local.get 1
      i32.const 3
      i32.ge_u
      if ;; label = @2
        unreachable
      end
      i32.const 1
      i32.mul
      i32.add
      i32.load8_s
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $corner_x (;3;) (type 3) (result i32)
    (local i32 i32 i32)
    i32.const 65372
    i32.const 8
    i32.add
    i32.load
    i32.const 65364
    i32.load
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    return
    unreachable
  )
  (func $heading (;4;) (type 4) (result i32)
    i32.const 65388
    i32.const 4
    i32.add
    i32.load
    return
    unreachable
  )
  (func $scale (;5;) (type 5) (result i64)
    i64.const -9223372036854775807
    return
    unreachable
  )
  (func $ones (;6;) (type 6) (result i32)
    (local $i i32) (local $sum i32) (local i32 i32 i32 i32)
    i32.const 0
    local.set $i
    i32.const 0
    local.set $sum
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 64
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $sum
        i32.const 65396
        local.get $i
        local.tee 2
        local.get 2
        i32.const 64
        i32.ge_u
        if ;; label = @3
          unreachable
        end
        i32.const 2
        i32.mul
        i32.add
        i32.load16_u
        i32.add
        local.tee 3
        i32.const 65535
        i32.and
        local.tee 4
        local.get 3
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 4
        local.set $sum
        local.get $i
        i32.const 1
        local.set 4
        local.tee 3
        local.get 4
        i32.add
        local.tee 5
        local.get 3
        i32.lt_s
        local.get 4
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 5
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $sum
    return
    unreachable
  )
  (func $derived (;7;) (type 7) (result i32)
    i32.const 32
    return
    unreachable
  )
  (func $copy_then_write (;8;) (type 8) (result i32)
    (local $p i32) (local $__frame_ptr i32) (local i32 i32 i32 i32 i32)
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
    i32.const 65344
    local.set 6
    local.set 5
    local.get 5
    local.get 6
    i64.load align=1
    i64.store align=1
    local.get 5
    local.get 6
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.const 100
    i32.store
    local.get $p
    i32.load
    i32.const 65344
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
  (func $returned (;9;) (type 9) (result i32)
    (local $p i32) (local $__frame_ptr i32)
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
    call $primes
    local.get $__frame_ptr
    local.set $p
    local.get $p
    i32.const 12
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (data (;0;) (i32.const 65344) "\02\00\00\00\03\00\00\00\05\00\00\00\07\00\00\00\ff\00\01\00\03\00\00\00\fc\ff\ff\ff\03\00\00\00\fc\ff\ff\ff\0a\00\00\00\14\00\00\00\03\00\00\00\02\00\00\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00\01\00")
  (@custom "inference.checked" (after data) "\01\04\01\03\06\08")
)
