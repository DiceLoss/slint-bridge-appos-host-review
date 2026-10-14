#!/usr/bin/env python3
"""Analyze device-monotonic /proc samples and optional matched flip events."""
import argparse
import bisect
import csv
import json
from pathlib import Path
import statistics
import struct


def read_samples(path):
    with path.open() as f:
        meta = dict(item.split("=") for item in next(f).strip().split()[1:])
        rows = [{k: v if k == "phase" else int(v) for k, v in row.items()} for row in csv.DictReader(f)]
    assert all(a["mono_us"] < b["mono_us"] and a["cpu_ticks"] <= b["cpu_ticks"]
               for a, b in zip(rows, rows[1:]))
    return meta, rows


def read_flips(path, pid):
    data = path.read_bytes()
    magic, actual_pid, count, errors = struct.unpack_from("<IIII", data)
    if magic != 0x58394650 or actual_pid != pid or errors or count > 32768:
        raise ValueError(f"invalid flip probe: magic={magic:x} pid={actual_pid} count={count} errors={errors}")
    if len(data) != 16 + 32768 * 8:
        raise ValueError("truncated flip probe")
    values = struct.unpack_from(f"<{count}Q", data, 16)
    if any(b <= a for a, b in zip(values, values[1:])):
        raise ValueError("completion timestamps are not strictly increasing")
    return values


def summarize(rows, hz, seconds, flips=None):
    if len(rows) < 3:
        raise ValueError("insufficient resource samples")
    # Drop the first sample and cap the end inside the host action interval.
    # This also excludes setup after older phase markers left the prior label in place.
    cutoff = rows[0]["mono_us"] + int((seconds - 2) * 1e6)
    rows = [r for r in rows[1:] if r["mono_us"] <= cutoff]
    if len(rows) < 3:
        raise ValueError("insufficient interior samples")
    start, end = rows[0], rows[-1]
    elapsed = (end["mono_us"] - start["mono_us"]) / 1e6
    cpu = 100 * (end["cpu_ticks"] - start["cpu_ticks"]) / hz / elapsed
    per_sample = [100 * (b["cpu_ticks"] - a["cpu_ticks"]) / hz /
                  ((b["mono_us"] - a["mono_us"]) / 1e6) for a, b in zip(rows, rows[1:])]
    result = dict(seconds=elapsed, start_us=start["mono_us"], end_us=end["mono_us"], samples=len(rows),
                  cpu_percent_one_core=cpu, cpu_p95=statistics.quantiles(per_sample, n=100, method="inclusive")[94],
                  rss_mean_kib=statistics.mean(r["rss_kib"] for r in rows),
                  rss_min_kib=min(r["rss_kib"] for r in rows), rss_max_kib=max(r["rss_kib"] for r in rows),
                  process_hwm_kib=max(r["hwm_kib"] for r in rows),
                  major_faults=end["major_faults"] - start["major_faults"],
                  threads=sorted({r["threads"] for r in rows}),
                  cpu_khz=sorted({r["cpu_khz"] for r in rows}),
                  min_memfree_kib=min(r["free_kib"] for r in rows),
                  observer_cpu_percent=100 * (end["observer_ticks"] - start["observer_ticks"]) / hz / elapsed)
    if flips is not None:
        times = flips[bisect.bisect_right(flips, start["mono_us"]):bisect.bisect_right(flips, end["mono_us"])]
        result.update(completed_frames=len(times), fps=len(times) / elapsed)
        if len(times) >= 3:
            gaps = [(b - a) / 1000 for a, b in zip(times, times[1:])]
            result.update(frame_interval_median_ms=statistics.median(gaps),
                          frame_interval_p95_ms=statistics.quantiles(gaps, n=100, method="inclusive")[94],
                          frame_interval_max_ms=max(gaps))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=Path, required=True)
    parser.add_argument("--phases", type=Path, required=True)
    parser.add_argument("--flips", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    meta, rows = read_samples(args.samples)
    flips = read_flips(args.flips, int(meta["pid"])) if args.flips else None
    phases = json.loads(args.phases.read_text())
    results = {}
    for phase in phases:
        name = phase["phase"]
        if name.endswith("warmup"):
            continue
        values = [r for r in rows if r["phase"] == name]
        results[name] = summarize(values, int(meta["hz"]), phase["host_seconds"], flips)
        results[name]["gestures"] = phase["gestures"]
        if name in ("rich-vertical", "rich-horizontal"):
            assert results[name]["seconds"] >= 45
            assert phase["gestures"] >= 3
    output = dict(metadata=meta, cpu_basis="100% = one saturated logical CPU", phases=results)
    args.output.write_text(json.dumps(output, indent=2) + "\n")
    print(json.dumps(output, indent=2))


if __name__ == "__main__":
    main()
