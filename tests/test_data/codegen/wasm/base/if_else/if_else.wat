(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (param i32)))
  (export "if_only" (func $if_only))
  (export "if_else_branch" (func $if_else_branch))
  (export "if_with_local" (func $if_with_local))
  (export "if_else_with_local" (func $if_else_with_local))
  (export "nested_if" (func $nested_if))
  (export "if_void" (func $if_void))
  (func $if_only (;0;) (type 0) (param $x i32) (result i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if ;; label = @1
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_else_branch (;1;) (type 1) (param $x i32) (result i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if ;; label = @1
      i32.const 1
      return
    else
      i32.const 0
      return
    end
    unreachable
  )
  (func $if_with_local (;2;) (type 2) (param $x i32) (result i32)
    (local $r i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if ;; label = @1
      local.get $x
      local.set $r
      local.get $r
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_else_with_local (;3;) (type 3) (param $x i32) (result i32)
    (local $a i32) (local $b i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if ;; label = @1
      local.get $x
      local.set $a
      local.get $a
      return
    else
      local.get $x
      local.set $b
      local.get $b
      return
    end
    unreachable
  )
  (func $nested_if (;4;) (type 4) (param $x i32) (param $y i32) (result i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if ;; label = @1
      local.get $y
      i32.const 0
      i32.gt_s
      if ;; label = @2
        i32.const 2
        return
      end
      i32.const 1
      return
    end
    i32.const 0
    return
    unreachable
  )
  (func $if_void (;5;) (type 5) (param $x i32)
    (local $ignored i32)
    local.get $x
    i32.const 0
    i32.gt_s
    if ;; label = @1
      i32.const 1
      local.set $ignored
    end
  )
)
