(module $output
  (type (;0;) (func (param i64) (result i64)))
  (type (;1;) (func (param i64) (result i64)))
  (type (;2;) (func (param i64 i64) (result i64)))
  (type (;3;) (func (param i64 i64) (result i64)))
  (type (;4;) (func (param i64 i64) (result i64)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (param i32) (result i32)))
  (type (;7;) (func (param i64) (result i64)))
  (type (;8;) (func (param i64) (result i64)))
  (export "factorial_i64" (func $factorial_i64))
  (export "fibonacci_i64" (func $fibonacci_i64))
  (export "gcd_i64" (func $gcd_i64))
  (export "lcm_i64" (func $lcm_i64))
  (export "is_even" (func $is_even))
  (export "is_odd" (func $is_odd))
  (export "sum_range_i64" (func $sum_range_i64))
  (func $factorial_i64 (;0;) (type 0) (param $n i64) (result i64)
    (local $one i64)
    i64.const 1
    local.set $one
    local.get $n
    local.get $one
    i64.le_s
    if ;; label = @1
      local.get $one
      return
    end
    local.get $n
    local.get $n
    local.get $one
    i64.sub
    call $factorial_i64
    i64.mul
    return
    unreachable
  )
  (func $fibonacci_i64 (;1;) (type 1) (param $n i64) (result i64)
    (local $zero i64) (local $one i64) (local $two i64)
    i64.const 0
    local.set $zero
    i64.const 1
    local.set $one
    i64.const 2
    local.set $two
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
    local.get $n
    local.get $one
    i64.sub
    call $fibonacci_i64
    local.get $n
    local.get $two
    i64.sub
    call $fibonacci_i64
    i64.add
    return
    unreachable
  )
  (func $gcd_i64_helper (;2;) (type 2) (param $a i64) (param $b i64) (result i64)
    (local $zero i64)
    i64.const 0
    local.set $zero
    local.get $b
    local.get $zero
    i64.eq
    if ;; label = @1
      local.get $a
      return
    end
    local.get $b
    local.get $a
    local.get $b
    i64.rem_s
    call $gcd_i64_helper
    return
    unreachable
  )
  (func $gcd_i64 (;3;) (type 3) (param $a i64) (param $b i64) (result i64)
    (local $zero i64)
    i64.const 0
    local.set $zero
    local.get $a
    local.get $zero
    i64.lt_s
    if ;; label = @1
      i64.const 0
      local.get $a
      i64.sub
      local.get $b
      call $gcd_i64_helper
      return
    end
    local.get $a
    local.get $b
    call $gcd_i64_helper
    return
    unreachable
  )
  (func $lcm_i64 (;4;) (type 4) (param $a i64) (param $b i64) (result i64)
    (local $zero i64) (local $g i64)
    i64.const 0
    local.set $zero
    local.get $a
    local.get $zero
    i64.eq
    if ;; label = @1
      local.get $zero
      return
    end
    local.get $b
    local.get $zero
    i64.eq
    if ;; label = @1
      local.get $zero
      return
    end
    local.get $a
    local.get $b
    call $gcd_i64
    local.set $g
    local.get $a
    local.get $g
    i64.div_s
    local.get $b
    i64.mul
    return
    unreachable
  )
  (func $is_even (;5;) (type 5) (param $n i32) (result i32)
    local.get $n
    i32.const 1
    i32.and
    i32.const 0
    i32.eq
    return
    unreachable
  )
  (func $is_odd (;6;) (type 6) (param $n i32) (result i32)
    local.get $n
    i32.const 1
    i32.and
    i32.const 1
    i32.eq
    return
    unreachable
  )
  (func $abs_i64 (;7;) (type 7) (param $x i64) (result i64)
    (local $zero i64)
    i64.const 0
    local.set $zero
    local.get $x
    local.get $zero
    i64.lt_s
    if ;; label = @1
      i64.const 0
      local.get $x
      i64.sub
      return
    end
    local.get $x
    return
    unreachable
  )
  (func $sum_range_i64 (;8;) (type 8) (param $n i64) (result i64)
    (local $zero i64) (local $one i64) (local $result i64) (local $i i64)
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
    i64.const 0
    local.set $result
    i64.const 1
    local.set $i
    local.get $i
    local.get $n
    i64.le_s
    if ;; label = @1
      local.get $result
      local.get $i
      i64.add
      local.set $result
      local.get $i
      local.get $one
      i64.add
      local.set $i
      local.get $i
      local.get $n
      i64.le_s
      if ;; label = @2
        local.get $result
        local.get $i
        i64.add
        local.set $result
        local.get $i
        local.get $one
        i64.add
        local.set $i
        local.get $i
        local.get $n
        i64.le_s
        if ;; label = @3
          local.get $result
          local.get $i
          i64.add
          local.set $result
          local.get $i
          local.get $one
          i64.add
          local.set $i
          local.get $i
          local.get $n
          i64.le_s
          if ;; label = @4
            local.get $result
            local.get $i
            i64.add
            local.set $result
            local.get $i
            local.get $one
            i64.add
            local.set $i
            local.get $i
            local.get $n
            i64.le_s
            if ;; label = @5
              local.get $result
              local.get $i
              i64.add
              local.set $result
            end
          end
        end
      end
    end
    local.get $result
    return
    unreachable
  )
)
