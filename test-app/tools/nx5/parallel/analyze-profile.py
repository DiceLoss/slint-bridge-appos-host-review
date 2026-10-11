#!/usr/bin/env python3
"""Replay saved temporary probes; production code has no timing instrumentation."""
import argparse
import json
import re
import statistics
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("log", type=Path)
parser.add_argument("--require-overlap-ms", type=float, default=0)
args = parser.parse_args()
text = args.log.read_text()
events = sorted((int(t), stage) for t, stage in re.findall(
    r"\[DEBUG-parallel\] ns=(\d+) stage=(\w+)", text))
spans = {name: [] for name in ("render", "copy", "flip")}
started = {}
for time, stage in events:
    name, phase = stage.split("_")
    assert name in spans
    if phase == "begin":
        assert name not in started, f"concurrent {name} operations"
        started[name] = time
    else:
        assert phase == "end" and name in started
        spans[name].append((started.pop(name), time))
assert not started, "truncated timing log"
assert all(len(values) > 150 for values in spans.values())
assert all(not (a < d and b > c) for a, b in spans["copy"] for c, d in spans["flip"]), \
    "a framebuffer page was written while retirement was pending"
overlaps = [sum(max(0, min(b, d) - max(a, c)) for c, d in spans["flip"]) / 1e6
            for a, b in spans["render"][20:]]
fps = [float(v) for v in re.findall(r"presented=\d+ fps=([\d.]+)", text)]
assert len(fps) >= 3
assert not re.search(r"Error:|panicked|errors=[1-9]|TDE disabled|VO_Disable failed", text)
result = {
    "stage_samples": {name: len(values) for name, values in spans.items()},
    "warm_stage_mean_ms": {name: statistics.mean((b-a)/1e6 for a, b in values[20:])
                           for name, values in spans.items()},
    "warm_render_samples": len(overlaps),
    "renders_overlapping_flip": sum(v > 0 for v in overlaps),
    "mean_overlap_ms_per_render": statistics.mean(overlaps),
    "maximum_overlap_ms": max(overlaps),
    "framebuffer_copy_overlap_count": 0,
    "fps_batches": fps,
}
print(json.dumps(result, indent=2))
assert statistics.mean(overlaps) >= args.require_overlap_ms
