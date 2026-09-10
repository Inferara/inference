(module $output
  (type (;0;) (func (param i64 i64) (result i64)))
  (type (;1;) (func (param i64 i64) (result i64)))
  (type (;2;) (func (param i64 i64) (result i64)))
  (type (;3;) (func (param i64 i64) (result i64)))
  (type (;4;) (func (param i64 i64) (result i64)))
  (type (;5;) (func (param i64 i64) (result i64)))
  (type (;6;) (func (param i64 i64) (result i64)))
  (type (;7;) (func (param i64 i64) (result i64)))
  (type (;8;) (func (param i64 i64 i64) (result i64)))
  (type (;9;) (func (param i64 i64 i64) (result i64)))
  (type (;10;) (func (result i64)))
  (type (;11;) (func (param i64 i64) (result i64)))
  (export "add_i64" (func $add_i64))
  (export "sub_i64" (func $sub_i64))
  (export "mul_i64" (func $mul_i64))
  (export "div_i64_signed" (func $div_i64_signed))
  (export "div_u64" (func $div_u64))
  (export "mod_i64_signed" (func $mod_i64_signed))
  (export "mod_u64" (func $mod_u64))
  (export "add_i64_with_let" (func $add_i64_with_let))
  (export "sub_chain_i64" (func $sub_chain_i64))
  (export "mul_add_i64" (func $mul_add_i64))
  (export "add_max_i64" (func $add_max_i64))
  (export "sub_neg_result_i64" (func $sub_neg_result_i64))
  (func $add_i64 (;0;) (type 0) (param $a i64) (param $b i64) (result i64)
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
  (func $sub_i64 (;1;) (type 1) (param $a i64) (param $b i64) (result i64)
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
  (func $mul_i64 (;2;) (type 2) (param $a i64) (param $b i64) (result i64)
    (local i64 i64 i64)
    local.get $a
    local.get $b
    local.set 3
    local.tee 2
    local.get 3
    i64.mul
    local.set 4
    local.get 3
    i64.const 0
    i64.ne
    if ;; label = @1
      local.get 3
      i64.const -1
      i64.eq
      if ;; label = @2
        local.get 2
        i64.const -9223372036854775808
        i64.eq
        if ;; label = @3
          unreachable
        end
      else
        local.get 4
        local.get 3
        i64.div_s
        local.get 2
        i64.ne
        if ;; label = @3
          unreachable
        end
      end
    end
    local.get 4
    return
    unreachable
  )
  (func $div_i64_signed (;3;) (type 3) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.div_s
    return
    unreachable
  )
  (func $div_u64 (;4;) (type 4) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.div_u
    return
    unreachable
  )
  (func $mod_i64_signed (;5;) (type 5) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.rem_s
    return
    unreachable
  )
  (func $mod_u64 (;6;) (type 6) (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.rem_u
    return
    unreachable
  )
  (func $add_i64_with_let (;7;) (type 7) (param $a i64) (param $b i64) (result i64)
    (local $c i64) (local i64 i64 i64)
    local.get $a
    local.get $b
    local.set 4
    local.tee 3
    local.get 4
    i64.add
    local.tee 5
    local.get 3
    i64.lt_s
    local.get 4
    i64.const 0
    i64.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 5
    local.set $c
    local.get $c
    return
    unreachable
  )
  (func $sub_chain_i64 (;8;) (type 8) (param $a i64) (param $b i64) (param $c i64) (result i64)
    (local i64 i64 i64)
    local.get $a
    local.get $b
    local.set 4
    local.tee 3
    local.get 4
    i64.sub
    local.tee 5
    local.get 3
    i64.lt_s
    local.get 4
    i64.const 0
    i64.gt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 5
    local.get $c
    local.set 4
    local.tee 3
    local.get 4
    i64.sub
    local.tee 5
    local.get 3
    i64.lt_s
    local.get 4
    i64.const 0
    i64.gt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 5
    return
    unreachable
  )
  (func $mul_add_i64 (;9;) (type 9) (param $a i64) (param $b i64) (param $c i64) (result i64)
    (local i64 i64 i64)
    local.get $a
    local.get $b
    local.set 4
    local.tee 3
    local.get 4
    i64.mul
    local.set 5
    local.get 4
    i64.const 0
    i64.ne
    if ;; label = @1
      local.get 4
      i64.const -1
      i64.eq
      if ;; label = @2
        local.get 3
        i64.const -9223372036854775808
        i64.eq
        if ;; label = @3
          unreachable
        end
      else
        local.get 5
        local.get 4
        i64.div_s
        local.get 3
        i64.ne
        if ;; label = @3
          unreachable
        end
      end
    end
    local.get 5
    local.get $c
    local.set 4
    local.tee 3
    local.get 4
    i64.add
    local.tee 5
    local.get 3
    i64.lt_s
    local.get 4
    i64.const 0
    i64.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 5
    return
    unreachable
  )
  (func $add_max_i64 (;10;) (type 10) (result i64)
    (local $a i64) (local $b i64) (local i64 i64 i64)
    i64.const 9223372036854775806
    local.set $a
    i64.const 1
    local.set $b
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
  (func $sub_neg_result_i64 (;11;) (type 11) (param $a i64) (param $b i64) (result i64)
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
  (@custom "inference.checked" (after code) "\01\08\00\01\02\07\08\09\0a\0b")
)
