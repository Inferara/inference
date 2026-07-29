(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (result i32)))
  (type (;5;) (func (result i32)))
  (type (;6;) (func (param i32 i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "elem_copy" (func $elem_copy))
  (export "elem_copy_neighbours" (func $elem_copy_neighbours))
  (export "field_self_assign" (func $field_self_assign))
  (export "whole_self_assign" (func $whole_self_assign))
  (export "method_result_into_other_field" (func $method_result_into_other_field))
  (export "method_result_into_source_field" (func $method_result_into_source_field))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $elem_copy (;0;) (type 0) (param $i i32) (param $j i32) (result i32)
    (local $arr i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 6
    i32.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    i32.const 28
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    local.set $arr
    local.get $arr
    local.get $i
    local.tee 4
    local.get 4
    i32.const 4
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    local.get $arr
    local.get $j
    local.tee 4
    local.get 4
    i32.const 4
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    local.set 6
    local.set 5
    local.get 5
    local.get 6
    i64.load align=1
    i64.store align=1
    local.get $arr
    local.get $i
    local.tee 4
    local.get 4
    i32.const 4
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    i32.load
    i32.const 10
    i32.mul
    local.get $arr
    local.get $i
    local.tee 4
    local.get 4
    i32.const 4
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $elem_copy_neighbours (;1;) (type 1) (param $i i32) (param $j i32) (result i32)
    (local $brr i32) (local $__frame_ptr i32) (local i32 i32 i32)
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
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 6
    i32.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    i32.const 28
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    local.set $brr
    local.get $brr
    local.get $i
    local.tee 4
    local.get 4
    i32.const 4
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    local.get $brr
    local.get $j
    local.tee 4
    local.get 4
    i32.const 4
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 8
    i32.mul
    i32.add
    local.set 6
    local.set 5
    local.get 5
    local.get 6
    i64.load align=1
    i64.store align=1
    local.get $brr
    i32.load
    i32.const 1000
    i32.mul
    local.get $brr
    i32.const 8
    i32.add
    i32.load
    i32.const 100
    i32.mul
    i32.add
    local.get $brr
    i32.const 16
    i32.add
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $brr
    i32.const 24
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $field_self_assign (;2;) (type 2) (result i32)
    (local $h i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 9
    i32.store
    local.get $__frame_ptr
    local.set $h
    local.get $h
    local.get $h
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    local.get 2
    local.get 3
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $h
    i32.load
    i32.const 1000
    i32.mul
    local.get $h
    i32.const 4
    i32.add
    i32.load
    i32.const 100
    i32.mul
    i32.add
    local.get $h
    i32.const 12
    i32.add
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $h
    i32.const 16
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $whole_self_assign (;3;) (type 3) (result i32)
    (local $m i32) (local $n i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $m
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $m
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get 3
    local.get 4
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $n
    local.get $m
    local.get $n
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get 3
    local.get 4
    i64.load offset=8 align=1
    i64.store offset=8 align=1
    local.get $m
    i32.load
    i32.const 1000
    i32.mul
    local.get $m
    i32.const 4
    i32.add
    i32.load
    i32.const 100
    i32.mul
    i32.add
    local.get $m
    i32.const 8
    i32.add
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $m
    i32.const 12
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $method_result_into_other_field (;4;) (type 4) (result i32)
    (local $r i32) (local $t i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    local.set $r
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $r
    call $Pair.get_p
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $t
    local.get $r
    i32.const 8
    i32.add
    local.get $t
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get $r
    i32.load
    i32.const 1000
    i32.mul
    local.get $r
    i32.const 4
    i32.add
    i32.load
    i32.const 100
    i32.mul
    i32.add
    local.get $r
    i32.const 8
    i32.add
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $r
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $method_result_into_source_field (;5;) (type 5) (result i32)
    (local $s i32) (local $u i32) (local $__frame_ptr i32) (local i32 i32)
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
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 6
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    local.set $s
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.get $s
    call $Pair.get_p
    local.get $__frame_ptr
    i32.const 16
    i32.add
    local.set $u
    local.get $s
    local.get $u
    local.set 4
    local.set 3
    local.get 3
    local.get 4
    i64.load align=1
    i64.store align=1
    local.get $s
    i32.load
    i32.const 1000
    i32.mul
    local.get $s
    i32.const 4
    i32.add
    i32.load
    i32.const 100
    i32.mul
    i32.add
    local.get $s
    i32.const 8
    i32.add
    i32.load
    i32.const 10
    i32.mul
    i32.add
    local.get $s
    i32.const 8
    i32.add
    i32.const 4
    i32.add
    i32.load
    i32.add
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $Pair.get_p (;6;) (type 6) (param $sret i32) (param $self i32)
    (local i32 i32)
    local.get $sret
    local.get $self
    local.set 3
    local.set 2
    local.get 2
    local.get 3
    i64.load align=1
    i64.store align=1
    return
    unreachable
  )
)
