(module $output
  (type (;0;) (func (param i32)))
  (export "void_loop" (func $void_loop))
  (func $void_loop (;0;) (type 0) (param $n i32)
    (local $i i32) (local i32 i32 i32)
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
        local.set 3
        local.tee 2
        local.get 3
        i32.add
        local.tee 4
        local.get 2
        i32.lt_s
        local.get 3
        i32.const 0
        i32.lt_s
        i32.ne
        if ;; label = @3
          unreachable
        end
        local.get 4
        local.set $i
        br 0 (;@2;)
      end
    end
  )
  (@custom "inference.checked" (after code) "\01\01\00")
)
