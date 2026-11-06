/* SPDX-License-Identifier: FSL-1.1-MIT
 * Part of bl_audio.c (included from it, not compiled alone): the Windows device, WASAPI. */
/* Initialise shared mode at the requested period through IAudioClient3 (Windows 10 and later), rounded up to
 * the engine's fundamental period and kept within its range; returns the period in frames, or 0 to fall back. */
static UINT32 bla_init_short(WAVEFORMATEX* wf, int32_t want) {
  IAudioClient3* ac3 = NULL;
  UINT32 def_f, fund_f, min_f, max_f, p;
  if (want <= 0 || FAILED(IAudioClient_QueryInterface(bla_ac, &IID_IAudioClient3, (void**)&ac3))) return 0;
  if (FAILED(IAudioClient3_GetSharedModeEnginePeriod(ac3, wf, &def_f, &fund_f, &min_f, &max_f)) || (UINT32)want >= def_f) {
    IAudioClient3_Release(ac3);
    return 0;
  }
  p = ((UINT32)want + fund_f - 1) / fund_f * fund_f;
  p = p < min_f ? min_f : (p > max_f ? max_f : p);
  if (FAILED(IAudioClient3_InitializeSharedAudioStream(ac3, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, p, wf, NULL))) p = 0;
  IAudioClient3_Release(ac3);
  return p;
}

static int32_t bla_open_wasapi(int32_t silent, int32_t period_frames) {
  IMMDeviceEnumerator* en = NULL;
  WAVEFORMATEX* wf = NULL;
  REFERENCE_TIME def_p, min_p, lat;
  UINT32 short_p;
  if (!bla_load()) return -10;
  if (FAILED(bla_co_init(NULL, COINIT_MULTITHREADED))) return -2;
  if (FAILED(bla_co_create(&CLSID_MMDeviceEnumerator, NULL, CLSCTX_ALL, &IID_IMMDeviceEnumerator, (void**)&en))) return -3;
  if (FAILED(IMMDeviceEnumerator_GetDefaultAudioEndpoint(en, eRender, eConsole, &bla_dev))) return -4;
  IMMDeviceEnumerator_Release(en);
  if (FAILED(IMMDevice_Activate(bla_dev, &IID_IAudioClient, CLSCTX_ALL, NULL, (void**)&bla_ac))) return -5;
  if (FAILED(IAudioClient_GetMixFormat(bla_ac, &wf)) || wf->wBitsPerSample != 32) return -6;
  IAudioClient_GetDevicePeriod(bla_ac, &def_p, &min_p);
  short_p = bla_init_short(wf, period_frames);
  if (!short_p && FAILED(IAudioClient_Initialize(bla_ac, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0, wf, NULL))) return -7;
  bla_event = CreateEvent(NULL, FALSE, FALSE, NULL);
  IAudioClient_SetEventHandle(bla_ac, bla_event);
  IAudioClient_GetBufferSize(bla_ac, &bla_buf_frames);
  IAudioClient_GetStreamLatency(bla_ac, &lat);
  if (FAILED(IAudioClient_GetService(bla_ac, &IID_IAudioRenderClient, (void**)&bla_rc))) return -8;
  bla.rate = (int)wf->nSamplesPerSec;
  bla.channels = wf->nChannels;
  bla.period = short_p ? (int)short_p : (int)llround(def_p * 1e-7 * bla.rate);
  bla.buffer_ms = 1000.0 * bla_buf_frames / bla.rate;
  bla.stream_ms = lat * 1e-4;
  bla.silent = silent;
  bla_co_free(wf);
  DWORD task = 0;
  bla_mmcss = bla_av_set ? bla_av_set("Pro Audio", &task) : NULL;
  return FAILED(IAudioClient_Start(bla_ac)) ? -9 : 0;
}

/* EfficiencyClass is the byte after Flags in PROCESSOR_RELATIONSHIP (Windows 10); older SDK headers name it
 * Reserved[0], so it is read by offset. */
static BYTE bla_eff(PSYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX e) { return ((const BYTE*)&e->Processor)[1]; }

/* Restrict the calling thread to the cores of the highest efficiency class (the performance cores of a
 * hybrid CPU), in processor group 0. Returns the number of logical processors allowed, or a negative code. */
int32_t bl_audio_pin_performance_cores(void) {
  DWORD len = 0;
  GetLogicalProcessorInformationEx(RelationProcessorCore, NULL, &len);
  BYTE* buf = (BYTE*)malloc(len ? len : 1);
  if (!buf || !GetLogicalProcessorInformationEx(RelationProcessorCore, (PSYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX)buf, &len)) {
    free(buf);
    return -1;
  }
  BYTE top = 0;
  for (DWORD off = 0; off < len;) {
    PSYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX e = (PSYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX)(buf + off);
    if (bla_eff(e) > top) top = bla_eff(e);
    off += e->Size;
  }
  KAFFINITY mask = 0;
  for (DWORD off = 0; off < len;) {
    PSYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX e = (PSYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX)(buf + off);
    if (bla_eff(e) == top && e->Processor.GroupMask[0].Group == 0) mask |= e->Processor.GroupMask[0].Mask;
    off += e->Size;
  }
  free(buf);
  if (!mask || !SetThreadAffinityMask(GetCurrentThread(), mask)) return -2;
  int32_t n = 0;
  for (KAFFINITY m = mask; m; m >>= 1) n += (int32_t)(m & 1);
  return n;
}

