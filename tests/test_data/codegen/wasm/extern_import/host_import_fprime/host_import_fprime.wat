(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (result i64)))
  (type (;3;) (func (param i32) (result i64)))
  (import "fprime_core" "telemetry" (func (;0;) (type 0)))
  (import "fprime_core" "command" (func (;1;) (type 1)))
  (import "env" "clock_ms" (func (;2;) (type 2)))
  (export "report" (func $report))
  (func $report (;3;) (type 3) (param $channel i32) (result i64)
    (local $ack i32) (local $sent i32)
    local.get $channel
    call 1
    local.set $ack
    local.get $channel
    local.get $ack
    call 0
    local.set $sent
    local.get $sent
    i32.const 0
    i32.eq
    if ;; label = @1
      i64.const 0
      return
    end
    call 2
    return
    unreachable
  )
)
