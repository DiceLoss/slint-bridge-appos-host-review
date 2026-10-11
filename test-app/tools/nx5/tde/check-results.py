#!/usr/bin/env python3
"""Replay the recorded device comparison; never initializes hardware."""
import json
import re
from pathlib import Path

root = Path(__file__).resolve().parents[4] / 'evidence/nx5-tde'
log = (root / 'compare-final.log').read_text()
assert 'test result: ok. 1 passed' in log
cases = re.findall(r'correctness name=(\S+) max_channel_error=(\d+)', log)
assert len(cases) >= 9 and all(int(error) <= 1 for _, error in cases)
for name in ['odd-clip-fallback', 'opacity-fallback', 'scale-fallback', 'copy-on-write-cache']:
    assert (name, '0') in cases
for name in ['small-image', 'rotation-180', 'transparent-destination']:
    assert f'fallback name={name} exact=true' in log
assert 'fallback injected_pre_submit_error=pass' in log
assert 'correctness name=incremental-motion frames=80 max_channel_error=1' in log
pages = (root / 'demo-pages.log').read_text()
frames = re.findall(r'demo_frame=(\d+) yoffset=(\d+) max_error=(\d+) bad_channels=(\d+)', pages)
assert len(frames) == 100
assert [int(frame[0]) for frame in frames] == list(range(100))
assert all(int(frame[2]) <= 1 and int(frame[3]) == 0 for frame in frames)
assert all(frames[i][1] != frames[i-1][1] for i in range(1,100))
bench = [dict(name=n, hardware_ms=float(h), software_ms=float(s), speedup=float(r))
         for n, h, s, r in re.findall(r'benchmark name=(\S+) frames=60 hardware_ms=([\d.]+) software_ms=([\d.]+) speedup=([\d.]+)', log)]
assert len(bench) == 3 and all(row['speedup'] > 1.0 for row in bench)
assert len(re.findall(r'^PASS fault=', (root/'native-faults.log').read_text(), re.M)) == 6
print(json.dumps({'correctness': dict(cases), 'benchmark': bench, 'fallback': 'passed'}, indent=2))
