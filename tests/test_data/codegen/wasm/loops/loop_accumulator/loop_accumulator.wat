(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (export "sum_1_to_n" (func $sum_1_to_n))
  (export "factorial" (func $factorial))
  (export "power" (func $power))
  (func $sum_1_to_n (;0;) (type 0) (param $n i32) (result i32)
    (local $sum i32) (local $i i32)
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
        i32.add
        local.set $sum
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
  (func $factorial (;1;) (type 1) (param $n i32) (result i32)
    (local $result i32) (local $i i32)
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
        i32.mul
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
  (func $power (;2;) (type 2) (param $base i32) (param $exp i32) (result i32)
    (local $result i32) (local $i i32)
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
        i32.mul
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
)
