import importlib.util
from pathlib import Path
import unittest


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


analysis = load("analyze-showcase-performance")
profile = load("profile-showcase")


class EvidenceChecks(unittest.TestCase):
    def test_phase_boundary_and_missing_batch(self):
        before = "presented=150 fps=1.00 flip_avg_ms=34.0\n"
        after = before + "presented=300 fps=5.00 flip_avg_ms=34.0\npresented=450 fps=29.00 flip_avg_ms=34.0\n"
        self.assertEqual(profile.complete_batches(before, after), [(450, 29.0, 34.0)])
        with self.assertRaises(ValueError):
            profile.complete_batches(before, after.replace("presented=450", "presented=600"))

    def test_cpu_uses_elapsed_time_and_kernel_tick_rate(self):
        log = ("ready ticks_per_second=100\n"
               "sample mono_us=1000000 realtime_us=1000000 phase=list rss_kib=10 hwm_kib=12 available_kib=100 cpu_ticks=200\n"
               "sample mono_us=3000000 realtime_us=3000000 phase=list rss_kib=11 hwm_kib=12 available_kib=99 cpu_ticks=300\n")
        self.assertEqual(analysis.resources(log)["list"]["cpu_percent_one_core"]["median"], 50)

    def test_touch_uses_syn_report_and_rejects_incomplete_evidence(self):
        raw = ("event kernel_us=1000000 read_us=1000100 type=1 code=330 value=1\n"
               "event kernel_us=1000100 read_us=1000100 type=0 code=0 value=0\n"
               "event kernel_us=2000000 read_us=2000100 type=1 code=330 value=0\n"
               "event kernel_us=2000100 read_us=2000100 type=0 code=0 value=0\n")
        events = [dict(event={kind: dict(position={})}, timestampMs=str(stamp), result="Accepted")
                  for kind, stamp in [("pointerPressed", 1002), ("pointerReleased", 2003)]]
        result = analysis.touch(raw, dict(events=events))
        self.assertAlmostEqual(result["overall"]["maximum"], 2.9)
        for invalid in [dict(events=events[:-1]), dict(events=events, droppedCount="1"),
                        dict(events=events, unknownEventCount="1")]:
            with self.assertRaises(ValueError):
                analysis.touch(raw, invalid)
        with self.assertRaises(ValueError):
            analysis.touch("event kernel_us=1 read_us=1 type=0 code=3 value=0\n" + raw,
                           dict(events=events))


if __name__ == "__main__":
    unittest.main()
