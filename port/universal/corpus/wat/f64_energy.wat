(module
  (import "env" "print_f64" (func $print_f64 (param f64)))
  (func $ke (param $m f64) (param $v f64) (result f64)
    (return (f64.mul (f64.const 0.5) (f64.mul (local.get $m) (f64.mul (local.get $v) (local.get $v)))))
  )
  (func $main
    (call $print_f64 (call $ke (f64.const 4.5) (f64.const 4.0)))
  )
  (export "main" (func $main))
)
