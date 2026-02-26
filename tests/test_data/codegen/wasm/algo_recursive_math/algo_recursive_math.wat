(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (param i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (param i32) (result i32)))
  (export "factorial" (func $factorial))
  (export "fibonacci" (func $fibonacci))
  (export "gcd" (func $gcd))
  (export "power" (func $power))
  (export "digit_sum" (func $digit_sum))
  (export "digit_count" (func $digit_count))
  (func $abs_i32 (;0;) (type 0) (param $x i32) (result i32)
    local.get $x
    i32.const 0
    i32.lt_s
    if ;; label = @1
      i32.const 0
      local.get $x
      i32.sub
      return
    end
    local.get $x
    return
    unreachable
  )
  (func $factorial (;1;) (type 1) (param $n i32) (result i32)
    local.get $n
    i32.const 1
    i32.le_s
    if ;; label = @1
      i32.const 1
      return
    end
    local.get $n
    local.get $n
    i32.const 1
    i32.sub
    call $factorial
    i32.mul
    return
    unreachable
  )
  (func $fibonacci (;2;) (type 2) (param $n i32) (result i32)
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
    local.get $n
    i32.const 1
    i32.sub
    call $fibonacci
    local.get $n
    i32.const 2
    i32.sub
    call $fibonacci
    i32.add
    return
    unreachable
  )
  (func $gcd (;3;) (type 3) (param $a i32) (param $b i32) (result i32)
    local.get $b
    i32.const 0
    i32.eq
    if ;; label = @1
      local.get $a
      call $abs_i32
      return
    end
    local.get $b
    local.get $a
    local.get $b
    i32.rem_s
    call $gcd
    return
    unreachable
  )
  (func $power (;4;) (type 4) (param $base i32) (param $exp i32) (result i32)
    (local $half i32)
    local.get $exp
    i32.const 0
    i32.le_s
    if ;; label = @1
      i32.const 1
      return
    end
    local.get $exp
    i32.const 2
    i32.rem_s
    i32.const 0
    i32.eq
    if ;; label = @1
      local.get $base
      local.get $exp
      i32.const 2
      i32.div_s
      call $power
      local.set $half
      local.get $half
      local.get $half
      i32.mul
      return
    end
    local.get $base
    local.get $base
    local.get $exp
    i32.const 1
    i32.sub
    call $power
    i32.mul
    return
    unreachable
  )
  (func $digit_sum (;5;) (type 5) (param $n i32) (result i32)
    (local $a i32)
    local.get $n
    call $abs_i32
    local.set $a
    local.get $a
    i32.const 10
    i32.lt_s
    if ;; label = @1
      local.get $a
      return
    end
    local.get $a
    i32.const 10
    i32.rem_s
    local.get $a
    i32.const 10
    i32.div_s
    call $digit_sum
    i32.add
    return
    unreachable
  )
  (func $digit_count (;6;) (type 6) (param $n i32) (result i32)
    (local $a i32)
    local.get $n
    call $abs_i32
    local.set $a
    local.get $a
    i32.const 10
    i32.lt_s
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 1
    local.get $a
    i32.const 10
    i32.div_s
    call $digit_count
    i32.add
    return
    unreachable
  )
)
