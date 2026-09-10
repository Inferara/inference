(module $output
  (type (;0;) (func (result i32)))
  (type (;1;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;2;) (func (param i32 i32 i32 i32) (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (result i32)))
  (export "nest_8_add" (func $nest_8_add))
  (export "nest_mixed_ops" (func $nest_mixed_ops))
  (export "nest_comparison" (func $nest_comparison))
  (export "nest_call_in_expr" (func $nest_call_in_expr))
  (export "nest_paren_deep" (func $nest_paren_deep))
  (func $nest_8_add (;0;) (type 0) (result i32)
    (local i32 i32 i32)
    i32.const 1
    i32.const 2
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    i32.const 3
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    i32.const 4
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    i32.const 5
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    i32.const 6
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    i32.const 7
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    i32.const 8
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    i32.const 9
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    return
    unreachable
  )
  (func $nest_mixed_ops (;1;) (type 1) (param $a i32) (param $b i32) (param $c i32) (param $d i32) (result i32)
    (local i32 i32 i32)
    local.get $a
    local.get $b
    local.set 5
    local.tee 4
    local.get 5
    i32.add
    local.tee 6
    local.get 4
    i32.lt_s
    local.get 5
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 6
    local.get $c
    local.get $d
    local.set 5
    local.tee 4
    local.get 5
    i32.sub
    local.tee 6
    local.get 4
    i32.lt_s
    local.get 5
    i32.const 0
    i32.gt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 6
    local.set 5
    local.tee 4
    local.get 5
    i32.mul
    local.set 6
    local.get 5
    i32.const 0
    i32.ne
    if ;; label = @1
      local.get 5
      i32.const -1
      i32.eq
      if ;; label = @2
        local.get 4
        i32.const -2147483648
        i32.eq
        if ;; label = @3
          unreachable
        end
      else
        local.get 6
        local.get 5
        i32.div_s
        local.get 4
        i32.ne
        if ;; label = @3
          unreachable
        end
      end
    end
    local.get 6
    local.get $a
    local.get $b
    local.set 5
    local.tee 4
    local.get 5
    i32.sub
    local.tee 6
    local.get 4
    i32.lt_s
    local.get 5
    i32.const 0
    i32.gt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 6
    local.get $c
    local.get $d
    local.set 5
    local.tee 4
    local.get 5
    i32.add
    local.tee 6
    local.get 4
    i32.lt_s
    local.get 5
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 6
    local.set 5
    local.tee 4
    local.get 5
    i32.mul
    local.set 6
    local.get 5
    i32.const 0
    i32.ne
    if ;; label = @1
      local.get 5
      i32.const -1
      i32.eq
      if ;; label = @2
        local.get 4
        i32.const -2147483648
        i32.eq
        if ;; label = @3
          unreachable
        end
      else
        local.get 6
        local.get 5
        i32.div_s
        local.get 4
        i32.ne
        if ;; label = @3
          unreachable
        end
      end
    end
    local.get 6
    local.set 5
    local.tee 4
    local.get 5
    i32.add
    local.tee 6
    local.get 4
    i32.lt_s
    local.get 5
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 6
    return
    unreachable
  )
  (func $nest_comparison (;2;) (type 2) (param $a i32) (param $b i32) (param $c i32) (param $d i32) (result i32)
    local.get $a
    local.get $b
    i32.gt_s
    if (result i32) ;; label = @1
      local.get $c
      local.get $d
      i32.lt_s
      if (result i32) ;; label = @2
        i32.const 1
      else
        local.get $a
        local.get $c
        i32.eq
      end
    else
      i32.const 0
    end
    return
    unreachable
  )
  (func $helper_square (;3;) (type 3) (param $x i32) (result i32)
    (local i32 i32 i32)
    local.get $x
    local.get $x
    local.set 2
    local.tee 1
    local.get 2
    i32.mul
    local.set 3
    local.get 2
    i32.const 0
    i32.ne
    if ;; label = @1
      local.get 2
      i32.const -1
      i32.eq
      if ;; label = @2
        local.get 1
        i32.const -2147483648
        i32.eq
        if ;; label = @3
          unreachable
        end
      else
        local.get 3
        local.get 2
        i32.div_s
        local.get 1
        i32.ne
        if ;; label = @3
          unreachable
        end
      end
    end
    local.get 3
    return
    unreachable
  )
  (func $nest_call_in_expr (;4;) (type 4) (param $x i32) (result i32)
    (local i32 i32 i32)
    local.get $x
    call $helper_square
    local.get $x
    i32.const 1
    local.set 2
    local.tee 1
    local.get 2
    i32.add
    local.tee 3
    local.get 1
    i32.lt_s
    local.get 2
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 3
    call $helper_square
    local.set 2
    local.tee 1
    local.get 2
    i32.add
    local.tee 3
    local.get 1
    i32.lt_s
    local.get 2
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 3
    i32.const 2
    local.set 2
    local.tee 1
    local.get 2
    i32.mul
    local.set 3
    local.get 2
    i32.const 0
    i32.ne
    if ;; label = @1
      local.get 2
      i32.const -1
      i32.eq
      if ;; label = @2
        local.get 1
        i32.const -2147483648
        i32.eq
        if ;; label = @3
          unreachable
        end
      else
        local.get 3
        local.get 2
        i32.div_s
        local.get 1
        i32.ne
        if ;; label = @3
          unreachable
        end
      end
    end
    local.get 3
    return
    unreachable
  )
  (func $nest_paren_deep (;5;) (type 5) (result i32)
    (local i32 i32 i32)
    i32.const 1
    i32.const 2
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    i32.const 3
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    i32.const 4
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    i32.const 5
    local.set 1
    local.tee 0
    local.get 1
    i32.add
    local.tee 2
    local.get 0
    i32.lt_s
    local.get 1
    i32.const 0
    i32.lt_s
    i32.ne
    if ;; label = @1
      unreachable
    end
    local.get 2
    return
    unreachable
  )
  (@custom "inference.checked" (after code) "\01\05\00\01\03\04\05")
)
