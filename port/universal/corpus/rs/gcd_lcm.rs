fn gcd(a: i64, b: i64) -> i64 {
    let mut x: i64 = a;
    let mut y: i64 = b;
    while y != 0 {
        let r: i64 = x % y;
        x = y;
        y = r;
    }
    return x;
}
fn main() {
    let g: i64 = gcd(252, 105);
    println!("{}", g);
    println!("{}", (252 / g) * 105);
}
