/* SPDX-License-Identifier: FSL-1.1-MIT
 * bl_audio: real-time audio output for BuildLang programs, with a deadline counter. See bl_audio.h. */
#include "bl_audio.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
  int device, rate, channels, period, silent, open;
  int64_t periods, misses;
  double render_max, render_sum, t_wake, t_next, buffer_ms, stream_ms;
} BlaState;
static BlaState bla;

#ifdef _WIN32
#define COBJMACROS
#include <windows.h>
/* A host that set NOGDI before windows.h (BuildLang's generated C does) skipped the GDI types, and
 * wingdi.h's include guard keeps them out; audioclient.h needs BITMAPINFOHEADER through mmreg.h. */
#ifdef NOGDI
#undef NOGDI
#undef _WINGDI_
#endif
#include <wingdi.h>
#include <initguid.h>
#include <audioclient.h>
#include <mmdeviceapi.h>
/* ole32 and avrt are loaded at run time, so a program carrying this file needs no extra link flags. */
typedef HRESULT(WINAPI* BlaCoInit)(LPVOID, DWORD);
typedef HRESULT(WINAPI* BlaCoCreate)(REFCLSID, LPUNKNOWN, DWORD, REFIID, LPVOID*);
typedef void(WINAPI* BlaCoFree)(LPVOID);
typedef HANDLE(WINAPI* BlaAvSet)(LPCSTR, LPDWORD);
typedef BOOL(WINAPI* BlaAvRevert)(HANDLE);
static BlaCoInit bla_co_init;
static BlaCoCreate bla_co_create;
static BlaCoFree bla_co_free;
static BlaAvSet bla_av_set;
static BlaAvRevert bla_av_revert;
static int bla_load(void) {
  HMODULE ole = LoadLibraryA("ole32.dll"), avrt = LoadLibraryA("avrt.dll");
  if (!ole) return 0;
  bla_co_init = (BlaCoInit)(void*)GetProcAddress(ole, "CoInitializeEx");
  bla_co_create = (BlaCoCreate)(void*)GetProcAddress(ole, "CoCreateInstance");
  bla_co_free = (BlaCoFree)(void*)GetProcAddress(ole, "CoTaskMemFree");
  if (avrt) {
    bla_av_set = (BlaAvSet)(void*)GetProcAddress(avrt, "AvSetMmThreadCharacteristicsA");
    bla_av_revert = (BlaAvRevert)(void*)GetProcAddress(avrt, "AvRevertMmThreadCharacteristics");
  }
  return bla_co_init && bla_co_create && bla_co_free;
}
static IMMDevice* bla_dev;
static IAudioClient* bla_ac;
static IAudioRenderClient* bla_rc;
static HANDLE bla_event, bla_mmcss;
static UINT32 bla_buf_frames;
static double bla_now(void) {
  LARGE_INTEGER f, t;
  QueryPerformanceFrequency(&f);
  QueryPerformanceCounter(&t);
  return (double)t.QuadPart / (double)f.QuadPart;
}
static void bla_sleep_until(double t) {
  double d = t - bla_now();
  if (d > 0.002) Sleep((DWORD)((d - 0.001) * 1000.0));
  while (bla_now() < t) { }
}
#else
#include <time.h>
static double bla_now(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (double)ts.tv_sec + ts.tv_nsec * 1e-9;
}
static void bla_sleep_until(double t) {
  double d = t - bla_now();
  if (d <= 0) return;
  struct timespec ts = {(time_t)d, (long)((d - (time_t)d) * 1e9)};
  nanosleep(&ts, NULL);
}
#endif

static int32_t bla_open_null(int32_t rate, int32_t period) {
  bla.rate = rate > 0 ? rate : 48000;
  bla.channels = 2;
  bla.period = period > 0 && period <= BL_AUDIO_MAX_FRAMES ? period : 480;
  bla.buffer_ms = 1000.0 * bla.period / bla.rate;
  bla.stream_ms = 0;
  bla.t_next = bla_now();
  return 0;
}

#ifdef _WIN32
#include "bl_audio_win.c"   /* WASAPI, the shorter period, performance cores, the click probes */
#endif

