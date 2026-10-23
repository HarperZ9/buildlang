#include <stdio.h>
#include "main.h"

int main(void) {
    Fighter f = { 7, { 1.5, -2.0 }, 10.0, true };
    Fighter g = lw_damage(f, 4.25);
    lw_step(&g, 0.5);
    printf("%d\n", lw_add(40, 2));
    printf("%d %.2f %.2f %.2f %d\n", g.id, g.pos.x, g.pos.z, g.hp, g.alive ? 1 : 0);
    g = lw_damage(g, 6.0);
    printf("%.2f %d\n", g.hp, g.alive ? 1 : 0);
    printf("sizeof %d\n", (int)sizeof(Fighter));
    return 0;
}
