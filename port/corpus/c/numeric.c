#include <stdio.h>
#include <math.h>

const int STEPS = 1000;

// Integrate sqrt(1 - x^2) on [0, 1] by the midpoint rule: a quarter of pi.
double quarter_circle(int steps) {
    double h = 1.0 / steps;
    double sum = 0;
    for (int i = 0; i < steps; i++) {
        double x = (i + 0.5) * h;
        sum += sqrt(1.0 - x * x);
    }
    return sum * h;
}

double newton_sqrt(double v) {
    double g = v / 2;
    int n = 0;
    do {
        g = (g + v / g) / 2.0;
        n++;
    } while (fabs(g * g - v) > 1e-12 && n < 60);
    return g;
}

int main(void) {
    double q = quarter_circle(STEPS);
    printf("pi is about %.6f\n", 4 * q);
    printf("sqrt(2) = %.9f, floor(7.9) = %f\n", newton_sqrt(2), floor(7.9));
    int third = 10 / 3;
    float f = 10 / 3.0f;
    long big = 2000000L * 3;
    printf("%d %.3f %ld %c%c\n", third, f, big, 'o', 107);
    unsigned int mask = (255u << 4) & 0xF0F;
    printf("mask %u, mixed %.2f, ternary %d\n", mask, third * 0.5 + (third > 2), third > 2 ? 7 : 9);
    return 0;
}
