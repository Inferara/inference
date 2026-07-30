(module $output
  (type (;0;) (func (result i64)))
  (type (;1;) (func (result i64)))
  (type (;2;) (func (result i64)))
  (type (;3;) (func (result i64)))
  (type (;4;) (func (result i64)))
  (type (;5;) (func (result i64)))
  (type (;6;) (func (param i64) (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "assigned_elements" (func $assigned_elements))
  (export "reassigned_first_element" (func $reassigned_first_element))
  (export "const_array_element" (func $const_array_element))
  (export "const_array_unsigned_max" (func $const_array_unsigned_max))
  (export "struct_field_array_element" (func $struct_field_array_element))
  (export "element_expressions" (func $element_expressions))
  (export "peer_typed_element" (func $peer_typed_element))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $assigned_elements (;0;) (type 0) (result i64)
    (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    local.set $a
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 1099511627776
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 2199023255552
    i64.store
    local.get $__frame_ptr
    drop
    local.get $a
    i64.load
    local.get $a
    i32.const 8
    i32.add
    i64.load
    i64.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $reassigned_first_element (;1;) (type 1) (result i64)
    (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 1
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 2
    i64.store
    local.get $__frame_ptr
    local.set $a
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 4398046511104
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 3
    i64.store
    local.get $__frame_ptr
    drop
    local.get $a
    i64.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $const_array_element (;2;) (type 2) (result i64)
    (local $WIDE i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 32
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 32
    memory.fill
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 1099511627776
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 2199023255552
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 4398046511104
    i64.store
    local.get $__frame_ptr
    local.set $WIDE
    local.get $WIDE
    i32.const 16
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $const_array_unsigned_max (;3;) (type 3) (result i64)
    (local $MAXES i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const -1
    i64.store
    local.get $__frame_ptr
    local.set $MAXES
    local.get $MAXES
    i32.const 8
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $struct_field_array_element (;4;) (type 4) (result i64)
    (local $h i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i64.const 1099511627776
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 2199023255552
    i64.store
    local.get $__frame_ptr
    local.set $h
    local.get $h
    i32.const 8
    i32.add
    i64.load
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $element_expressions (;5;) (type 5) (result i64)
    (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 1
    i64.const 40
    i64.shl
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 1
    i64.const 20
    i64.shl
    i64.const 1
    i64.const 20
    i64.shl
    i64.mul
    i64.store
    local.get $__frame_ptr
    local.set $a
    local.get $a
    i64.load
    local.get $a
    i32.const 8
    i32.add
    i64.load
    i64.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $peer_typed_element (;6;) (type 6) (param $v i64) (result i64)
    (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    local.get $__frame_ptr
    i32.const 0
    i32.add
    local.get $v
    i64.const 1
    i64.add
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 2
    i64.store
    local.get $__frame_ptr
    local.set $a
    local.get $a
    i64.load
    local.get $a
    i32.const 8
    i32.add
    i64.load
    i64.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
)
