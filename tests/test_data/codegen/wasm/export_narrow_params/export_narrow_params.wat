(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (param i32) (result i32)))
  (type (;7;) (func (param i32) (result i32)))
  (type (;8;) (func (param i32) (result i32)))
  (type (;9;) (func (param i32) (result i32)))
  (type (;10;) (func (param i32) (result i32)))
  (type (;11;) (func (param i32) (result i32)))
  (type (;12;) (func (param i32 i32 i32) (result i32)))
  (export "id_u8" (func $id_u8))
  (export "id_i8" (func $id_i8))
  (export "id_u16" (func $id_u16))
  (export "id_i16" (func $id_i16))
  (export "id_bool" (func $id_bool))
  (export "gt100_u8" (func $gt100_u8))
  (export "is_neg_i8" (func $is_neg_i8))
  (export "bool_if" (func $bool_if))
  (export "bool_eq_true" (func $bool_eq_true))
  (export "bool_and_pass" (func $bool_and_pass))
  (export "call_helper" (func $call_helper))
  (export "mixed" (func $mixed))
  (func $id_u8 (;0;) (type 0) (param $v i32) (result i32)
    local.get $v
    i32.const 255
    i32.and
    local.set $v
    local.get $v
    return
    unreachable
  )
  (func $id_i8 (;1;) (type 1) (param $v i32) (result i32)
    local.get $v
    i32.const 24
    i32.shl
    i32.const 24
    i32.shr_s
    local.set $v
    local.get $v
    return
    unreachable
  )
  (func $id_u16 (;2;) (type 2) (param $v i32) (result i32)
    local.get $v
    i32.const 65535
    i32.and
    local.set $v
    local.get $v
    return
    unreachable
  )
  (func $id_i16 (;3;) (type 3) (param $v i32) (result i32)
    local.get $v
    i32.const 16
    i32.shl
    i32.const 16
    i32.shr_s
    local.set $v
    local.get $v
    return
    unreachable
  )
  (func $id_bool (;4;) (type 4) (param $b i32) (result i32)
    local.get $b
    i32.eqz
    i32.eqz
    local.set $b
    local.get $b
    return
    unreachable
  )
  (func $gt100_u8 (;5;) (type 5) (param $v i32) (result i32)
    (local $H i32)
    local.get $v
    i32.const 255
    i32.and
    local.set $v
    i32.const 100
    local.set $H
    local.get $v
    local.get $H
    i32.gt_u
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $is_neg_i8 (;6;) (type 6) (param $v i32) (result i32)
    (local $L i32)
    local.get $v
    i32.const 24
    i32.shl
    i32.const 24
    i32.shr_s
    local.set $v
    i32.const 0
    local.set $L
    local.get $v
    local.get $L
    i32.lt_s
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $bool_if (;7;) (type 7) (param $b i32) (result i32)
    local.get $b
    i32.eqz
    i32.eqz
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
  (func $bool_eq_true (;8;) (type 8) (param $b i32) (result i32)
    local.get $b
    i32.eqz
    i32.eqz
    local.set $b
    local.get $b
    i32.const 1
    i32.eq
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $bool_and_pass (;9;) (type 9) (param $b i32) (result i32)
    (local $t i32) (local $r i32)
    local.get $b
    i32.eqz
    i32.eqz
    local.set $b
    i32.const 1
    local.set $t
    local.get $t
    if (result i32) ;; label = @1
      local.get $b
    else
      i32.const 0
    end
    local.set $r
    local.get $r
    i32.const 1
    i32.eq
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $helper_u8 (;10;) (type 10) (param $v i32) (result i32)
    local.get $v
    return
    unreachable
  )
  (func $call_helper (;11;) (type 11) (param $v i32) (result i32)
    local.get $v
    i32.const 255
    i32.and
    local.set $v
    local.get $v
    call $helper_u8
    return
    unreachable
  )
  (func $mixed (;12;) (type 12) (param $a i32) (param $x i32) (param $b i32) (result i32)
    (local $Z i32)
    local.get $a
    i32.const 255
    i32.and
    local.set $a
    local.get $b
    i32.eqz
    i32.eqz
    local.set $b
    local.get $b
    if ;; label = @1
      local.get $x
      return
    end
    i32.const 0
    local.set $Z
    local.get $a
    local.get $Z
    i32.gt_u
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
)
