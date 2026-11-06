/* SPDX-License-Identifier: FSL-1.1-MIT
 * Part of bl_audio.c (included from it, not compiled alone): events from another process through a lock-free ring. */
/* ---- Events from another process: one line per event on standard input -------------------------------
 * A reader thread parses lines "t a b c d e" (six numbers) into a single-producer single-consumer ring; the
 * audio thread pops them without blocking or locking. Only the reader writes `head` and only the audio
 * thread writes `tail`, each with a release store the other side reads with an acquire load. */
#define BLA_RING 4096
static double bla_ring[BLA_RING][6];
static volatile long bla_head, bla_tail;
#if defined(_MSC_VER) && !defined(__clang__)
#include <intrin.h>
#define BLA_STORE(p, v) _InterlockedExchange((volatile long*)(p), (v))
#define BLA_LOAD(p) _InterlockedOr((volatile long*)(p), 0)
#else
#define BLA_STORE(p, v) __atomic_store_n((p), (v), __ATOMIC_RELEASE)
#define BLA_LOAD(p) __atomic_load_n((p), __ATOMIC_ACQUIRE)
#endif

static void bla_reader_loop(void) {
  char line[512];
  while (fgets(line, sizeof line, stdin)) {
    double v[6] = {0, 0, 0, 0, 0, 0};
    if (sscanf(line, "%lf %lf %lf %lf %lf %lf", &v[0], &v[1], &v[2], &v[3], &v[4], &v[5]) < 2) continue;
    long h = bla_head;
    if (h - BLA_LOAD(&bla_tail) >= BLA_RING) continue;          /* full: drop rather than block the reader */
    memcpy(bla_ring[h % BLA_RING], v, sizeof v);
    BLA_STORE(&bla_head, h + 1);
  }
  BLA_STORE(&bla_head, -bla_head - 1);   /* negative: input closed */
}

#ifdef _WIN32
static DWORD WINAPI bla_reader(LPVOID arg) { (void)arg; bla_reader_loop(); return 0; }
int32_t bl_events_start(void) { return CreateThread(NULL, 0, bla_reader, NULL, 0, NULL) ? 0 : -1; }
#else
#include <pthread.h>
static void* bla_reader(void* arg) { (void)arg; bla_reader_loop(); return NULL; }
int32_t bl_events_start(void) { pthread_t t; return pthread_create(&t, NULL, bla_reader, NULL) == 0 ? 0 : -1; }
#endif

/* Pop one event into out[0..5]: returns 1 for an event, 0 for none yet, -1 when the input has closed and the
 * ring is empty. */
int32_t bl_events_pop(double* out) {
  long h = BLA_LOAD(&bla_head), closed = h < 0;
  if (closed) h = -h - 1;
  if (bla_tail >= h) return closed ? -1 : 0;
  memcpy(out, bla_ring[bla_tail % BLA_RING], 6 * sizeof(double));
  BLA_STORE(&bla_tail, bla_tail + 1);
  return 1;
}

/* Seconds on the system's monotonic clock (QueryPerformanceCounter on Windows, CLOCK_MONOTONIC elsewhere),
 * the clock an engine in another process can stamp its events with. */
double bl_audio_now(void) { return bla_now(); }
