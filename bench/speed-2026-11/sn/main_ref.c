/* spectral-norm, single thread, the same loops as main.bld. */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
static double a(long long i, long long j) { return 1.0 / (double)((i + j) * (i + j + 1) / 2 + i + 1); }
static void mul_av(long long n, const double *v, double *out) {
    for (long long i = 0; i < n; i++) { double s = 0.0; for (long long j = 0; j < n; j++) s += a(i, j) * v[j]; out[i] = s; }
}
static void mul_atv(long long n, const double *v, double *out) {
    for (long long i = 0; i < n; i++) { double s = 0.0; for (long long j = 0; j < n; j++) s += a(j, i) * v[j]; out[i] = s; }
}
static void mul_atav(long long n, const double *v, double *out, double *tmp) { mul_av(n, v, tmp); mul_atv(n, tmp, out); }
int main(int argc, char **argv) {
    long long n = atoll(argv[1]);
    double *u = malloc(n * sizeof(double)), *v = malloc(n * sizeof(double)), *t = malloc(n * sizeof(double));
    for (long long i = 0; i < n; i++) { u[i] = 1.0; v[i] = 0.0; t[i] = 0.0; }
    for (int k = 0; k < 10; k++) { mul_atav(n, u, v, t); mul_atav(n, v, u, t); }
    double vbv = 0.0, vv = 0.0;
    for (long long i = 0; i < n; i++) { vbv += u[i] * v[i]; vv += v[i] * v[i]; }
    printf("%.9f\n", sqrt(vbv / vv));
    return 0;
}
