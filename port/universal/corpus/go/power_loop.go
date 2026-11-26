package main
import "fmt"
func pow_i64(base int64, exp int64) int64 {
    acc := 1
    i := 0
    for i < exp {
        acc = acc * base
        i = i + 1
    }
    return acc
}
func main() {
    fmt.Println(pow_i64(3, 9))
}
