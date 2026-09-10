(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (export "find_threshold" (func $find_threshold))
  (export "first_multiple_of" (func $first_multiple_of))
  (func $find_threshold (;0;) (type 0) (param $start i32) (result i32)
    (local $i i32) (local i32 i32 i32)
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
        local.set 3
        local.tee 2
        local.get 3
        i32.add
        local.tee 4
        local.get 2
        i32.lt_s
        local.get 3
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 4
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $i
    return
    unreachable
  )
  (func $first_multiple_of (;1;) (type 1) (param $n i32) (param $limit i32) (result i32)
    (local $i i32) (local i32 i32 i32)
    i32.const 1
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        local.set 4
        local.tee 3
        local.get 4
        i32.mul
        local.set 5
        local.get 4
        i32.const 0
        i32.ne
        if ;; label = @3
          local.get 4
          i32.const -1
          i32.eq
          if ;; label = @4
            local.get 3
            i32.const -2147483648
            i32.eq
            if ;; label = @5
              unreachable
            end
          else
            local.get 5
            local.get 4
            i32.div_s
            local.get 3
            i32.ne
            if ;; label = @5
              unreachable
            end
          end
        end
        local.get 5
        local.get $limit
        i32.ge_s
        if ;; label = @3
          br 2 (;@1;)
        end
        local.get $i
        i32.const 1
        local.set 4
        local.tee 3
        local.get 4
        i32.add
        local.tee 5
        local.get 3
        i32.lt_s
        local.get 4
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 5
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $i
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\02\00\01")
)
