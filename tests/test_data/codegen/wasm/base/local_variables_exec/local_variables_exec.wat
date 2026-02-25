(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i64)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i64)))
  (type (;8;) (func (result i32)))
  (type (;9;) (func (result i32)))
  (type (;10;) (func (result i32)))
  (export "let_i32_literal" (func $let_i32_literal))
  (export "let_i64_literal" (func $let_i64_literal))
  (export "let_i8_literal" (func $let_i8_literal))
  (export "let_i16_literal" (func $let_i16_literal))
  (export "let_u8_literal" (func $let_u8_literal))
  (export "let_u16_literal" (func $let_u16_literal))
  (export "let_u32_literal" (func $let_u32_literal))
  (export "let_u64_literal" (func $let_u64_literal))
  (export "let_bool_literal_true" (func $let_bool_literal_true))
  (export "let_bool_literal_false" (func $let_bool_literal_false))
  (export "let_from_identifier" (func $let_from_identifier))
  (func $let_i32_literal (;0;) (type 0) (result i32)
    (local $x i32)
    i32.const 42
    local.set $x
    local.get $x
    return
    unreachable
  )
  (func $let_i64_literal (;1;) (type 1) (result i64)
    (local $y i64)
    i64.const -9223372036854775808
    local.set $y
    local.get $y
    return
    unreachable
  )
  (func $let_i8_literal (;2;) (type 2) (result i32)
    (local $a i32)
    i32.const -128
    local.set $a
    local.get $a
    return
    unreachable
  )
  (func $let_i16_literal (;3;) (type 3) (result i32)
    (local $b i32)
    i32.const -32768
    local.set $b
    local.get $b
    return
    unreachable
  )
  (func $let_u8_literal (;4;) (type 4) (result i32)
    (local $c i32)
    i32.const 255
    local.set $c
    local.get $c
    return
    unreachable
  )
  (func $let_u16_literal (;5;) (type 5) (result i32)
    (local $d i32)
    i32.const 65535
    local.set $d
    local.get $d
    return
    unreachable
  )
  (func $let_u32_literal (;6;) (type 6) (result i32)
    (local $e i32)
    i32.const -1
    local.set $e
    local.get $e
    return
    unreachable
  )
  (func $let_u64_literal (;7;) (type 7) (result i64)
    (local $f i64)
    i64.const -1
    local.set $f
    local.get $f
    return
    unreachable
  )
  (func $let_bool_literal_true (;8;) (type 8) (result i32)
    (local $flag i32)
    i32.const 1
    local.set $flag
    local.get $flag
    return
    unreachable
  )
  (func $let_bool_literal_false (;9;) (type 9) (result i32)
    (local $flag i32)
    i32.const 0
    local.set $flag
    local.get $flag
    return
    unreachable
  )
  (func $let_from_identifier (;10;) (type 10) (result i32)
    (local $x i32) (local $y i32)
    i32.const 10
    local.set $x
    local.get $x
    local.set $y
    local.get $y
    return
    unreachable
  )
)
