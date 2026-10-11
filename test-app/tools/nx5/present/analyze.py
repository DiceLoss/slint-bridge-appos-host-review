#!/usr/bin/env python3
"""Replay diagnostic observations. These logs cannot certify physical scanout.

Exit 0: expected observations reproduced, including a failing negative control.
With --require-admission, exit 2 when observations pass but admission lacks proof.
Exit 1: missing/inconsistent evidence or an unexpected observation failure.
"""
import argparse
import json
import statistics
from pathlib import Path


HEX_FIELDS = {"cfg", "update", "work", "status", "mask", "raw", "addr", "base", "expected_addr"}


def records(text, kind):
    result = []
    for line in text.splitlines():
        if line.startswith(kind + " "):
            fields = {}
            for item in line.split()[1:]:
                if "=" not in item:
                    continue
                key, value = item.split("=", 1)
                fields[key] = int(value, 16 if key in HEX_FIELDS else 10)
            result.append(fields)
    return result


def timing(values):
    return {"min": min(values), "mean": statistics.mean(values), "max": max(values)}


def analyze_reuse(text):
    flips, frames = records(text, "reuse"), records(text, "capture")
    mode, result = records(text, "mode"), records(text, "result")
    if len(mode) != 1 or len(result) != 1 or not flips or len(frames) < 2:
        raise ValueError("incomplete reuse run")
    if [f["step"] for f in flips] != list(range(mode[0]["requested_flips"])):
        raise ValueError("missing/duplicate reuse steps")
    if [f["index"] for f in frames] != list(range(len(frames))):
        raise ValueError("missing/duplicate capture frames")
    if any(not (f["start"] <= f["returned"] <= f["poison_start"] < f["poison_end"]) for f in flips):
        raise ValueError("invalid reuse timestamp order")
    white = sum(f["min"] >= 254 for f in frames)
    black = sum(f["max"] <= 1 for f in frames)
    bad = sum(not (f["min"] >= 254 or f["max"] <= 1) for f in frames)
    if any(f["min"] > f["max"] or f["max"] > 255 or
           f["valid"] != int(f["min"] >= 254 or f["max"] <= 1) for f in frames):
        raise ValueError("inconsistent pixel classifications")
    r = result[0]
    if (r["frames"], r["white"], r["black"], r["mixed_or_poison"]) != (len(frames), white, black, bad):
        raise ValueError("summary disagrees with captured pixels")
    if r["capture_error"] or not white or not black:
        raise ValueError("capture unavailable or missing a displayed test color")
    if "disable rc=00000000" not in text or "close rc=00000000" not in text:
        raise ValueError("capture cleanup not confirmed")
    if "retire_restore pixels_equal=1" not in text:
        raise ValueError("pixel restoration not confirmed")
    intervals = [b["pts"] - a["pts"] for a, b in zip(frames, frames[1:])]
    if min(intervals) <= 0:
        raise ValueError("capture timestamps do not advance")
    observed_failure = bad > 0
    if r["failed"] != int(observed_failure):
        raise ValueError("run result disagrees with observations")
    return {
        "flips": len(flips), "captured_frames": len(frames), "white": white, "black": black,
        "mixed_or_poison": bad, "early_reuse_control": bool(mode[0]["early_reuse"]),
        "observation_passed": not observed_failure,
        "capture_period_us": timing(intervals),
        "capture_fps": 1_000_000 / statistics.mean(intervals),
        "return_to_poison_us": timing([(f["poison_start"] - f["returned"]) / 1000 for f in flips]),
        "flip_ms": timing([(f["returned"] - f["start"]) / 1_000_000 for f in flips]),
        "sampling": "all 600 rows, 5 columns; luma only; WBC enabled",
    }


def register_observations(text):
    first = text.splitlines()[0].replace("driver=fyfb ", "")
    meta = records(first, "device")[0]
    flips = records(text, "flip")
    returned = records(text, "returned")
    done = records(text, "done")
    regular = [f for f in flips if f["step"] >= 0]
    if len(done) != 1 or done[0]["failed"] or done[0]["interrupted"] or not done[0]["restored"] or done[0]["dropped"]:
        raise ValueError("register run did not finish cleanly")
    if [f["step"] for f in regular] != list(range(meta["flips"])) or len(flips) != len(returned):
        raise ValueError("register observations incomplete")
    for f, r in zip(flips, returned):
        if f["rc"] or f["read_rc"] or f["actual"] != f["requested"]:
            raise ValueError("page submission not acknowledged")
        expected = meta["base"] + f["requested"] * meta["stride"]
        if r["addr"] != expected or f["expected_addr"] != expected or r["update"] & 0x20:
            raise ValueError("post-return address/shadow check failed")
        if r["work"] & 0x700:
            raise ValueError("unexpected update mode")
    return {"flips": len(regular), "post_return_checks_passed": True,
            "irq_deltas": sorted({f["irq_after"] - f["irq_before"] for f in regular}),
            "flip_ms": timing([(f["end"] - f["start"]) / 1_000_000 for f in regular])}


def check_recovery(root, name, expected_exit):
    before = (root / f"{name}.pixels-before.sha256").read_text().split()[0]
    after = (root / f"{name}.pixels-after.sha256").read_text().split()[0]
    summary = (root / f"{name}.recovery.txt").read_text()
    fields = dict(item.split("=", 1) for item in summary.split()[1:])
    state = (root / f"{name}.owner-after.txt").read_text()
    process = dict(line.split(":", 1) for line in state.splitlines() if ":" in line)
    if before != after or fields["original_pan"] != fields["restored_pan"]:
        raise ValueError(f"{name}: restoration mismatch")
    if (int(fields["probe_exit"]) != expected_exit or process["State"].split()[0] not in {"S", "R"}
            or process["Pid"].strip() != fields["owner"] or process["Name"].strip() != "slint-demo"):
        raise ValueError(f"{name}: unexpected exit or stopped UI")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("--require-admission", action="store_true")
    args = parser.parse_args()
    try:
        result = {"registers": register_observations((args.evidence / "register-phases.log").read_text())}
        check_recovery(args.evidence, "register-phases", 0)
        for name in ("reuse-idle", "reuse-cpu", "early-control"):
            run = analyze_reuse((args.evidence / f"{name}.log").read_text())
            negative = name == "early-control"
            if run["early_reuse_control"] != negative or run["observation_passed"] == negative:
                raise ValueError(f"{name}: unexpected observation result")
            check_recovery(args.evidence, name, int(negative))
            result[name] = run
        result["tear_free_admission"] = "not_admitted"
        result["reason"] = "WBC samples about 30 fps of a 60 Hz output and only five columns; physical frames and DMA retirement contract remain unverified."
        result["observations_reproduced"] = True
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 2 if args.require_admission else 0
    except (OSError, ValueError, KeyError, IndexError) as exc:
        print(json.dumps({"observations_reproduced": False, "error": str(exc)}, ensure_ascii=False))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
