// frametrace D3D11 hook: a proxy/injected DLL that hooks the shared
// ID3D11DeviceContext vtable and feeds every bind/draw to the frametrace C ABI,
// turning the implicit frame state machine into an observable symbol table.
//
// BUILD-VERIFIED ONLY: this compiles and links (hook/build.bat); its runtime is
// validated against a live D3D11 app, not in CI.
//
// Hooking is by vtable-pointer swap (no MinHook): the ID3D11DeviceContext vtable
// is shared by every context in the d3d11 runtime, so swapping a slot hooks all.

#include <windows.h>
#include <d3d11.h>
#include <cstdint>
#include <cstdio>
#include <vector>
#include "frametrace.h"

// Canonical ID3D11DeviceContext vtable slot indices (the interface is frozen).
enum {
    VT_PSSetShaderResources = 8,
    VT_DrawIndexed = 12,
    VT_Draw = 13,
    VT_DrawIndexedInstanced = 20,
    VT_DrawInstanced = 21,
    VT_VSSetShaderResources = 25,
    VT_GSSetShaderResources = 31,
    VT_OMSetRenderTargets = 33,
    VT_OMSetRenderTargetsAndUAVs = 34,
    VT_DrawAuto = 38,
    VT_Dispatch = 41,
    VT_HSSetShaderResources = 59,
    VT_DSSetShaderResources = 63,
    VT_CSSetShaderResources = 67,
    VT_CSSetUnorderedAccessViews = 68
};

// Stage codes match the C ABI: 0=Vs 1=Ps 2=Cs 3=Gs 4=Hs 5=Ds.
// Kind codes: 0=Srv 1=Rtv 2=Dsv 3=DsvReadOnly 4=Uav.

typedef void (STDMETHODCALLTYPE *PFN_SetSRV)(ID3D11DeviceContext*, UINT, UINT, ID3D11ShaderResourceView* const*);
typedef void (STDMETHODCALLTYPE *PFN_SetUAV)(ID3D11DeviceContext*, UINT, UINT, ID3D11UnorderedAccessView* const*, const UINT*);
typedef void (STDMETHODCALLTYPE *PFN_OMSetRT)(ID3D11DeviceContext*, UINT, ID3D11RenderTargetView* const*, ID3D11DepthStencilView*);
typedef void (STDMETHODCALLTYPE *PFN_OMSetRTUAV)(ID3D11DeviceContext*, UINT, ID3D11RenderTargetView* const*, ID3D11DepthStencilView*, UINT, UINT, ID3D11UnorderedAccessView* const*, const UINT*);
typedef void (STDMETHODCALLTYPE *PFN_Draw)(ID3D11DeviceContext*, UINT, UINT);
typedef void (STDMETHODCALLTYPE *PFN_DrawIndexed)(ID3D11DeviceContext*, UINT, UINT, INT);
typedef void (STDMETHODCALLTYPE *PFN_DrawInstanced)(ID3D11DeviceContext*, UINT, UINT, UINT, UINT);
typedef void (STDMETHODCALLTYPE *PFN_DrawIndexedInstanced)(ID3D11DeviceContext*, UINT, UINT, UINT, INT, UINT);
typedef void (STDMETHODCALLTYPE *PFN_DrawAuto)(ID3D11DeviceContext*);
typedef void (STDMETHODCALLTYPE *PFN_Dispatch)(ID3D11DeviceContext*, UINT, UINT, UINT);

static FrameState* g_fs = nullptr;
static PFN_SetSRV g_origPS = nullptr, g_origVS = nullptr, g_origCS = nullptr;
static PFN_SetSRV g_origGS = nullptr, g_origHS = nullptr, g_origDS = nullptr;
static PFN_SetUAV g_origCSUAV = nullptr;
static PFN_OMSetRT g_origOM = nullptr;
static PFN_OMSetRTUAV g_origOMUAV = nullptr;
static PFN_Draw g_origDraw = nullptr;
static PFN_DrawIndexed g_origDrawIndexed = nullptr;
static PFN_DrawInstanced g_origDrawInstanced = nullptr;
static PFN_DrawIndexedInstanced g_origDrawIdxInst = nullptr;
static PFN_DrawAuto g_origDrawAuto = nullptr;
static PFN_Dispatch g_origDispatch = nullptr;

static uint64_t resource_of(ID3D11View* v) {
    if (!v) return 0;
    ID3D11Resource* r = nullptr;
    v->GetResource(&r);
    uint64_t id = (uint64_t)r;
    if (r) r->Release();
    return id;
}

// A DSV with read-only depth can coexist with a depth SRV without a hazard.
static int dsv_kind(ID3D11DepthStencilView* dsv) {
    D3D11_DEPTH_STENCIL_VIEW_DESC d;
    dsv->GetDesc(&d);
    if (d.Flags & D3D11_DSV_READ_ONLY_DEPTH) return 3; // DsvReadOnly
    return 2;                                          // Dsv (write)
}

