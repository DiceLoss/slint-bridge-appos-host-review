"""Exercise preparation against a local Git fixture without network access."""
import hashlib
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("prepare_slint", ROOT / "tools/prepare-slint.py")
prepare_slint = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(prepare_slint)


class PreparationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "consumer with spaces"
        self.root.mkdir()
        upstream = Path(self.temporary.name) / "upstream"
        upstream.mkdir()
        g = lambda *args: prepare_slint.git(upstream, *args)
        g("init", "--quiet")
        g("config", "user.name", "Fixture")
        g("config", "user.email", "fixture@example.invalid")
        (upstream / "Cargo.toml").write_text('[package]\nname = "fixture"\nversion = "1.18.1"\n')
        (upstream / "lib.rs").write_text("pub const VALUE: u8 = 1;\n")
        g("add", ".")
        g("commit", "--quiet", "-m", "Fixture")
        commit, tree = g("rev-parse", "HEAD"), g("rev-parse", "HEAD^{tree}")
        (upstream / "lib.rs").write_text("pub const VALUE: u8 = 2;\n")
        diff = subprocess.check_output(["git", "-C", str(upstream), "diff", "--binary", "--full-index"])
        g("add", ".")
        patched_tree = g("write-tree")
        bundle = self.root / "patches/slint"
        bundle.mkdir(parents=True)
        (bundle / "fixture.patch").write_bytes(diff)
        self.manifest = bundle / "manifest.json"
        self.spec = dict(version="1.18.1", upstream=dict(url=str(upstream), commit=commit, tree=tree),
                         patch="fixture.patch", patch_sha256=hashlib.sha256(diff).hexdigest(),
                         patched_tree=patched_tree, packages={"fixture": "."})
        self.save_spec()

    def save_spec(self):
        self.manifest.write_text(json.dumps(self.spec))

    def test_fresh_reuse_offline_and_external_config(self):
        config = prepare_slint.prepare(self.root)
        source = self.root / ".slint/source"
        self.assertIn("= 2", (source / "lib.rs").read_text())
        expected = tomllib.loads(config.read_text())["patch"]["crates-io"]["fixture"]["path"]
        self.assertEqual(Path(expected), source)
        before = config.read_bytes()
        original = prepare_slint.git
        def no_fetch(directory, *args):
            self.assertNotIn("fetch", args)
            return original(directory, *args)
        with patch.object(prepare_slint, "git", side_effect=no_fetch):
            self.assertEqual(prepare_slint.prepare(self.root, offline=True), config)
            prepare_slint.prepare(self.root)
        self.assertEqual(config.read_bytes(), before)

    def test_offline_without_source_fails(self):
        with self.assertRaisesRegex(RuntimeError, "No prepared"):
            prepare_slint.prepare(self.root, offline=True)

    def test_concurrent_preparation_publishes_once(self):
        with ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(lambda _: prepare_slint.prepare(self.root), range(2)))
        self.assertEqual(results[0], results[1])
        prepare_slint.verify(self.root / ".slint/source", self.spec)

    def test_staged_and_untracked_changes_are_rejected(self):
        prepare_slint.prepare(self.root)
        source = self.root / ".slint/source"
        file = source / "extra.rs"
        file.write_text("local source\n")
        with self.assertRaisesRegex(RuntimeError, "local changes"):
            prepare_slint.prepare(self.root, offline=True)
        prepare_slint.git(source, "add", "extra.rs")
        with self.assertRaisesRegex(RuntimeError, "tree"):
            prepare_slint.prepare(self.root, offline=True)
        self.assertEqual(file.read_text(), "local source\n")

    def test_bad_patch_and_wrong_tree_do_not_publish(self):
        self.spec["patch_sha256"] = "0" * 64
        self.save_spec()
        with self.assertRaisesRegex(RuntimeError, "checksum"):
            prepare_slint.prepare(self.root)
        diff = (self.manifest.parent / "fixture.patch").read_bytes()
        self.spec["patch_sha256"] = hashlib.sha256(diff).hexdigest()
        self.spec["patched_tree"] = "0" * 40
        self.save_spec()
        with self.assertRaisesRegex(RuntimeError, "tree"):
            prepare_slint.prepare(self.root)
        self.assertFalse((self.root / ".slint/source").exists())
        self.assertFalse(list((self.root / ".slint").glob("prepare-*")))

    def test_modified_source_is_preserved_and_rejected(self):
        prepare_slint.prepare(self.root)
        file = self.root / ".slint/source/lib.rs"
        file.write_text("local changes\n")
        with self.assertRaisesRegex(RuntimeError, "local changes"):
            prepare_slint.prepare(self.root, offline=True)
        self.assertEqual(file.read_text(), "local changes\n")

    def test_wrong_official_tree_is_rejected(self):
        self.spec["upstream"]["tree"] = "0" * 40
        self.save_spec()
        with self.assertRaisesRegex(RuntimeError, "Official"):
            prepare_slint.prepare(self.root)
        self.assertFalse((self.root / ".slint/source").exists())


class RepositoryConfigurationTests(unittest.TestCase):
    def test_manifest_and_tracked_cargo_overrides_agree(self):
        spec = json.loads((ROOT / "patches/slint/manifest.json").read_text())
        config = tomllib.loads((ROOT / ".cargo/config.toml").read_text())
        self.assertEqual(config["patch"]["crates-io"],
                         {name: {"path": f".slint/source/{path}"} for name, path in spec["packages"].items()})
        diff = (ROOT / "patches/slint" / spec["patch"]).read_bytes()
        self.assertEqual(hashlib.sha256(diff).hexdigest(), spec["patch_sha256"])


if __name__ == "__main__":
    unittest.main()
