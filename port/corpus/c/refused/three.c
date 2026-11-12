#include <stdio.h>

/* Three constructs outside the first subset. The tool must list all three and write nothing. */
struct point { int x; int y; };

void bump(int *p) { *p = *p + 1; }

int main(void) {
    int v = 1;
    switch (v) { case 1: v = 2; }
    printf("%d\n", v);
    return 0;
}
