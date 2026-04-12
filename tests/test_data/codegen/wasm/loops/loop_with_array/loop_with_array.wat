(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "sum_array_elements" (func $sum_array_elements))
  (export "fill_and_sum" (func $fill_and_sum))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $sum_array_elements (;0;) (type 0) (result i32)
    (local $arr i32) (local $sum i32) (local $i i32) (local $__frame_ptr i32)
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
    i32.const 12
    i32.add
    i32.const 40
    i32.store
    local.get $__frame_ptr
    local.set $arr
    i32.const 0
    local.set $sum
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 4
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $sum
        local.get $arr
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        i32.load
        i32.add
        local.set $sum
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $sum
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $fill_and_sum (;1;) (type 1) (param $n i32) (result i32)
    (local $arr i32) (local $i i32) (local $__frame_ptr i32)
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
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 5
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $arr
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        local.get $i
        local.get $n
        i32.mul
        i32.store
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
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
    local.get $arr
    i32.const 12
    i32.add
    i32.load
    i32.add
    local.get $arr
    i32.const 16
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
)
