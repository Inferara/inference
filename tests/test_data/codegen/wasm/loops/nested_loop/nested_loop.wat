(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (export "nested_count" (func $nested_count))
  (export "nested_break" (func $nested_break))
  (func $nested_count (;0;) (type 0) (result i32)
    (local $total i32) (local $i i32) (local $j i32)
    i32.const 0
    local.set $total
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 3
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        i32.const 0
        local.set $j
        block ;; label = @3
          loop ;; label = @4
            local.get $j
            i32.const 4
            i32.lt_s
            i32.eqz
            br_if 1 (;@3;)
            local.get $total
            i32.const 1
            i32.add
            local.set $total
            local.get $j
            i32.const 1
            i32.add
            local.set $j
            br 0 (;@4;)
          end
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $total
    return
    unreachable
  )
  (func $nested_break (;1;) (type 1) (result i32)
    (local $result i32) (local $i i32) (local $j i32)
    i32.const 0
    local.set $result
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 10
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        i32.const 0
        local.set $j
        block ;; label = @3
          loop ;; label = @4
            local.get $j
            i32.const 3
            i32.ge_s
            if ;; label = @5
              br 2 (;@3;)
            end
            local.get $result
            i32.const 1
            i32.add
            local.set $result
            local.get $j
            i32.const 1
            i32.add
            local.set $j
            br 0 (;@4;)
          end
        end
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
)
