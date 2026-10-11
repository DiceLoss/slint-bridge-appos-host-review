#!/usr/bin/env python3
"""Assert the on-device demo's recorded frame rate and optional WBC sample."""
import argparse
import re
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('log', type=Path)
parser.add_argument('--min-fps', type=float, default=28)
parser.add_argument('--wbc', action='store_true')
args = parser.parse_args()
log = args.log.read_text()
fps = [float(n) for n in re.findall(r'presented=\d+ fps=([\d.]+)', log)]
assert fps, 'no completed presentation batch'
assert min(fps) >= args.min_fps, f'FPS below {args.min_fps}: {fps}'
assert not re.search(r'Error:|panicked|PresentationFailed|VO_Disable failed', log), 'runtime failure'
if args.wbc:
    summary = re.search(r'summary frames=(\d+) black=(\d+) white=(\d+) mixed=(\d+).*result=(\d+)', log)
    assert summary, 'missing WBC summary'
    frames, black, white, mixed, result = map(int, summary.groups())
    assert frames >= 300 and black and white and mixed == 0 and result == 0, summary.group()
    assert len(re.findall(r'^frame=', log, re.M)) == frames, 'incomplete sample log'
    timestamps = [int(n) for n in re.findall(r'^frame=\d+ pts=(\d+)', log, re.M)]
    capture_fps = (frames - 1) * 1_000_000 / (timestamps[-1] - timestamps[0])
    assert 28 <= capture_fps <= 32, f'WBC cadence {capture_fps:.2f} FPS'
print(f'PASS: {len(fps)} batches, FPS {min(fps):.2f}..{max(fps):.2f}' + ('; WBC clean' if args.wbc else ''))
