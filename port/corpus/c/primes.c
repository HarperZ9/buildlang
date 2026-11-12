#include <stdio.h>

/* Count primes below a bound and print the last few. */
int is_prime(int n) {
    if (n < 2) return 0;
    for (int d = 2; d * d <= n; d++) {
        if (n % d == 0) return 0;
    }
    return 1;
}

int main(void) {
    int count = 0;
    int last = 0;
    for (int i = 0; i < 1000; i++) {
        if (!is_prime(i)) continue;
        count++;
        last = i;
    }
    printf("primes below 1000: %d, largest %d\n", count, last);
    return 0;
}
