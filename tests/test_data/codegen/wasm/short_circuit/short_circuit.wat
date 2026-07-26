(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32 i32) (result i32)))
  (type (;5;) (func (param i32 i32) (result i32)))
  (type (;6;) (func (param i32 i32) (result i32)))
  (type (;7;) (func (param i32 i32 i32) (result i32)))
  (type (;8;) (func (result i32)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "guard_div" (func $guard_div))
  (export "guard_div_or" (func $guard_div_or))
  (export "and_rhs_runs" (func $and_rhs_runs))
  (export "or_rhs_runs" (func $or_rhs_runs))
  (export "chain3_div" (func $chain3_div))
  (export "trap_kind" (func $trap_kind))
  (export "mixed_not" (func $mixed_not))
  (export "prec_mix" (func $prec_mix))
  (export "loop_guard" (func $loop_guard))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $guard_div (;0;) (type 0) (param $x i32) (result i32)
    local.get $x
    i32.const 0
    i32.ne
    if (result i32) ;; label = @1
      i32.const 100
      local.get $x
      i32.div_s
      i32.const 1
      i32.gt_s
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $guard_div_or (;1;) (type 1) (param $x i32) (result i32)
    local.get $x
    i32.const 0
    i32.eq
    if (result i32) ;; label = @1
      i32.const 1
    else
      i32.const 100
      local.get $x
      i32.div_s
      i32.const 1
      i32.gt_s
    end
    return
    unreachable
  )
  (func $and_rhs_runs (;2;) (type 2) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const 0
    i32.ne
    if (result i32) ;; label = @1
      i32.const 100
      local.get $b
      i32.div_s
      i32.const 0
      i32.gt_s
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $or_rhs_runs (;3;) (type 3) (param $a i32) (param $b i32) (result i32)
    local.get $a
    i32.const 0
    i32.eq
    if (result i32) ;; label = @1
      i32.const 1
    else
      i32.const 100
      local.get $b
      i32.div_s
      i32.const 0
      i32.gt_s
    end
    return
    unreachable
  )
  (func $chain3_div (;4;) (type 4) (param $a i32) (param $b i32) (param $c i32) (result i32)
    i32.const 100
    local.get $a
    i32.div_s
    i32.const 0
    i32.gt_s
    if (result i32) ;; label = @1
      i32.const 100
      local.get $b
      i32.div_s
      i32.const 0
      i32.gt_s
    else
      i32.const 0
    end
    if (result i32) ;; label = @1
      i32.const 100
      local.get $c
      i32.div_s
      i32.const 0
      i32.gt_s
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $trap_kind (;5;) (type 5) (param $a i32) (param $i i32) (result i32)
    (local $arr i32) (local $__frame_ptr i32) (local i32)
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
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $arr
    i32.const 100
    local.get $a
    i32.div_s
    i32.const 0
    i32.gt_s
    if (result i32) ;; label = @1
      local.get $arr
      local.get $i
      local.tee 4
      local.get 4
      i32.const 2
      i32.ge_u
      if ;; label = @2
        unreachable
      end
      i32.const 4
      i32.mul
      i32.add
      i32.load
      i32.const 0
      i32.gt_s
    else
      i32.const 0
    end
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $mixed_not (;6;) (type 6) (param $a i32) (param $x i32) (result i32)
    local.get $a
    i32.eqz
    if (result i32) ;; label = @1
      i32.const 100
      local.get $x
      i32.div_s
      i32.const 0
      i32.gt_s
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $prec_mix (;7;) (type 7) (param $a i32) (param $b i32) (param $c i32) (result i32)
    local.get $a
    if (result i32) ;; label = @1
      i32.const 1
    else
      local.get $b
      if (result i32) ;; label = @2
        local.get $c
      else
        i32.const 0
      end
    end
    return
    unreachable
  )
  (func $loop_guard (;8;) (type 8) (result i32)
    (local $arr i32) (local $sum i32) (local $i i32) (local $__frame_ptr i32) (local i32)
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
    local.set $arr
    i32.const 0
    local.set $sum
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 4
        i32.lt_s
        if (result i32) ;; label = @3
          local.get $arr
          local.get $i
          local.tee 4
          local.get 4
          i32.const 4
          i32.ge_u
          if ;; label = @4
            unreachable
          end
          i32.const 4
          i32.mul
          i32.add
          i32.load
          i32.const 0
          i32.gt_s
        else
          i32.const 0
        end
        i32.eqz
        br_if 1 (;@1;)
        local.get $sum
        local.get $arr
        local.get $i
        local.tee 4
        local.get 4
        i32.const 4
        i32.ge_u
        if ;; label = @3
          unreachable
        end
        i32.const 4
        i32.mul
        i32.add
        i32.load
        i32.add
        local.set $sum
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $sum
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
)
