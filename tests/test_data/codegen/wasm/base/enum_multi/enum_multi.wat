(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (export "direction_west" (func $direction_west))
  (export "shape_triangle" (func $shape_triangle))
  (export "first_variants" (func $first_variants))
  (func $direction_west (;0;) (type 0) (result i32)
    (local $d i32)
    i32.const 3
    local.set $d
    local.get $d
    return
    unreachable
  )
  (func $shape_triangle (;1;) (type 1) (result i32)
    (local $s i32)
    i32.const 2
    local.set $s
    local.get $s
    return
    unreachable
  )
  (func $first_variants (;2;) (type 2) (result i32)
    (local $d i32) (local $s i32)
    i32.const 0
    local.set $d
    i32.const 0
    local.set $s
    local.get $d
    return
    unreachable
  )
)
