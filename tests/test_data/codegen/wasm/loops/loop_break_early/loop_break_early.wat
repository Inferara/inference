(module $output
  (type (;0;) (func (param i32) (result i32)))
  (export "loop_break_early" (func $loop_break_early))
  (func $loop_break_early (;0;) (type 0) (param $n i32) (result i32)
    (local $sum i32) (local $i i32)
    i32.const 0
    local.set $sum
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $sum
        local.get $i
        i32.add
        local.set $sum
        local.get $sum
        i32.const 10
        i32.gt_s
        if ;; label = @3
          br 2 (;@1;)
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $sum
    return
    unreachable
  )
)
