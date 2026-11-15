/* Full-stack proof: a plain C program linking the Rust staticlib, driving the
   tracer through the C ABI exactly as a D3D11 hook would, reporting the
   canonical SSR read/write hazard. Built by c_demo/build.bat. */
#include "frametrace.h"
#include <stdio.h>

int main(void) {
    FrameState* fs = ft_new();

    /* SSR surface 0x5232: an SRV (view 1) and an RTV (view 2). */
    ft_register_view(fs, 1, 0x5232, 0); /* Srv */
    ft_register_view(fs, 2, 0x5232, 1); /* Rtv */

    uint64_t rtvs[1] = { 2 };
    ft_set_render_targets(fs, rtvs, 1, 0);

    uint64_t srvs[1] = { 1 };
    ft_set_shader_resources(fs, 1, 27, srvs, 1); /* PS t27 */

    ft_draw(fs);

    size_t n = ft_hazard_count(fs);
    printf("hazards at draw: %zu\n", n);
    for (size_t i = 0; i < n; i++) {
        printf("  kind=%d resource=0x%llx\n",
               ft_hazard_kind(fs, i),
               (unsigned long long)ft_hazard_resource(fs, i));
    }

    uint64_t nulls[1] = { 0 };
    ft_set_shader_resources(fs, 1, 27, nulls, 1);
    printf("hazards after unbind: %zu\n", ft_hazard_count(fs));

    ft_free(fs);
    return n == 1 ? 0 : 1;
}
