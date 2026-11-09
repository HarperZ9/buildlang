// fannkuch-redux: checksum and maximum flip count over all permutations of n.
#include <stdio.h>
#include <stdlib.h>
int main(int argc, char **argv) {
    int n = argc > 1 ? atoi(argv[1]) : 7;
    int perm[16], perm1[16], count[16];
    int max_flips = 0, checksum = 0, odd = 0, r = n;
    for (int i = 0; i < n; ++i) perm1[i] = i;
    for (;;) {
        for (; r != 1; --r) count[r - 1] = r;
        for (int i = 0; i < n; ++i) perm[i] = perm1[i];
        int flips = 0, k;
        while ((k = perm[0]) != 0) {
            for (int lo = 0, hi = k; lo < hi; ++lo, --hi) {
                int t = perm[lo]; perm[lo] = perm[hi]; perm[hi] = t;
            }
            ++flips;
        }
        if (flips > max_flips) max_flips = flips;
        checksum += odd ? -flips : flips;
        odd = !odd;
        for (;;) {
            if (r == n) {
                printf("%d\nPfannkuchen(%d) = %d\n", checksum, n, max_flips);
                return 0;
            }
            int p0 = perm1[0];
            for (int j = 0; j < r; ++j) perm1[j] = perm1[j + 1];
            perm1[r] = p0;
            if (--count[r] > 0) break;
            ++r;
        }
    }
}
