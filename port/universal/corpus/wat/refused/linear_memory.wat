(module
  ;; Linear memory ops must be refused cleanly
  (memory 1)
  (func $read0 (result i32)
    (return (i32.load (i32.const 0)))
  )
)
