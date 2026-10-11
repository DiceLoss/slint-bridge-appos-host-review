#!/usr/bin/env python3
"""Replay completed frames, page correctness, WBC and fallback evidence."""
import argparse
import json
import re
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
root = args.directory
run = (root / "stability.log").read_text()
fps = [float(v) for v in re.findall(r"presented=\d+ fps=([\d.]+)", run)]
assert len(fps) >= 15
assert min(fps[1:]) >= 28.8 and max(fps) <= 30.5, fps
assert not re.search(r"Error:|panicked|errors=[1-9]|TDE disabled|VO_Disable failed", run)
stats = re.findall(r"tde frames=(\d+) copy=(\d+) blend=(\d+) software=(\d+) cache_hits=(\d+) errors=(\d+)", run)
assert stats and all(int(x[-1]) == 0 for x in stats)
assert all(int(stats[-1][i]) > 0 for i in range(5))
pages = re.findall(r"demo_frame=(\d+) yoffset=(\d+) max_error=(\d+) bad_channels=(\d+)",
                   (root / "pages.log").read_text())
assert len(pages) == 100
for i, (number, offset, error, bad) in enumerate(pages):
    assert int(number) == i and offset in ("0", "600") and int(error) <= 1 and bad == "0"
    assert not i or offset != pages[i - 1][1]
render = (root / "render-compare.log").read_text()
assert "test result: ok. 1 passed" in render
assert "correctness name=incremental-motion frames=80 max_channel_error=1" in render
assert "fallback injected_pre_submit_error=pass" in render
for name in ("small-image", "rotation-180", "transparent-destination"):
    assert f"fallback name={name} exact=true" in render
pattern = (root / "pattern.log").read_text()
summary = re.search(r"summary frames=(\d+) black=(\d+) white=(\d+) mixed=(\d+).*first_pts=(\d+) last_pts=(\d+) result=(\d+)", pattern)
assert summary
count, black, white, mixed, first, last, status = map(int, summary.groups())
assert count == 300 and black + white == count and min(black, white) > 0 and mixed == status == 0
wbc_fps = (count - 1) * 1e6 / (last - first)
assert 28 <= wbc_fps <= 32
control = (root / "mixed-control.log").read_text()
assert "summary frames=30 black=0 white=0 mixed=30" in control and "result=1" in control
profile = json.loads((root / "generic-profile.json").read_text())
assert profile["mean_overlap_ms_per_render"] >= 1
assert profile["framebuffer_copy_overlap_count"] == 0
before = [float(v) for v in re.findall(r"presented=\d+ fps=([\d.]+)", (root / "generic-before.log").read_text())]
assert len(before) >= 2 and min(profile["fps_batches"][1:]) > max(before[1:])
print(json.dumps({"completed_batches": len(fps), "first_batch_fps": fps[0],
                  "steady_fps_min": min(fps[1:]), "steady_fps_max": max(fps[1:]),
                  "last_tde_stats": list(map(int, stats[-1])), "double_page_frames": len(pages),
                  "wbc_samples": count, "wbc_mixed": mixed, "wbc_fps": wbc_fps,
                  "generic_serial_fps": before[1:], "generic_parallel_fps": profile["fps_batches"][1:],
                  "generic_mean_overlap_ms": profile["mean_overlap_ms_per_render"]}, indent=2))
