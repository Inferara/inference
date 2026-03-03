(module $output
  (type (;0;) (func (param i32)))
  (export "void_loop" (func $void_loop))
  (func $void_loop (;0;) (type 0) (param $n i32)
    (local $i i32)
    i32.const 0
    local.set $i
    block ;; label = @1
      loop ;; label = @2
        local.get $i
        local.get $n
        i32.lt_s
        i32.eqz
        br_if 1 (;@1;)
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br 0 (;@2;)
      end
    end
  )
)
