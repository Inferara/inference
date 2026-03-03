(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (export "count_to_ten" (func $count_to_ten))
  (export "count_down" (func $count_down))
  (func $count_to_ten (;0;) (type 0) (result i32)
    (local $i i32)
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 10
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $i
    return
    unreachable
  )
  (func $count_down (;1;) (type 1) (param $n i32) (result i32)
    block ;; label = @1
      loop ;; label = @2
        local.get $n
        i32.const 0
        i32.gt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $n
        i32.const 1
        i32.sub
        local.set $n
        br 0 (;@2;)
      end
    end
    local.get $n
    return
    unreachable
  )
)
