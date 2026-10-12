#!/usr/bin/env python3
"""Validate one exclusive alpha-regression run's pixels, WBC and exit evidence."""
import argparse
import csv
import json
from pathlib import Path
import re


def check(directory):
    read = lambda name: (directory / name).read_text()
    assert int(read('rc')) == 0
    read('done')
    log = read('alpha.log')
    assert 'alpha: stages=5 result=0' in log
    assert not re.search(r'Error:|panicked|VO_Disable failed|MMZ release failed', log)
    stages = re.findall(r'alpha: stage=(\d+) offset=(\d+) visible=(true|false) page=([01]) passed=true', log)
    assert [(int(s), int(x), v) for s, x, v, _ in stages] == [
        (0, 4, 'true'), (1, 12, 'true'), (2, 8, 'false'), (3, 8, 'true'), (4, 4, 'false')]
    pages = [int(p) for _, _, _, p in stages]
    assert all(a != b for a, b in zip(pages, pages[1:])), 'pages did not alternate'
    pixels = re.findall(r'alpha: framebuffer page=([01]) pixels=(\d+) bad=(\d+)', log)
    assert len(pixels) == 5 and all(int(count) > 0 and int(bad) == 0 for _, count, bad in pixels)
    wbc = re.findall(r'alpha: wbc sample=(\d+) pts=(\d+) paired_points=600 max_delta=(\d+) bad=(\d+)', log)
    assert len(wbc) == 25 and all(int(delta) <= 2 and int(bad) == 0 for _, _, delta, bad in wbc)
    assert all(int(b[1]) > int(a[1]) for a, b in zip(wbc, wbc[1:])), 'WBC samples are stale'
    for stage in range(5):
        rows = list(csv.DictReader((directory / f'stage-{stage}-wbc.csv').open()))
        assert {(int(r['sample']), int(r['channel']), int(r['alpha_index'])) for r in rows} == {
            (sample, channel, alpha) for sample in range(5) for channel in range(3) for alpha in range(8)}
        assert len(rows) == 120
        for row in rows:
            assert all(abs(int(row[c]) - int(row[f'reference_{c}'])) <= 2 for c in ['Y', 'C0', 'C1'])
            if stage in [2, 4]:
                assert int(row['Y']) <= 1 and abs(int(row['C0']) - 128) <= 1 and abs(int(row['C1']) - 128) <= 1
            elif int(row['alpha_index']) == 7:
                assert int(row['Y']) >= [32, 100, 10][int(row['channel'])]
    summaries = [re.search(r'used=(\d+)KB.*block_number=(\d+)', read(name)).groups()
                 for name in ['initial.mmz', 'final.mmz']]
    assert summaries[0] == summaries[1]
    assert 'bridge_tde_' not in read('final.mmz') and 'vou-wbc' not in read('final.mmz')
    tde = 'slint-bridge: TDE enabled;' in log
    live_names = re.findall(r'name="(bridge_tde_[^"]+)"', read('live.mmz'))
    assert sorted(live_names) == (['bridge_tde_sourc', 'bridge_tde_sourc', 'bridge_tde_targe'] if tde else [])
    vo = read('final.vo')
    devices = vo.split('-----DEV CONFIG', 1)[1].split('-----VIDEO LAYER STATUS', 1)[0]
    assert not re.search(r'^\s*\d+\s+Y\b', devices, re.M), 'VO stayed enabled'
    wbc_status = vo.split('-----WBC INFO', 1)[1]
    assert not re.search(r'^\s*\d+\s+\d+\s+\d+', wbc_status, re.M), 'WBC stayed active'
    before = read('kernel-before'); after = read('kernel-after')
    marker = '\n'.join(before.splitlines()[-20:]) + '\n'
    at = after.find(marker); assert at >= 0 and marker.strip(), 'kernel history missing'
    delta = after[at + len(marker):]
    pid = int(read('pid'))
    assert f'MMB LEAK(pid={pid})' not in delta and 'Out of memory' not in delta
    return dict(passed=True, tde_enabled=tde, stages=5, physical_pixels=sum(int(n) for _, n, _ in pixels),
                pages=pages, wbc_samples=25, paired_points=15000,
                max_channel_delta=max(int(d) for _, _, d, _ in wbc), final_mmz_kib=int(summaries[1][0]))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    print(json.dumps(check(parser.parse_args().directory), indent=2))
