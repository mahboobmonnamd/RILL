/* Permanent positive control for T-SPAWN.
 *
 * This binary deliberately does the thing the GUI must never do: it creates a
 * PTY and spawns a shell on it. The T-SPAWN check is run against this file and
 * MUST report a violation. If it comes back clean, the check is broken and the
 * gate fails — regardless of what it said about Rill.app.
 *
 * This is the control the original gate lacked. It used `nm -U`, which lists
 * *defined* symbols, while every symbol it asserted on can only ever be an
 * *undefined* import. The command excluded exactly the set the assertion
 * inspected, so it passed on any binary at all (docs/SPIKE-0-AUDIT.md S1-1).
 *
 * Never linked into anything shipped. Built only by the T-SPAWN test.
 */

#include <stdio.h>
#include <unistd.h>
#include <util.h>

int main(void) {
    int master = -1;
    pid_t pid = forkpty(&master, NULL, NULL, NULL);
    if (pid == 0) {
        execl("/bin/sh", "sh", (char *)NULL);
        _exit(127);
    }
    printf("spawner: master=%d child=%d\n", master, (int)pid);
    return 0;
}
