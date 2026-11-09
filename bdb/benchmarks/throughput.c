/* SPDX-License-Identifier: AGPL-3.0-only */
/* Point-select and update throughput through bdb/lib, or through SQLite's C API.
 *
 *   throughput qdb|sqlite DB N OPS MODE
 *
 * Builds t (id INTEGER PRIMARY KEY, v INTEGER) with N rows in one transaction
 * (untimed), then times OPS statements inside one transaction:
 *   select  SELECT v FROM t WHERE id = k
 *   update  UPDATE t SET v = v + 1 WHERE id = k
 *   mixed   the two alternating
 * k walks the keys in a fixed permutation. Every statement is passed as SQL
 * text, so both engines parse each one. qdb prints its result to stdout (it
 * has no row API), so run it with stdout sent to the null device; SQLite's
 * callback formats each row into a buffer. Prints one line to stderr:
 *   ENGINE MODE N OPS SECONDS OPS_PER_SECOND
 * The qdb build defines USE_QDB and links libmain.a; the SQLite build declares
 * the five functions it calls and links the sqlite3 library. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
static double now(void) {
    LARGE_INTEGER f, c;
    QueryPerformanceFrequency(&f);
    QueryPerformanceCounter(&c);
    return (double)c.QuadPart / (double)f.QuadPart;
}
#else
#include <time.h>
static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + t.tv_nsec * 1e-9;
}
#endif

#ifdef USE_QDB
#include "main.h"
static DB* h;
static void eng_open(const char* path) { h = qdb_open((uint8_t*)path); }
static void eng_exec(const char* sql) { qdb_exec(h, (uint8_t*)sql); }
static void eng_close(void) { qdb_close(h); }
static const char* ENGINE = "qdb";
#else
typedef struct sqlite3 sqlite3;
int sqlite3_open(const char* filename, sqlite3** db);
int sqlite3_exec(sqlite3* db, const char* sql, int (*cb)(void*, int, char**, char**), void* arg, char** err);
int sqlite3_close(sqlite3* db);
void sqlite3_free(void* p);
static sqlite3* h;
static char rowbuf[4096];
static int on_row(void* arg, int n, char** vals, char** names) {
    (void)arg; (void)names;
    size_t o = 0;
    for (int i = 0; i < n && o < sizeof(rowbuf) - 64; i++)
        o += (size_t)snprintf(rowbuf + o, sizeof(rowbuf) - o, "%s%s", i ? " | " : "", vals[i] ? vals[i] : "NULL");
    return 0;
}
static void eng_open(const char* path) { sqlite3_open(path, &h); }
static void eng_exec(const char* sql) {
    char* err = NULL;
    if (sqlite3_exec(h, sql, on_row, NULL, &err) != 0) {
        fprintf(stderr, "sqlite: %s: %s\n", sql, err ? err : "?");
        exit(1);
    }
}
static void eng_close(void) { sqlite3_close(h); }
static const char* ENGINE = "sqlite";
#endif

int main(int argc, char** argv) {
    if (argc < 5) {
        fprintf(stderr, "usage: throughput DB N OPS select|update|mixed\n");
        return 2;
    }
    const char* path = argv[1];
    long n = atol(argv[2]);
    long ops = atol(argv[3]);
    const char* mode = argv[4];
    char sql[256];
    eng_open(path);
    eng_exec("CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER)");
#ifdef USE_QDB
    eng_exec("CREATE INDEX t_id ON t (id)");
#endif
    eng_exec("BEGIN");
    for (long i = 0; i < n; i++) {
        snprintf(sql, sizeof sql, "INSERT INTO t VALUES (%ld, %ld)", i, (i * 7) % 1000);
        eng_exec(sql);
    }
    eng_exec("COMMIT");
    eng_exec("BEGIN");
    double t0 = now();
    for (long i = 0; i < ops; i++) {
        long k = (i * 7919) % n;
        int upd = strcmp(mode, "update") == 0 || (strcmp(mode, "mixed") == 0 && (i & 1));
        if (upd) snprintf(sql, sizeof sql, "UPDATE t SET v = v + 1 WHERE id = %ld", k);
        else snprintf(sql, sizeof sql, "SELECT v FROM t WHERE id = %ld", k);
        eng_exec(sql);
    }
    double dt = now() - t0;
    eng_exec("COMMIT");
    /* The end state, for the harness to compare across engines: the last
     * line of stdout is SUM(v). */
    eng_exec("SELECT SUM(v) FROM t");
#ifndef USE_QDB
    printf("%s\n", rowbuf);
#endif
    eng_close();
    fprintf(stderr, "%s %s %ld %ld %.6f %.1f\n", ENGINE, mode, n, ops, dt, ops / dt);
    return 0;
}
