#include <stdio.h>
#define LIMIT 12
#define SCALE (2.5)
#define OFFSET -3
#define BIG 2000000000L
#define SWITCH_H

static int kind(int n) {
    switch (n % 5) {
    case 0:
        return 10;
    case 1:
    case 2:
        return 20 + n;
    default:
        return -1;
    }
}

int main(void) {
    int total = 0;
    long wide = BIG;
    for (int i = 0; i < LIMIT; i++) {
        int k = kind(i);
        switch (k) {
        case 10:
            total += 1;
            break;
        case -1: {
            int j = 0;
            while (j < 3) {
                j++;
                if (j == 2) continue;
                total += j;
            }
            break;
        }
        default:
            if (k > 25) {
                total += 100;
                break;
            }
            total += k;
            break;
        }
    }
    char c = 'b';
    switch (c) {
    case 'a': printf("a\n"); break;
    case 'b': printf("b %d\n", total + OFFSET); break;
    }
    printf("%d %.2f %ld\n", total, SCALE * LIMIT, wide / 7);
    return 0;
}
