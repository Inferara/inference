(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (param i64) (result i64)))
  (type (;6;) (func (param i64 i64) (result i64)))
  (type (;7;) (func (param i64 i64) (result i64)))
  (type (;8;) (func (param i32 i32) (result i32)))
  (type (;9;) (func (param i32) (result i32)))
  (type (;10;) (func (param i32 i32) (result i32)))
  (type (;11;) (func (param i32) (result i32)))
  (export "fibonacci_iter" (func $fibonacci_iter))
  (export "gcd_iter" (func $gcd_iter))
  (export "is_prime_iter" (func $is_prime_iter))
  (export "isqrt" (func $isqrt))
  (export "pow_iter" (func $pow_iter))
  (export "fibonacci_iter_i64" (func $fibonacci_iter_i64))
  (export "gcd_iter_i64" (func $gcd_iter_i64))
  (export "pow_iter_i64" (func $pow_iter_i64))
  (export "gcd_u8" (func $gcd_u8))
  (export "fibonacci_i16" (func $fibonacci_i16))
  (export "pow_u16" (func $pow_u16))
  (export "is_prime_bool" (func $is_prime_bool))
  (func $fibonacci_iter (;0;) (type 0) (param $n i32) (result i32)
    (local $a i32) (local $b i32) (local $i i32) (local $next i32)
    local.get $n
    i32.const 0
    i32.le_s
    if ;; label = @1
      i32.const 0
      return
    end
    local.get $n
    i32.const 1
    i32.eq
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    local.set $a
    i32.const 1
    local.set $b
    i32.const 2
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.le_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $a
        local.get $b
        i32.add
        local.set $next
        local.get $b
        local.set $a
        local.get $next
        local.set $b
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $b
    return
    unreachable
  )
  (func $gcd_iter (;1;) (type 1) (param $a i32) (param $b i32) (result i32)
    (local $x i32) (local $y i32) (local $t i32)
    local.get $a
    local.set $x
    local.get $b
    local.set $y
    local.get $x
    i32.const 0
    i32.lt_s
    if ;; label = @1
      i32.const 0
      local.get $x
      i32.sub
      local.set $x
    end
    local.get $y
    i32.const 0
    i32.lt_s
    if ;; label = @1
      i32.const 0
      local.get $y
      i32.sub
      local.set $y
    end
    block ;; label = @1
      loop ;; label = @2
        local.get $y
        i32.const 0
        i32.gt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $y
        local.set $t
        local.get $x
        local.get $y
        i32.rem_s
        local.set $y
        local.get $t
        local.set $x
        br 0 (;@2;)
      end
    end
    local.get $x
    return
    unreachable
  )
  (func $is_prime_iter (;2;) (type 2) (param $n i32) (result i32)
    (local $result i32) (local $d i32)
    local.get $n
    i32.const 1
    i32.le_s
    if ;; label = @1
      i32.const 0
      return
    end
    local.get $n
    i32.const 3
    i32.le_s
    if ;; label = @1
      i32.const 1
      return
    end
    local.get $n
    i32.const 2
    i32.rem_s
    i32.const 0
    i32.eq
    if ;; label = @1
      i32.const 0
      return
    end
    i32.const 1
    local.set $result
    i32.const 3
    local.set $d
    block ;; label = @1
      loop ;; label = @2
        local.get $d
        local.get $d
        i32.mul
        local.get $n
        i32.le_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $n
        local.get $d
        i32.rem_s
        i32.const 0
        i32.eq
        if ;; label = @3
          i32.const 0
          local.set $result
          br 2 (;@1;)
        end
        local.get $d
        i32.const 2
        i32.add
        local.set $d
        br 0 (;@2;)
      end
    end
    local.get $result
    return
    unreachable
  )
  (func $isqrt (;3;) (type 3) (param $n i32) (result i32)
    (local $x i32) (local $y i32)
    local.get $n
    i32.const 0
    i32.le_s
    if ;; label = @1
      i32.const 0
      return
    end
    local.get $n
    local.set $x
    local.get $x
    i32.const 1
    i32.add
    i32.const 2
    i32.div_s
    local.set $y
    block ;; label = @1
      loop ;; label = @2
        local.get $y
        local.get $x
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $y
        local.set $x
        local.get $x
        local.get $n
        local.get $x
        i32.div_s
        i32.add
        i32.const 2
        i32.div_s
        local.set $y
        br 0 (;@2;)
      end
    end
    local.get $x
    return
    unreachable
  )
  (func $pow_iter (;4;) (type 4) (param $base i32) (param $exp i32) (result i32)
    (local $result i32) (local $b i32) (local $e i32)
    i32.const 1
    local.set $result
    local.get $base
    local.set $b
    local.get $exp
    local.set $e
    block ;; label = @1
      loop ;; label = @2
        local.get $e
        i32.const 0
        i32.gt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $e
        i32.const 1
        i32.and
        i32.const 1
        i32.eq
        if ;; label = @3
          local.get $result
          local.get $b
          i32.mul
          local.set $result
        end
        local.get $b
        local.get $b
        i32.mul
        local.set $b
        local.get $e
        i32.const 1
        i32.shr_s
        local.set $e
        br 0 (;@2;)
      end
    end
    local.get $result
    return
    unreachable
  )
  (func $fibonacci_iter_i64 (;5;) (type 5) (param $n i64) (result i64)
    (local $zero i64) (local $one i64) (local $a i64) (local $b i64) (local $i i64) (local $next i64)
    i64.const 0
    local.set $zero
    i64.const 1
    local.set $one
    local.get $n
    local.get $zero
    i64.le_s
    if ;; label = @1
      local.get $zero
      return
    end
    local.get $n
    local.get $one
    i64.eq
    if ;; label = @1
      local.get $one
      return
    end
    i64.const 0
    local.set $a
    i64.const 1
    local.set $b
    i64.const 2
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i64.le_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $a
        local.get $b
        i64.add
        local.set $next
        local.get $b
        local.set $a
        local.get $next
        local.set $b
        local.get $i
        local.get $one
        i64.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $b
    return
    unreachable
  )
  (func $gcd_iter_i64 (;6;) (type 6) (param $a i64) (param $b i64) (result i64)
    (local $zero i64) (local $x i64) (local $y i64) (local $t i64)
    i64.const 0
    local.set $zero
    local.get $a
    local.set $x
    local.get $b
    local.set $y
    local.get $x
    local.get $zero
    i64.lt_s
    if ;; label = @1
      local.get $zero
      local.get $x
      i64.sub
      local.set $x
    end
    local.get $y
    local.get $zero
    i64.lt_s
    if ;; label = @1
      local.get $zero
      local.get $y
      i64.sub
      local.set $y
    end
    block ;; label = @1
      loop ;; label = @2
        local.get $y
        local.get $zero
        i64.gt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $y
        local.set $t
        local.get $x
        local.get $y
        i64.rem_s
        local.set $y
        local.get $t
        local.set $x
        br 0 (;@2;)
      end
    end
    local.get $x
    return
    unreachable
  )
  (func $pow_iter_i64 (;7;) (type 7) (param $base i64) (param $exp i64) (result i64)
    (local $zero i64) (local $one i64) (local $result i64) (local $b i64) (local $e i64)
    i64.const 0
    local.set $zero
    i64.const 1
    local.set $one
    i64.const 1
    local.set $result
    local.get $base
    local.set $b
    local.get $exp
    local.set $e
    block ;; label = @1
      loop ;; label = @2
        local.get $e
        local.get $zero
        i64.gt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $e
        local.get $one
        i64.and
        local.get $one
        i64.eq
        if ;; label = @3
          local.get $result
          local.get $b
          i64.mul
          local.set $result
        end
        local.get $b
        local.get $b
        i64.mul
        local.set $b
        local.get $e
        local.get $one
        i64.shr_s
        local.set $e
        br 0 (;@2;)
      end
    end
    local.get $result
    return
    unreachable
  )
  (func $gcd_u8 (;8;) (type 8) (param $a i32) (param $b i32) (result i32)
    (local $zero i32) (local $x i32) (local $y i32) (local $t i32)
    local.get $a
    i32.const 255
    i32.and
    local.set $a
    local.get $b
    i32.const 255
    i32.and
    local.set $b
    i32.const 0
    local.set $zero
    local.get $a
    local.set $x
    local.get $b
    local.set $y
    block ;; label = @1
      loop ;; label = @2
        local.get $y
        local.get $zero
        i32.gt_u
        i32.eqz
        br_if 1 (;@1;)
        local.get $y
        local.set $t
        local.get $x
        local.get $y
        i32.rem_u
        local.set $y
        local.get $t
        local.set $x
        br 0 (;@2;)
      end
    end
    local.get $x
    return
    unreachable
  )
  (func $fibonacci_i16 (;9;) (type 9) (param $n i32) (result i32)
    (local $zero i32) (local $one i32) (local $a i32) (local $b i32) (local $i i32) (local $next i32)
    local.get $n
    i32.const 16
    i32.shl
    i32.const 16
    i32.shr_s
    local.set $n
    i32.const 0
    local.set $zero
    i32.const 1
    local.set $one
    local.get $n
    local.get $zero
    i32.le_s
    if ;; label = @1
      local.get $zero
      return
    end
    local.get $n
    local.get $one
    i32.eq
    if ;; label = @1
      local.get $one
      return
    end
    i32.const 0
    local.set $a
    i32.const 1
    local.set $b
    i32.const 2
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.le_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $a
        local.get $b
        i32.add
        i32.const 16
        i32.shl
        i32.const 16
        i32.shr_s
        local.set $next
        local.get $b
        local.set $a
        local.get $next
        local.set $b
        local.get $i
        local.get $one
        i32.add
        i32.const 16
        i32.shl
        i32.const 16
        i32.shr_s
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $b
    return
    unreachable
  )
  (func $pow_u16 (;10;) (type 10) (param $base i32) (param $exp i32) (result i32)
    (local $zero i32) (local $one i32) (local $result i32) (local $b i32) (local $e i32)
    local.get $base
    i32.const 65535
    i32.and
    local.set $base
    local.get $exp
    i32.const 65535
    i32.and
    local.set $exp
    i32.const 0
    local.set $zero
    i32.const 1
    local.set $one
    i32.const 1
    local.set $result
    local.get $base
    local.set $b
    local.get $exp
    local.set $e
    block ;; label = @1
      loop ;; label = @2
        local.get $e
        local.get $zero
        i32.gt_u
        i32.eqz
        br_if 1 (;@1;)
        local.get $e
        local.get $one
        i32.and
        i32.const 65535
        i32.and
        local.get $one
        i32.eq
        if ;; label = @3
          local.get $result
          local.get $b
          i32.mul
          i32.const 65535
          i32.and
          local.set $result
        end
        local.get $b
        local.get $b
        i32.mul
        i32.const 65535
        i32.and
        local.set $b
        local.get $e
        local.get $one
        i32.const 15
        i32.and
        i32.shr_u
        local.set $e
        br 0 (;@2;)
      end
    end
    local.get $result
    return
    unreachable
  )
  (func $is_prime_bool (;11;) (type 11) (param $n i32) (result i32)
    (local $result i32) (local $d i32)
    local.get $n
    i32.const 1
    i32.le_s
    if ;; label = @1
      i32.const 0
      return
    end
    local.get $n
    i32.const 3
    i32.le_s
    if ;; label = @1
      i32.const 1
      return
    end
    local.get $n
    i32.const 2
    i32.rem_s
    i32.const 0
    i32.eq
    if ;; label = @1
      i32.const 0
      return
    end
    i32.const 1
    local.set $result
    i32.const 3
    local.set $d
    block ;; label = @1
      loop ;; label = @2
        local.get $d
        local.get $d
        i32.mul
        local.get $n
        i32.le_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $n
        local.get $d
        i32.rem_s
        i32.const 0
        i32.eq
        if ;; label = @3
          i32.const 0
          local.set $result
          br 2 (;@1;)
        end
        local.get $d
        i32.const 2
        i32.add
        local.set $d
        br 0 (;@2;)
      end
    end
    local.get $result
    return
    unreachable
  )
)
