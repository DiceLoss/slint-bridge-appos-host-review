import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("analyze", Path(__file__).with_name("analyze-performance.py"))
analyze = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analyze)


class PerformanceAnalysis(unittest.TestCase):
    def test_single_core_cpu_and_matching_window_frames(self):
        rows = [dict(mono_us=i*1000000, cpu_ticks=i*100, rss_kib=2048, hwm_kib=4096,
                     threads=3, major_faults=0, cpu_khz=912000, free_kib=64000, shmem_kib=32000,
                     observer_ticks=0) for i in range(10)]
        flips = tuple(i*200000 for i in range(46))
        result = analyze.summarize(rows, 100, 9, flips)
        self.assertEqual(result["cpu_percent_one_core"], 100)
        self.assertEqual(result["seconds"], 6)
        self.assertEqual(result["completed_frames"], 30)
        self.assertEqual(result["fps"], 5)
        self.assertEqual(result["frame_interval_p95_ms"], 200)
        self.assertEqual(result["rss_mean_kib"], 2048)
        self.assertEqual(analyze.summarize(rows, 100, 9, ())['fps'], 0)

    def test_rejects_wrong_process_errors_truncation_and_reordered_events(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "flips.bin"
            data = bytearray(16 + 32768*8)
            struct.pack_into('<IIIIQQ', data, 0, 0x58394650, 42, 2, 0, 100, 200)
            path.write_bytes(data)
            self.assertEqual(analyze.read_flips(path, 42), (100, 200))
            with self.assertRaises(ValueError): analyze.read_flips(path, 43)
            struct.pack_into('<I', data, 12, 1)
            path.write_bytes(data)
            with self.assertRaises(ValueError): analyze.read_flips(path, 42)
            struct.pack_into('<I', data, 12, 0)
            struct.pack_into('<Q', data, 24, 100)
            path.write_bytes(data)
            with self.assertRaises(ValueError): analyze.read_flips(path, 42)
            path.write_bytes(data[:20])
            with self.assertRaises(ValueError): analyze.read_flips(path, 42)


if __name__ == '__main__':
    unittest.main()
