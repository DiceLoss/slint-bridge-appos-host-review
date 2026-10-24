# AppOS host backend review handoff

This temporary review repository exists because `omni-stack-gen/slint_bridge`
disables forks and the current contributor cannot push a review branch. It is
not a source authority and AppOS must not depend on it.

Apply the eight patches in lexical order to `omni-stack-gen/slint_bridge` at
`main@4bbe0ac`:

```sh
git am 000*.patch
```

The resulting source state includes the AppOS Activity-window retention fix at
local commit `60a0e1e`. Do not assume that the upstream commit hash will match after
`git am`; AppOS must pin the resulting official immutable revision.
The feature matrix passed for `appos-host`, `fbdev`, `nx1`, `nx5`, `nx5-tde`,
`nx5-mma-background`, `nx5-test-hooks`, `f1`, and `f1-g2d`; twenty invalid
feature combinations were rejected. This is host and cross-build evidence,
not physical board qualification.

The fifth patch fixes the first PLC NX5 canary failure: host fonts were
registered before Slint associated the renderer with its context. Registration
now occurs after `show()` and before the first render, with a dedicated
regression test. `appos-host` has 66 passing library tests after this patch.

The sixth patch fixes the second PLC NX5 canary failure: an Activity transition
may construct the incoming Slint root while the outgoing root is still alive.
Both roots now reuse the single physical fullscreen adapter, as they did on the
legacy AppOS backend. A regression test covers that transition ordering, and
`appos-host` has 67 passing library tests after this patch.

The seventh and eighth patches fix the PLC NX5 long-navigation failure. The
AppOS host keeps exactly one strong physical `WindowAdapter` for the process,
but releases the outgoing retained scene at every Activity boundary and trims
the allocator after the old component root is gone. The regression test drops
the outgoing component completely before it creates the incoming component and
then proves that both use the same adapter. The final AppOS-host suite has 67
passing library tests. On the PLC NX5, all 15 Activities passed two complete
rounds and 100 real evdev `main`/`phone-monitor` cycles completed without OOM;
RSS stabilized near 20.9 MiB, with 3 Shell threads and 8 Shell file descriptors.
This is narrowly scoped physical evidence for the tested PLC NX5 candidate, not
qualification for other boards.

After upstream review and merge, AppOS must pin the immutable upstream merge
revision. This temporary repository can then be retired.
