#include <stdio.h>
#define SQUARE(x) ((x) * (x))
#define NAME "text"

int main(void) {
    int v = 1;
    for (int i = 0; i < 3; i++) {
        switch (i) {
        case 0:
            v += 1;
        case 1:
            v += 2;
            break;
        default:
            continue;
        }
    }
    printf("%d\n", v);
    return 0;
}
