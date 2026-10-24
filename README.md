# AppOS host backend review handoff

This temporary review repository exists because `omni-stack-gen/slint_bridge`
disables forks and the current contributor cannot push a review branch. It is
not a source authority and AppOS must not depend on it.

Apply the six patches in lexical order to `omni-stack-gen/slint_bridge` at
`main@4bbe0ac`:

```sh
git am 000*.patch
```

The resulting source state includes the AppOS Activity-window reuse fix at
local commit `7d80ab2`. Do not assume that the upstream commit hash will match after
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

After upstream review and merge, AppOS must pin the immutable upstream merge
revision. This temporary repository can then be retired.
