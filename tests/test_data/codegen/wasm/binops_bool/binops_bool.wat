(module $output
  (type (;0;) (func (param i32 i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32 i32) (result i32)))
  (type (;2;) (func (param i32 i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32 i32) (result i32)))
  (type (;5;) (func (param i32 i32) (result i32)))
  (type (;6;) (func (param i32 i32) (result i32)))
  (type (;7;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;8;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;9;) (func (param i32 i32 i32) (result i32)))
  (type (;10;) (func (param i32 i32 i32) (result i32)))
  (type (;11;) (func (param i32 i32) (result i32)))
  (type (;12;) (func (param i32 i32) (result i32)))
  (type (;13;) (func (param i32 i32) (result i32)))
  (type (;14;) (func (param i32 i32 i32) (result i32)))
  (type (;15;) (func (param i32 i32) (result i32)))
  (type (;16;) (func (param i32 i32) (result i32)))
  (export "and3" (func $and3))
  (export "or3" (func $or3))
  (export "and_or" (func $and_or))
  (export "or_and" (func $or_and))
  (export "not_and_or" (func $not_and_or))
  (export "de_morgan_and" (func $de_morgan_and))
  (export "de_morgan_or" (func $de_morgan_or))
  (export "cmp_and_cmp" (func $cmp_and_cmp))
  (export "cmp_or_cmp" (func $cmp_or_cmp))
  (export "between" (func $between))
  (export "not_between" (func $not_between))
  (export "all_same_sign" (func $all_same_sign))
  (export "xor_bool" (func $xor_bool))
  (export "implies" (func $implies))
  (export "bool_majority3" (func $bool_majority3))
  (export "eq_bool" (func $eq_bool))
  (export "ne_bool" (func $ne_bool))
  (func $and3 (;0;) (type 0) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      local.get $b
    else
      i32.const 0
    end
    if (result i32) ;; label = @1
      local.get $c
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $or3 (;1;) (type 1) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $b
    end
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $c
    end
    return
    unreachable
  )
  (func $and_or (;2;) (type 2) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      local.get $b
    else
      i32.const 0
    end
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $c
    end
    return
    unreachable
  )
  (func $or_and (;3;) (type 3) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $b
      if (result i32) ;; label = @2
        local.get $c
      else
        i32.const 0
      end
    end
    return
    unreachable
  )
  (func $not_and_or (;4;) (type 4) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    i32.eqz
    if (result i32) ;; label = @1
      local.get $b
      if (result i32) ;; label = @2
        i32.const 1
      else
        local.get $c
      end
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $de_morgan_and (;5;) (type 5) (param $a i32) (param $b i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      local.get $b
    else
      i32.const 0
    end
    i32.eqz
    return
    unreachable
  )
  (func $de_morgan_or (;6;) (type 6) (param $a i32) (param $b i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $b
    end
    i32.eqz
    return
    unreachable
  )
  (func $cmp_and_cmp (;7;) (type 7) (param $a i32) (param $b i32) (param $c i32) (param $d i32) (result i32)
    local.get $a
    local.get $b
    i32.lt_s
    if (result i32) ;; label = @1
      local.get $c
      local.get $d
      i32.lt_s
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $cmp_or_cmp (;8;) (type 8) (param $a i32) (param $b i32) (param $c i32) (param $d i32) (result i32)
    local.get $a
    local.get $b
    i32.gt_s
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $c
      local.get $d
      i32.gt_s
    end
    return
    unreachable
  )
  (func $between (;9;) (type 9) (param $x i32) (param $lo i32) (param $hi i32) (result i32)
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
    return
    unreachable
  )
  (func $not_between (;10;) (type 10) (param $x i32) (param $lo i32) (param $hi i32) (result i32)
    local.get $x
    local.get $lo
    i32.lt_s
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $x
      local.get $hi
      i32.gt_s
    end
    return
    unreachable
  )
  (func $all_same_sign (;11;) (type 11) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const 0
    i32.ge_s
    local.get $b
    i32.const 0
    i32.ge_s
    i32.eq
    return
    unreachable
  )
  (func $xor_bool (;12;) (type 12) (param $a i32) (param $b i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $b
    end
    if (result i32) ;; label = @1
      local.get $a
      if (result i32) ;; label = @2
        local.get $b
      else
        i32.const 0
      end
      i32.eqz
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $implies (;13;) (type 13) (param $a i32) (param $b i32) (result i32)
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
  (func $bool_majority3 (;14;) (type 14) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      local.get $b
    else
      i32.const 0
    end
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $b
      if (result i32) ;; label = @2
        local.get $c
      else
        i32.const 0
      end
    end
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $a
      if (result i32) ;; label = @2
        local.get $c
      else
        i32.const 0
      end
    end
    return
    unreachable
  )
  (func $eq_bool (;15;) (type 15) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.eq
    return
    unreachable
  )
  (func $ne_bool (;16;) (type 16) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.ne
    return
    unreachable
  )
)
