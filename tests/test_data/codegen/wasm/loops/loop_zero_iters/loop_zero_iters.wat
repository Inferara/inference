(module $output
  (type (;0;) (func (result i32)))
  (export "loop_zero_iters" (func $loop_zero_iters))
  (func $loop_zero_iters (;0;) (type 0) (result i32)
    (local $x i32)
    i32.const 0
    local.set $x
    block ;; label = @1
      loop ;; label = @2
        i32.const 0
        i32.eqz
        br_if 1 (;@1;)
        i32.const 1
        local.set $x
        br 0 (;@2;)
      end
    end
    local.get $x
    return
    unreachable
  )
)
