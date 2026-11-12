#include <stdio.h>
#define ROWS 3
#define COLS 4

const int CELLS = ROWS * COLS;
const int MASK = 0xF0 | 0x0F;
const long WIDE = 1000L * 1000 * 3;
const double RATE = 1.5 * 2 + 0.25;
const float HALF = 1.0f / 2;
const int SHIFTED = 1 << 10;
const int NEGATIVE = -(ROWS + 1);

int main(void) {
    int grid[12];
    for (int i = 0; i < CELLS; i++) {
        grid[i] = (i * 37 + NEGATIVE) & MASK;
    }
    int sum = 0;
    for (int r = 0; r < ROWS; r++) {
        for (int c = 0; c < COLS; c++) {
            sum += grid[r * COLS + c];
        }
    }
    printf("%d %d %ld\n", CELLS, sum, WIDE / 7);
    printf("%.3f %.2f %d\n", RATE * CELLS, HALF + 0.25, SHIFTED + NEGATIVE);
    return 0;
}
