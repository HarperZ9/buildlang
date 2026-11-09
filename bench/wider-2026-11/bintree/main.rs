// binary-trees, arena form (same plan as main.bld).
fn build(d: i64, left: &mut Vec<i64>, right: &mut Vec<i64>) -> i64 {
    let (mut l, mut r) = (-1i64, -1i64);
    if d > 0 {
        l = build(d - 1, left, right);
        r = build(d - 1, left, right);
    }
    left.push(l);
    right.push(r);
    left.len() as i64 - 1
}
fn check(i: i64, left: &[i64], right: &[i64]) -> i64 {
    let l = left[i as usize];
    if l < 0 {
        return 1;
    }
    1 + check(l, left, right) + check(right[i as usize], left, right)
}
fn main() {
    let n: i64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let (mut ll, mut lr) = (Vec::new(), Vec::new());
    let long_root = build(n, &mut ll, &mut lr);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let mut d = 4;
    while d <= n {
        let iters = 1i64 << (n - d + 4);
        let mut total = 0i64;
        for _ in 0..iters {
            left.clear();
            right.clear();
            let root = build(d, &mut left, &mut right);
            total += check(root, &left, &right);
        }
        println!("{} trees of depth {} check: {}", iters, d, total);
        d += 2;
    }
    println!("long lived tree of depth {} check: {}", n, check(long_root, &ll, &lr));
}
