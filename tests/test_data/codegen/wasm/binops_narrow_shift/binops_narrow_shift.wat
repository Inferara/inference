(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (param i32 i32) (result i32)))
  (type (;6;) (func (param i32 i32) (result i32)))
  (type (;7;) (func (param i32 i32) (result i32)))
  (type (;8;) (func (param i32) (result i32)))
  (type (;9;) (func (param i32) (result i32)))
  (export "shl_u8" (func $shl_u8))
  (export "shr_u8" (func $shr_u8))
  (export "shl_i8" (func $shl_i8))
  (export "shr_i8" (func $shr_i8))
  (export "shl_u16" (func $shl_u16))
  (export "shr_u16" (func $shr_u16))
  (export "shl_i16" (func $shl_i16))
  (export "shr_i16" (func $shr_i16))
  (export "shl_u8_const" (func $shl_u8_const))
  (export "shr_i16_const" (func $shr_i16_const))
  (func $shl_u8 (;0;) (type 0) (param $a i32) (param $k i32) (result i32)
    local.get $a
    i32.const 255
    i32.and
    local.set $a
    local.get $k
    i32.const 255
    i32.and
    local.set $k
    local.get $a
    local.get $k
    i32.const 7
    i32.and
    i32.shl
    i32.const 255
    i32.and
    return
    unreachable
  )
  (func $shr_u8 (;1;) (type 1) (param $a i32) (param $k i32) (result i32)
    local.get $a
    i32.const 255
    i32.and
    local.set $a
    local.get $k
    i32.const 255
    i32.and
    local.set $k
    local.get $a
    local.get $k
    i32.const 7
    i32.and
    i32.shr_u
    return
    unreachable
  )
  (func $shl_i8 (;2;) (type 2) (param $a i32) (param $k i32) (result i32)
    local.get $a
    i32.const 24
    i32.shl
    i32.const 24
    i32.shr_s
    local.set $a
    local.get $k
    i32.const 24
    i32.shl
    i32.const 24
    i32.shr_s
    local.set $k
    local.get $a
    local.get $k
    i32.const 7
    i32.and
    i32.shl
    i32.const 24
    i32.shl
    i32.const 24
    i32.shr_s
    return
    unreachable
  )
  (func $shr_i8 (;3;) (type 3) (param $a i32) (param $k i32) (result i32)
    local.get $a
    i32.const 24
    i32.shl
    i32.const 24
    i32.shr_s
    local.set $a
    local.get $k
    i32.const 24
    i32.shl
    i32.const 24
    i32.shr_s
    local.set $k
    local.get $a
    local.get $k
    i32.const 7
    i32.and
    i32.shr_s
    return
    unreachable
  )
  (func $shl_u16 (;4;) (type 4) (param $a i32) (param $k i32) (result i32)
    local.get $a
    i32.const 65535
    i32.and
    local.set $a
    local.get $k
    i32.const 65535
    i32.and
    local.set $k
    local.get $a
    local.get $k
    i32.const 15
    i32.and
    i32.shl
    i32.const 65535
    i32.and
    return
    unreachable
  )
  (func $shr_u16 (;5;) (type 5) (param $a i32) (param $k i32) (result i32)
    local.get $a
    i32.const 65535
    i32.and
    local.set $a
    local.get $k
    i32.const 65535
    i32.and
    local.set $k
    local.get $a
    local.get $k
    i32.const 15
    i32.and
    i32.shr_u
    return
    unreachable
  )
  (func $shl_i16 (;6;) (type 6) (param $a i32) (param $k i32) (result i32)
    local.get $a
    i32.const 16
    i32.shl
    i32.const 16
    i32.shr_s
    local.set $a
    local.get $k
    i32.const 16
    i32.shl
    i32.const 16
    i32.shr_s
    local.set $k
    local.get $a
    local.get $k
    i32.const 15
    i32.and
    i32.shl
    i32.const 16
    i32.shl
    i32.const 16
    i32.shr_s
    return
    unreachable
  )
  (func $shr_i16 (;7;) (type 7) (param $a i32) (param $k i32) (result i32)
    local.get $a
    i32.const 16
    i32.shl
    i32.const 16
    i32.shr_s
    local.set $a
    local.get $k
    i32.const 16
    i32.shl
    i32.const 16
    i32.shr_s
    local.set $k
    local.get $a
    local.get $k
    i32.const 15
    i32.and
    i32.shr_s
    return
    unreachable
  )
  (func $shl_u8_const (;8;) (type 8) (param $a i32) (result i32)
    (local $K i32)
    local.get $a
    i32.const 255
    i32.and
    local.set $a
    i32.const 3
    local.set $K
    local.get $a
    local.get $K
    i32.const 7
    i32.and
    i32.shl
    i32.const 255
    i32.and
    return
    unreachable
  )
  (func $shr_i16_const (;9;) (type 9) (param $a i32) (result i32)
    (local $K i32)
    local.get $a
    i32.const 16
    i32.shl
    i32.const 16
    i32.shr_s
    local.set $a
    i32.const 15
    local.set $K
    local.get $a
    local.get $K
    i32.const 15
    i32.and
    i32.shr_s
    return
    unreachable
  )
)
