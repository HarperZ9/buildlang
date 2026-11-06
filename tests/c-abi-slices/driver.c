/* Host for tests/c-abi-slices: prints results that depend on the length the
 * host passes. With one argument k it then reads index k of a 3-element slice,
 * which must trap (the control) when k >= 3. With two arguments k w it then
 * writes index w of a 3-element buffer, which must trap when w >= 3. */
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
    float b[3] = {1.0f, 2.0f, 3.0f};
    sl_scale(b, 3, 2.0f);
    sl_put(b, 3, 2, 7.5f);
    printf("scaled %.1f %.1f %.1f\n", b[0], b[1], b[2]);
    fflush(stdout);
    if (argc > 2) {
        sl_put(b, 3, (uintptr_t)atoi(argv[2]), 9.0f);
        printf("wrote %.1f\n", b[2]);
        return 0;
    }
    if (argc > 1) {
        printf("read %.4f\n", sl_at(a, 3, (uintptr_t)atoi(argv[1])));
    }
    return 0;
}
