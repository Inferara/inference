(module $output
  (type (;0;) (func))
  (type (;1;) (func))
  (type (;2;) (func (result i32)))
  (export "main" (func $main))
  (func $spelled_unit (;0;) (type 0)
    return
  )
  (func $spelled_parens (;1;) (type 1)
    return
  )
  (func $main (;2;) (type 2) (result i32)
    call $spelled_unit
    call $spelled_parens
    i32.const 9
    return
    unreachable
  )
)
