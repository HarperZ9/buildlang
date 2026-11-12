#include <stdio.h>

int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }

void banner(int width) {
    int i = 0;
    while (i < width) { printf("="); i += 1; }
    printf("\n");
}

int main(void) {
    int a[8] = {5, 3, 8, 1, 9, 2, 7, 4};
    int n = 8;
    // bubble sort
    for (int i = 0; i < n - 1; i++) {
        for (int j = 0; j < n - 1 - i; j++) {
            if (a[j] > a[j + 1]) {
                int tmp = a[j];
                a[j] = a[j + 1];
                a[j + 1] = tmp;
            }
        }
    }
    banner(16);
    for (int i = 0; i < n; i++) printf("%d ", a[i]);
    printf("\n");
    double hist[4];
    for (int i = 0; i < n; i++) hist[a[i] % 4] += a[i] / 2.0;
    printf("%.1f %.1f %.1f %.1f\n", hist[0], hist[1], hist[2], hist[3]);
    int total = 0;
    int k = 0;
    while (1) {
        k++;
        if (k % 2 == 0) continue;
        if (k > 15) break;
        total += fib(k);
    }
    printf("100%% done: fib sum %d, {braces} kept\n", total);
    banner(16);
    return 0;
}
