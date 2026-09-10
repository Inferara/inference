(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i64)))
  (type (;3;) (func (result i64)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i64)))
  (type (;8;) (func (param i32 i32) (result i32)))
  (type (;9;) (func (param i32 i32) (result i32)))
  (type (;10;) (func (param i64 i64) (result i64)))
  (type (;11;) (func (param i64 i64) (result i64)))
  (type (;12;) (func (param i32 i32) (result i32)))
  (type (;13;) (func (param i32 i32) (result i32)))
  (type (;14;) (func (param i32) (result i32)))
  (type (;15;) (func (param i64) (result i64)))
  (export "i32_max_plus_one_wrap" (func $i32_max_plus_one_wrap))
  (export "i32_min_minus_one_wrap" (func $i32_min_minus_one_wrap))
  (export "i64_max_plus_one_wrap" (func $i64_max_plus_one_wrap))
  (export "i64_min_minus_one_wrap" (func $i64_min_minus_one_wrap))
  (export "u32_max_plus_one_wrap" (func $u32_max_plus_one_wrap))
  (export "i32_mul_overflow_wrap" (func $i32_mul_overflow_wrap))
  (export "i32_neg_min_wrap" (func $i32_neg_min_wrap))
  (export "i64_neg_min_wrap" (func $i64_neg_min_wrap))
  (export "i32_add" (func $i32_add))
  (export "i32_sub" (func $i32_sub))
  (export "i64_add" (func $i64_add))
  (export "i64_sub" (func $i64_sub))
  (export "u32_add" (func $u32_add))
  (export "i32_mul" (func $i32_mul))
  (export "i32_neg" (func $i32_neg))
  (export "i64_neg" (func $i64_neg))
  (func $i32_max_plus_one_wrap (;0;) (type 0) (result i32)
    (local $max i32)
    i32.const 2147483647
    local.set $max
    local.get $max
    i32.const 1
    i32.add
    return
    unreachable
  )
  (func $i32_min_minus_one_wrap (;1;) (type 1) (result i32)
    (local $min i32)
    i32.const -2147483648
    local.set $min
    local.get $min
    i32.const 1
    i32.sub
    return
    unreachable
  )
  (func $i64_max_plus_one_wrap (;2;) (type 2) (result i64)
    (local $max i64) (local $one i64)
    i64.const 9223372036854775807
    local.set $max
    i64.const 1
    local.set $one
    local.get $max
    local.get $one
    i64.add
    return
    unreachable
  )
  (func $i64_min_minus_one_wrap (;3;) (type 3) (result i64)
    (local $min i64) (local $one i64)
    i64.const -9223372036854775808
    local.set $min
    i64.const 1
    local.set $one
    local.get $min
    local.get $one
    i64.sub
    return
    unreachable
  )
  (func $u32_max_plus_one_wrap (;4;) (type 4) (result i32)
    (local $max i32) (local $one i32)
    i32.const -1
    local.set $max
    i32.const 1
    local.set $one
    local.get $max
    local.get $one
    i32.add
    return
    unreachable
  )
  (func $i32_mul_overflow_wrap (;5;) (type 5) (result i32)
    (local $big i32)
    i32.const 2147483647
    local.set $big
    local.get $big
    i32.const 2
    i32.mul
    return
    unreachable
  )
  (func $i32_neg_min_wrap (;6;) (type 6) (result i32)
    (local $min i32)
    i32.const -2147483648
    local.set $min
    i32.const 0
    local.get $min
    i32.sub
    return
    unreachable
  )
  (func $i64_neg_min_wrap (;7;) (type 7) (result i64)
    (local $min i64)
    i64.const -9223372036854775808
    local.set $min
    i64.const 0
    local.get $min
    i64.sub
    return
    unreachable
  )
  (func $i32_add (;8;) (type 8) (param $a i32) (param $b i32) (result i32)
    (local i32 i32 i32)
    local.get $a
    local.get $b
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
    if ;; label = @1
      unreachable
    end
    local.get 4
    return
    unreachable
  )
  (func $i32_sub (;9;) (type 9) (param $a i32) (param $b i32) (result i32)
    (local i32 i32 i32)
    local.get $a
    local.get $b
    local.set 3
    local.tee 2
    local.get 3
    i32.sub
    local.tee 4
    local.get 2
    i32.lt_s
    local.get 3
    i32.const 0
    i32.gt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 4
    return
    unreachable
  )
  (func $i64_add (;10;) (type 10) (param $a i64) (param $b i64) (result i64)
    (local i64 i64 i64)
    local.get $a
    local.get $b
    local.set 3
    local.tee 2
    local.get 3
    i64.add
    local.tee 4
    local.get 2
    i64.lt_s
    local.get 3
    i64.const 0
    i64.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 4
    return
    unreachable
  )
  (func $i64_sub (;11;) (type 11) (param $a i64) (param $b i64) (result i64)
    (local i64 i64 i64)
    local.get $a
    local.get $b
    local.set 3
    local.tee 2
    local.get 3
    i64.sub
    local.tee 4
    local.get 2
    i64.lt_s
    local.get 3
    i64.const 0
    i64.gt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 4
    return
    unreachable
  )
  (func $u32_add (;12;) (type 12) (param $a i32) (param $b i32) (result i32)
    (local i32 i32 i32)
    local.get $a
    local.get $b
    local.tee 3
    i32.add
    local.tee 4
    local.get 3
    i32.lt_u
    if ;; label = @1
      unreachable
    end
    local.get 4
    return
    unreachable
  )
  (func $i32_mul (;13;) (type 13) (param $a i32) (param $b i32) (result i32)
    (local i32 i32 i32)
    local.get $a
    local.get $b
    local.set 3
    local.tee 2
    local.get 3
    i32.mul
    local.set 4
    local.get 3
    i32.const 0
    i32.ne
    if ;; label = @1
      local.get 3
      i32.const -1
      i32.eq
      if ;; label = @2
        local.get 2
        i32.const -2147483648
        i32.eq
        if ;; label = @3
          unreachable
        end
      else
        local.get 4
        local.get 3
        i32.div_s
        local.get 2
        i32.ne
        if ;; label = @3
          unreachable
        end
      end
    end
    local.get 4
    return
    unreachable
  )
  (func $i32_neg (;14;) (type 14) (param $a i32) (result i32)
    (local i32 i32 i32)
    i32.const 0
    local.get $a
    i32.sub
    local.tee 3
    i32.const -2147483648
    i32.eq
    if ;; label = @1
      unreachable
    end
    local.get 3
    return
    unreachable
  )
  (func $i64_neg (;15;) (type 15) (param $a i64) (result i64)
    (local i64 i64 i64)
    i64.const 0
    local.get $a
    i64.sub
    local.tee 3
    i64.const -9223372036854775808
    i64.eq
    if ;; label = @1
      unreachable
    end
    local.get 3
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\08\08\09\0a\0b\0c\0d\0e\0f")
)
