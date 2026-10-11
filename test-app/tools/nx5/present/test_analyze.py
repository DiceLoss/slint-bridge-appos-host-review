import json
import subprocess
import sys
import unittest
from pathlib import Path

import analyze

ROOT = Path(__file__).resolve().parents[4]
EVIDENCE = ROOT / "evidence/nx5-present/device-run"


class EvidenceTests(unittest.TestCase):
    def test_actual_negative_control_detects_active_page_corruption(self):
        result = analyze.analyze_reuse((EVIDENCE / "early-control.log").read_text())
        self.assertFalse(result["observation_passed"])
        self.assertEqual(result["mixed_or_poison"], 11)

    def test_observed_clean_run_does_not_hide_missing_physical_frames(self):
        result = analyze.analyze_reuse((EVIDENCE / "reuse-idle.log").read_text())
        self.assertTrue(result["observation_passed"])
        self.assertLess(result["capture_fps"], 32)

    def test_missing_capture_is_not_a_pass(self):
        text = (EVIDENCE / "reuse-idle.log").read_text()
        text = "\n".join(line for line in text.splitlines() if not line.startswith("capture index=5 "))
        with self.assertRaises(ValueError):
            analyze.analyze_reuse(text)

    def test_truncated_run_is_not_a_pass(self):
        text = (EVIDENCE / "reuse-idle.log").read_text().split("retire_restore")[0]
        with self.assertRaises(ValueError):
            analyze.analyze_reuse(text)

    def test_yoffset_match_does_not_override_pending_shadow_update(self):
        text = (EVIDENCE / "register-phases.log").read_text()
        lines = text.splitlines()
        i = next(i for i, line in enumerate(lines) if line.startswith("returned "))
        lines[i] = lines[i].replace("update=00000000", "update=00000020")
        with self.assertRaises(ValueError):
            analyze.register_observations("\n".join(lines))

    def test_replayed_evidence_cannot_admit_hardware(self):
        run = subprocess.run([sys.executable, str(ROOT / "test-app/tools/nx5/present/analyze.py"),
                              str(EVIDENCE), "--require-admission"], text=True, capture_output=True)
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertEqual(json.loads(run.stdout)["tear_free_admission"], "not_admitted")


if __name__ == "__main__":
    unittest.main()
