(module $output
  (type (;0;) (func (param i32) (result i32)))
  (type (;1;) (func (param i32) (result i32)))
  (type (;2;) (func (param i32 i32) (result i32)))
  (type (;3;) (func (param i32 i32) (result i32)))
  (type (;4;) (func (param i32 i32) (result i32)))
  (type (;5;) (func (param i32 i32) (result i32)))
  (type (;6;) (func (param i32) (result i32)))
  (type (;7;) (func (param i32 i32) (result i32)))
  (type (;8;) (func (param i32) (result i32)))
  (type (;9;) (func (param i32) (result i32)))
  (export "popcount" (func $popcount))
  (export "is_power_of_2" (func $is_power_of_2))
  (export "get_bit" (func $get_bit))
  (export "set_bit" (func $set_bit))
  (export "clear_bit" (func $clear_bit))
  (export "toggle_bit" (func $toggle_bit))
  (export "lowest_set_bit" (func $lowest_set_bit))
  (export "rotate_left_8" (func $rotate_left_8))
  (export "count_leading_zeros" (func $count_leading_zeros))
  (export "byte_swap_16" (func $byte_swap_16))
  (func $popcount (;0;) (type 0) (param $n i32) (result i32)
    (local $count i32) (local $x i32)
    i32.const 0
    local.set $count
    local.get $n
    local.set $x
    block ;; label = @1
      loop ;; label = @2
        local.get $x
        i32.const 0
        i32.ne
        i32.eqz
        br_if 1 (;@1;)
        local.get $x
        local.get $x
        i32.const 1
        i32.sub
        i32.and
        local.set $x
        local.get $count
        i32.const 1
        i32.add
        local.set $count
        br 0 (;@2;)
      end
    end
    local.get $count
    return
    unreachable
  )
  (func $is_power_of_2 (;1;) (type 1) (param $n i32) (result i32)
    local.get $n
    i32.const 0
    i32.le_s
    if ;; label = @1
      i32.const 0
      return
    end
    local.get $n
    local.get $n
    i32.const 1
    i32.sub
    i32.and
    i32.const 0
    i32.eq
    return
    unreachable
  )
  (func $get_bit (;2;) (type 2) (param $x i32) (param $pos i32) (result i32)
    local.get $x
    local.get $pos
    i32.shr_s
    i32.const 1
    i32.and
    return
    unreachable
  )
  (func $set_bit (;3;) (type 3) (param $x i32) (param $pos i32) (result i32)
    local.get $x
    i32.const 1
    local.get $pos
    i32.shl
    i32.or
    return
    unreachable
  )
  (func $clear_bit (;4;) (type 4) (param $x i32) (param $pos i32) (result i32)
    local.get $x
    i32.const 1
    local.get $pos
    i32.shl
    i32.const -1
    i32.xor
    i32.and
    return
    unreachable
  )
  (func $toggle_bit (;5;) (type 5) (param $x i32) (param $pos i32) (result i32)
    local.get $x
    i32.const 1
    local.get $pos
    i32.shl
    i32.xor
    return
    unreachable
  )
  (func $lowest_set_bit (;6;) (type 6) (param $n i32) (result i32)
    local.get $n
    i32.const 0
    local.get $n
    i32.sub
    i32.and
    return
    unreachable
  )
  (func $rotate_left_8 (;7;) (type 7) (param $x i32) (param $r i32) (result i32)
    (local $shift i32)
    local.get $r
    i32.const 7
    i32.and
    local.set $shift
    local.get $x
    local.get $shift
    i32.shl
    local.get $x
    i32.const 8
    local.get $shift
    i32.sub
    i32.shr_s
    i32.or
    i32.const 255
    i32.and
    return
    unreachable
  )
  (func $count_leading_zeros (;8;) (type 8) (param $n i32) (result i32)
    (local $count i32) (local $bit i32)
    local.get $n
    i32.const 0
    i32.eq
    if ;; label = @1
      i32.const 32
      return
    end
    i32.const 0
    local.set $count
    i32.const 31
    local.set $bit
    block ;; label = @1
      loop ;; label = @2
        local.get $n
        local.get $bit
        i32.shr_s
        i32.const 1
        i32.and
        i32.const 0
        i32.eq
        i32.eqz
        br_if 1 (;@1;)
        local.get $count
        i32.const 1
        i32.add
        local.set $count
        local.get $bit
        i32.const 1
        i32.sub
        local.set $bit
        br 0 (;@2;)
      end
    end
    local.get $count
    return
    unreachable
  )
  (func $byte_swap_16 (;9;) (type 9) (param $x i32) (result i32)
    (local $lo i32) (local $hi i32)
    local.get $x
    i32.const 255
    i32.and
    local.set $lo
    local.get $x
    i32.const 8
    i32.shr_s
    i32.const 255
    i32.and
    local.set $hi
    local.get $lo
    i32.const 8
    i32.shl
    local.get $hi
    i32.or
    return
    unreachable
  )
)
