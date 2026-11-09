// fannkuch-redux: checksum and maximum flip count over all permutations of n.
fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(7);
    let mut perm = [0usize; 16];
    let mut perm1 = [0usize; 16];
    let mut count = [0usize; 16];
    let (mut max_flips, mut checksum, mut odd, mut r) = (0i32, 0i32, false, n);
    for (i, p) in perm1.iter_mut().enumerate().take(n) {
        *p = i;
    }
    loop {
        while r != 1 {
            count[r - 1] = r;
            r -= 1;
        }
        perm[..n].copy_from_slice(&perm1[..n]);
        let mut flips = 0;
        loop {
            let k = perm[0];
            if k == 0 {
                break;
            }
            perm[..=k].reverse();
            flips += 1;
        }
        max_flips = max_flips.max(flips);
        checksum += if odd { -flips } else { flips };
        odd = !odd;
        loop {
            if r == n {
                println!("{}\nPfannkuchen({}) = {}", checksum, n, max_flips);
                return;
            }
            let p0 = perm1[0];
            perm1.copy_within(1..=r, 0);
            perm1[r] = p0;
            count[r] -= 1;
            if count[r] > 0 {
                break;
            }
            r += 1;
        }
    }
}
