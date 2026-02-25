(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (param i32 i32) (result i32)))
  (type (;7;) (func (param i32 i32) (result i32)))
  (type (;8;) (func (param i64 i64) (result i64)))
  (type (;9;) (func (param i64 i64) (result i64)))
  (type (;10;) (func (param i64 i64) (result i64)))
  (type (;11;) (func (param i32 i32) (result i32)))
  (type (;12;) (func (param i32 i32) (result i32)))
  (export "shl_i32_by0" (func $shl_i32_by0))
  (export "shl_i32_by1" (func $shl_i32_by1))
  (export "shl_i32_by31" (func $shl_i32_by31))
  (export "shr_s_i32_by0" (func $shr_s_i32_by0))
  (export "shr_s_i32_by1" (func $shr_s_i32_by1))
  (export "shr_s_i32_by31" (func $shr_s_i32_by31))
  (export "shr_u_i32" (func $shr_u_i32))
  (export "shl_u32" (func $shl_u32))
  (export "shl_i64" (func $shl_i64))
  (export "shr_s_i64" (func $shr_s_i64))
  (export "shr_u_i64" (func $shr_u_i64))
  (export "shl_by_amount" (func $shl_by_amount))
  (export "shr_by_amount" (func $shr_by_amount))
  (func $shl_i32_by0 (;0;) (type 0) (param $a i32) (result i32)
    local.get $a
    i32.const 0
    i32.shl
    return
    unreachable
  )
  (func $shl_i32_by1 (;1;) (type 1) (param $a i32) (result i32)
    local.get $a
    i32.const 1
    i32.shl
    return
    unreachable
  )
  (func $shl_i32_by31 (;2;) (type 2) (param $a i32) (result i32)
    local.get $a
    i32.const 31
    i32.shl
    return
    unreachable
  )
  (func $shr_s_i32_by0 (;3;) (type 3) (param $a i32) (result i32)
    local.get $a
    i32.const 0
    i32.shr_s
    return
    unreachable
  )
  (func $shr_s_i32_by1 (;4;) (type 4) (param $a i32) (result i32)
    local.get $a
    i32.const 1
    i32.shr_s
    return
    unreachable
  )
  (func $shr_s_i32_by31 (;5;) (type 5) (param $a i32) (result i32)
    local.get $a
    i32.const 31
    i32.shr_s
    return
    unreachable
  )
  (func $shr_u_i32 (;6;) (type 6) (param $a i32) (param $n i32) (result i32)
    local.get $a
    local.get $n
    i32.shr_u
    return
    unreachable
  )
  (func $shl_u32 (;7;) (type 7) (param $a i32) (param $n i32) (result i32)
    local.get $a
    local.get $n
    i32.shl
    return
    unreachable
  )
  (func $shl_i64 (;8;) (type 8) (param $a i64) (param $n i64) (result i64)
    local.get $a
    local.get $n
    i64.shl
    return
    unreachable
  )
  (func $shr_s_i64 (;9;) (type 9) (param $a i64) (param $n i64) (result i64)
    local.get $a
    local.get $n
    i64.shr_s
    return
    unreachable
  )
  (func $shr_u_i64 (;10;) (type 10) (param $a i64) (param $n i64) (result i64)
    local.get $a
    local.get $n
    i64.shr_u
    return
    unreachable
  )
  (func $shl_by_amount (;11;) (type 11) (param $a i32) (param $n i32) (result i32)
    local.get $a
    local.get $n
    i32.shl
    return
    unreachable
  )
  (func $shr_by_amount (;12;) (type 12) (param $a i32) (param $n i32) (result i32)
    local.get $a
    local.get $n
    i32.shr_s
    return
    unreachable
  )
)
