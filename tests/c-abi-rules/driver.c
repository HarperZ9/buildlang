/* The Low Water rules module through its C ABI, built from BuildLang
 * (examples/lowwater-rules). The checks restate raw-native's
 * tests/test_lowwater_c.cpp in C: a skill is its attribute plus training plus
 * worn-item mods; a check passes at total >= dc; a meter clamps to [min, max]
 * and queues its scene at max. The rules state is an opaque handle. */
#include <stdio.h>
#include <string.h>
#include "main.h"

static int failures = 0, checks = 0;
#define CHECK(c) do { checks++; if (!(c)) { failures++; printf("CHECK failed: %s (line %d)\n", #c, __LINE__); } } while (0)
#define S(x) ((uint8_t*)(x))
#define T(p) ((const char*)(p))

int main(void) {
    const char* lwd =
        "@rules\ncheck-mode: fixed\nattr-min: 1\n\n@attr pen\n\n@skill sum\nattr: pen\n\n@item hat\nslot: hat\nmod: skill.sum +1\n\n"
        "@archetype clerk\nattr: pen 3\nskill: sum 2\nitem: hat 1\n\n"
        "@meter cold\nmax: 10\nstart: 2\nfull: chilled\n\n@node chilled\nnar: Cold.\n";
    LwRules* r = lw_rules_create(S(lwd), 7); CHECK(r != NULL);
    CHECK(lw_rules_skill(r, S("sum")) == 1);
    CHECK(lw_rules_become(r, S("clerk")) == 0);
    CHECK(lw_rules_attr(r, S("pen")) == 3); CHECK(lw_rules_skill(r, S("sum")) == 6);
    uint8_t* label = NULL;
    CHECK(lw_rules_passive(r, S("sum"), 6, &label) == 1); CHECK(strcmp(T(label), "Trivial") == 0);
    CHECK(lw_rules_passive(r, S("sum"), 8, &label) == 0); CHECK(strcmp(T(label), "Easy") == 0);
    CHECK(lw_rules_become(r, S("nobody")) == -1);
    CHECK(strcmp(T(lw_rules_last_error()), "become: no archetype nobody") == 0);
    CHECK(lw_rules_meter(r, S("cold")) == 2);
    CHECK(lw_rules_meter_add(r, S("cold"), 99) == 0); CHECK(lw_rules_meter(r, S("cold")) == 10);
    uint8_t* id = NULL; uint8_t* node = lw_rules_take_full(r, &id);
    CHECK(node && strcmp(T(node), "chilled") == 0 && strcmp(T(id), "cold") == 0);
    CHECK(lw_rules_take_full(r, &id) == NULL);
    CHECK(lw_rules_meter_add(r, S("nope"), 1) == -1);
    lw_rules_destroy(r);

    /* A record that needs the story expression language is refused at create. */
    LwRules* bad = lw_rules_create(S("@attr pen\n\n@skill sum\nbase: pen * 2\n"), 1);
    CHECK(bad == NULL);
    CHECK(strstr(T(lw_rules_last_error()), "not ported") != NULL);

    printf("%d of %d checks passed\n", checks - failures, checks);
    return failures ? 1 : 0;
}
