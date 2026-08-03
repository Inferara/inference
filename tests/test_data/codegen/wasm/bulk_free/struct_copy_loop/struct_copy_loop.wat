(module $output
  (type (;0;) (func (param i32) (result i64)))
  (type (;1;) (func (param i32 i32) (result i64)))
  (type (;2;) (func (param i32) (result i64)))
  (type (;3;) (func (result i64)))
  (type (;4;) (func (result i64)))
  (type (;5;) (func (result i64)))
  (type (;6;) (func (param i32) (result i32)))
  (type (;7;) (func (result i32)))
  (type (;8;) (func (param i32) (result i64)))
  (type (;9;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "block_ends" (func $block_ends))
  (export "block_body" (func $block_body))
  (export "block_clobber" (func $block_clobber))
  (export "call_block_ends" (func $call_block_ends))
  (export "call_block_body" (func $call_block_body))
  (export "block_value_semantics" (func $block_value_semantics))
  (export "frame_edges" (func $frame_edges))
  (export "call_frame_edges" (func $call_frame_edges))
  (export "frame_body_ends" (func $frame_body_ends))
  (export "call_frame_body_ends" (func $call_frame_body_ends))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $block_ends (;0;) (type 0) (param $b i32) (result i64)
    (local $__frame_ptr i32) (local i32)
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
    i32.const 0
    local.set 2
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 2
      i32.add
      local.get $b
      local.get 2
      i32.add
      i64.load align=1
      i64.store align=1
      local.get 2
      i32.const 8
      i32.add
      local.tee 2
      i32.const 160
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    local.set $b
    local.get $b
    local.get $b
    i64.load
    i64.store
    local.get $b
    i64.load
    i64.const 1000
    i64.mul
    local.get $b
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
  (func $block_body (;1;) (type 1) (param $b i32) (param $i i32) (result i64)
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
      local.get $b
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
    local.set $b
    local.get $b
    local.get $b
    i64.load
    i64.store
    local.get $b
    i32.const 8
    i32.add
    local.get $i
    local.tee 3
    local.get 3
    i32.const 18
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
  (func $block_clobber (;2;) (type 2) (param $b i32) (result i64)
    (local $__frame_ptr i32) (local i32)
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
    i32.const 0
    local.set 2
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 2
      i32.add
      local.get $b
      local.get 2
      i32.add
      i64.load align=1
      i64.store align=1
      local.get 2
      i32.const 8
      i32.add
      local.tee 2
      i32.const 160
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    local.set $b
    local.get $b
    i64.const 777
    i64.store
    local.get $b
    i64.load
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_block_ends (;3;) (type 3) (result i64)
    (local $x i32) (local $__frame_ptr i32) (local i32)
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
    local.set $x
    local.get $x
    call $block_ends
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_block_body (;4;) (type 4) (result i64)
    (local $y i32) (local $__frame_ptr i32) (local i32)
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
    local.set $y
    local.get $y
    i32.const 0
    call $block_body
    i64.const 1000
    i64.mul
    local.get $y
    i32.const 9
    call $block_body
    i64.const 100
    i64.mul
    i64.add
    local.get $y
    i32.const 17
    call $block_body
    i64.add
    local.get $__frame_ptr
    i32.const 160
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $block_value_semantics (;5;) (type 5) (result i64)
    (local $z i32) (local $ignored i64) (local $__frame_ptr i32) (local i32)
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
    local.set $z
    local.get $z
    call $block_clobber
    local.set $ignored
    local.get $z
    i64.load
    i64.const 1000
    i64.mul
    local.get $z
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
  (func $frame_edges (;6;) (type 6) (param $f i32) (result i32)
    (local $__frame_ptr i32) (local i32)
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
    i32.const 0
    local.set 2
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 2
      i32.add
      local.get $f
      local.get 2
      i32.add
      i64.load align=1
      i64.store align=1
      local.get 2
      i32.const 8
      i32.add
      local.tee 2
      i32.const 144
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    local.set $f
    local.get $f
    local.get $f
    i32.load8_u
    i32.store8
    local.get $f
    i32.load8_u
    i32.const 10
    i32.mul
    i32.const 255
    i32.and
    local.get $f
    i32.const 136
    i32.add
    i32.load8_u
    i32.add
    i32.const 255
    i32.and
    local.get $__frame_ptr
    i32.const 144
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_frame_edges (;7;) (type 7) (result i32)
    (local $w i32) (local $__frame_ptr i32) (local i32)
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
    i32.const 6
    i32.store8
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 51
    i64.store
    local.get $__frame_ptr
    i32.const 128
    i32.add
    i64.const 53
    i64.store
    local.get $__frame_ptr
    i32.const 136
    i32.add
    i32.const 7
    i32.store8
    local.get $__frame_ptr
    local.set $w
    local.get $w
    call $frame_edges
    local.get $__frame_ptr
    i32.const 144
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $frame_body_ends (;8;) (type 8) (param $f i32) (result i64)
    (local $__frame_ptr i32) (local i32)
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
    i32.const 0
    local.set 2
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 2
      i32.add
      local.get $f
      local.get 2
      i32.add
      i64.load align=1
      i64.store align=1
      local.get 2
      i32.const 8
      i32.add
      local.tee 2
      i32.const 144
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    local.set $f
    local.get $f
    local.get $f
    i32.load8_u
    i32.store8
    local.get $f
    i32.const 8
    i32.add
    i64.load
    i64.const 1000
    i64.mul
    local.get $f
    i32.const 8
    i32.add
    i32.const 64
    i32.add
    i64.load
    i64.const 100
    i64.mul
    i64.add
    local.get $f
    i32.const 8
    i32.add
    i32.const 120
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
  (func $call_frame_body_ends (;9;) (type 9) (result i64)
    (local $v i32) (local $__frame_ptr i32) (local i32)
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
    i32.const 6
    i32.store8
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 51
    i64.store
    local.get $__frame_ptr
    i32.const 128
    i32.add
    i64.const 53
    i64.store
    local.get $__frame_ptr
    i32.const 136
    i32.add
    i32.const 7
    i32.store8
    local.get $__frame_ptr
    local.set $v
    local.get $v
    call $frame_body_ends
    local.get $__frame_ptr
    i32.const 144
    i32.add
    global.set 0
    return
    unreachable
  )
)
