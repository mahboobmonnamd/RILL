/* T-SPAWN required mutation. Linked only when packaging with
 * RILL_MUTATE=openpty_in_main_m. Never part of the shipping GUI, so
 * scripts/lint-planes.sh can keep forbidding openpty in host/.
 *
 * A constructor so main.m does not name a PTY primitive.
 */
#include <util.h>

__attribute__((constructor)) static void rill_mutate_openpty(void) {
    int master = -1, slave = -1;
    (void)openpty(&master, &slave, NULL, NULL, NULL);
}
