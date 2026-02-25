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
  (type (;13;) (func (param i32 i32) (result i32)))
  (type (;14;) (func (param i32 i32) (result i32)))
  (type (;15;) (func (param i32 i32) (result i32)))
  (export "add_i8" (func $add_i8))
  (export "sub_i8" (func $sub_i8))
  (export "mul_i8" (func $mul_i8))
  (export "div_i8" (func $div_i8))
  (export "lt_i8" (func $lt_i8))
  (export "add_u8" (func $add_u8))
  (export "div_u8" (func $div_u8))
  (export "lt_u8" (func $lt_u8))
  (export "shr_u8" (func $shr_u8))
  (export "add_i16" (func $add_i16))
  (export "div_i16" (func $div_i16))
  (export "lt_i16" (func $lt_i16))
  (export "add_u16" (func $add_u16))
  (export "div_u16" (func $div_u16))
  (export "gt_u16" (func $gt_u16))
  (export "mod_u16" (func $mod_u16))
  (func $add_i8 (;0;) (type 0) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $sub_i8 (;1;) (type 1) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.sub
    return
    unreachable
  )
  (func $mul_i8 (;2;) (type 2) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.mul
    return
    unreachable
  )
  (func $div_i8 (;3;) (type 3) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.div_s
    return
    unreachable
  )
  (func $lt_i8 (;4;) (type 4) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.lt_s
    return
    unreachable
  )
  (func $add_u8 (;5;) (type 5) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $div_u8 (;6;) (type 6) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.div_u
    return
    unreachable
  )
  (func $lt_u8 (;7;) (type 7) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.lt_u
    return
    unreachable
  )
  (func $shr_u8 (;8;) (type 8) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.shr_u
    return
    unreachable
  )
  (func $add_i16 (;9;) (type 9) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $div_i16 (;10;) (type 10) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.div_s
    return
    unreachable
  )
  (func $lt_i16 (;11;) (type 11) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.lt_s
    return
    unreachable
  )
  (func $add_u16 (;12;) (type 12) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
    return
    unreachable
  )
  (func $div_u16 (;13;) (type 13) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.div_u
    return
    unreachable
  )
  (func $gt_u16 (;14;) (type 14) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.gt_u
    return
    unreachable
  )
  (func $mod_u16 (;15;) (type 15) (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.rem_u
    return
    unreachable
  )
)
