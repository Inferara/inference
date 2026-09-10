(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (export "count_evens" (func $count_evens))
  (export "abs_sum" (func $abs_sum))
  (func $count_evens (;0;) (type 0) (param $n i32) (result i32)
    (local $count i32) (local $i i32) (local i32 i32 i32)
    i32.const 0
    local.set $count
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $i
        i32.const 2
        i32.rem_s
        i32.const 0
        i32.eq
        if ;; label = @3
          local.get $count
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
          if ;; label = @4
            unreachable
          end
          local.get 5
          local.set $count
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
    local.get $count
    return
    unreachable
  )
  (func $abs_sum (;1;) (type 1) (param $n i32) (result i32)
    (local $sum i32) (local $i i32) (local i32 i32 i32)
    i32.const 0
    local.set $sum
    i32.const 0
    local.get $n
    local.set 4
    local.tee 3
    local.get 4
    i32.sub
    local.tee 5
    local.get 3
    i32.lt_s
    local.get 4
    i32.const 0
    i32.gt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 5
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.le_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $i
        i32.const 0
        i32.lt_s
        if ;; label = @3
          local.get $sum
          local.get $i
          local.set 4
          local.tee 3
          local.get 4
          i32.sub
          local.tee 5
          local.get 3
          i32.lt_s
          local.get 4
          i32.const 0
          i32.gt_s
          i32.ne
          if ;; label = @4
            unreachable
          end
          local.get 5
          local.set $sum
        else
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
          if ;; label = @4
            unreachable
          end
          local.get 5
          local.set $sum
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
    local.get $sum
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\02\00\01")
)
