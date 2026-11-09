// n-body, the same loops and operation order as main.bld.
#[derive(Clone, Copy)]
struct B { x: f64, y: f64, z: f64, vx: f64, vy: f64, vz: f64, m: f64 }
fn energy(b: &[B]) -> f64 {
    let mut e = 0.0;
    for i in 0..5 {
        let p = b[i];
        e = e + 0.5 * p.m * (p.vx * p.vx + p.vy * p.vy + p.vz * p.vz);
        for j in i + 1..5 {
            let q = b[j];
            let (dx, dy, dz) = (p.x - q.x, p.y - q.y, p.z - q.z);
            e = e - p.m * q.m / (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }
    e
}
fn advance(b: &mut [B], dt: f64) {
    for i in 0..5 {
        for j in i + 1..5 {
            let (p, q) = (b[i], b[j]);
            let (dx, dy, dz) = (p.x - q.x, p.y - q.y, p.z - q.z);
            let d2 = dx * dx + dy * dy + dz * dz;
            let mag = dt / (d2 * d2.sqrt());
            b[i] = B { vx: p.vx - dx * q.m * mag, vy: p.vy - dy * q.m * mag, vz: p.vz - dz * q.m * mag, ..p };
            b[j] = B { vx: q.vx + dx * p.m * mag, vy: q.vy + dy * p.m * mag, vz: q.vz + dz * p.m * mag, ..q };
        }
    }
    for p in b.iter_mut() {
        p.x = p.x + dt * p.vx;
        p.y = p.y + dt * p.vy;
        p.z = p.z + dt * p.vz;
    }
}
fn main() {
    let n: i64 = std::env::args().nth(1).unwrap().parse().unwrap();
    let pi = 3.141592653589793;
    let sm = 4.0 * pi * pi;
    let dp = 365.24;
    let mut b = vec![
        B { x: 0.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0, m: sm },
        B { x: 4.84143144246472090e+00, y: -1.16032004402742839e+00, z: -1.03622044471123109e-01, vx: 1.66007664274403694e-03 * dp, vy: 7.69901118419740425e-03 * dp, vz: -6.90460016972063023e-05 * dp, m: 9.54791938424326609e-04 * sm },
        B { x: 8.34336671824457987e+00, y: 4.12479856412430479e+00, z: -4.03523417114321381e-01, vx: -2.76742510726862411e-03 * dp, vy: 4.99852801234917238e-03 * dp, vz: 2.30417297573763929e-05 * dp, m: 2.85885980666130812e-04 * sm },
        B { x: 1.28943695621391310e+01, y: -1.51111514016986312e+01, z: -2.23307578892655734e-01, vx: 2.96460137564761618e-03 * dp, vy: 2.37847173959480950e-03 * dp, vz: -2.96589568540237556e-05 * dp, m: 4.36624404335156298e-05 * sm },
        B { x: 1.53796971148509165e+01, y: -2.59193146099879641e+01, z: 1.79258772950371181e-01, vx: 2.68067772490389322e-03 * dp, vy: 1.62824170038242295e-03 * dp, vz: -9.51592254519715870e-05 * dp, m: 5.15138902046611451e-05 * sm },
    ];
    let (mut px, mut py, mut pz) = (0.0, 0.0, 0.0);
    for p in &b {
        px = px + p.vx * p.m;
        py = py + p.vy * p.m;
        pz = pz + p.vz * p.m;
    }
    b[0].vx = 0.0 - px / sm;
    b[0].vy = 0.0 - py / sm;
    b[0].vz = 0.0 - pz / sm;
    println!("{:.9}", energy(&b));
    for _ in 0..n {
        advance(&mut b, 0.01);
    }
    println!("{:.9}", energy(&b));
}
