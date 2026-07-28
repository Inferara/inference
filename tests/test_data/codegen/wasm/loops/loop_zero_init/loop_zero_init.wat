(module $output
  (type (;0;) (func (param i32) (result i64)))
  (type (;1;) (func (param i32) (result i64)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (param i32) (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (param i32) (result i64)))
  (type (;6;) (func (param i32) (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "arr" (func $arr))
  (export "strct" (func $strct))
  (export "boolarr" (func $boolarr))
  (export "partial" (func $partial))
  (export "twod" (func $twod))
  (export "nested" (func $nested))
  (export "if_in_loop" (func $if_in_loop))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $arr (;0;) (type 0) (param $n i32) (result i64)
    (local $ONE i64) (local $acc i64) (local $k i32) (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    i64.const 1
    local.set $ONE
    i64.const 0
    local.set $acc
    i32.const 0
    local.set $k
    block ;; label = @1
      loop ;; label = @2
        local.get $k
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $__frame_ptr
        i32.const 0
        i32.add
        i64.const 0
        i64.store
        local.get $__frame_ptr
        local.set $a
        local.get $a
        local.get $a
        i64.load
        local.get $ONE
        i64.add
        i64.store
        local.get $acc
        local.get $a
        i64.load
        i64.add
        local.set $acc
        local.get $k
        i32.const 1
        i32.add
        local.set $k
        br 0 (;@2;)
      end
    end
    local.get $acc
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $strct (;1;) (type 1) (param $n i32) (result i64)
    (local $ONE i64) (local $acc i64) (local $k i32) (local $s i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    i64.const 1
    local.set $ONE
    i64.const 0
    local.set $acc
    i32.const 0
    local.set $k
    block ;; label = @1
      loop ;; label = @2
        local.get $k
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $__frame_ptr
        i64.const 0
        i64.store
        local.get $__frame_ptr
        i32.const 8
        i32.add
        i64.const 0
        i64.store
        local.get $__frame_ptr
        local.set $s
        local.get $s
        local.get $s
        i64.load
        local.get $ONE
        i64.add
        i64.store
        local.get $acc
        local.get $s
        i64.load
        i64.add
        local.set $acc
        local.get $k
        i32.const 1
        i32.add
        local.set $k
        br 0 (;@2;)
      end
    end
    local.get $acc
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $boolarr (;2;) (type 2) (param $n i32) (result i32)
    (local $count i32) (local $k i32) (local $b i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    i32.const 0
    local.set $count
    i32.const 0
    local.set $k
    block ;; label = @1
      loop ;; label = @2
        local.get $k
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $__frame_ptr
        i32.const 0
        i32.add
        i32.const 0
        i32.store8
        local.get $__frame_ptr
        i32.const 1
        i32.add
        i32.const 0
        i32.store8
        local.get $__frame_ptr
        local.set $b
        local.get $b
        i32.load8_u
        if ;; label = @3
          local.get $count
          i32.const 1
          i32.add
          local.set $count
        end
        local.get $b
        i32.const 1
        i32.store8
        local.get $k
        i32.const 1
        i32.add
        local.set $k
        br 0 (;@2;)
      end
    end
    local.get $count
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $partial (;3;) (type 3) (param $n i32) (result i32)
    (local $acc i32) (local $k i32) (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    i32.const 0
    local.set $acc
    i32.const 0
    local.set $k
    block ;; label = @1
      loop ;; label = @2
        local.get $k
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $__frame_ptr
        i32.const 0
        i32.add
        i32.const 5
        i32.store
        local.get $__frame_ptr
        i32.const 4
        i32.add
        i32.const 0
        i32.store
        local.get $__frame_ptr
        i32.const 8
        i32.add
        i32.const 0
        i32.store
        local.get $__frame_ptr
        local.set $a
        local.get $a
        i32.const 4
        i32.add
        local.get $a
        i32.const 4
        i32.add
        i32.load
        i32.const 1
        i32.add
        i32.store
        local.get $acc
        local.get $a
        i32.load
        i32.add
        local.get $a
        i32.const 4
        i32.add
        i32.load
        i32.add
        local.set $acc
        local.get $k
        i32.const 1
        i32.add
        local.set $k
        br 0 (;@2;)
      end
    end
    local.get $acc
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $twod (;4;) (type 4) (param $n i32) (result i32)
    (local $acc i32) (local $k i32) (local $g i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    i32.const 0
    local.set $acc
    i32.const 0
    local.set $k
    block ;; label = @1
      loop ;; label = @2
        local.get $k
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $__frame_ptr
        i32.const 0
        i32.add
        i32.const 0
        i32.store
        local.get $__frame_ptr
        i32.const 4
        i32.add
        i32.const 0
        i32.store
        local.get $__frame_ptr
        i32.const 8
        i32.add
        i32.const 0
        i32.store
        local.get $__frame_ptr
        i32.const 12
        i32.add
        i32.const 0
        i32.store
        local.get $__frame_ptr
        local.set $g
        local.get $g
        local.get $g
        i32.load
        i32.const 1
        i32.add
        i32.store
        local.get $g
        i32.const 8
        i32.add
        i32.const 4
        i32.add
        local.get $g
        i32.const 8
        i32.add
        i32.const 4
        i32.add
        i32.load
        i32.const 1
        i32.add
        i32.store
        local.get $acc
        local.get $g
        i32.load
        i32.add
        local.get $g
        i32.const 8
        i32.add
        i32.const 4
        i32.add
        i32.load
        i32.add
        local.set $acc
        local.get $k
        i32.const 1
        i32.add
        local.set $k
        br 0 (;@2;)
      end
    end
    local.get $acc
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $nested (;5;) (type 5) (param $n i32) (result i64)
    (local $ONE i64) (local $acc i64) (local $i i32) (local $j i32) (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    i64.const 1
    local.set $ONE
    i64.const 0
    local.set $acc
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        i32.const 0
        local.set $j
        block ;; label = @3
          loop ;; label = @4
            local.get $j
            i32.const 2
            i32.lt_s
            i32.eqz
            br_if 1 (;@3;)
            local.get $__frame_ptr
            i32.const 0
            i32.add
            i64.const 0
            i64.store
            local.get $__frame_ptr
            local.set $a
            local.get $a
            local.get $a
            i64.load
            local.get $ONE
            i64.add
            i64.store
            local.get $acc
            local.get $a
            i64.load
            i64.add
            local.set $acc
            local.get $j
            i32.const 1
            i32.add
            local.set $j
            br 0 (;@4;)
          end
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $acc
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $if_in_loop (;6;) (type 6) (param $n i32) (result i64)
    (local $ONE i64) (local $acc i64) (local $k i32) (local $a i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 16
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 16
    memory.fill
    i64.const 1
    local.set $ONE
    i64.const 0
    local.set $acc
    i32.const 0
    local.set $k
    block ;; label = @1
      loop ;; label = @2
        local.get $k
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $k
        i32.const 0
        i32.ge_s
        if ;; label = @3
          local.get $__frame_ptr
          i32.const 0
          i32.add
          i64.const 0
          i64.store
          local.get $__frame_ptr
          local.set $a
          local.get $a
          local.get $a
          i64.load
          local.get $ONE
          i64.add
          i64.store
          local.get $acc
          local.get $a
          i64.load
          i64.add
          local.set $acc
        end
        local.get $k
        i32.const 1
        i32.add
        local.set $k
        br 0 (;@2;)
      end
    end
    local.get $acc
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
)
