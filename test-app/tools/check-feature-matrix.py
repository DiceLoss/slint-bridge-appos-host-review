#!/usr/bin/env python3
"""Check library/application board features and the public and private tests."""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def cargo(*args):
    return subprocess.run(["cargo", *args], cwd=ROOT, text=True, capture_output=True)


def main():
    source_check = subprocess.run(
        ["python3", str(ROOT / "test-app" / "tools" / "check-slint-source.py")],
        cwd=ROOT,
        text=True,
        capture_output=True,
    )
    if source_check.returncode:
        raise SystemExit(source_check.stdout + source_check.stderr)
    results = {}
    results["slint_source"] = source_check.stdout.strip()
    for feature in ("appos-host", "fbdev", "nx1", "nx5", "nx5-tde", "nx5-mma-background", "nx5-test-hooks", "f1", "f1-g2d"):
        scope = ("-p", "slint-bridge") if feature == "appos-host" else ("--workspace",)
        run = cargo("test", *scope, "--offline", "--locked", "--no-default-features", "--features", feature, "-j", "4")
        if run.returncode:
            raise SystemExit(run.stdout + run.stderr)
        results[feature] = "tests passed"
    for feature, expected in ((None, "select one board feature"),
                              ("appos-host,fbdev", "mutually exclusive"),
                              ("appos-host,nx1", "mutually exclusive"),
                              ("appos-host,nx5", "mutually exclusive"),
                              ("appos-host,f1", "mutually exclusive"),
                              ("fbdev,nx5", "mutually exclusive"),
                              ("fbdev,nx5-tde", "mutually exclusive"),
                              ("fbdev,nx5-mma-background", "mutually exclusive"),
                              ("f1,fbdev", "mutually exclusive"),
                              ("f1,nx5", "mutually exclusive"),
                              ("f1,nx5-tde", "mutually exclusive"),
                              ("f1,nx5-mma-background", "mutually exclusive"),
                              ("f1-g2d,fbdev", "mutually exclusive"),
                              ("f1-g2d,nx5-tde", "mutually exclusive"),
                              ("nx1,fbdev", "mutually exclusive"),
                              ("nx1,nx5", "mutually exclusive"),
                              ("nx1,nx5-tde", "mutually exclusive"),
                              ("nx1,nx5-mma-background", "mutually exclusive"),
                              ("nx1,f1", "mutually exclusive"),
                              ("nx1,f1-g2d", "mutually exclusive")):
        args = ["check", "--offline", "--locked", "--no-default-features", "--lib"]
        if feature:
            args += ["--features", feature]
        run = cargo(*args)
        if run.returncode == 0 or expected not in run.stderr:
            raise SystemExit(f"unexpected feature selection result: {feature}\n{run.stdout}{run.stderr}")
        results[feature or "none"] = "rejected as expected"
    tree = cargo("tree", "--offline", "--locked", "--no-default-features", "--features", "fbdev", "-e", "normal", "--prefix", "none")
    if tree.returncode:
        raise SystemExit(tree.stderr)
    forbidden = ("i-slint-backend-winit", "i-slint-backend-qt", "i-slint-backend-linuxkms",
                 "i-slint-renderer-skia", "i-slint-renderer-femtovg")
    if any(package in tree.stdout for package in forbidden):
        raise SystemExit("software-only feature unexpectedly includes a desktop/GPU backend")
    results["desktop_and_gpu_backends"] = "absent"
    print(json.dumps(results, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
