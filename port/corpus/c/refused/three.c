#include <stdio.h>

/* Three constructs outside the subset. The tool must list all three and write nothing. */
struct point { int x; int y; };

void bump(int *p) { *p = *p + 1; }

int main(void) {
    int v = 1;
    goto end;
    printf("%d\n", v);
    return 0;
}
