(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (param i32 i32) (result i32)))
  (type (;7;) (func (param i32 i32) (result i32)))
  (type (;8;) (func (param i32 i32) (result i32)))
  (type (;9;) (func (param i32 i32) (result i32)))
  (type (;10;) (func (param i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "call_receiver_and_part" (func $call_receiver_and_part))
  (export "call_receiver_and_receiver" (func $call_receiver_and_receiver))
  (export "call_mut_receiver_and_part" (func $call_mut_receiver_and_part))
  (export "call_mut_receiver_and_receiver" (func $call_mut_receiver_and_receiver))
  (export "call_native_sub_object" (func $call_native_sub_object))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $part_sum (;0;) (type 0) (param $q i32) (result i32)
    local.get $q
    i32.load
    i32.const 10
    i32.mul
    local.get $q
    i32.const 4
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
  (func $call_receiver_and_part (;1;) (type 1) (result i32)
    (local $h i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
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
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $h
    local.get $h
    local.get $h
    i32.const 4
    i32.add
    call $Holder.read_with_part
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_receiver_and_receiver (;2;) (type 2) (result i32)
    (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
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
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $g
    local.get $g
    local.get $g
    call $Holder.read_with_holder
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_mut_receiver_and_part (;3;) (type 3) (result i32)
    (local $k i32) (local $inner i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
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
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $k
    local.get $k
    local.get $k
    i32.const 4
    i32.add
    call $Holder.bump_with_part
    local.set $inner
    local.get $inner
    i32.const 100
    i32.mul
    local.get $k
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $k
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_mut_receiver_and_receiver (;4;) (type 4) (result i32)
    (local $r i32) (local $inner i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
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
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $r
    local.get $r
    local.get $r
    call $Holder.bump_with_holder
    local.set $inner
    local.get $inner
    i32.const 100
    i32.mul
    local.get $r
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $r
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $call_native_sub_object (;5;) (type 5) (result i32)
    (local $m i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
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
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $m
    local.get $m
    call $Holder.native_sub_object
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $Holder.read_with_part (;6;) (type 6) (param $self i32) (param $q i32) (result i32)
    local.get $self
    i32.load
    i32.const 1000
    i32.mul
    local.get $self
    i32.const 4
    i32.add
    i32.load
    i32.const 100
    i32.mul
    i32.add
    local.get $q
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $q
    i32.const 4
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
  (func $Holder.read_with_holder (;7;) (type 7) (param $self i32) (param $o i32) (result i32)
    local.get $self
    i32.load
    i32.const 1000
    i32.mul
    local.get $self
    i32.const 4
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.const 100
    i32.mul
    i32.add
    local.get $o
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $o
    i32.const 4
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    return
    unreachable
  )
  (func $Holder.bump_with_part (;8;) (type 8) (param $self i32) (param $q i32) (result i32)
    (local $__frame_ptr i32)
    global.get 0
    i32.const 16
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
    local.get $self
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.get $self
    i32.load offset=8 align=1
    i32.store offset=8 align=1
    local.get $__frame_ptr
    local.set $self
    local.get $self
    local.get $self
    i32.load
    i32.const 1
    i32.add
    i32.store
    local.get $self
    i32.load
    i32.const 100
    i32.mul
    local.get $q
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $q
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $Holder.bump_with_holder (;9;) (type 9) (param $self i32) (param $o i32) (result i32)
    (local $__frame_ptr i32)
    global.get 0
    i32.const 16
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
    local.get $self
    i64.load align=1
    i64.store align=1
    local.get $__frame_ptr
    local.get $self
    i32.load offset=8 align=1
    i32.store offset=8 align=1
    local.get $__frame_ptr
    local.set $self
    local.get $self
    local.get $self
    i32.load
    i32.const 1
    i32.add
    i32.store
    local.get $self
    i32.load
    i32.const 100
    i32.mul
    local.get $o
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $o
    i32.const 4
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $Holder.native_sub_object (;10;) (type 10) (param $self i32) (result i32)
    local.get $self
    i32.const 4
    i32.add
    call $part_sum
    return
    unreachable
  )
)
