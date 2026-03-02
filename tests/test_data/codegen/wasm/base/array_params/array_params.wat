(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (param i32) (result i32)))
  (type (;7;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "sum_array" (func $sum_array))
  (export "call_sum" (func $call_sum))
  (export "mutate_copy" (func $mutate_copy))
  (export "verify_copy_semantics" (func $verify_copy_semantics))
  (export "two_array_params" (func $two_array_params))
  (export "call_two_params" (func $call_two_params))
  (export "bool_array_param" (func $bool_array_param))
  (export "call_bool_param" (func $call_bool_param))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $sum_array (;0;) (type 0) (param $arr i32) (result i32)
    (local $__frame_ptr i32)
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
    local.get $arr
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    local.get $arr
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $arr
    i32.const 8
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    local.set $arr
    local.get $arr
    i32.load
    local.get $arr
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $arr
    i32.const 8
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $call_sum (;1;) (type 1) (result i32)
    (local $data i32) (local $__frame_ptr i32)
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
    local.set $data
    local.get $data
    call $sum_array
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $mutate_copy (;2;) (type 2) (param $arr i32) (result i32)
    (local $__frame_ptr i32)
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
    local.get $arr
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    local.get $arr
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $arr
    i32.const 8
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    local.set $arr
    local.get $arr
    i32.const 99
    i32.store
    local.get $arr
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $verify_copy_semantics (;3;) (type 3) (result i32)
    (local $data i32) (local $ignored i32) (local $__frame_ptr i32)
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
    local.set $data
    local.get $data
    call $mutate_copy
    local.set $ignored
    local.get $data
    i32.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $two_array_params (;4;) (type 4) (param $a i32) (param $b i32) (result i32)
    (local $__frame_ptr i32)
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
    local.get $a
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    local.get $a
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    local.set $a
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.get $b
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    local.get $b
    i32.const 4
    i32.add
    i32.load
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    local.set $b
    local.get $a
    i32.load
    local.get $a
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $b
    i32.load
    i32.add
    local.get $b
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $call_two_params (;5;) (type 5) (result i32)
    (local $x i32) (local $y i32) (local $__frame_ptr i32)
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
    i32.const 10
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 20
    i32.store
    local.get $__frame_ptr
    local.set $x
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
    i32.const 8
    i32.add
    local.set $y
    local.get $x
    local.get $y
    call $two_array_params
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $bool_array_param (;6;) (type 6) (param $flags i32) (result i32)
    (local $__frame_ptr i32)
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
    local.get $flags
    i32.load8_u
    i32.store8
    local.get $__frame_ptr
    i32.const 1
    i32.add
    local.get $flags
    i32.const 1
    i32.add
    i32.load8_u
    i32.store8
    local.get $__frame_ptr
    i32.const 2
    i32.add
    local.get $flags
    i32.const 2
    i32.add
    i32.load8_u
    i32.store8
    local.get $__frame_ptr
    local.set $flags
    local.get $flags
    i32.load8_u
    if ;; label = @1
      local.get $flags
      i32.const 2
      i32.add
      i32.load8_u
      if ;; label = @2
        i32.const 1
        local.get $__frame_ptr
        i32.const 16
        i32.add
        global.set 0
        return
      end
    end
    i32.const 0
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
  (func $call_bool_param (;7;) (type 7) (result i32)
    (local $f i32) (local $__frame_ptr i32)
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
    i32.const 1
    i32.add
    i32.const 0
    i32.store8
    local.get $__frame_ptr
    i32.const 2
    i32.add
    i32.const 1
    i32.store8
    local.get $__frame_ptr
    local.set $f
    local.get $f
    call $bool_array_param
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    unreachable
  )
)
