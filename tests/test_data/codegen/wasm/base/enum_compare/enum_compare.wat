(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (export "are_equal" (func $are_equal))
  (export "are_not_equal" (func $are_not_equal))
  (export "is_active" (func $is_active))
  (func $are_equal (;0;) (type 0) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const 2
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    local.get $b
    i32.const 2
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    local.get $a
    local.get $b
    i32.eq
    return
    unreachable
  )
  (func $are_not_equal (;1;) (type 1) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const 2
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    local.get $b
    i32.const 2
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    local.get $a
    local.get $b
    i32.ne
    return
    unreachable
  )
  (func $is_active (;2;) (type 2) (param $s i32) (result i32)
    local.get $s
    i32.const 2
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    local.get $s
    i32.const 0
    i32.eq
    return
    unreachable
  )
)