int32_t bl_audio_open(int32_t device, int32_t rate, int32_t period_frames, int32_t silent) {
  memset(&bla, 0, sizeof bla);
  bla.device = device;
  bla.silent = silent;
  int32_t rc;
#ifdef _WIN32
  rc = device == 1 ? bla_open_wasapi(silent, period_frames) : bla_open_null(rate, period_frames);
#else
  rc = device == 1 ? -1 : bla_open_null(rate, period_frames);
#endif
  bla.open = rc == 0;
  return rc;
}

static double bla_buf[BL_AUDIO_MAX_FRAMES];
double* bl_audio_buffer(void) { return bla_buf; }
double* bl_audio_alloc(uintptr_t count) { return (double*)calloc(count ? count : 1, sizeof(double)); }
int32_t bl_audio_rate(void) { return bla.rate; }
int32_t bl_audio_channels(void) { return bla.channels; }
int32_t bl_audio_period(void) { return bla.period; }

int32_t bl_audio_wait(void) {
  if (!bla.open) return 0;
#ifdef _WIN32
  if (bla.device == 1) {
    UINT32 pad = 0;
    WaitForSingleObject(bla_event, 2000);
    bla.t_wake = bla_now();
    IAudioClient_GetCurrentPadding(bla_ac, &pad);
    UINT32 space = bla_buf_frames - pad;
    UINT32 n = space < (UINT32)bla.period ? space : (UINT32)bla.period;
    return (int32_t)(n < BL_AUDIO_MAX_FRAMES ? n : BL_AUDIO_MAX_FRAMES);
  }
#endif
  bla.t_next += (double)bla.period / bla.rate;
  bla_sleep_until(bla.t_next);
  bla.t_wake = bla_now();
  return bla.period;
}

static void bla_account(void) {
  double r = bla_now() - bla.t_wake;
  bla.periods += 1;
  bla.render_sum += r;
  if (r > bla.render_max) bla.render_max = r;
  if (r > (double)bla.period / bla.rate) bla.misses += 1;
}

int32_t bl_audio_write(double* mono, uintptr_t frames) {
  if (!bla.open) return -1;
#ifdef _WIN32
  if (bla.device == 1 && frames > 0) {
    BYTE* data;
    if (SUCCEEDED(IAudioRenderClient_GetBuffer(bla_rc, (UINT32)frames, &data))) {
      float* o = (float*)data;
      for (uintptr_t j = 0; j < frames; ++j)
        for (int c = 0; c < bla.channels; ++c) o[j * bla.channels + c] = (float)mono[j];
      IAudioRenderClient_ReleaseBuffer(bla_rc, (UINT32)frames, bla.silent ? AUDCLNT_BUFFERFLAGS_SILENT : 0);
    }
  }
#else
  (void)mono;
#endif
  bla_account();
  return 0;
}

int64_t bl_audio_periods(void) { return bla.periods; }
int64_t bl_audio_misses(void) { return bla.misses; }
double bl_audio_render_ms_max(void) { return bla.render_max * 1e3; }
double bl_audio_render_ms_mean(void) { return bla.periods ? bla.render_sum / bla.periods * 1e3 : 0.0; }
double bl_audio_buffer_ms(void) { return bla.buffer_ms; }
double bl_audio_stream_latency_ms(void) { return bla.stream_ms; }

#ifndef _WIN32
int32_t bl_audio_pin_performance_cores(void) { return 0; }
double bl_audio_loopback_ms(double level) { (void)level; return -1; }
double bl_audio_roundtrip_ms(double level, double threshold) { (void)level; (void)threshold; return -1; }
#endif

void bl_audio_close(void) {
#ifdef _WIN32
  if (bla.device == 1 && bla.open) {
    IAudioClient_Stop(bla_ac);
    if (bla_mmcss && bla_av_revert) bla_av_revert(bla_mmcss);
    IAudioRenderClient_Release(bla_rc);
    IAudioClient_Release(bla_ac);
    IMMDevice_Release(bla_dev);
  }
#endif
  bla.open = 0;
}

#include "bl_events.c"   /* events from another process, and the shared clock */
