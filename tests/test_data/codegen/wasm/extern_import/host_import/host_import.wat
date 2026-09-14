(module $output
  (type (;0;) (func (result i64)))
  (type (;1;) (func (result i64)))
  (import "env" "clock_ms" (func (;0;) (type 0)))
  (export "now" (func $now))
  (func $now (;1;) (type 1) (result i64)
    call 0
    return
    unreachable
  )
)
