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
    (local $arr i32) (local $sum i32) (local $i i32) (local $__frame_ptr i32) (local i32 i32 i32 i32)
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
        local.tee 4
        local.get 4
        i32.const 4
        i32.ge_u
        if ;; label = @3
          unreachable
        end
        i32.const 4
        i32.mul
        i32.add
        i32.load
        local.set 6
        local.tee 5
        local.get 6
        i32.add
        local.tee 7
        local.get 5
        i32.lt_s
        local.get 6
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 7
        local.set $sum
        local.get $i
        i32.const 1
        local.set 6
        local.tee 5
        local.get 6
        i32.add
        local.tee 7
        local.get 5
        i32.lt_s
        local.get 6
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 7
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
    (local $arr i32) (local $i i32) (local $__frame_ptr i32) (local i32 i32 i32 i32)
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
        local.tee 4
        local.get 4
        i32.const 5
        i32.ge_u
        if ;; label = @3
          unreachable
        end
        i32.const 4
        i32.mul
        i32.add
        local.get $i
        local.get $n
        local.set 6
        local.tee 5
        local.get 6
        i32.mul
        local.set 7
        local.get 6
        i32.const 0
        i32.ne
        if ;; label = @3
          local.get 6
          i32.const -1
          i32.eq
          if ;; label = @4
            local.get 5
            i32.const -2147483648
            i32.eq
            if ;; label = @5
              unreachable
            end
          else
            local.get 7
            local.get 6
            i32.div_s
            local.get 5
            i32.ne
            if ;; label = @5
              unreachable
            end
          end
        end
        local.get 7
        i32.store
        local.get $i
        i32.const 1
        local.set 6
        local.tee 5
        local.get 6
        i32.add
        local.tee 7
        local.get 5
        i32.lt_s
        local.get 6
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 7
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
    local.set 6
    local.tee 5
    local.get 6
    i32.add
    local.tee 7
    local.get 5
    i32.lt_s
    local.get 6
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 7
    local.get $arr
    i32.const 8
    i32.add
    i32.load
    local.set 6
    local.tee 5
    local.get 6
    i32.add
    local.tee 7
    local.get 5
    i32.lt_s
    local.get 6
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 7
    local.get $arr
    i32.const 12
    i32.add
    i32.load
    local.set 6
    local.tee 5
    local.get 6
    i32.add
    local.tee 7
    local.get 5
    i32.lt_s
    local.get 6
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 7
    local.get $arr
    i32.const 16
    i32.add
    i32.load
    local.set 6
    local.tee 5
    local.get 6
    i32.add
    local.tee 7
    local.get 5
    i32.lt_s
    local.get 6
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 7
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\02\00\01")
)
