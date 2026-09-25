(module $output
  (type (;0;) (func (param i32 i32 i32)))
  (type (;1;) (func (param i64)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32)))
  (type (;4;) (func (param i32 i32 i32 i32 i32) (result i32)))
  (type (;5;) (func (result i64)))
  (type (;6;) (func (param i32) (result i64)))
  (import "fprime_core" "panic" (func (;0;) (type 0)))
  (import "fprime_core" "rsleep" (func (;1;) (type 1)))
  (import "fprime_core" "command" (func (;2;) (type 2)))
  (import "fprime_core" "message" (func (;3;) (type 3)))
  (import "fprime_core" "telemetry" (func (;4;) (type 4)))
  (import "env" "clock_ms" (func (;5;) (type 5)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "report" (func $report))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $report (;6;) (type 6) (param $channel i32) (result i64)
    (local $text i32) (local $id i32) (local $time i32) (local $value i32) (local $status i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i64.const 0
    i64.store
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=8
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=16
    local.get $__frame_ptr
    i64.const 0
    i64.store offset=24
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i32.const 114
    i32.store8
    local.get $__frame_ptr
    i32.const 1
    i32.add
    i32.const 101
    i32.store8
    local.get $__frame_ptr
    i32.const 2
    i32.add
    i32.const 112
    i32.store8
    local.get $__frame_ptr
    i32.const 3
    i32.add
    i32.const 111
    i32.store8
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 114
    i32.store8
    local.get $__frame_ptr
    i32.const 5
    i32.add
    i32.const 116
    i32.store8
    local.get $__frame_ptr
    local.set $text
    local.get $text
    i32.const 6
    call 3
    i32.const 1
    local.get $channel
    call 2
    local.set $id
    local.get $__frame_ptr
    i32.const 6
    i32.add
    i32.const 1
    i32.store8
    local.get $__frame_ptr
    i32.const 7
    i32.add
    i32.const 2
    i32.store8
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 3
    i32.store8
    local.get $__frame_ptr
    i32.const 9
    i32.add
    i32.const 4
    i32.store8
    local.get $__frame_ptr
    i32.const 10
    i32.add
    i32.const 5
    i32.store8
    local.get $__frame_ptr
    i32.const 11
    i32.add
    i32.const 6
    i32.store8
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 7
    i32.store8
    local.get $__frame_ptr
    i32.const 13
    i32.add
    i32.const 8
    i32.store8
    local.get $__frame_ptr
    i32.const 14
    i32.add
    i32.const 9
    i32.store8
    local.get $__frame_ptr
    i32.const 15
    i32.add
    i32.const 10
    i32.store8
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 11
    i32.store8
    local.get $__frame_ptr
    i32.const 6
    i32.add
    local.set $time
    local.get $__frame_ptr
    i32.const 17
    i32.add
    i32.const 21
    i32.store8
    local.get $__frame_ptr
    i32.const 17
    i32.add
    local.set $value
    local.get $id
    local.get $time
    i32.const 11
    local.get $value
    i32.const 4
    call 4
    local.set $status
    local.get $status
    i32.const 0
    i32.ne
    if ;; label = @1
      i64.const 0
      local.get $__frame_ptr
      i32.const 32
      i32.add
      global.set 0
      return
    end
    call 5
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
)
