(module $output
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i32 i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (param i32 i32) (result i32)))
  (type (;6;) (func (param i32 i32) (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "at_129" (func $at_129))
  (export "at_130" (func $at_130))
  (export "at_131" (func $at_131))
  (export "at_135" (func $at_135))
  (export "at_u32_33" (func $at_u32_33))
  (export "at_u16_67" (func $at_u16_67))
  (export "clobber_135" (func $clobber_135))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $at_129 (;0;) (type 0) (param $data i32) (param $i i32) (result i32)
    (local i32)
    local.get $data
    local.get $i
    local.tee 2
    local.get 2
    i32.const 129
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 1
    i32.mul
    i32.add
    i32.load8_u
    return
    unreachable
  )
  (func $at_130 (;1;) (type 1) (param $data i32) (param $i i32) (result i32)
    (local i32)
    local.get $data
    local.get $i
    local.tee 2
    local.get 2
    i32.const 130
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 1
    i32.mul
    i32.add
    i32.load8_u
    return
    unreachable
  )
  (func $at_131 (;2;) (type 2) (param $data i32) (param $i i32) (result i32)
    (local i32)
    local.get $data
    local.get $i
    local.tee 2
    local.get 2
    i32.const 131
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 1
    i32.mul
    i32.add
    i32.load8_u
    return
    unreachable
  )
  (func $at_135 (;3;) (type 3) (param $data i32) (param $i i32) (result i32)
    (local i32)
    local.get $data
    local.get $i
    local.tee 2
    local.get 2
    i32.const 135
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 1
    i32.mul
    i32.add
    i32.load8_u
    return
    unreachable
  )
  (func $at_u32_33 (;4;) (type 4) (param $data i32) (param $i i32) (result i32)
    (local i32)
    local.get $data
    local.get $i
    local.tee 2
    local.get 2
    i32.const 33
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 4
    i32.mul
    i32.add
    i32.load
    return
    unreachable
  )
  (func $at_u16_67 (;5;) (type 5) (param $data i32) (param $i i32) (result i32)
    (local i32)
    local.get $data
    local.get $i
    local.tee 2
    local.get 2
    i32.const 67
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 2
    i32.mul
    i32.add
    i32.load16_u
    return
    unreachable
  )
  (func $clobber_135 (;6;) (type 6) (param $data i32) (param $i i32) (result i32)
    (local $__frame_ptr i32) (local i32 i32)
    global.get 0
    i32.const 144
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    i32.const 0
    local.set 4
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 4
      i32.add
      i64.const 0
      i64.store
      local.get $__frame_ptr
      local.get 4
      i32.add
      i64.const 0
      i64.store offset=8
      local.get 4
      i32.const 16
      i32.add
      local.tee 4
      i32.const 144
      i32.ne
      br_if 0 (;@1;)
    end
    i32.const 0
    local.set 4
    loop ;; label = @1
      local.get $__frame_ptr
      local.get 4
      i32.add
      local.get $data
      local.get 4
      i32.add
      i64.load align=1
      i64.store align=1
      local.get 4
      i32.const 8
      i32.add
      local.tee 4
      i32.const 128
      i32.ne
      br_if 0 (;@1;)
    end
    local.get $__frame_ptr
    local.get $data
    i32.load offset=128 align=1
    i32.store offset=128 align=1
    local.get $__frame_ptr
    local.get $data
    i32.load16_u offset=132 align=1
    i32.store16 offset=132 align=1
    local.get $__frame_ptr
    local.get $data
    i32.load8_u offset=134
    i32.store8 offset=134
    local.get $__frame_ptr
    local.set $data
    local.get $data
    local.get $i
    local.tee 3
    local.get 3
    i32.const 135
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 1
    i32.mul
    i32.add
    i32.const 255
    i32.store8
    local.get $data
    local.get $i
    local.tee 3
    local.get 3
    i32.const 135
    i32.ge_u
    if ;; label = @1
      unreachable
    end
    i32.const 1
    i32.mul
    i32.add
    i32.load8_u
    local.get $__frame_ptr
    i32.const 144
    i32.add
    global.set 0
    return
    unreachable
  )
)
