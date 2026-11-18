/* Reports how the C compiler built this translation unit: 1 when gcc or clang
 * optimised it (__OPTIMIZE__), 0 when it did not, -1 for a compiler that does
 * not say (MSVC). */
int bl_opt_state(void) {
#if defined(__GNUC__) || defined(__clang__)
#ifdef __OPTIMIZE__
    return 1;
#else
    return 0;
#endif
#else
    return -1;
#endif
}
