(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (param i32)))
  (type (;8;) (func (result i32)))
  (type (;9;) (func (result i32)))
  (type (;10;) (func (result i32)))
  (type (;11;) (func (result i32)))
  (type (;12;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "read_c0x" (func $read_c0x))
  (export "read_c1y" (func $read_c1y))
  (export "read_var_index" (func $read_var_index))
  (export "write_c1y" (func $write_c1y))
  (export "write_whole_elem" (func $write_whole_elem))
  (export "grid_param" (func $grid_param))
  (export "call_grid_param" (func $call_grid_param))
  (export "make_grid" (func $make_grid))
  (export "make_and_read" (func $make_and_read))
  (export "mixed_offsets" (func $mixed_offsets))
  (export "two_grids" (func $two_grids))
  (export "zero_field_elem" (func $zero_field_elem))
  (export "read_i64" (func $read_i64))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $read_c0x (;0;) (type 0) (result i32)
    (local $g i32) (local $__frame_ptr i32)
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
    local.set $g
    local.get $g
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $read_c1y (;1;) (type 1) (result i32)
    (local $g i32) (local $__frame_ptr i32)
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
    local.set $g
    local.get $g
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $read_var_index (;2;) (type 2) (param $i i32) (result i32)
    (local $g i32) (local $__frame_ptr i32) (local i32)
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
    local.set $g
    local.get $g
    local.get $i
    local.tee 3
    local.get 3
    i32.const 2
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $write_c1y (;3;) (type 3) (result i32)
    (local $g i32) (local $__frame_ptr i32)
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
    local.set $g
    local.get $g
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.const 99
    i32.store
    local.get $g
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $write_whole_elem (;4;) (type 4) (result i32)
    (local $g i32) (local $p i32) (local $__frame_ptr i32) (local i32 i32)
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
    local.set $g
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 77
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 88
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $p
    local.get $g
    local.get $p
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get $g
    i32.load
    local.get $g
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $grid_param (;5;) (type 5) (param $g i32) (result i32)
    local.get $g
    i32.load
    local.get $g
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
  (func $call_grid_param (;6;) (type 6) (result i32)
    (local $g i32) (local $__frame_ptr i32)
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
    i32.const 6
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 9
    i32.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    call $grid_param
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $make_grid (;7;) (type 7) (param $sret i32)
    local.get $sret
    i32.const 5
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    i32.const 6
    i32.store
    local.get $sret
    i32.const 8
    i32.add
    i32.const 7
    i32.store
    local.get $sret
    i32.const 12
    i32.add
    i32.const 8
    i32.store
    return
    unreachable
  )
  (func $make_and_read (;8;) (type 8) (result i32)
    (local $g i32) (local $__frame_ptr i32)
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
    call $make_grid
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i32.const 8
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $mixed_offsets (;9;) (type 9) (result i32)
    (local $m i32) (local $__frame_ptr i32)
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
    i32.const 11
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 100
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 200
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 300
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 400
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 99
    i32.store
    local.get $__frame_ptr
    local.set $m
    local.get $m
    i32.load
    local.get $m
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $m
    i32.const 4
    i32.add
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $m
    i32.const 20
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $two_grids (;10;) (type 10) (result i32)
    (local $a i32) (local $b i32) (local $__frame_ptr i32)
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
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 6
    i32.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    i32.const 28
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $b
    local.get $a
    i32.load
    local.get $b
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $zero_field_elem (;11;) (type 11) (result i32)
    (local $g i32) (local $__frame_ptr i32)
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
    i32.const 4
    i32.add
    i32.const 5
    i32.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    i32.const 4
    i32.add
    i32.load
    local.get $g
    i32.const 8
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $read_i64 (;12;) (type 12) (result i64)
    (local $g i32) (local $__frame_ptr i32)
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
    i32.const 24
    i32.add
    i64.const 4
    i64.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
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
)
