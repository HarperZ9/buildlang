package main
func main() {
    x := 1
    ch := make(chan int)
    go worker(ch)
}
