(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i64)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i64)))
  (export "signed_i8" (func $signed_i8))
  (export "signed_i16" (func $signed_i16))
  (export "signed_i32" (func $signed_i32))
  (export "signed_i64" (func $signed_i64))
  (export "unsigned_u8" (func $unsigned_u8))
  (export "unsigned_u16" (func $unsigned_u16))
  (export "unsigned_u32" (func $unsigned_u32))
  (export "unsigned_u64" (func $unsigned_u64))
  (func $signed_i8 (;0;) (type 0) (result i32)
    (local $a i32)
    i32.const -128
    local.set $a
    local.get $a
    return
    unreachable
  )
  (func $signed_i16 (;1;) (type 1) (result i32)
    (local $a i32)
    i32.const -32768
    local.set $a
    local.get $a
    return
    unreachable
  )
  (func $signed_i32 (;2;) (type 2) (result i32)
    (local $a i32)
    i32.const -2147483648
    local.set $a
    local.get $a
    return
    unreachable
  )
  (func $signed_i64 (;3;) (type 3) (result i64)
    (local $a i64)
    i64.const -9223372036854775808
    local.set $a
    local.get $a
    return
    unreachable
  )
  (func $unsigned_u8 (;4;) (type 4) (result i32)
    (local $a i32)
    i32.const 255
    local.set $a
    local.get $a
    return
    unreachable
  )
  (func $unsigned_u16 (;5;) (type 5) (result i32)
    (local $b i32)
    i32.const 65535
    local.set $b
    local.get $b
    return
    unreachable
  )
  (func $unsigned_u32 (;6;) (type 6) (result i32)
    (local $c i32)
    i32.const -1
    local.set $c
    local.get $c
    return
    unreachable
  )
  (func $unsigned_u64 (;7;) (type 7) (result i64)
    (local $d i64)
    i64.const -1
    local.set $d
    local.get $d
    return
    unreachable
  )
)
