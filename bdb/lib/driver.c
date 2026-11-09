/* SPDX-License-Identifier: AGPL-3.0-only */
/* Run a file of statements, one per line, on one open qdb database.
 *
 *   driver DB STATEMENTS_FILE
 *
 * Prints "> <statement>" before each statement's output, the same framing
 * bdb/tests/test_qdb_lib.py gives the CLI, so the two outputs compare
 * byte for byte. */
#include <stdio.h>
#include <string.h>
#include "main.h"

int main(int argc, char** argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: driver DB STATEMENTS_FILE\n");
        return 2;
    }
    FILE* f = fopen(argv[2], "rb");
    if (!f) {
        fprintf(stderr, "cannot open %s\n", argv[2]);
        return 2;
    }
    setvbuf(stdout, NULL, _IONBF, 0);
    DB* d = qdb_open((uint8_t*)argv[1]);
    static char line[1 << 20];
    while (fgets(line, sizeof(line), f)) {
        size_t n = strlen(line);
        while (n > 0 && (line[n - 1] == '\n' || line[n - 1] == '\r')) line[--n] = 0;
        if (n == 0) continue;
        printf("> %s\n", line);
        qdb_exec(d, (uint8_t*)line);
    }
    fclose(f);
    qdb_close(d);
    return 0;
}
