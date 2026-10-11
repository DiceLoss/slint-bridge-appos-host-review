#!/usr/bin/env python3
"""Summarize the temporary phase timing probe from an on-device texture run."""
import argparse
import json
import re
import statistics
from pathlib import Path
parser=argparse.ArgumentParser()
parser.add_argument('log',type=Path)
parser.add_argument('--warmup',type=int,default=20)
args=parser.parse_args()
text=args.log.read_text()
frames=[dict(zip(['frame','render','copy','present','pixels'],map(int,row))) for row in re.findall(r'\[DEBUG-fps\] frame=(\d+) render_us=(\d+) copy_us=(\d+) present_us=(\d+) pixels=(\d+)',text)]
loops=[dict(zip(['frame','interval','update','wait'],map(int,row))) for row in re.findall(r'\[DEBUG-loop\] frame=(\d+) interval_us=(\d+) update_us=(\d+) wait_us=(\d+)',text)]
frames=[f for f in frames if f['frame']>args.warmup]
loops=[f for f in loops if f['frame']>args.warmup]
assert len(frames)>=100 and len(loops)>=100,'insufficient completed samples'
def summarize(values):
    ordered=sorted(values)
    return {'mean_ms':statistics.mean(values)/1000,'p50_ms':statistics.median(values)/1000,'p95_ms':ordered[int((len(ordered)-1)*.95)]/1000,'max_ms':max(values)/1000}
result={'frames':len(frames),'phases':{key:summarize([f[key] for f in frames]) for key in ['render','copy','present']},'loop':{key:summarize([f[key] for f in loops]) for key in ['interval','update','wait']},'mean_written_pixels':statistics.mean(f['pixels'] for f in frames)}
print(json.dumps(result,indent=2))
