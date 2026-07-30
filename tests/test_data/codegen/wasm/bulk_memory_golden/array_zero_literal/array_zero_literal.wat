(module $output
  (type (;0;) (func (param i32)))
  (type (;1;) (func (param i32)))
  (type (;2;) (func (param i32)))
  (type (;3;) (func (param i32)))
  (type (;4;) (func (param i32)))
  (type (;5;) (func (param i32)))
  (type (;6;) (func (param i32)))
  (type (;7;) (func (param i32)))
  (type (;8;) (func (param i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "all_zeros_i32" (func $all_zeros_i32))
  (export "all_zeros_u64" (func $all_zeros_u64))
  (export "mixed_values" (func $mixed_values))
  (export "all_zeros_bool" (func $all_zeros_bool))
  (export "sret_direct_zeros" (func $sret_direct_zeros))
  (export "parenthesized_zeros" (func $parenthesized_zeros))
  (export "negated_zeros" (func $negated_zeros))
  (export "single_zero" (func $single_zero))
  (export "mixed_bool" (func $mixed_bool))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $all_zeros_i32 (;0;) (type 0) (param $sret i32)
    (local $arr i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    local.set $arr
    local.get $sret
    local.get $arr
    i32.const 16
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $all_zeros_u64 (;1;) (type 1) (param $sret i32)
    (local $arr i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
    memory.fill
    local.get $__frame_ptr
    local.set $arr
    local.get $sret
    local.get $arr
    i32.const 24
    memory.copy
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $mixed_values (;2;) (type 2) (param $sret i32)
    (local $arr i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 1
    i32.store
    local.get $__frame_ptr
    local.set $arr
    local.get $sret
    local.get $arr
    i32.const 12
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $all_zeros_bool (;3;) (type 3) (param $sret i32)
    (local $arr i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    local.set $arr
    local.get $sret
    local.get $arr
    i32.const 2
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $sret_direct_zeros (;4;) (type 4) (param $sret i32)
    local.get $sret
    i32.const 0
    i32.store
    local.get $sret
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    local.get $sret
    i32.const 8
    i32.add
    i32.const 0
    i32.store
    return
    unreachable
  )
  (func $parenthesized_zeros (;5;) (type 5) (param $sret i32)
    (local $arr i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    local.set $arr
    local.get $sret
    local.get $arr
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $negated_zeros (;6;) (type 6) (param $sret i32)
    (local $arr i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    local.set $arr
    local.get $sret
    local.get $arr
    i32.const 8
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $single_zero (;7;) (type 7) (param $sret i32)
    (local $arr i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    local.set $arr
    local.get $sret
    local.get $arr
    i32.const 4
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $mixed_bool (;8;) (type 8) (param $sret i32)
    (local $arr i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i32.const 1
    i32.store8
    local.get $__frame_ptr
    i32.const 2
    i32.add
    i32.const 1
    i32.store8
    local.get $__frame_ptr
    local.set $arr
    local.get $sret
    local.get $arr
    i32.const 3
    memory.copy
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
)
