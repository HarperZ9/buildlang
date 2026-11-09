// binary-trees, arena form (same plan as main.bld).
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
typedef struct { int64_t *p; size_t len, cap; } vec;
static void push(vec *v, int64_t x) {
    if (v->len == v->cap) {
        v->cap = v->cap ? v->cap * 2 : 8;
        v->p = realloc(v->p, v->cap * sizeof(int64_t));
        if (!v->p) abort();
    }
    v->p[v->len++] = x;
}
static int64_t build(int64_t d, vec *left, vec *right) {
    int64_t l = -1, r = -1;
    if (d > 0) { l = build(d - 1, left, right); r = build(d - 1, left, right); }
    push(left, l); push(right, r);
    return (int64_t)left->len - 1;
}
static int64_t check(int64_t i, const vec *left, const vec *right) {
    int64_t l = left->p[i];
    if (l < 0) return 1;
    return 1 + check(l, left, right) + check(right->p[i], left, right);
}
int main(int argc, char **argv) {
    int64_t n = argc > 1 ? atoll(argv[1]) : 10;
    vec ll = {0}, lr = {0}, left = {0}, right = {0};
    int64_t long_root = build(n, &ll, &lr);
    for (int64_t d = 4; d <= n; d += 2) {
        int64_t iters = (int64_t)1 << (n - d + 4), total = 0;
        for (int64_t k = 0; k < iters; ++k) {
            left.len = 0; right.len = 0;
            int64_t root = build(d, &left, &right);
            total += check(root, &left, &right);
        }
        printf("%lld trees of depth %lld check: %lld\n", (long long)iters, (long long)d, (long long)total);
    }
    printf("long lived tree of depth %lld check: %lld\n", (long long)n, (long long)check(long_root, &ll, &lr));
    return 0;
}
