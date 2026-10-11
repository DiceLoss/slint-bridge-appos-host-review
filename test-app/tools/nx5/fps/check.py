#!/usr/bin/env python3
"""Replay full-scene FPS, pixel and fallback acceptance from saved NX5 logs."""
import argparse
import json
import re
from pathlib import Path
parser=argparse.ArgumentParser()
parser.add_argument('directory',type=Path)
args=parser.parse_args()
def read(name): return (args.directory/name).read_text()
run=read('stability.log')
fps=[float(x) for x in re.findall(r'presented=\d+ fps=([\d.]+)',run)]
assert len(fps)>=15,'need at least 2250 completed frames'
assert min(fps[1:])>=28.8 and max(fps)<=30.5, fps
assert not re.search(r'Error:|TDE disabled|panicked|VO_Disable failed',run)
stats=re.findall(r'tde frames=(\d+) copy=(\d+) blend=(\d+) software=(\d+) cache_hits=(\d+) errors=(\d+)',run)
assert stats and all(int(s[-1])==0 for s in stats)
assert int(stats[-1][1])>0 and int(stats[-1][2])>0 and int(stats[-1][3])>0
cadence=read('flip-cadence.log')
limit=float(re.search(r'max_fps=([\d.]+)',cadence)[1])
assert min(fps[1:])>=limit*.99
pages=read('pages.log')
rows=re.findall(r'demo_frame=(\d+) yoffset=(\d+) max_error=(\d+) bad_channels=(\d+)',pages)
assert len(rows)==100 and [int(r[0]) for r in rows]==list(range(100))
assert all(int(r[2])<=1 and int(r[3])==0 for r in rows)
assert all(rows[i][1]!=rows[i-1][1] for i in range(1,len(rows)))
render=read('render-compare.log')
assert 'test result: ok. 1 passed' in render
assert 'fallback injected_pre_submit_error=pass' in render
for name in ['small-image','rotation-180','transparent-destination']:
    assert f'fallback name={name} exact=true' in render
assert 'correctness name=incremental-motion frames=80 max_channel_error=1' in render
assert 'PASS pixel-conversion lengths=0..65 alignments=0..3 guards=unchanged' in read('native-neon-test.log')
assert 'test result: ok. 1 passed' in read('sparse.log')
print(json.dumps({'completed_frame_batches':len(fps),'first_batch_fps':fps[0],
    'steady_fps_min':min(fps[1:]),'steady_fps_max':max(fps[1:]),'blocking_flip_ceiling_fps':limit,
    'double_page_frames_verified':len(rows),'last_tde_stats':list(map(int,stats[-1]))},indent=2))
