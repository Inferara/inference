(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (export "is_up" (func $is_up))
  (export "dir_to_int" (func $dir_to_int))
  (export "pass_through" (func $pass_through))
  (func $is_up (;0;) (type 0) (param $d i32) (result i32)
    local.get $d
    i32.const 4
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    local.get $d
    i32.const 0
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
  (func $dir_to_int (;1;) (type 1) (param $d i32) (result i32)
    local.get $d
    i32.const 4
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    local.get $d
    i32.const 0
    i32.eq
    if ;; label = @1
      i32.const 10
      return
    end
    local.get $d
    i32.const 1
    i32.eq
    if ;; label = @1
      i32.const 20
      return
    end
    local.get $d
    i32.const 2
    i32.eq
    if ;; label = @1
      i32.const 30
      return
    end
    i32.const 40
    return
    unreachable
  )
  (func $pass_through (;2;) (type 2) (param $d i32) (result i32)
    local.get $d
    i32.const 4
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    local.get $d
    return
    unreachable
  )
)
