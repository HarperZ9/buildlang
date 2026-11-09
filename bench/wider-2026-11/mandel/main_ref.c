// mandelbrot: n by n grid, 50 iterations; prints the in-set count and a hash
// of the in-set bits in row order.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc, char **argv) {
    int n = argc > 1 ? atoi(argv[1]) : 200;
    double nf = n;
    long long inside = 0;
    uint64_t h = 1469598103934665603ULL;
    for (int y = 0; y < n; ++y) {
        double ci = 2.0 * y / nf - 1.0;
        for (int x = 0; x < n; ++x) {
            double cr = 2.0 * x / nf - 1.5, zr = 0, zi = 0, tr = 0, ti = 0;
            for (int i = 0; i < 50 && tr + ti <= 4.0; ++i) {
                zi = 2.0 * zr * zi + ci;
                zr = tr - ti + cr;
                tr = zr * zr;
                ti = zi * zi;
            }
            uint64_t bit = tr + ti <= 4.0;
            inside += (long long)bit;
            h = (h ^ bit) * 1099511628211ULL;
        }
    }
    printf("%lld\n%llu\n", inside, (unsigned long long)h);
    return 0;
}
