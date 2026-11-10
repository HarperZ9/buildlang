// fasta, random-select kernel (same plan as main.bld).
fn select(n: i64, cum: &[f64], counts: &mut [i64]) -> u64 {
    let mut seed: i64 = 42;
    let mut h: u64 = 1469598103934665603;
    for _ in 0..n {
        seed = (seed * 3877 + 29573) % 139968;
        let r = (seed as f64) / 139968.0;
        let mut c = 0usize;
        while c < 3 && r >= cum[c] {
            c += 1;
        }
        counts[c] += 1;
        h = (h ^ (c as u64)).wrapping_mul(1099511628211);
    }
    h
}
fn main() {
    let n: i64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let cum = vec![0.3029549426680, 0.5009432431601, 0.6984905497992, 1.0];
    let mut counts = vec![0i64; 4];
    let h = select(n, &cum, &mut counts);
    println!("{} {} {} {}", counts[0], counts[1], counts[2], counts[3]);
    println!("{}", h);
}
