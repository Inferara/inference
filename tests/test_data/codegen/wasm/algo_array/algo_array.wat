(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32) (result i32)))
  (type (;3;) (func (result i32)))
  (type (;4;) (func (param i32) (result i32)))
  (type (;5;) (func (param i32) (result i32)))
  (type (;6;) (func (result i32)))
  (type (;7;) (func (result i32)))
  (type (;8;) (func (result i32)))
  (type (;9;) (func (result i32)))
  (type (;10;) (func (param i32) (result i32)))
  (type (;11;) (func (result i64)))
  (memory (;0;) 1 1)
  (global (;0;) (mut i32) i32.const 65536)
  (export "linear_search" (func $linear_search))
  (export "binary_search" (func $binary_search))
  (export "bubble_sort_element" (func $bubble_sort_element))
  (export "dot_product" (func $dot_product))
  (export "array_max" (func $array_max))
  (export "prefix_sum_element" (func $prefix_sum_element))
  (export "sum_u8_array" (func $sum_u8_array))
  (export "min_i8_array" (func $min_i8_array))
  (export "max_i16_array" (func $max_i16_array))
  (export "sum_u16_array" (func $sum_u16_array))
  (export "search_u32_array" (func $search_u32_array))
  (export "dot_product_i64" (func $dot_product_i64))
  (export "memory" (memory 0))
  (export "__stack_pointer" (global 0))
  (func $linear_search (;0;) (type 0) (param $target i32) (result i32)
    (local $arr i32) (local $result i32) (local $i i32) (local $__frame_ptr i32)
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
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 9
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 6
    i32.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    i32.const 28
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $arr
    i32.const 8
    local.set $result
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 8
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $arr
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        i32.load
        local.get $target
        i32.eq
        if ;; label = @3
          local.get $i
          local.set $result
          br 2 (;@1;)
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $result
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $binary_search (;1;) (type 1) (param $target i32) (result i32)
    (local $arr i32) (local $result i32) (local $low i32) (local $high i32) (local $mid i32) (local $val i32) (local $__frame_ptr i32)
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
    i32.const 2
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 5
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 12
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 16
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 23
    i32.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i32.const 38
    i32.store
    local.get $__frame_ptr
    i32.const 28
    i32.add
    i32.const 56
    i32.store
    local.get $__frame_ptr
    local.set $arr
    i32.const 8
    local.set $result
    i32.const 0
    local.set $low
    i32.const 7
    local.set $high
    block ;; label = @1
      loop ;; label = @2
        local.get $low
        local.get $high
        i32.le_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $low
        local.get $high
        i32.add
        i32.const 2
        i32.div_s
        local.set $mid
        local.get $arr
        local.get $mid
        i32.const 4
        i32.mul
        i32.add
        i32.load
        local.set $val
        local.get $val
        local.get $target
        i32.eq
        if ;; label = @3
          local.get $mid
          local.set $result
          br 2 (;@1;)
        end
        local.get $val
        local.get $target
        i32.lt_s
        if ;; label = @3
          local.get $mid
          i32.const 1
          i32.add
          local.set $low
        else
          local.get $mid
          i32.const 1
          i32.sub
          local.set $high
        end
        br 0 (;@2;)
      end
    end
    local.get $result
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $bubble_sort_element (;2;) (type 2) (param $idx i32) (result i32)
    (local $arr i32) (local $i i32) (local $j i32) (local $k i32) (local $tmp i32) (local $__frame_ptr i32)
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
    i32.const 8
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 9
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $arr
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 6
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        i32.const 0
        local.set $j
        block ;; label = @3
          loop ;; label = @4
            local.get $j
            i32.const 5
            i32.lt_s
            i32.eqz
            br_if 1 (;@3;)
            local.get $j
            i32.const 1
            i32.add
            local.set $k
            local.get $arr
            local.get $j
            i32.const 4
            i32.mul
            i32.add
            i32.load
            local.get $arr
            local.get $k
            i32.const 4
            i32.mul
            i32.add
            i32.load
            i32.gt_s
            if ;; label = @5
              local.get $arr
              local.get $j
              i32.const 4
              i32.mul
              i32.add
              i32.load
              local.set $tmp
              local.get $arr
              local.get $j
              i32.const 4
              i32.mul
              i32.add
              local.get $arr
              local.get $k
              i32.const 4
              i32.mul
              i32.add
              i32.load
              i32.store
              local.get $arr
              local.get $k
              i32.const 4
              i32.mul
              i32.add
              local.get $tmp
              i32.store
            end
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
    local.get $arr
    local.get $idx
    i32.const 4
    i32.mul
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $dot_product (;3;) (type 3) (result i32)
    (local $a i32) (local $b i32) (local $sum i32) (local $i i32) (local $__frame_ptr i32)
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
    local.set $a
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
    i32.const 16
    i32.add
    local.set $b
    i32.const 0
    local.set $sum
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 4
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $sum
        local.get $a
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        i32.load
        local.get $b
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        i32.load
        i32.mul
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
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $array_max (;4;) (type 4) (param $n i32) (result i32)
    (local $arr i32) (local $max_val i32) (local $i i32) (local $__frame_ptr i32)
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
    i32.const 3
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 7
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 1
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 9
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 4
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 6
    i32.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i32.const 8
    i32.store
    local.get $__frame_ptr
    i32.const 28
    i32.add
    i32.const 2
    i32.store
    local.get $__frame_ptr
    local.set $arr
    local.get $arr
    i32.load
    local.set $max_val
    i32.const 1
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $arr
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        i32.load
        local.get $max_val
        i32.gt_s
        if ;; label = @3
          local.get $arr
          local.get $i
          i32.const 4
          i32.mul
          i32.add
          i32.load
          local.set $max_val
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $max_val
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $prefix_sum_element (;5;) (type 5) (param $idx i32) (result i32)
    (local $arr i32) (local $i i32) (local $running i32) (local $__frame_ptr i32)
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
    local.set $arr
    i32.const 1
    local.set $i
    local.get $arr
    i32.load
    local.set $running
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 6
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $running
        local.get $arr
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        i32.load
        i32.add
        local.set $running
        local.get $arr
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        local.get $running
        i32.store
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $arr
    local.get $idx
    i32.const 4
    i32.mul
    i32.add
    i32.load
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $sum_u8_array (;6;) (type 6) (result i32)
    (local $arr i32) (local $sum i32) (local $i i32) (local $__frame_ptr i32)
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
    i32.store8
    local.get $__frame_ptr
    i32.const 1
    i32.add
    i32.const 2
    i32.store8
    local.get $__frame_ptr
    i32.const 2
    i32.add
    i32.const 3
    i32.store8
    local.get $__frame_ptr
    i32.const 3
    i32.add
    i32.const 4
    i32.store8
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 5
    i32.store8
    local.get $__frame_ptr
    i32.const 5
    i32.add
    i32.const 6
    i32.store8
    local.get $__frame_ptr
    i32.const 6
    i32.add
    i32.const 7
    i32.store8
    local.get $__frame_ptr
    i32.const 7
    i32.add
    i32.const 8
    i32.store8
    local.get $__frame_ptr
    local.set $arr
    i32.const 0
    local.set $sum
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 8
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $sum
        local.get $arr
        local.get $i
        i32.const 1
        i32.mul
        i32.add
        i32.load8_u
        i32.add
        i32.const 255
        i32.and
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
  (func $min_i8_array (;7;) (type 7) (result i32)
    (local $arr i32) (local $min_val i32) (local $i i32) (local $val i32) (local $__frame_ptr i32)
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
    i32.const 50
    i32.store8
    local.get $__frame_ptr
    i32.const 1
    i32.add
    i32.const 30
    i32.store8
    local.get $__frame_ptr
    i32.const 2
    i32.add
    i32.const 80
    i32.store8
    local.get $__frame_ptr
    i32.const 3
    i32.add
    i32.const 10
    i32.store8
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 60
    i32.store8
    local.get $__frame_ptr
    i32.const 5
    i32.add
    i32.const 40
    i32.store8
    local.get $__frame_ptr
    local.set $arr
    local.get $arr
    i32.load8_s
    local.set $min_val
    i32.const 1
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 6
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $arr
        local.get $i
        i32.const 1
        i32.mul
        i32.add
        i32.load8_s
        local.set $val
        local.get $val
        local.get $min_val
        i32.lt_s
        if ;; label = @3
          local.get $val
          local.set $min_val
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $min_val
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $max_i16_array (;8;) (type 8) (result i32)
    (local $arr i32) (local $max_val i32) (local $i i32) (local $val i32) (local $__frame_ptr i32)
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
    i32.const 300
    i32.store16
    local.get $__frame_ptr
    i32.const 2
    i32.add
    i32.const 700
    i32.store16
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 100
    i32.store16
    local.get $__frame_ptr
    i32.const 6
    i32.add
    i32.const 900
    i32.store16
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 400
    i32.store16
    local.get $__frame_ptr
    i32.const 10
    i32.add
    i32.const 600
    i32.store16
    local.get $__frame_ptr
    local.set $arr
    local.get $arr
    i32.load16_s
    local.set $max_val
    i32.const 1
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 6
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $arr
        local.get $i
        i32.const 2
        i32.mul
        i32.add
        i32.load16_s
        local.set $val
        local.get $val
        local.get $max_val
        i32.gt_s
        if ;; label = @3
          local.get $val
          local.set $max_val
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $max_val
    local.get $__frame_ptr
    i32.const 16
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $sum_u16_array (;9;) (type 9) (result i32)
    (local $arr i32) (local $sum i32) (local $i i32) (local $__frame_ptr i32)
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
    i32.const 1000
    i32.store16
    local.get $__frame_ptr
    i32.const 2
    i32.add
    i32.const 2000
    i32.store16
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 3000
    i32.store16
    local.get $__frame_ptr
    i32.const 6
    i32.add
    i32.const 4000
    i32.store16
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 5000
    i32.store16
    local.get $__frame_ptr
    i32.const 10
    i32.add
    i32.const 6000
    i32.store16
    local.get $__frame_ptr
    local.set $arr
    i32.const 0
    local.set $sum
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 6
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $sum
        local.get $arr
        local.get $i
        i32.const 2
        i32.mul
        i32.add
        i32.load16_u
        i32.add
        i32.const 65535
        i32.and
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
  (func $search_u32_array (;10;) (type 10) (param $target i32) (result i32)
    (local $arr i32) (local $result i32) (local $i i32) (local $__frame_ptr i32)
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
    i32.const 100
    i32.store
    local.get $__frame_ptr
    i32.const 4
    i32.add
    i32.const 200
    i32.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i32.const 300
    i32.store
    local.get $__frame_ptr
    i32.const 12
    i32.add
    i32.const 400
    i32.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i32.const 500
    i32.store
    local.get $__frame_ptr
    i32.const 20
    i32.add
    i32.const 600
    i32.store
    local.get $__frame_ptr
    local.set $arr
    i32.const 6
    local.set $result
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 6
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $arr
        local.get $i
        i32.const 4
        i32.mul
        i32.add
        i32.load
        local.get $target
        i32.eq
        if ;; label = @3
          local.get $i
          local.set $result
          br 2 (;@1;)
        end
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
    local.get $result
    local.get $__frame_ptr
    i32.const 32
    i32.add
    global.set 0
    return
    unreachable
  )
  (func $dot_product_i64 (;11;) (type 11) (result i64)
    (local $a i32) (local $b i32) (local $sum i64) (local $i i32) (local $__frame_ptr i32)
    global.get 0
    i32.const 64
    i32.sub
    local.tee $__frame_ptr
    global.set 0
    local.get $__frame_ptr
    i32.const 0
    i32.const 64
    memory.fill
    local.get $__frame_ptr
    i32.const 0
    i32.add
    i64.const 100000
    i64.store
    local.get $__frame_ptr
    i32.const 8
    i32.add
    i64.const 200000
    i64.store
    local.get $__frame_ptr
    i32.const 16
    i32.add
    i64.const 300000
    i64.store
    local.get $__frame_ptr
    i32.const 24
    i32.add
    i64.const 400000
    i64.store
    local.get $__frame_ptr
    local.set $a
    local.get $__frame_ptr
    i32.const 32
    i32.add
    i64.const 500000
    i64.store
    local.get $__frame_ptr
    i32.const 40
    i32.add
    i64.const 600000
    i64.store
    local.get $__frame_ptr
    i32.const 48
    i32.add
    i64.const 700000
    i64.store
    local.get $__frame_ptr
    i32.const 56
    i32.add
    i64.const 800000
    i64.store
    local.get $__frame_ptr
    i32.const 32
    i32.add
    local.set $b
    i64.const 0
    local.set $sum
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        i32.const 4
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $sum
        local.get $a
        local.get $i
        i32.const 8
        i32.mul
        i32.add
        i64.load
        local.get $b
        local.get $i
        i32.const 8
        i32.mul
        i32.add
        i64.load
        i64.mul
        i64.add
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
    i32.const 64
    i32.add
    global.set 0
    return
    unreachable
  )
)
