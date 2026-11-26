fn sum_sq(n: i64) i64 {
    var s: i64 = 0;
    var i: i64 = 1;
    while (i <= n) {
        s = s + i * i;
        i = i + 1;
    }
    return s;
}
pub fn main() void {
    print(sum_sq(10));
}
