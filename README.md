# AppOS host backend review handoff

This temporary review repository exists because `omni-stack-gen/slint_bridge`
disables forks and the current contributor cannot push a review branch. It is
not a source authority and AppOS must not depend on it.

Apply the four patches in lexical order to `omni-stack-gen/slint_bridge` at
`main@4bbe0ac`:

```sh
git am 000*.patch
```

The resulting commit is `eb99482718a3ad8ddd7ad042090b21b79c697ca7`.
The feature matrix passed for `appos-host`, `fbdev`, `nx1`, `nx5`, `nx5-tde`,
`nx5-mma-background`, `nx5-test-hooks`, `f1`, and `f1-g2d`; twenty invalid
feature combinations were rejected. This is host and cross-build evidence,
not physical board qualification.

After upstream review and merge, AppOS must pin the immutable upstream merge
revision. This temporary repository can then be retired.
