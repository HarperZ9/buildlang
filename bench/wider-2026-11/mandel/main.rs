// mandelbrot: n by n grid, 50 iterations; prints the in-set count and a hash
// of the in-set bits in row order.
fn main() {
    let n: u32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(200);
    let nf = n as f64;
    let mut inside = 0i64;
    let mut h: u64 = 1469598103934665603;
    for y in 0..n {
        let ci = 2.0 * (y as f64) / nf - 1.0;
        for x in 0..n {
            let cr = 2.0 * (x as f64) / nf - 1.5;
            let (mut zr, mut zi, mut tr, mut ti) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
            let mut i = 0;
            while i < 50 && tr + ti <= 4.0 {
                zi = 2.0 * zr * zi + ci;
                zr = tr - ti + cr;
                tr = zr * zr;
                ti = zi * zi;
                i += 1;
            }
            let bit = (tr + ti <= 4.0) as u64;
            inside += bit as i64;
            h = (h ^ bit).wrapping_mul(1099511628211);
        }
    }
    println!("{}\n{}", inside, h);
}
