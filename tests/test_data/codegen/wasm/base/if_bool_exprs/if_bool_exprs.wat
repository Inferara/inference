(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32) (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (param i32 i32 i32) (result i32)))
  (type (;6;) (func (param i32 i32 i32) (result i32)))
  (type (;7;) (func (param i32 i32) (result i32)))
  (type (;8;) (func (param i32 i32) (result i32)))
  (type (;9;) (func (param i32 i32 i32) (result i32)))
  (type (;10;) (func (param i32) (result i32)))
  (type (;11;) (func (param i32 i32) (result i32)))
  (type (;12;) (func (param i32 i32) (result i32)))
  (type (;13;) (func (param i32 i32) (result i32)))
  (type (;14;) (func (param i32) (result i32)))
  (type (;15;) (func (param i32 i32) (result i32)))
  (export "if_bool_param" (func $if_bool_param))
  (export "if_not_param" (func $if_not_param))
  (export "if_and" (func $if_and))
  (export "if_or" (func $if_or))
  (export "if_not_cmp" (func $if_not_cmp))
  (export "if_and_or" (func $if_and_or))
  (export "if_or_and" (func $if_or_and))
  (export "if_demorgan_and" (func $if_demorgan_and))
  (export "if_demorgan_or" (func $if_demorgan_or))
  (export "if_between" (func $if_between))
  (export "if_bool_local" (func $if_bool_local))
  (export "if_bool_local_complex" (func $if_bool_local_complex))
  (export "if_bool_eq" (func $if_bool_eq))
  (export "if_bool_ne" (func $if_bool_ne))
  (export "cond_returns_bool" (func $cond_returns_bool))
  (export "if_else_complex" (func $if_else_complex))
  (func $if_bool_param (;0;) (type 0) (param $cond i32) (result i32)
    local.get $cond
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_not_param (;1;) (type 1) (param $cond i32) (result i32)
    local.get $cond
    i32.eqz
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_and (;2;) (type 2) (param $x i32) (param $y i32) (result i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if (result i32) ;; label = @1
      local.get $y
      i32.const 0
      i32.gt_s
    else
      i32.const 0
    end
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_or (;3;) (type 3) (param $x i32) (param $y i32) (result i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $y
      i32.const 0
      i32.gt_s
    end
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_not_cmp (;4;) (type 4) (param $x i32) (result i32)
    local.get $x
    i32.const 0
    i32.gt_s
    i32.eqz
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_and_or (;5;) (type 5) (param $a i32) (param $b i32) (param $c i32) (result i32)
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
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_or_and (;6;) (type 6) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    i32.const 0
    i32.gt_s
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $b
      i32.const 0
      i32.gt_s
      if (result i32) ;; label = @2
        local.get $c
        i32.const 0
        i32.gt_s
      else
        i32.const 0
      end
    end
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_demorgan_and (;7;) (type 7) (param $a i32) (param $b i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      local.get $b
    else
      i32.const 0
    end
    i32.eqz
    if ;; label = @1
      i32.const 1
      return
    else
      i32.const 0
      return
    end
    unreachable
  )
  (func $if_demorgan_or (;8;) (type 8) (param $a i32) (param $b i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $b
    end
    i32.eqz
    if ;; label = @1
      i32.const 1
      return
    else
      i32.const 0
      return
    end
    unreachable
  )
  (func $if_between (;9;) (type 9) (param $x i32) (param $lo i32) (param $hi i32) (result i32)
    local.get $x
    local.get $lo
    i32.ge_s
    if (result i32) ;; label = @1
      local.get $x
      local.get $hi
      i32.le_s
    else
      i32.const 0
    end
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_bool_local (;10;) (type 10) (param $x i32) (result i32)
    (local $b i32)
    local.get $x
    i32.const 0
    i32.gt_s
    local.set $b
    local.get $b
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_bool_local_complex (;11;) (type 11) (param $x i32) (param $y i32) (result i32)
    (local $b i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if (result i32) ;; label = @1
      local.get $y
      i32.const 0
      i32.gt_s
    else
      i32.const 0
    end
    local.set $b
    local.get $b
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_bool_eq (;12;) (type 12) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.eq
    if ;; label = @1
      i32.const 1
      return
    else
      i32.const 0
      return
    end
    unreachable
  )
  (func $if_bool_ne (;13;) (type 13) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.ne
    if ;; label = @1
      i32.const 1
      return
    else
      i32.const 0
      return
    end
    unreachable
  )
  (func $cond_returns_bool (;14;) (type 14) (param $x i32) (result i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_else_complex (;15;) (type 15) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const 0
    i32.gt_s
    if (result i32) ;; label = @1
      local.get $b
      i32.const 0
      i32.gt_s
    else
      i32.const 0
    end
    if ;; label = @1
      local.get $a
      return
    else
      local.get $b
      return
    end
    unreachable
  )
)
