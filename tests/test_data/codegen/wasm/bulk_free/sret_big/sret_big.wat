(module $output
  (type (;0;) (func (param i32)))
  (type (;1;) (func (result i64)))
  (type (;2;) (func (result i64)))
  (type (;3;) (func (result i64)))
  (type (;4;) (func (param i32) (result i64)))
  (type (;5;) (func (param i32)))
  (type (;6;) (func (result i64)))
  (type (;7;) (func (result i64)))
  (type (;8;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "make_20" (func $make_20))
  (export "read_20_first" (func $read_20_first))
  (export "read_20_middle" (func $read_20_middle))
  (export "read_20_last" (func $read_20_last))
  (export "read_20_dynamic" (func $read_20_dynamic))
  (export "make_block" (func $make_block))
  (export "read_block_ends" (func $read_block_ends))
  (export "read_block_body" (func $read_block_body))
  (export "sret_neighbour_intact" (func $sret_neighbour_intact))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $make_20 (;0;) (type 0) (param $sret i32)
    (local $a i32) (local $__frame_ptr i32) (local i32)
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
    local.set $a
    i32.const 0
    local.set 3
    loop ;; label = @1
      local.get $sret
      local.get 3
      i32.add
      local.get $a
      local.get 3
      i32.add
      i64.load align=1
      i64.store align=1
      local.get 3
      i32.const 8
      i32.add
      local.tee 3
      i32.const 160
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $read_20_first (;1;) (type 1) (result i64)
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
    call $make_20
    local.get $__frame_ptr
    local.set $b
    local.get $b
    i64.load
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $read_20_middle (;2;) (type 2) (result i64)
    (local $c i32) (local $__frame_ptr i32) (local i32)
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
    call $make_20
    local.get $__frame_ptr
    local.set $c
    local.get $c
    i32.const 80
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $read_20_last (;3;) (type 3) (result i64)
    (local $d i32) (local $__frame_ptr i32) (local i32)
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
    call $make_20
    local.get $__frame_ptr
    local.set $d
    local.get $d
    i32.const 152
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $read_20_dynamic (;4;) (type 4) (param $i i32) (result i64)
    (local $e i32) (local $__frame_ptr i32) (local i32 i32)
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
    local.get $__frame_ptr
    call $make_20
    local.get $__frame_ptr
    local.set $e
    local.get $e
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
  (func $make_block (;5;) (type 5) (param $sret i32)
    (local $f i32) (local $__frame_ptr i32) (local i32)
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
    i64.const 3
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 41
    i64.store
    local.get $__frame_ptr
    i32.const 144
    i32.add
    i64.const 43
    i64.store
    local.get $__frame_ptr
    i32.const 152
    i32.add
    i64.const 4
    i64.store
    local.get $__frame_ptr
    local.set $f
    i32.const 0
    local.set 3
    loop ;; label = @1
      local.get $sret
      local.get 3
      i32.add
      local.get $f
      local.get 3
      i32.add
      i64.load align=1
      i64.store align=1
      local.get 3
      i32.const 8
      i32.add
      local.tee 3
      i32.const 160
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $read_block_ends (;6;) (type 6) (result i64)
    (local $g i32) (local $__frame_ptr i32) (local i32)
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
    call $make_block
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i64.load
    i64.const 1000
    i64.mul
    local.get $g
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
  (func $read_block_body (;7;) (type 7) (result i64)
    (local $h i32) (local $__frame_ptr i32) (local i32)
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
    call $make_block
    local.get $__frame_ptr
    local.set $h
    local.get $h
    i32.const 8
    i32.add
    i64.load
    i64.const 1000
    i64.mul
    local.get $h
    i32.const 8
    i32.add
    i32.const 72
    i32.add
    i64.load
    i64.const 100
    i64.mul
    i64.add
    local.get $h
    i32.const 8
    i32.add
    i32.const 136
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
  (func $sret_neighbour_intact (;8;) (type 8) (result i64)
    (local $guard i32) (local $k i32) (local $__frame_ptr i32) (local i32)
    global.get 0
    i32.const 176
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
      i32.const 176
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 61
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 62
    i64.store
    local.get $__frame_ptr
    local.set $guard
    local.get $__frame_ptr
    i32.const 16
    i32.add
    call $make_20
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $k
    local.get $k
    i64.load
    i64.const 1000
    i64.mul
    local.get $guard
    i64.load
    i64.const 10
    i64.mul
    i64.add
    local.get $guard
    i32.const 8
    i32.add
    i64.load
    i64.add
    local.get $__frame_ptr
    i32.const 176
    i32.add
    global.set 0
    return
    unreachable
  )
)