static void onSetSRV(int stage, UINT start, UINT n, ID3D11ShaderResourceView* const* views) {
    if (!g_fs) return;
    std::vector<uint64_t> ids;
    ids.reserve(n);
    for (UINT i = 0; i < n; i++) {
        ID3D11ShaderResourceView* v = views ? views[i] : nullptr;
        ids.push_back((uint64_t)v);
        if (v) ft_register_view(g_fs, (uint64_t)v, resource_of((ID3D11View*)v), 0);
    }
    ft_set_shader_resources(g_fs, stage, start, ids.empty() ? nullptr : ids.data(), n);
}

static void STDMETHODCALLTYPE hkPS(ID3D11DeviceContext* c, UINT s, UINT n, ID3D11ShaderResourceView* const* v) { onSetSRV(1, s, n, v); g_origPS(c, s, n, v); }
static void STDMETHODCALLTYPE hkVS(ID3D11DeviceContext* c, UINT s, UINT n, ID3D11ShaderResourceView* const* v) { onSetSRV(0, s, n, v); g_origVS(c, s, n, v); }
static void STDMETHODCALLTYPE hkCS(ID3D11DeviceContext* c, UINT s, UINT n, ID3D11ShaderResourceView* const* v) { onSetSRV(2, s, n, v); g_origCS(c, s, n, v); }
static void STDMETHODCALLTYPE hkGS(ID3D11DeviceContext* c, UINT s, UINT n, ID3D11ShaderResourceView* const* v) { onSetSRV(3, s, n, v); g_origGS(c, s, n, v); }
static void STDMETHODCALLTYPE hkHS(ID3D11DeviceContext* c, UINT s, UINT n, ID3D11ShaderResourceView* const* v) { onSetSRV(4, s, n, v); g_origHS(c, s, n, v); }
static void STDMETHODCALLTYPE hkDS(ID3D11DeviceContext* c, UINT s, UINT n, ID3D11ShaderResourceView* const* v) { onSetSRV(5, s, n, v); g_origDS(c, s, n, v); }

static void STDMETHODCALLTYPE hkCSUAV(ID3D11DeviceContext* c, UINT s, UINT n, ID3D11UnorderedAccessView* const* uavs, const UINT* counts) {
    if (g_fs) {
        std::vector<uint64_t> ids;
        ids.reserve(n);
        for (UINT i = 0; i < n; i++) {
            ID3D11UnorderedAccessView* u = uavs ? uavs[i] : nullptr;
            ids.push_back((uint64_t)u);
            if (u) ft_register_view(g_fs, (uint64_t)u, resource_of((ID3D11View*)u), 4);
        }
        ft_set_unordered_access_views(g_fs, s, ids.empty() ? nullptr : ids.data(), n);
    }
    g_origCSUAV(c, s, n, uavs, counts);
}

static void register_rts(UINT n, ID3D11RenderTargetView* const* rtvs, ID3D11DepthStencilView* dsv) {
    std::vector<uint64_t> ids;
    ids.reserve(n);
    for (UINT i = 0; i < n; i++) {
        ID3D11RenderTargetView* r = rtvs ? rtvs[i] : nullptr;
        ids.push_back((uint64_t)r);
        if (r) ft_register_view(g_fs, (uint64_t)r, resource_of((ID3D11View*)r), 1);
    }
    if (dsv) ft_register_view(g_fs, (uint64_t)dsv, resource_of((ID3D11View*)dsv), dsv_kind(dsv));
    ft_set_render_targets(g_fs, ids.empty() ? nullptr : ids.data(), n, (uint64_t)dsv);
}

static void STDMETHODCALLTYPE hkOM(ID3D11DeviceContext* c, UINT n, ID3D11RenderTargetView* const* rtvs, ID3D11DepthStencilView* dsv) {
    if (g_fs) register_rts(n, rtvs, dsv);
    g_origOM(c, n, rtvs, dsv);
}

static void STDMETHODCALLTYPE hkOMUAV(ID3D11DeviceContext* c, UINT nRTV, ID3D11RenderTargetView* const* rtvs, ID3D11DepthStencilView* dsv,
                                      UINT uavStart, UINT nUAV, ID3D11UnorderedAccessView* const* uavs, const UINT* counts) {
    if (g_fs) {
        if (nRTV != D3D11_KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL) register_rts(nRTV, rtvs, dsv);
        if (nUAV != D3D11_KEEP_UNORDERED_ACCESS_VIEWS) {
            std::vector<uint64_t> ids;
            ids.reserve(nUAV);
            for (UINT i = 0; i < nUAV; i++) {
                ID3D11UnorderedAccessView* u = uavs ? uavs[i] : nullptr;
                ids.push_back((uint64_t)u);
                if (u) ft_register_view(g_fs, (uint64_t)u, resource_of((ID3D11View*)u), 4);
            }
            ft_set_unordered_access_views(g_fs, uavStart, ids.empty() ? nullptr : ids.data(), nUAV);
        }
    }
    g_origOMUAV(c, nRTV, rtvs, dsv, uavStart, nUAV, uavs, counts);
}

