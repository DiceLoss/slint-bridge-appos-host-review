#!/usr/bin/env python3
"""Replay NX5 scheduler timing, completed-frame throughput and output checks."""
import argparse
import json
import re
import statistics
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
root = args.directory

def frames(name, batches):
    text = (root / name).read_text()
    fps = [float(v) for v in re.findall(r"presented=\d+ fps=([\d.]+)", text)]
    assert len(fps) >= batches, name
    assert min(fps[1:]) >= 28.8 and max(fps) <= 30.5, (name, fps)
    assert not re.search(r"Error:|panicked|errors=[1-9]|TDE disabled|VO_Disable failed", text)
    stats = re.findall(r"tde frames=(\d+) copy=(\d+) blend=(\d+) software=(\d+) cache_hits=(\d+) errors=(\d+)", text)
    assert stats and all(int(v) > 0 for v in stats[-1][:5]) and int(stats[-1][-1]) == 0
    return fps

generic = frames("generic-stability.log", 15)
optimized = frames("optimized-stability.log", 15)
pilot = frames("after.log", 5)
before = [float(v) for v in re.findall(r"presented=\d+ fps=([\d.]+)", (root / "before.log").read_text())]
assert len(before) >= 5 and max(before[1:]) < 28
ticks = int(re.search(r"clock_ticks=(\d+)", (root / "clock-ticks.txt").read_text())[1])

cpu = {}
for mode in ("before", "after"):
    lines = (root / f"cpu-{mode}.log").read_text().splitlines()
    assert len(lines) == 10
    snapshots = []
    for index in range(0, len(lines), 2):
        uptime = float(lines[index].split()[0])
        fields = lines[index + 1].split(") ", 1)[1].split()
        snapshots.append((uptime, int(fields[11]) + int(fields[12])))
    start, end = snapshots[1], snapshots[-1]
    elapsed = end[0] - start[0]
    assert elapsed >= 15
    cpu[mode] = (end[1] - start[1]) / ticks / elapsed * 100

medians = {}
for file in ("wait-probe.log", "wait-hybrid.log"):
    for mode in range(5):
        values = [float(v) for v in re.findall(
            rf"mode={mode} requested_us=33334 sample=\d+ elapsed_us=([\d.]+)",
            (root / file).read_text())]
        if values:
            assert len(values) == 12
            medians[mode] = statistics.median(values)
assert len(medians) == 5
assert all(39000 <= medians[m] <= 41000 for m in range(4))
assert 33334 <= medians[4] < 33500
assert ".resolution: 10000000 nsecs" in (root / "timer-resolution.txt").read_text()

pattern = (root / "pattern.log").read_text()
match = re.search(r"summary frames=(\d+) black=(\d+) white=(\d+) mixed=(\d+).*first_pts=(\d+) last_pts=(\d+) result=(\d+)", pattern)
assert match
count, black, white, mixed, first, last, status = map(int, match.groups())
assert count == 300 and black + white == count and min(black, white) > 0 and mixed == status == 0
assert len(re.findall(r"^frame=", pattern, re.M)) == count
wbc_fps = (count - 1) * 1e6 / (last - first)
assert 28 <= wbc_fps <= 32
control = (root / "mixed-control.log").read_text()
assert "summary frames=30 black=0 white=0 mixed=30" in control and "result=1" in control
print(json.dumps({"before_fps": before, "pilot_fps": pilot,
                  "generic_90s_fps": generic, "optimized_90s_fps": optimized,
                  "cpu_percent_one_core": cpu,
                  "wait_33334us_median_us": medians,
                  "wbc_frames": count, "wbc_mixed": mixed, "wbc_fps": wbc_fps}, indent=2))
