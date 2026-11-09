// spectral-norm, single thread, the same loops as main.bld.
fn a(i: i64, j: i64) -> f64 {
    1.0 / (((i + j) * (i + j + 1) / 2 + i + 1) as f64)
}
fn mul_av(n: i64, v: &[f64], out: &mut [f64]) {
    for i in 0..n {
        let mut s = 0.0;
        for j in 0..n {
            s += a(i, j) * v[j as usize];
        }
        out[i as usize] = s;
    }
}
fn mul_atv(n: i64, v: &[f64], out: &mut [f64]) {
    for i in 0..n {
        let mut s = 0.0;
        for j in 0..n {
            s += a(j, i) * v[j as usize];
        }
        out[i as usize] = s;
    }
}
fn mul_atav(n: i64, v: &[f64], out: &mut [f64], tmp: &mut [f64]) {
    mul_av(n, v, tmp);
    mul_atv(n, tmp, out);
}
fn main() {
    let n: i64 = std::env::args().nth(1).unwrap().parse().unwrap();
    let mut u = vec![1.0; n as usize];
    let mut v = vec![0.0; n as usize];
    let mut t = vec![0.0; n as usize];
    for _ in 0..10 {
        mul_atav(n, &u, &mut v, &mut t);
        mul_atav(n, &v, &mut u, &mut t);
    }
    let (mut vbv, mut vv) = (0.0, 0.0);
    for i in 0..n as usize {
        vbv += u[i] * v[i];
        vv += v[i] * v[i];
    }
    println!("{:.9}", (vbv / vv).sqrt());
}
