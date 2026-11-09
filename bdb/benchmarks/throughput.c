/* SPDX-License-Identifier: AGPL-3.0-only */
/* Point-select and update throughput through bdb/lib, or through SQLite's C API.
 *
 *   throughput DB N OPS MODE [text|prep]
 *
 * Builds t (id INTEGER PRIMARY KEY, v INTEGER) with N rows in one transaction
 * (untimed), then times OPS statements inside one transaction:
 *   select  SELECT v FROM t WHERE id = k
 *   update  UPDATE t SET v = v + 1 WHERE id = k
 *   mixed   the two alternating
 * k walks the keys in a fixed permutation. API text (the default, P19): every
 * statement is passed as SQL text, so both engines parse each one; qdb prints
 * its result to stdout, so run it with stdout sent to the null device, and
 * SQLite's callback formats each row into a buffer. API prep (P25): both
 * engines prepare the two statements once, with `?` for the key, then bind,
 * step and copy each result cell into a buffer. Prints one line to stderr:
 *   ENGINE MODE N OPS SECONDS OPS_PER_SECOND
 * The qdb build defines USE_QDB and links libmain.a; the SQLite build declares
 * the functions it calls and is compiled with the SQLite amalgamation. */
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
static char cellbuf[4096];
static QStmt* ps_sel;
static QStmt* ps_upd;
static void eng_prepare(void) {
    ps_sel = qdb_prepare(h, (uint8_t*)"SELECT v FROM t WHERE id = ?");
    ps_upd = qdb_prepare(h, (uint8_t*)"UPDATE t SET v = v + 1 WHERE id = ?");
}
static void eng_run(int upd, long k) {
    QStmt* s = upd ? ps_upd : ps_sel;
    qdb_reset(s);
    qdb_bind_int64(s, 1, k);
    while (qdb_step(h, s) == 100) qdb_column_text(s, 0, (uint8_t*)cellbuf, sizeof cellbuf);
}
static void eng_unprepare(void) { qdb_finalize(ps_sel); qdb_finalize(ps_upd); }
#else
typedef struct sqlite3 sqlite3;
int sqlite3_open(const char* filename, sqlite3** db);
int sqlite3_exec(sqlite3* db, const char* sql, int (*cb)(void*, int, char**, char**), void* arg, char** err);
int sqlite3_close(sqlite3* db);
void sqlite3_free(void* p);
typedef struct sqlite3_stmt sqlite3_stmt;
int sqlite3_prepare_v2(sqlite3* db, const char* sql, int n, sqlite3_stmt** st, const char** tail);
int sqlite3_bind_int64(sqlite3_stmt* st, int i, long long v);
int sqlite3_step(sqlite3_stmt* st);
int sqlite3_reset(sqlite3_stmt* st);
int sqlite3_finalize(sqlite3_stmt* st);
const unsigned char* sqlite3_column_text(sqlite3_stmt* st, int i);
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
static char cellbuf[4096];
static sqlite3_stmt* ps_sel;
static sqlite3_stmt* ps_upd;
static void eng_prepare(void) {
    sqlite3_prepare_v2(h, "SELECT v FROM t WHERE id = ?", -1, &ps_sel, NULL);
    sqlite3_prepare_v2(h, "UPDATE t SET v = v + 1 WHERE id = ?", -1, &ps_upd, NULL);
}
static void eng_run(int upd, long k) {
    sqlite3_stmt* s = upd ? ps_upd : ps_sel;
    sqlite3_reset(s);
    sqlite3_bind_int64(s, 1, k);
    int rc;
    while ((rc = sqlite3_step(s)) == 100) {
        const unsigned char* v = sqlite3_column_text(s, 0);
        size_t n = v ? strlen((const char*)v) : 0;
        if (n >= sizeof cellbuf) n = sizeof cellbuf - 1;
        memcpy(cellbuf, v ? (const char*)v : "", n);
        cellbuf[n] = 0;
    }
    if (rc != 101) {
        fprintf(stderr, "sqlite: step returned %d\n", rc);
        exit(1);
    }
}
static void eng_unprepare(void) { sqlite3_finalize(ps_sel); sqlite3_finalize(ps_upd); }
#endif

int main(int argc, char** argv) {
    if (argc < 5) {
        fprintf(stderr, "usage: throughput DB N OPS select|update|mixed [text|prep]\n");
        return 2;
    }
    const char* path = argv[1];
    long n = atol(argv[2]);
    long ops = atol(argv[3]);
    const char* mode = argv[4];
    int prep = argc > 5 && strcmp(argv[5], "prep") == 0;
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
    if (prep) {
        /* One untimed select, so the timed run starts from a warm engine:
         * qdb folds the loaded rows into its index at the first indexed read,
         * where SQLite built its key B-tree during the (untimed) load. */
        eng_prepare();
        eng_run(0, 0);
    }
    double t0 = now();
    for (long i = 0; i < ops; i++) {
        long k = (i * 7919) % n;
        int upd = strcmp(mode, "update") == 0 || (strcmp(mode, "mixed") == 0 && (i & 1));
        if (prep) {
            eng_run(upd, k);
            continue;
        }
        if (upd) snprintf(sql, sizeof sql, "UPDATE t SET v = v + 1 WHERE id = %ld", k);
        else snprintf(sql, sizeof sql, "SELECT v FROM t WHERE id = %ld", k);
        eng_exec(sql);
    }
    double dt = now() - t0;
    if (prep) eng_unprepare();
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