/* Find the click in the loopback stream: returns when its packet arrived (s, QPC clock), or a negative value. */
static double bla_capture_click(IAudioCaptureClient* cc, int ch, double threshold, double t_end) {
  LARGE_INTEGER f;
  QueryPerformanceFrequency(&f);
  while (bla_now() < t_end) {
    UINT32 n = 0;
    BYTE* data;
    DWORD flags;
    UINT64 pos, qpc;
    if (FAILED(IAudioCaptureClient_GetNextPacketSize(cc, &n)) || n == 0) { Sleep(1); continue; }
    if (FAILED(IAudioCaptureClient_GetBuffer(cc, &data, &n, &flags, &pos, &qpc))) return -1;
    float* s = (float*)data;
    for (UINT32 j = 0; j < n; ++j)
      if (!(flags & AUDCLNT_BUFFERFLAGS_SILENT) && fabs(s[j * ch]) > threshold) {
        double arrived = bla_now();
        IAudioCaptureClient_ReleaseBuffer(cc, n);
        (void)qpc;   /* the packet timestamp did not share an epoch with the render clock in tests */
        return arrived;
      }
    IAudioCaptureClient_ReleaseBuffer(cc, n);
  }
  return -1;
}

/* Open a shared-mode capture client: the render device's loopback, or the default input device. */
static IAudioClient* bla_open_capture(int loopback, IAudioCaptureClient** cc, int* ch) {
  IAudioClient* cap = NULL;
  IMMDevice* dev = bla_dev;
  IMMDeviceEnumerator* en = NULL;
  WAVEFORMATEX* wf = NULL;
  if (!loopback) {
    if (FAILED(bla_co_create(&CLSID_MMDeviceEnumerator, NULL, CLSCTX_ALL, &IID_IMMDeviceEnumerator, (void**)&en))) return NULL;
    HRESULT hr = IMMDeviceEnumerator_GetDefaultAudioEndpoint(en, eCapture, eConsole, &dev);
    IMMDeviceEnumerator_Release(en);
    if (FAILED(hr)) return NULL;
  }
  if (FAILED(IMMDevice_Activate(dev, &IID_IAudioClient, CLSCTX_ALL, NULL, (void**)&cap))) return NULL;
  IAudioClient_GetMixFormat(cap, &wf);
  *ch = wf->nChannels;
  if (wf->wBitsPerSample != 32 || FAILED(IAudioClient_Initialize(cap, AUDCLNT_SHAREMODE_SHARED,
      loopback ? AUDCLNT_STREAMFLAGS_LOOPBACK : 0, 2000000, 0, wf, NULL))) { bla_co_free(wf); IAudioClient_Release(cap); return NULL; }
  bla_co_free(wf);
  IAudioClient_GetService(cap, &IID_IAudioCaptureClient, (void**)cc);
  IAudioClient_Start(cap);
  return cap;
}

/* Render one click and time its arrival in a capture stream. */
static double bla_click_trip(int loopback, double level, double threshold) {
  IAudioCaptureClient* cc = NULL;
  int ch = 1;
  if (bla.device != 1 || !bla.open) return -1;
  IAudioClient* cap = bla_open_capture(loopback, &cc, &ch);
  if (!cap) return -2;
  int keep = bla.silent;
  bla.silent = 0;
  double* buf = calloc((size_t)bla.period * 4, sizeof(double));
  double t_click = -1, t_heard = -1;
  for (int p = 0; p < 60 && t_heard < 0; ++p) {
    int32_t n = bl_audio_wait();
    memset(buf, 0, sizeof(double) * (size_t)n);
    if (p == 20 && n > 0) buf[0] = level;
    bl_audio_write(buf, (uintptr_t)n);
    if (p == 20) t_click = bla_now();
    double found = bla_capture_click(cc, ch, threshold, bla_now() + 0.004);
    if (p >= 20) t_heard = found;
  }
  bla.silent = keep;
  free(buf);
  IAudioClient_Stop(cap);
  IAudioCaptureClient_Release(cc);
  IAudioClient_Release(cap);
  return t_heard > 0 && t_click > 0 ? (t_heard - t_click) * 1e3 : -4;
}

double bl_audio_loopback_ms(double level) { return bla_click_trip(1, level, 0.5 * level); }
double bl_audio_roundtrip_ms(double level, double threshold) { return bla_click_trip(0, level, threshold); }
