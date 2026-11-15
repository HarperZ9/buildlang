/* photon-frametrace C ABI. A C or C++ D3D11 vtable hook drives the Rust core
   through these functions. View/resource ids are the ID3D11 pointers cast to
   uint64; id 0 means null / unbound.

   ViewKind: 0=Srv 1=Rtv 2=Dsv 3=DsvReadOnly 4=Uav
   Stage:    0=Vs  1=Ps  2=Cs  3=Gs 4=Hs 5=Ds */
#ifndef PHOTON_FRAMETRACE_H
#define PHOTON_FRAMETRACE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct FrameState FrameState;

FrameState* ft_new(void);
void        ft_free(FrameState*);

void ft_register_view(FrameState*, uint64_t view, uint64_t resource, int kind);
void ft_set_shader_resources(FrameState*, int stage, uint32_t start, const uint64_t* views, size_t n);
void ft_set_unordered_access_views(FrameState*, uint32_t start, const uint64_t* views, size_t n);
void ft_set_render_targets(FrameState*, const uint64_t* rtvs, size_t n, uint64_t dsv);
void ft_draw(FrameState*);
void ft_dispatch(FrameState*);

size_t   ft_hazard_count(const FrameState*);
int      ft_hazard_kind(const FrameState*, size_t i); /* 0=ReadWrite 1=WriteWrite -1=none */
const char* ft_hazard_kind_name(const FrameState*, size_t i); /* "ReadWrite"|"WriteWrite"|"none" */
uint64_t ft_hazard_resource(const FrameState*, size_t i);

/* Restore-verify: diff a SAVED vs a RESTORED snapshot; nonzero leak count means
   the effect was not transparent to the host (slots it never set are corrupted). */
typedef struct Snapshot Snapshot;
Snapshot* ft_snapshot(const FrameState*);
void      ft_snapshot_free(Snapshot*);
size_t    ft_restore_leak_count(const Snapshot* saved, const Snapshot* restored);
size_t    ft_restore_first_leak(const Snapshot* saved, const Snapshot* restored, char* buf, size_t len);

/* Temporal (history ping-pong): declare the pair, call ft_present per frame. */
void   ft_present(FrameState*);
void   ft_declare_history_pair(FrameState*, uint64_t a, uint64_t b);
size_t ft_temporal_violation_count(const FrameState*);
size_t ft_temporal_witness(const FrameState*, size_t i, char* buf, size_t len);

/* Agent-facing gate: adjudicate a claim string against observed state.
   Returns 0=Confirmed 1=Contradicted 2=Unresolvable; writes the witness. */
int ft_adjudicate(const FrameState*, const char* claim, char* buf, size_t len);

#ifdef __cplusplus
}
#endif
#endif
