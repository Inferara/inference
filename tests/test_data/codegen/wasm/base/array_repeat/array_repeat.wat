(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i64)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i32)))
  (type (;8;) (func (result i32)))
  (type (;9;) (func (result i32)))
  (type (;10;) (func (result i32)))
  (type (;11;) (func (result i32)))
  (type (;12;) (func (result i32)))
  (type (;13;) (func (param i32)))
  (type (;14;) (func (result i32)))
  (type (;15;) (func (result i32)))
  (type (;16;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "fill_eight" (func $fill_eight))
  (export "fill_five" (func $fill_five))
  (export "fill_bytes" (func $fill_bytes))
  (export "fill_wide" (func $fill_wide))
  (export "fill_long" (func $fill_long))
  (export "zeros" (func $zeros))
  (export "from_call" (func $from_call))
  (export "points" (func $points))
  (export "nested" (func $nested))
  (export "rows" (func $rows))
  (export "reassigned" (func $reassigned))
  (export "self_referencing" (func $self_referencing))
  (export "returned" (func $returned))
  (export "field" (func $field))
  (export "constant" (func $constant))
  (export "rezeroed" (func $rezeroed))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $seven (;0;) (type 0) (result i32)
    i32.const 7
    return
    unreachable
  )
  (func $fill_eight (;1;) (type 1) (result i32)
    (local $a i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    i32.const 3
    i32.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load align=1
    i32.store offset=4 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=16 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=8 align=1
    i64.store offset=24 align=1
    local.get $__frame_ptr
    local.set $a
    local.get $a
    i32.load
    local.get $a
    i32.const 28
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
  (func $fill_five (;2;) (type 2) (result i32)
    (local $a i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load align=1
    i32.store offset=4 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load align=1
    i32.store offset=16 align=1
    local.get $__frame_ptr
    local.set $a
    local.get $a
    i32.load
    local.get $a
    i32.const 16
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
  (func $fill_bytes (;3;) (type 3) (result i32)
    (local $a i32) (local $__frame_ptr i32)
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
    i32.const 255
    i32.store8
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load8_u
    i32.store8 offset=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load16_u align=1
    i32.store16 offset=2 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load16_u align=1
    i32.store16 offset=4 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load8_u offset=2
    i32.store8 offset=6
    local.get $__frame_ptr
    local.set $a
    local.get $a
    i32.const 6
    i32.add
    i32.load8_u
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $fill_wide (;4;) (type 4) (result i64)
    (local $a i32) (local $__frame_ptr i32)
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
    i64.const -1
    i64.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=16 align=1
    local.get $__frame_ptr
    local.set $a
    local.get $a
    i32.const 16
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $fill_long (;5;) (type 5) (result i32)
    (local $a i32) (local $__frame_ptr i32) (local i32 i32 i32 i32)
    global.get 0
    i32.const 256
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    i32.const 0
    local.set 5
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 5
      i32.add
      i64.const 0
      i64.store
      local.get $__frame_ptr
      local.get 5
      i32.add
      i64.const 0
      i64.store offset=8
      local.get 5
      i32.const 16
      i32.add
      local.tee 5
      i32.const 256
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i32.const 1
    i32.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load align=1
    i32.store offset=4 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=16 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=8 align=1
    i64.store offset=24 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=32 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=8 align=1
    i64.store offset=40 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=16 align=1
    i64.store offset=48 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=24 align=1
    i64.store offset=56 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=64 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=8 align=1
    i64.store offset=72 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=16 align=1
    i64.store offset=80 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=24 align=1
    i64.store offset=88 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=32 align=1
    i64.store offset=96 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=40 align=1
    i64.store offset=104 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=48 align=1
    i64.store offset=112 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=56 align=1
    i64.store offset=120 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=128 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=8 align=1
    i64.store offset=136 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=16 align=1
    i64.store offset=144 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=24 align=1
    i64.store offset=152 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=32 align=1
    i64.store offset=160 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=40 align=1
    i64.store offset=168 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=48 align=1
    i64.store offset=176 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=56 align=1
    i64.store offset=184 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=64 align=1
    i64.store offset=192 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=72 align=1
    i64.store offset=200 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=80 align=1
    i64.store offset=208 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=88 align=1
    i64.store offset=216 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=96 align=1
    i64.store offset=224 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=104 align=1
    i64.store offset=232 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=112 align=1
    i64.store offset=240 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=120 align=1
    i64.store offset=248 align=1
    local.get $__frame_ptr
    local.set $a
    local.get $a
    i32.const 128
    i32.add
    i32.load
    local.get $a
    i32.const 252
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
    i32.const 256
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $zeros (;6;) (type 6) (result i32)
    (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 64
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
    local.set $a
    local.get $a
    i32.const 60
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 64
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $from_call (;7;) (type 7) (result i32)
    (local $a i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    call $seven
    i32.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load align=1
    i32.store offset=4 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    local.set $a
    local.get $a
    i32.load
    local.get $a
    i32.const 12
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
  (func $points (;8;) (type 8) (result i32)
    (local $ps i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=16 align=1
    local.get $__frame_ptr
    local.set $ps
    local.get $ps
    i32.const 16
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
    local.get $ps
    i32.const 16
    i32.add
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
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $nested (;9;) (type 9) (result i32)
    (local $g i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load align=1
    i32.store offset=4 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=16 align=1
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i32.load
    local.get $g
    i32.const 16
    i32.add
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
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $rows (;10;) (type 10) (result i32)
    (local $row i32) (local $g i32) (local $__frame_ptr i32) (local i32 i32 i32 i32 i32)
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
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 6
    i32.store
    local.get $__frame_ptr
    local.set $row
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $row
    local.set 7
    local.set 6
    local.get 6
    local.get 7
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=8 align=1
    i64.store offset=16 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=8 align=1
    i64.store offset=24 align=1
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $g
    local.get $g
    i32.const 16
    i32.add
    i32.load
    i32.const 10
    local.set 4
    local.tee 3
    local.get 4
    i32.mul
    local.set 5
    local.get 4
    i32.const 0
    i32.ne
    if ;; label = @1
      local.get 4
      i32.const -1
      i32.eq
      if ;; label = @2
        local.get 3
        i32.const -2147483648
        i32.eq
        if ;; label = @3
          unreachable
        end
      else
        local.get 5
        local.get 4
        i32.div_s
        local.get 3
        i32.ne
        if ;; label = @3
          unreachable
        end
      end
    end
    local.get 5
    local.get $g
    i32.const 16
    i32.add
    i32.const 4
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
  (func $reassigned (;11;) (type 11) (result i32)
    (local $a i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    i32.const 12
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $a
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load align=1
    i32.store offset=4 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    drop
    local.get $a
    i32.load
    local.get $a
    i32.const 12
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
  (func $self_referencing (;12;) (type 12) (result i32)
    (local $a i32) (local $__frame_ptr i32) (local i32 i32 i32 i32 i32)
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
    i32.const 12
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $a
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $a
    i32.const 12
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load offset=16 align=1
    i32.store offset=20 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load offset=16 align=1
    i64.store offset=24 align=1
    local.get $a
    local.get $__frame_ptr
    i32.const 16
    i32.add
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
    local.get $a
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
    local.get $a
    i32.const 12
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
  (func $returned (;13;) (type 13) (param $sret i32)
    local.get $sret
    i32.const 0
    i32.add
    i32.const 9
    i32.store
    local.get $sret
    local.get $sret
    i32.load align=1
    i32.store offset=4 align=1
    local.get $sret
    local.get $sret
    i32.load align=1
    i32.store offset=8 align=1
    return
    unreachable
  )
  (func $field (;14;) (type 14) (result i32)
    (local $t i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    i32.const 6
    i32.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load align=1
    i32.store offset=4 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i64.load align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $t
    local.get $t
    i32.const 12
    i32.add
    i32.load
    local.get $t
    i32.const 16
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
  (func $constant (;15;) (type 15) (result i32)
    (local $C i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    i32.const 11
    i32.store
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load align=1
    i32.store offset=4 align=1
    local.get $__frame_ptr
    local.get $__frame_ptr
    i32.load align=1
    i32.store offset=8 align=1
    local.get $__frame_ptr
    local.set $C
    local.get $C
    i32.load
    local.get $C
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
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $rezeroed (;16;) (type 16) (result i32)
    (local $seen i32) (local $i i32) (local $a i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    i32.const 0
    local.set $seen
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 2
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $__frame_ptr
        i32.const 0
        i32.add
        i32.const 0
        i32.store
        local.get $__frame_ptr
        local.get $__frame_ptr
        i32.load align=1
        i32.store offset=4 align=1
        local.get $__frame_ptr
        local.get $__frame_ptr
        i64.load align=1
        i64.store offset=8 align=1
        local.get $__frame_ptr
        local.set $a
        local.get $seen
        local.get $a
        i32.load
        local.set 5
        local.tee 4
        local.get 5
        i32.add
        local.tee 6
        local.get 4
        i32.lt_s
        local.get 5
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 6
        local.get $a
        i32.const 12
        i32.add
        i32.load
        local.set 5
        local.tee 4
        local.get 5
        i32.add
        local.tee 6
        local.get 4
        i32.lt_s
        local.get 5
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 6
        local.set $seen
        local.get $a
        i32.const 5
        i32.store
        local.get $a
        i32.const 12
        i32.add
        i32.const 6
        i32.store
        local.get $i
        i32.const 1
        local.set 5
        local.tee 4
        local.get 5
        i32.add
        local.tee 6
        local.get 4
        i32.lt_s
        local.get 5
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 6
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $seen
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\0c\01\02\05\07\08\09\0a\0b\0c\0e\0f\10")
)
