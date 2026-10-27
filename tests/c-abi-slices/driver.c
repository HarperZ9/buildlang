/* Host for tests/c-abi-slices: prints results that depend on the length the
 * host passes. With one argument k it then reads index k of a 3-element slice,
 * which must trap (the control) when k >= 3. */
#include <stdio.h>
#include <stdlib.h>
#include "main.h"

int main(int argc, char** argv) {
    float a[5] = {1.5f, 2.0f, 3.25f, -4.0f, 10.0f};
    float g[27];
    for (int i = 0; i < 27; i++) g[i] = (float)i;
    printf("total5 %.4f\n", sl_total(a, 5));
    printf("total3 %.4f\n", sl_total(a, 3));
    printf("at4 %.4f\n", sl_at(a, 5, 4));
    printf("empty %d %d\n", (int)sl_is_empty(a, 0), (int)sl_is_empty(a, 5));
    printf("grid %.1f %.1f\n", sl_grid_at(g, 3, 2, 1, 0), sl_grid_at(g, 3, 2, 2, 2));
    fflush(stdout);
    if (argc > 1) {
        printf("read %.4f\n", sl_at(a, 3, (uintptr_t)atoi(argv[1])));
    }
    return 0;
}