// Emit a draw/dispatch checkpoint, then surface any live hazard to the debugger.
static void checkpoint(bool dispatch) {
    if (!g_fs) return;
    if (dispatch) ft_dispatch(g_fs); else ft_draw(g_fs);
    size_t n = ft_hazard_count(g_fs);
    for (size_t i = 0; i < n; i++) {
        char buf[256];
        sprintf_s(buf, sizeof(buf), "[frametrace] %s hazard on resource 0x%llx",
                  ft_hazard_kind_name(g_fs, i),
                  (unsigned long long)ft_hazard_resource(g_fs, i));
        OutputDebugStringA(buf);
    }
}

static void STDMETHODCALLTYPE hkDraw(ID3D11DeviceContext* c, UINT a, UINT b) { checkpoint(false); g_origDraw(c, a, b); }
static void STDMETHODCALLTYPE hkDrawIndexed(ID3D11DeviceContext* c, UINT a, UINT b, INT d) { checkpoint(false); g_origDrawIndexed(c, a, b, d); }
static void STDMETHODCALLTYPE hkDrawInstanced(ID3D11DeviceContext* c, UINT a, UINT b, UINT d, UINT e) { checkpoint(false); g_origDrawInstanced(c, a, b, d, e); }
static void STDMETHODCALLTYPE hkDrawIdxInst(ID3D11DeviceContext* c, UINT a, UINT b, UINT d, INT e, UINT f) { checkpoint(false); g_origDrawIdxInst(c, a, b, d, e, f); }
static void STDMETHODCALLTYPE hkDrawAuto(ID3D11DeviceContext* c) { checkpoint(false); g_origDrawAuto(c); }
static void STDMETHODCALLTYPE hkDispatch(ID3D11DeviceContext* c, UINT a, UINT b, UINT d) { checkpoint(true); g_origDispatch(c, a, b, d); }

// Swap one vtable slot, saving the original. The vtable lives in d3d11.dll and
// is shared by all contexts, so this hooks every context process-wide.
static void hook_slot(void** vtbl, int idx, void* hook, void** orig) {
    DWORD prot;
    VirtualProtect(&vtbl[idx], sizeof(void*), PAGE_EXECUTE_READWRITE, &prot);
    *orig = vtbl[idx];
    vtbl[idx] = hook;
    VirtualProtect(&vtbl[idx], sizeof(void*), prot, &prot);
}

static bool install() {
    D3D_FEATURE_LEVEL fl;
    ID3D11Device* dev = nullptr;
    ID3D11DeviceContext* ctx = nullptr;
    HRESULT hr = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0, nullptr, 0,
                                   D3D11_SDK_VERSION, &dev, &fl, &ctx);
    if (FAILED(hr))
        hr = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr, 0, nullptr, 0,
                               D3D11_SDK_VERSION, &dev, &fl, &ctx);
    if (FAILED(hr) || !ctx) return false;

    void** vtbl = *(void***)ctx;
    g_fs = ft_new();

    hook_slot(vtbl, VT_PSSetShaderResources, (void*)&hkPS, (void**)&g_origPS);
    hook_slot(vtbl, VT_VSSetShaderResources, (void*)&hkVS, (void**)&g_origVS);
    hook_slot(vtbl, VT_GSSetShaderResources, (void*)&hkGS, (void**)&g_origGS);
    hook_slot(vtbl, VT_HSSetShaderResources, (void*)&hkHS, (void**)&g_origHS);
    hook_slot(vtbl, VT_DSSetShaderResources, (void*)&hkDS, (void**)&g_origDS);
    hook_slot(vtbl, VT_CSSetShaderResources, (void*)&hkCS, (void**)&g_origCS);
    hook_slot(vtbl, VT_CSSetUnorderedAccessViews, (void*)&hkCSUAV, (void**)&g_origCSUAV);
    hook_slot(vtbl, VT_OMSetRenderTargets, (void*)&hkOM, (void**)&g_origOM);
    hook_slot(vtbl, VT_OMSetRenderTargetsAndUAVs, (void*)&hkOMUAV, (void**)&g_origOMUAV);
    hook_slot(vtbl, VT_Draw, (void*)&hkDraw, (void**)&g_origDraw);
    hook_slot(vtbl, VT_DrawIndexed, (void*)&hkDrawIndexed, (void**)&g_origDrawIndexed);
    hook_slot(vtbl, VT_DrawInstanced, (void*)&hkDrawInstanced, (void**)&g_origDrawInstanced);
    hook_slot(vtbl, VT_DrawIndexedInstanced, (void*)&hkDrawIdxInst, (void**)&g_origDrawIdxInst);
    hook_slot(vtbl, VT_DrawAuto, (void*)&hkDrawAuto, (void**)&g_origDrawAuto);
    hook_slot(vtbl, VT_Dispatch, (void*)&hkDispatch, (void**)&g_origDispatch);

    ctx->Release();
    dev->Release();
    OutputDebugStringA("[frametrace] hooks installed");
    return true;
}

static DWORD WINAPI init_thread(LPVOID) {
    install();
    return 0;
}

BOOL WINAPI DllMain(HINSTANCE hinst, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(hinst);
        CreateThread(nullptr, 0, init_thread, nullptr, 0, nullptr);
    }
    return TRUE;
}
