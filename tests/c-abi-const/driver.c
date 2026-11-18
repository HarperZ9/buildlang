/* Host for tests/c-abi-const. Every pointer it passes to a `*const` parameter
 * points to const data, so the header must say `const` for this file to build
 * with -Werror (a plain `uint8_t*` parameter discards the qualifier). */
#include <stdio.h>
#include <stdint.h>
#include "main.h"

static const uint8_t table[4] = {1, 2, 3, 250};
static const uint8_t* const rows[2] = {table, table + 2};

int main(void) {
    uint8_t buf[3];
    cc_fill(buf, 3, 7);
    const uint8_t* same = cc_same(table);
    printf("sum %u\n", (unsigned)cc_sum(table, 4));
    printf("fill %u\n", (unsigned)cc_sum(buf, 3));
    printf("same %d\n", same == table);
    printf("rows %u\n", (unsigned)cc_rows(rows, 2));
    return 0;
}
