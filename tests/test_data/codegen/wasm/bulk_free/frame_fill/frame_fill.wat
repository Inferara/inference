(module $output
  (type (;0;) (func (result i64)))
  (type (;1;) (func (result i64)))
  (type (;2;) (func (result i64)))
  (type (;3;) (func (result i64)))
  (type (;4;) (func (result i64)))
  (type (;5;) (func (param i32) (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "fill_128_unrolled" (func $fill_128_unrolled))
  (export "fill_144_loop" (func $fill_144_loop))
  (export "fill_160_loop" (func $fill_160_loop))
  (export "fill_320_loop" (func $fill_320_loop))
  (export "fill_multi_slot" (func $fill_multi_slot))
  (export "fill_read_dynamic" (func $fill_read_dynamic))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $fill_128_unrolled (;0;) (type 0) (result i64)
    (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 128
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
    i64.const 0
    i64.store offset=80
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=88
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=96
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=104
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=112
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=120
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 3
    i64.store
    local.get $__frame_ptr
    i32.const 120
    i32.add
    i64.const 4
    i64.store
    local.get $__frame_ptr
    local.set $a
    local.get $a
    i64.load
    i64.const 1000
    i64.mul
    local.get $a
    i32.const 56
    i32.add
    i64.load
    i64.const 100
    i64.mul
    i64.add
    local.get $a
    i32.const 120
    i32.add
    i64.load
    i64.add
    local.get $__frame_ptr
    i32.const 128
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $fill_144_loop (;1;) (type 1) (result i64)
    (local $b i32) (local $__frame_ptr i32) (local i32)
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
    i64.const 5
    i64.store
    local.get $__frame_ptr
    i32.const 136
    i32.add
    i64.const 6
    i64.store
    local.get $__frame_ptr
    local.set $b
    local.get $b
    i64.load
    i64.const 1000
    i64.mul
    local.get $b
    i32.const 72
    i32.add
    i64.load
    i64.const 100
    i64.mul
    i64.add
    local.get $b
    i32.const 136
    i32.add
    i64.load
    i64.add
    local.get $__frame_ptr
    i32.const 144
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $fill_160_loop (;2;) (type 2) (result i64)
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
    i32.const 0
    i32.add
    i64.const 7
    i64.store
    local.get $__frame_ptr
    i32.const 152
    i32.add
    i64.const 8
    i64.store
    local.get $__frame_ptr
    local.set $c
    local.get $c
    i64.load
    i64.const 1000
    i64.mul
    local.get $c
    i32.const 80
    i32.add
    i64.load
    i64.const 100
    i64.mul
    i64.add
    local.get $c
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
  (func $fill_320_loop (;3;) (type 3) (result i64)
    (local $d i32) (local $__frame_ptr i32) (local i32)
    global.get 0
    i32.const 320
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
      i32.const 320
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 1
    i64.store
    local.get $__frame_ptr
    i32.const 312
    i32.add
    i64.const 2
    i64.store
    local.get $__frame_ptr
    local.set $d
    local.get $d
    i64.load
    i64.const 1000
    i64.mul
    local.get $d
    i32.const 104
    i32.add
    i64.load
    i64.const 100
    i64.mul
    i64.add
    local.get $d
    i32.const 208
    i32.add
    i64.load
    i64.const 10
    i64.mul
    i64.add
    local.get $d
    i32.const 312
    i32.add
    i64.load
    i64.add
    local.get $__frame_ptr
    i32.const 320
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $fill_multi_slot (;4;) (type 4) (result i64)
    (local $e i32) (local $f i32) (local $__frame_ptr i32) (local i32)
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
    i64.const 9
    i64.store
    local.get $__frame_ptr
    i32.const 136
    i32.add
    i64.const 1
    i64.store
    local.get $__frame_ptr
    local.set $e
    local.get $__frame_ptr
    i32.const 168
    i32.add
    i64.const 2
    i64.store
    local.get $__frame_ptr
    i32.const 144
    i32.add
    local.set $f
    local.get $e
    i64.load
    i64.const 1000
    i64.mul
    local.get $e
    i32.const 64
    i32.add
    i64.load
    i64.const 100
    i64.mul
    i64.add
    local.get $f
    i64.load
    i64.const 10
    i64.mul
    i64.add
    local.get $f
    i32.const 24
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
  (func $fill_read_dynamic (;5;) (type 5) (param $i i32) (result i64)
    (local $g i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 0
    i32.add
    i64.const 4
    i64.store
    local.get $__frame_ptr
    i32.const 152
    i32.add
    i64.const 5
    i64.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
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
)
