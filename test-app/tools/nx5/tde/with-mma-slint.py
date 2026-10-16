#!/usr/bin/env python3
"""Run one validation Cargo command against a locally patched Slint checkout."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[4]
SOURCE = "http://192.168.253.238/wshl/slint-last.git"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--slint-root", type=Path, required=True)
    parser.add_argument("cargo_args", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.cargo_args
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("provide a Cargo subcommand after --")
    slint = args.slint_root.resolve()
    patch = ROOT / "test-app/devices/nx5/mma-background/slint-external-pixels.patch"
    subprocess.run(["git", "apply", "--reverse", "--check", str(patch)], cwd=slint, check=True)
    lock = ROOT / "Cargo.lock"
    original = lock.read_bytes()
    packages = {p["name"] for p in tomllib.loads(original.decode())["package"]
                if p.get("source", "").startswith("git+" + SOURCE)}
    manifests = subprocess.check_output(["git", "ls-files", "**/Cargo.toml"], cwd=slint, text=True)
    paths = {}
    for name in manifests.splitlines():
        manifest = slint / name
        package = tomllib.loads(manifest.read_text()).get("package", {}).get("name")
        if package in packages:
            paths[package] = manifest.parent
    if paths.keys() != packages:
        raise SystemExit(f"Missing local Slint packages: {packages - paths.keys()}")
    with tempfile.TemporaryDirectory(prefix="nx5-mma-slint-") as directory:
        config = Path(directory) / "patch.toml"
        config.write_text(f'[patch."{SOURCE}"]\n' + "\n".join(
            f"{name} = {{ path = {json.dumps(str(path))} }}" for name, path in sorted(paths.items())) + "\n")
        try:
            # Resolve only the local-source replacement, retaining locked versions.
            subprocess.run(["cargo", "metadata", "--offline", "--config", str(config),
                            "--format-version", "1"], cwd=ROOT, stdout=subprocess.DEVNULL, check=True)
            # clippy forwards configuration placed after its subcommand.
            result = subprocess.run(["cargo", command[0], "--config", str(config), *command[1:]], cwd=ROOT)
        finally:
            lock.write_bytes(original)
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
