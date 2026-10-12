/* Host-only SDK lifetime model; no hardware access. */
#include <assert.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int references;
static int cached_fd = -1;
static void write_state(void) {
    FILE *state = fopen(getenv("SLINT_BRIDGE_TEST_VO_STATE"), "w");
    assert(state);
    fputs("-----DEV CONFIG\n", state);
    if (references) fputs("0 Y\n", state);
    fputs("-----VIDEO LAYER STATUS\n", state);
    fclose(state);
}
int VO_Enable(void) {
    /* Like the vendor SYS/VB cache: no unload destructor closes this fd. */
    if (cached_fd < 0) cached_fd = open("/dev/null", O_RDONLY);
    assert(cached_fd >= 0);
    ++references;
    write_state();
    return 1;
}
int VO_Disable(void) {
    if (references > 0) --references;
    write_state();
    return 1;
}
int test_vo_references(void) { return references; }
__attribute__((constructor)) static void initialize(void) {
    const char *automatic = getenv("SLINT_BRIDGE_TEST_AUTO_VO");
    if (automatic && !strcmp(automatic, "1")) VO_Enable();
    else write_state();
}
