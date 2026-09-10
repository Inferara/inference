(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (param i32 i32) (result i32)))
  (type (;6;) (func (param i32 i32) (result i32)))
  (type (;7;) (func (param i32 i32) (result i32)))
  (type (;8;) (func (param i32 i32) (result i32)))
  (type (;9;) (func (param i32 i32) (result i32)))
  (type (;10;) (func (param i32 i32) (result i32)))
  (type (;11;) (func (param i32 i32) (result i32)))
  (type (;12;) (func (param i32 i32) (result i32)))
  (type (;13;) (func (param i64 i64) (result i64)))
  (type (;14;) (func (param i64 i64) (result i64)))
  (type (;15;) (func (param i32 i32) (result i32)))
  (type (;16;) (func (param i32 i32) (result i32)))
  (type (;17;) (func (param i64) (result i64)))
  (export "neg_add" (func $neg_add))
  (export "neg_sub" (func $neg_sub))
  (export "neg_mul" (func $neg_mul))
  (export "add_neg" (func $add_neg))
  (export "bitnot_and" (func $bitnot_and))
  (export "bitnot_or" (func $bitnot_or))
  (export "and_bitnot" (func $and_bitnot))
  (export "xor_bitnot" (func $xor_bitnot))
  (export "not_and" (func $not_and))
  (export "not_or" (func $not_or))
  (export "and_not" (func $and_not))
  (export "not_eq" (func $not_eq))
  (export "not_lt" (func $not_lt))
  (export "neg_i64_add" (func $neg_i64_add))
  (export "bitnot_i64_and" (func $bitnot_i64_and))
  (export "neg_shift" (func $neg_shift))
  (export "bitnot_shift" (func $bitnot_shift))
  (export "neg_i64" (func $neg_i64))
  (func $neg_add (;0;) (type 0) (param $a i32) (param $b i32) (result i32)
    (local i32 i32 i32)
    i32.const 0
    local.get $a
    i32.sub
    local.tee 4
    i32.const -2147483648
    i32.eq
    if ;; label = @1
      unreachable
    end
    local.get 4
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
  (func $neg_sub (;1;) (type 1) (param $a i32) (param $b i32) (result i32)
    (local i32 i32 i32)
    i32.const 0
    local.get $a
    i32.sub
    local.tee 4
    i32.const -2147483648
    i32.eq
    if ;; label = @1
      unreachable
    end
    local.get 4
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
  (func $neg_mul (;2;) (type 2) (param $a i32) (param $b i32) (result i32)
    (local i32 i32 i32)
    i32.const 0
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
    i32.sub
    local.tee 4
    i32.const -2147483648
    i32.eq
    if ;; label = @1
      unreachable
    end
    local.get 4
    return
    unreachable
  )
  (func $add_neg (;3;) (type 3) (param $a i32) (param $b i32) (result i32)
    (local i32 i32 i32)
    local.get $a
    i32.const 0
    local.get $b
    i32.sub
    local.tee 4
    i32.const -2147483648
    i32.eq
    if ;; label = @1
      unreachable
    end
    local.get 4
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
  (func $bitnot_and (;4;) (type 4) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const -1
    i32.xor
    local.get $b
    i32.and
    return
    unreachable
  )
  (func $bitnot_or (;5;) (type 5) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const -1
    i32.xor
    local.get $b
    i32.or
    return
    unreachable
  )
  (func $and_bitnot (;6;) (type 6) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.const -1
    i32.xor
    i32.and
    return
    unreachable
  )
  (func $xor_bitnot (;7;) (type 7) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.const -1
    i32.xor
    i32.xor
    return
    unreachable
  )
  (func $not_and (;8;) (type 8) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.eqz
    i32.eqz
    local.set $a
    local.get $b
    i32.eqz
    i32.eqz
    local.set $b
    local.get $a
    i32.eqz
    if (result i32) ;; label = @1
      local.get $b
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $not_or (;9;) (type 9) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.eqz
    i32.eqz
    local.set $a
    local.get $b
    i32.eqz
    i32.eqz
    local.set $b
    local.get $a
    i32.eqz
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $b
    end
    return
    unreachable
  )
  (func $and_not (;10;) (type 10) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.eqz
    i32.eqz
    local.set $a
    local.get $b
    i32.eqz
    i32.eqz
    local.set $b
    local.get $a
    if (result i32) ;; label = @1
      local.get $b
      i32.eqz
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $not_eq (;11;) (type 11) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.eq
    i32.eqz
    return
    unreachable
  )
  (func $not_lt (;12;) (type 12) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.lt_s
    i32.eqz
    return
    unreachable
  )
  (func $neg_i64_add (;13;) (type 13) (param $a i64) (param $b i64) (result i64)
    (local i64 i64 i64)
    i64.const 0
    local.get $a
    i64.sub
    local.tee 4
    i64.const -9223372036854775808
    i64.eq
    if ;; label = @1
      unreachable
    end
    local.get 4
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
  (func $bitnot_i64_and (;14;) (type 14) (param $a i64) (param $b i64) (result i64)
    local.get $a
    i64.const -1
    i64.xor
    local.get $b
    i64.and
    return
    unreachable
  )
  (func $neg_shift (;15;) (type 15) (param $a i32) (param $b i32) (result i32)
    (local i32 i32 i32)
    i32.const 0
    local.get $a
    i32.sub
    local.tee 4
    i32.const -2147483648
    i32.eq
    if ;; label = @1
      unreachable
    end
    local.get 4
    local.get $b
    i32.shl
    return
    unreachable
  )
  (func $bitnot_shift (;16;) (type 16) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const -1
    i32.xor
    local.get $b
    i32.shr_s
    return
    unreachable
  )
  (func $neg_i64 (;17;) (type 17) (param $a i64) (result i64)
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
  (@custom "inference.checked" (after code) "\01\07\00\01\02\03\0d\0f\11")
)
