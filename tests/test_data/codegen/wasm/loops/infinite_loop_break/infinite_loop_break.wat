(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (export "find_threshold" (func $find_threshold))
  (export "first_multiple_of" (func $first_multiple_of))
  (func $find_threshold (;0;) (type 0) (param $start i32) (result i32)
    (local $i i32)
    local.get $start
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 100
        i32.ge_s
        if ;; label = @3
          br 2 (;@1;)
        end
        local.get $i
        i32.const 7
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $i
    return
    unreachable
  )
  (func $first_multiple_of (;1;) (type 1) (param $n i32) (param $limit i32) (result i32)
    (local $i i32)
    i32.const 1
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.mul
        local.get $limit
        i32.ge_s
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
    local.get $i
    return
    unreachable
  )
)
