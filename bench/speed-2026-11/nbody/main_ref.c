/* n-body, the same loops and operation order as main.bld. */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
typedef struct { double x, y, z, vx, vy, vz, m; } B;
static double energy(const B *b) {
    double e = 0.0;
    for (int i = 0; i < 5; i++) {
        B p = b[i];
        e = e + 0.5 * p.m * (p.vx * p.vx + p.vy * p.vy + p.vz * p.vz);
        for (int j = i + 1; j < 5; j++) {
            B q = b[j];
            double dx = p.x - q.x, dy = p.y - q.y, dz = p.z - q.z;
            e = e - p.m * q.m / sqrt(dx * dx + dy * dy + dz * dz);
        }
    }
    return e;
}
static void advance(B *b, double dt) {
    for (int i = 0; i < 5; i++)
        for (int j = i + 1; j < 5; j++) {
            B p = b[i], q = b[j];
            double dx = p.x - q.x, dy = p.y - q.y, dz = p.z - q.z;
            double d2 = dx * dx + dy * dy + dz * dz;
            double mag = dt / (d2 * sqrt(d2));
            b[i].vx = p.vx - dx * q.m * mag; b[i].vy = p.vy - dy * q.m * mag; b[i].vz = p.vz - dz * q.m * mag;
            b[j].vx = q.vx + dx * p.m * mag; b[j].vy = q.vy + dy * p.m * mag; b[j].vz = q.vz + dz * p.m * mag;
        }
    for (int i = 0; i < 5; i++) { b[i].x = b[i].x + dt * b[i].vx; b[i].y = b[i].y + dt * b[i].vy; b[i].z = b[i].z + dt * b[i].vz; }
}
int main(int argc, char **argv) {
    long long n = atoll(argv[1]);
    double pi = 3.141592653589793, sm = 4.0 * pi * pi, dp = 365.24;
    B b[5] = {
        {0, 0, 0, 0, 0, 0, sm},
        {4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01, 1.66007664274403694e-03 * dp, 7.69901118419740425e-03 * dp, -6.90460016972063023e-05 * dp, 9.54791938424326609e-04 * sm},
        {8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01, -2.76742510726862411e-03 * dp, 4.99852801234917238e-03 * dp, 2.30417297573763929e-05 * dp, 2.85885980666130812e-04 * sm},
        {1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01, 2.96460137564761618e-03 * dp, 2.37847173959480950e-03 * dp, -2.96589568540237556e-05 * dp, 4.36624404335156298e-05 * sm},
        {1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01, 2.68067772490389322e-03 * dp, 1.62824170038242295e-03 * dp, -9.51592254519715870e-05 * dp, 5.15138902046611451e-05 * sm}};
    double px = 0, py = 0, pz = 0;
    for (int i = 0; i < 5; i++) { px = px + b[i].vx * b[i].m; py = py + b[i].vy * b[i].m; pz = pz + b[i].vz * b[i].m; }
    b[0].vx = 0.0 - px / sm; b[0].vy = 0.0 - py / sm; b[0].vz = 0.0 - pz / sm;
    printf("%.9f\n", energy(b));
    for (long long k = 0; k < n; k++) advance(b, 0.01);
    printf("%.9f\n", energy(b));
    return 0;
}
