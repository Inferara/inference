(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (export "sum_1_to_n" (func $sum_1_to_n))
  (export "factorial" (func $factorial))
  (export "power" (func $power))
  (func $sum_1_to_n (;0;) (type 0) (param $n i32) (result i32)
    (local $sum i32) (local $i i32) (local i32 i32 i32)
    i32.const 0
    local.set $sum
    i32.const 1
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.le_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $sum
        local.get $i
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
        local.set $sum
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
    local.get $sum
    return
    unreachable
  )
  (func $factorial (;1;) (type 1) (param $n i32) (result i32)
    (local $result i32) (local $i i32) (local i32 i32 i32)
    i32.const 1
    local.set $result
    i32.const 2
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.le_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $result
        local.get $i
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
        local.set $result
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
    local.get $result
    return
    unreachable
  )
  (func $power (;2;) (type 2) (param $base i32) (param $exp i32) (result i32)
    (local $result i32) (local $i i32) (local i32 i32 i32)
    i32.const 1
    local.set $result
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $exp
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $result
        local.get $base
        local.set 5
        local.tee 4
        local.get 5
        i32.mul
        local.set 6
        local.get 5
        i32.const 0
        i32.ne
        if ;; label = @3
          local.get 5
          i32.const -1
          i32.eq
          if ;; label = @4
            local.get 4
            i32.const -2147483648
            i32.eq
            if ;; label = @5
              unreachable
            end
          else
            local.get 6
            local.get 5
            i32.div_s
            local.get 4
            i32.ne
            if ;; label = @5
              unreachable
            end
          end
        end
        local.get 6
        local.set $result
        local.get $i
        i32.const 1
        local.set 5
        local.tee 4
        local.get 5
        i32.add
        local.tee 6
        local.get 4
        i32.lt_s
        local.get 5
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 6
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $result
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\03\00\01\02")
)
