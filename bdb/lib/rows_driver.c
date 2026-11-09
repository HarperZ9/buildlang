/* SPDX-License-Identifier: AGPL-3.0-only */
/* Run a file of statements through qdb's prepared-statement API (P25).
 *
 *   rows_driver DB STATEMENTS_FILE
 *
 * Each line is SQL, optionally followed by TAB-separated parameters for its
 * `?` placeholders: i:<integer>, t:<text> or n (NULL). Every statement is
 * prepared, bound, stepped to the end and finalized. Prints "> <line>", then
 * for a statement with result columns a "# " line of column names and one
 * line per row, cells joined by TAB, NULL written \N.
 * bdb/tests/test_qdb_rows.py compares the output of the same workload with
 * and without parameters. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "main.h"

static char cell[1 << 16];

int main(int argc, char** argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: rows_driver DB STATEMENTS_FILE\n");
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
        char* tab = strchr(line, '\t');
        if (tab) *tab = 0;
        QStmt* s = qdb_prepare(d, (uint8_t*)line);
        int k = 1;
        while (tab) {
            char* p = tab + 1;
            tab = strchr(p, '\t');
            if (tab) *tab = 0;
            if (p[0] == 'i') qdb_bind_int64(s, k, atoll(p + 2));
            else if (p[0] == 't') qdb_bind_text(s, k, (uint8_t*)(p + 2));
            else qdb_bind_null(s, k);
            k++;
        }
        int first = 1;
        while (qdb_step(d, s) == 100) {
            int nc = qdb_column_count(s);
            if (first) {
                printf("#");
                for (int i = 0; i < nc; i++) {
                    qdb_column_name(s, i, (uint8_t*)cell, sizeof cell);
                    printf(" %s", cell);
                }
                printf("\n");
                first = 0;
            }
            for (int i = 0; i < nc; i++) {
                int len = qdb_column_text(s, i, (uint8_t*)cell, sizeof cell);
                printf("%s%s", i ? "\t" : "", len < 0 ? "\\N" : cell);
            }
            printf("\n");
        }
        qdb_finalize(s);
    }
    fclose(f);
    qdb_close(d);
    return 0;
}
