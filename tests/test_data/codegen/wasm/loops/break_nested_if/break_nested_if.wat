(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (result i32)))
  (export "break_nested_if" (func $break_nested_if))
  (export "break_double_nested_if" (func $break_double_nested_if))
  (func $break_nested_if (;0;) (type 0) (param $x i32) (result i32)
    (local $result i32) (local $i i32)
    i32.const 0
    local.set $result
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 100
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $i
        local.get $x
        i32.gt_s
        if ;; label = @3
          local.get $i
          i32.const 2
          i32.rem_s
          i32.const 0
          i32.eq
          if ;; label = @4
            br 3 (;@1;)
          end
        end
        local.get $result
        i32.const 1
        i32.add
        local.set $result
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $result
    return
    unreachable
  )
  (func $break_double_nested_if (;1;) (type 1) (result i32)
    (local $count i32) (local $i i32)
    i32.const 0
    local.set $count
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 10
        i32.ge_s
        if ;; label = @3
          local.get $i
          i32.const 10
          i32.ge_s
          if ;; label = @4
            br 3 (;@1;)
          end
        end
        local.get $count
        i32.const 1
        i32.add
        local.set $count
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $count
    return
    unreachable
  )
)
