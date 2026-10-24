#!/usr/bin/env python3
"""Reject mixed or floating Slint source revisions in this workspace."""

import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
EXPECTED_GIT = "https://github.com/omni-stack-gen/slint-swash.git"
MANIFESTS = (ROOT / "Cargo.toml", ROOT / "test-app" / "Cargo.toml")


def slint_dependencies(manifest: Path):
    data = tomllib.loads(manifest.read_text())
    for section in ("dependencies", "build-dependencies", "dev-dependencies"):
        for name, dependency in data.get(section, {}).items():
            if name == "slint" or name == "slint-build" or name.startswith("i-slint-"):
                yield manifest, section, name, dependency


def main():
    references = list(slint_dependencies(MANIFESTS[0])) + list(slint_dependencies(MANIFESTS[1]))
    revisions = set()
    errors = []
    for manifest, section, name, dependency in references:
        if not isinstance(dependency, dict):
            errors.append(f"{manifest}:{section}.{name}: dependency must be an explicit table")
            continue
        if dependency.get("git") != EXPECTED_GIT:
            errors.append(f"{manifest}:{section}.{name}: unexpected git source")
        revision = dependency.get("rev")
        if not isinstance(revision, str) or len(revision) != 40:
            errors.append(f"{manifest}:{section}.{name}: exact 40-hex rev is required")
        else:
            revisions.add(revision)

    if len(revisions) != 1:
        errors.append(f"Slint dependencies use {len(revisions)} distinct revisions")

    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    lock_sources = {
        package["source"]
        for package in lock.get("package", [])
        if package["name"] == "slint" or package["name"] == "slint-build" or package["name"].startswith("i-slint-")
    }
    expected_source = f"git+{EXPECTED_GIT}?rev={next(iter(revisions), '')}#{next(iter(revisions), '')}"
    if lock_sources != {expected_source}:
        errors.append(f"Cargo.lock Slint sources differ from the manifest revision: {sorted(lock_sources)}")

    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Slint source consistency passed: {next(iter(revisions))}")


if __name__ == "__main__":
    main()
