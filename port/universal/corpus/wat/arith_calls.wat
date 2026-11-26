(module
  (import "env" "print_i64" (func $print_i64 (param i64)))
  (func $square (param $x i64) (result i64)
    (return (i64.mul (local.get $x) (local.get $x)))
  )
  (func $add_mul (param $a i64) (param $b i64) (result i64)
    (return (i64.add (i64.mul (local.get $a) (local.get $b)) (i64.const 5)))
  )
  (func $main
    (call $print_i64 (call $square (i64.const 5)))
    (call $print_i64 (call $add_mul (i64.const 6) (i64.const 7)))
  )
  (export "main" (func $main))
)
