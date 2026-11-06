/* SPDX-License-Identifier: FSL-1.1-MIT
 * bl_audio: a small real-time audio output for BuildLang programs, with a deadline counter.
 *
 * Devices: "wasapi" (Windows, shared mode, event driven, pro-audio thread class) and "null" (any OS: a
 * clock-paced device that takes the samples and discards them, for CI and for timing without sound).
 * A program opens a device, then loops: bl_audio_wait() blocks until the next period is due and returns
 * the number of frames to render; the program renders them and calls bl_audio_write(); every period's
 * render time is measured against the period and a period that overruns counts as a deadline miss.
 */
#ifndef BL_AUDIO_H
#define BL_AUDIO_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* device: 0 null, 1 wasapi. rate and period_frames are requests; the device may change them.
 * silent != 0 sends silence to the speakers while still timing and accepting the samples.
 * Returns 0 on success, a negative code on failure. */
int32_t bl_audio_open(int32_t device, int32_t rate, int32_t period_frames, int32_t silent);
int32_t bl_audio_rate(void);
int32_t bl_audio_channels(void);
int32_t bl_audio_period(void);
/* A buffer of BL_AUDIO_MAX_FRAMES doubles owned by the binding, for the program to render into. */
#define BL_AUDIO_MAX_FRAMES 16384
double* bl_audio_buffer(void);
/* Zeroed memory for `count` doubles, for state set up before the stream starts (never call it from the
 * render loop). Freed by the operating system at exit. NULL on failure. */
double* bl_audio_alloc(uintptr_t count);
/* Blocks until the next period; returns frames to render (0 on error). Starts the render timer. */
int32_t bl_audio_wait(void);
/* Takes `frames` mono samples, writes them to every channel, stops the render timer. */
int32_t bl_audio_write(double* mono, uintptr_t frames);
int64_t bl_audio_periods(void);
int64_t bl_audio_misses(void);
double bl_audio_render_ms_max(void);
double bl_audio_render_ms_mean(void);
/* Device latency figures in milliseconds: the device buffer and the reported stream latency. */
double bl_audio_buffer_ms(void);
double bl_audio_stream_latency_ms(void);
/* Renders one click and returns the milliseconds until it appears in the device's own loopback capture
 * (WASAPI). Loopback taps the shared-mode mix, so this times the software path to the mix, not the
 * analogue output or a round trip; on the reference machine it is under one period. Negative when the
 * device cannot measure it. */
double bl_audio_loopback_ms(double click_level);
/* Acoustic or cabled round trip: renders one click on the output and times its arrival at the default
 * input device, detected above `threshold`. Plays an audible click. Negative when there is no input. */
double bl_audio_roundtrip_ms(double click_level, double threshold);
void bl_audio_close(void);
#ifdef __cplusplus
}
#endif
#endif
