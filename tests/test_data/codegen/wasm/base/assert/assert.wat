(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (param i32) (result i32)))
  (type (;7;) (func (param i32 i32) (result i32)))
  (type (;8;) (func (param i32 i32) (result i32)))
  (type (;9;) (func (param i32 i32) (result i32)))
  (type (;10;) (func (param i32 i32 i32) (result i32)))
  (type (;11;) (func (param i32) (result i32)))
  (export "assert_literal_true" (func $assert_literal_true))
  (export "assert_variable" (func $assert_variable))
  (export "assert_in_if" (func $assert_in_if))
  (export "assert_in_loop_with_break" (func $assert_in_loop_with_break))
  (export "double_assert" (func $double_assert))
  (export "assert_bool_param" (func $assert_bool_param))
  (export "assert_not" (func $assert_not))
  (export "assert_and" (func $assert_and))
  (export "assert_or" (func $assert_or))
  (export "assert_eq_i32" (func $assert_eq_i32))
  (export "assert_complex" (func $assert_complex))
  (export "assert_bool_local" (func $assert_bool_local))
  (func $assert_literal_true (;0;) (type 0) (result i32)
    i32.const 1
    i32.eqz
    if ;; label = @1
      unreachable
    end
    i32.const 1
    return
    unreachable
  )
  (func $assert_variable (;1;) (type 1) (param $x i32) (result i32)
    local.get $x
    i32.const 0
    i32.gt_s
    i32.eqz
    if ;; label = @1
      unreachable
    end
    local.get $x
    return
    unreachable
  )
  (func $assert_in_if (;2;) (type 2) (param $x i32) (result i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if ;; label = @1
      local.get $x
      i32.const 100
      i32.lt_s
      i32.eqz
      if ;; label = @2
        unreachable
      end
      local.get $x
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $assert_in_loop_with_break (;3;) (type 3) (param $n i32) (result i32)
    (local $i i32)
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
        local.get $n
        i32.lt_s
        i32.eqz
        if ;; label = @3
          unreachable
        end
        local.get $i
        i32.const 5
        i32.ge_s
        if ;; label = @3
          br 2 (;@1;)
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $i
    return
    unreachable
  )
  (func $double_assert (;4;) (type 4) (param $x i32) (param $y i32) (result i32)
    local.get $x
    i32.const 0
    i32.gt_s
    i32.eqz
    if ;; label = @1
      unreachable
    end
    local.get $y
    i32.const 0
    i32.gt_s
    i32.eqz
    if ;; label = @1
      unreachable
    end
    local.get $x
    local.get $y
    i32.add
    return
    unreachable
  )
  (func $assert_bool_param (;5;) (type 5) (param $b i32) (result i32)
    local.get $b
    i32.eqz
    i32.eqz
    local.set $b
    local.get $b
    i32.eqz
    if ;; label = @1
      unreachable
    end
    i32.const 1
    return
    unreachable
  )
  (func $assert_not (;6;) (type 6) (param $b i32) (result i32)
    local.get $b
    i32.eqz
    i32.eqz
    local.set $b
    local.get $b
    i32.eqz
    i32.eqz
    if ;; label = @1
      unreachable
    end
    i32.const 1
    return
    unreachable
  )
  (func $assert_and (;7;) (type 7) (param $a i32) (param $b i32) (result i32)
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
    else
      i32.const 0
    end
    i32.eqz
    if ;; label = @1
      unreachable
    end
    i32.const 1
    return
    unreachable
  )
  (func $assert_or (;8;) (type 8) (param $a i32) (param $b i32) (result i32)
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
      i32.const 1
    else
      local.get $b
    end
    i32.eqz
    if ;; label = @1
      unreachable
    end
    i32.const 1
    return
    unreachable
  )
  (func $assert_eq_i32 (;9;) (type 9) (param $x i32) (param $y i32) (result i32)
    local.get $x
    local.get $y
    i32.eq
    i32.eqz
    if ;; label = @1
      unreachable
    end
    local.get $x
    return
    unreachable
  )
  (func $assert_complex (;10;) (type 10) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    i32.const 0
    i32.gt_s
    if (result i32) ;; label = @1
      local.get $b
      i32.const 10
      i32.lt_s
      if (result i32) ;; label = @2
        i32.const 1
      else
        local.get $c
        i32.const 0
        i32.eq
      end
    else
      i32.const 0
    end
    i32.eqz
    if ;; label = @1
      unreachable
    end
    local.get $a
    local.get $b
    i32.add
    local.get $c
    i32.add
    return
    unreachable
  )
  (func $assert_bool_local (;11;) (type 11) (param $x i32) (result i32)
    (local $b i32)
    local.get $x
    i32.const 0
    i32.gt_s
    local.set $b
    local.get $b
    i32.eqz
    if ;; label = @1
      unreachable
    end
    local.get $x
    return
    unreachable
  )
)
