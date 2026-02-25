(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i64) (result i64)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (param i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (export "identity_i32" (func $identity_i32))
  (export "identity_i64" (func $identity_i64))
  (export "identity_bool" (func $identity_bool))
  (export "first_of_two" (func $first_of_two))
  (export "second_of_two" (func $second_of_two))
  (func $identity_i32 (;0;) (type 0) (param $x i32) (result i32)
    local.get $x
    return
    unreachable
  )
  (func $identity_i64 (;1;) (type 1) (param $x i64) (result i64)
    local.get $x
    return
    unreachable
  )
  (func $identity_bool (;2;) (type 2) (param $x i32) (result i32)
    local.get $x
    return
    unreachable
  )
  (func $first_of_two (;3;) (type 3) (param $a i32) (param $b i32) (result i32)
    local.get $a
    return
    unreachable
  )
  (func $second_of_two (;4;) (type 4) (param $a i32) (param $b i32) (result i32)
    local.get $b
    return
    unreachable
  )
)
