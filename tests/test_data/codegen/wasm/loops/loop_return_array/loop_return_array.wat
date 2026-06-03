(module $output
  (type (;0;) (func (param i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "loop_return_array" (func $loop_return_array))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $loop_return_array (;0;) (type 0) (param $n i32) (result i32)
    (local $arr i32) (local $result i32) (local $i i32) (local $__frame_ptr i32) (local i32)
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
    local.set $result
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 4
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $arr
        local.get $i
        local.tee 5
        local.get 5
        i32.const 4
        i32.ge_u
        if ;; label = @3
          unreachable
        end
        i32.const 4
        i32.mul
        i32.add
        i32.load
        local.get $n
        i32.gt_s
        if ;; label = @3
          local.get $arr
          local.get $i
          local.tee 5
          local.get 5
          i32.const 4
          i32.ge_u
          if ;; label = @4
            unreachable
          end
          i32.const 4
          i32.mul
          i32.add
          i32.load
          local.set $result
          br 2 (;@1;)
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $result
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
)
