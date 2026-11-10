// fasta, random-select kernel (same plan as main.bld).
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
static uint64_t select_n(int64_t n, const double *cum, int64_t *counts) {
    int64_t seed = 42;
    uint64_t h = 1469598103934665603ULL;
    for (int64_t i = 0; i < n; ++i) {
        seed = (seed * 3877 + 29573) % 139968;
        double r = (double)seed / 139968.0;
        int c = 0;
        while (c < 3 && r >= cum[c]) ++c;
        counts[c] += 1;
        h = (h ^ (uint64_t)c) * 1099511628211ULL;
    }
    return h;
}
int main(int argc, char **argv) {
    int64_t n = argc > 1 ? atoll(argv[1]) : 1000;
    double *cum = malloc(4 * sizeof(double));
    int64_t *counts = calloc(4, sizeof(int64_t));
    cum[0] = 0.3029549426680; cum[1] = 0.5009432431601; cum[2] = 0.6984905497992; cum[3] = 1.0;
    uint64_t h = select_n(n, cum, counts);
    printf("%lld %lld %lld %lld\n", (long long)counts[0], (long long)counts[1], (long long)counts[2], (long long)counts[3]);
    printf("%llu\n", (unsigned long long)h);
    return 0;
}
