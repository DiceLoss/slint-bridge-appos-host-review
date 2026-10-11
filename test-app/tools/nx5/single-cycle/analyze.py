#!/usr/bin/env python3
"""Replay matched VOU observations; optionally require single-cycle throughput."""
import argparse
import json
import re
import statistics
from pathlib import Path

HEX_FIELDS = {'base', 'requested', 'address', 'active', 'pending', 'update', 'flags', 'bss', 'lld', 'ufl'}


def fields(line):
    values = {}
    for key, value in re.findall(r'(\w+)=(\w+)', line):
        values[key] = value if key == 'read_mode' else int(value, 16 if key in HEX_FIELDS else 10)
    return values


def distribution(values):
    return {'mean': statistics.mean(values), 'min': min(values), 'max': max(values)}


def analyze(path):
    lines = path.read_text().splitlines()
    layout = [fields(line) for line in lines if line.startswith('layout ')]
    assert len(layout) == 1 and layout[0].get('read_mode') == 'kmem_cached', 'live kernel data must use its cached mapping'
    layout = layout[0]
    assert layout['page_bytes'] == 4096 * 600 and layout['original'] in (0, 600), 'unexpected framebuffer layout'
    flips = [fields(line) for line in lines if line.startswith('flip ')]
    samples = [fields(line) for line in lines if line.startswith('sample ')]
    summary = [fields(line) for line in lines if line.startswith('done ')]
    assert len(summary) == 1 and summary[0] == {'failed': 0, 'dropped': 0, 'restored': 1, 'samples': len(samples)}, 'probe failed, lost samples or did not restore the original page'
    assert [f['index'] for f in flips] == list(range(128)), 'missing or duplicate flips'
    assert all(a['ns'] < b['ns'] and a['irq'] <= b['irq'] for a, b in zip(samples, samples[1:])), 'non-monotonic sampling'
    page_addresses = {layout['base'], layout['base'] + layout['page_bytes']}
    stable = [s for s in samples if s['consistent']]
    for s in stable:
        assert s['active'] in page_addresses and s['address'] in page_addresses, 'unknown display address'
        assert s['pending'] in page_addresses | {0}, 'unknown pending address'
        assert s['queued'] in (0, 1), 'more than one submitted frame'
    transitions = []
    offset = layout['original']
    for flip in flips:
        offset = 600 - offset
        assert flip['rc'] == 0 and flip['actual'] == offset, 'failed or incorrect PAN'
        assert flip['requested'] == layout['base'] + offset * 4096, 'pages did not alternate'
        stage = next((s for s in stable if flip['start'] < s['ns'] < flip['end']
                      and s['pending'] == flip['requested'] and s['address'] == flip['requested']), None)
        assert stage is not None, 'staging phase not observed'
        assert stage['active'] != flip['requested'] and stage['queued'] == 0 and stage['update'] & 0x20, 'inconsistent staged/active state'
        active = next((s for s in stable if stage['ns'] < s['ns'] < flip['end'] + 2_000_000
                       and s['active'] == flip['requested'] and s['pending'] == 0 and not s['update'] & 0x20), None)
        assert active is not None, 'completed frame not observed near PAN return'
        transitions.append({
            'index': flip['index'], 'pause_us': flip['pause_us'],
            'request_to_stage_ms': (stage['ns'] - flip['start']) / 1e6,
            'request_to_active_ms': (active['ns'] - flip['start']) / 1e6,
            'stage_to_active_ms': (active['ns'] - stage['ns']) / 1e6,
            'return_minus_first_active_observation_ms': (flip['end'] - active['ns']) / 1e6,
        })
    # Skip eight warm-up flips; the first 64 have no intentional preparation delay.
    steady = transitions[8:64]
    intervals = [(flips[i]['end'] - flips[i-1]['end']) / 1e6 for i in range(8, 64)]
    result = {
        'flips_verified': len(flips), 'samples': len(samples),
        'inconsistent_samples_discarded': len(samples) - len(stable),
        'old_page_still_active_at_staging': len(transitions), 'restored': True,
        'steady_completion_interval_ms': distribution(intervals),
        'steady_fps': 1000 / statistics.mean(intervals),
        'no_preparation': {key: distribution([s[key] for s in steady]) for key in
            ('request_to_stage_ms', 'request_to_active_ms', 'stage_to_active_ms',
             'return_minus_first_active_observation_ms')},
        'sampling_note': 'Timestamps mark userspace observations, not exact IRQ/latch time; cached driver state is not an optical scanout proof.',
        'transitions': transitions,
    }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('log', type=Path)
    parser.add_argument('--min-fps', type=float)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = analyze(args.log)
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + '\n')
    summary = {key: value for key, value in result.items() if key != 'transitions'}
    print(json.dumps(summary, indent=2))
    if args.min_fps is not None and result['steady_fps'] < args.min_fps:
        print(f"FAIL: {result['steady_fps']:.3f} FPS < required {args.min_fps:.3f} FPS")
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
