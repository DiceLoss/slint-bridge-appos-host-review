#!/usr/bin/env python3
"""Check the matched #233 two-refresh flip cadence against saved observations."""
import argparse
import hashlib
import json
import re
import statistics
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
parser.add_argument("--min-fps", type=float)
args = parser.parse_args()
root = args.directory
tables = json.loads((root / "driver-tables.json").read_text())
for name, digest in tables["module_sha256"].items():
    assert hashlib.sha256((root / f"{name}.ko").read_bytes()).hexdigest() == digest

text = (root / "precise.log").read_text()
assert "done failed=0 dropped=0 restored=1" in text
registers = {int(a, 16): int(b, 16) for a, b in re.findall(
    r"reg offset=(\w+) value=(\w+)", text)}
htotal = (registers[0x28] & 0x1fff) + 1
vtotal = (registers[0x2c] & 0xfff) + 1
assert (htotal, vtotal) == (1344, 635)
clock_register = int(tables["clock_register_observed"], 16) & 0xfffff
assert tables["clock_register_observed"].lower() in (root / "clock-readback.txt").read_text().lower()
pixel_clock = next(c["hz"] for c in tables["clock_table"] if int(c["register"], 16) == clock_register)
requested_clock = htotal * vtotal * 60
assert pixel_clock == min(tables["clock_table"], key=lambda c: abs(c["hz"] - requested_clock))["hz"]
expected_period = htotal * vtotal / pixel_clock * 1000

rows = re.findall(
    r"flip index=(\d+) pause_us=(\d+) start=(\d+) end=(\d+) offset=(\d+) "
    r"rc=(-?\d+) read_rc=(-?\d+) actual=(\d+) update=(\w+) addr=(\w+)", text)
assert len(rows) == 256
flips = []
for i, row in enumerate(rows):
    index, pause, start, end, offset, rc, read_rc, actual = map(int, row[:8])
    assert index == i and rc == read_rc == 0 and actual == offset
    assert offset in (0, 600) and (not i or offset != flips[-1][3])
    assert int(row[8], 16) & 32 == 0
    assert int(row[9], 16) == 0x46019000 + offset * 4096
    flips.append((pause, start, end, offset))

samples = [tuple(map(int, row)) for row in re.findall(r"sample ns=(\d+) irq=(\d+)", text)]
interrupts = [b for a, b in zip(samples, samples[1:]) if b[1] != a[1]]
irq_hz = (interrupts[-1][1] - interrupts[0][1]) * 1e9 / (interrupts[-1][0] - interrupts[0][0])
assert abs(irq_hz - 1000 / expected_period) < .1

groups = {}
for name, lo, hi, periods in [("no_preparation", 8, 128, 2), ("prepare_8ms", 128, 160, 2),
                             ("prepare_15ms", 160, 192, 2), ("sweep_0_to_17ms", 192, 227, 2),
                             ("sweep_17_5_to_31_5ms", 227, 256, 3)]:
    intervals = [(flips[i][2] - flips[i - 1][2]) / 1e6 for i in range(lo, hi)]
    assert all(abs(t - periods * expected_period) < .5 for t in intervals), name
    groups[name] = {"samples": len(intervals), "mean_interval_ms": statistics.mean(intervals),
                    "min_interval_ms": min(intervals), "max_interval_ms": max(intervals)}
fps = 1000 / groups["no_preparation"]["mean_interval_ms"]
result = {"flips_verified": len(flips), "htotal": htotal, "vtotal": vtotal,
          "requested_pixel_clock_hz": requested_clock, "selected_pixel_clock_hz": pixel_clock,
          "calculated_refresh_hz": 1000 / expected_period, "observed_irq_hz": irq_hz,
          "blocking_flip_fps": fps, "groups": groups}
print(json.dumps(result, indent=2))
if args.min_fps is not None and fps < args.min_fps:
    raise SystemExit(f"FAIL: blocking flip {fps:.3f} FPS < required {args.min_fps:.3f} FPS")
