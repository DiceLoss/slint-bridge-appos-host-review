#!/usr/bin/env python3
"""Check one recorded NX5 startup-regression case, including process teardown."""
import argparse
import json
from pathlib import Path
import re


def check(base, size=None):
    def read(suffix):
        return Path(str(base) + suffix).read_text()

    mode = base.name
    assert mode in ["show-error", "retry", "queued", "timer", "queued-retry"]
    read(".done")
    pid, code = int(read(".pid")), int(read(".rc"))
    assert code == (1 if mode == "show-error" else 0), code
    log = read(".log")
    assert "slint-bridge: TDE enabled;" in log
    assert not re.search(r"panicked|watchdog expired|VO_Disable failed|hide after|errors=[1-9]", log)
    initial, final = read(".initial.mmz"), read(".mmz")
    summaries = [re.search(r"used=(\d+)KB.*block_number=(\d+)", state).groups() for state in [initial, final]]
    assert summaries[0] == summaries[1], "MMZ did not return to its initial baseline"
    for state in [initial, final]:
        assert "bridge_tde_" not in state and 'name="vou-wbc"' not in state
    for stage in ["initialized", "rejected" if mode == "show-error" else "finished"]:
        state = Path(str(base) + f".state/{stage}.mmz").read_text()
        assert state.count('name="bridge_tde_targe"') == 1
        assert state.count('name="bridge_tde_sourc"') == 2
    final_vo = read(".vo")
    devices = final_vo.split("-----DEV CONFIG", 1)[1].split("-----VIDEO LAYER", 1)[0]
    wbc = final_vo.split("-----WBC INFO", 1)[1]
    assert not re.search(r"^\s*0\s", devices, re.M), "VO still enabled"
    assert not re.search(r"^\s*0\s", wbc, re.M), "WBC still active"
    marker = "\n".join(read(".kernel-before").splitlines()[-20:]) + "\n"
    after = read(".kernel")
    offset = after.find(marker)
    assert marker.strip() and offset >= 0, "kernel log has no pre-run overlap"
    delta = after[offset + len(marker):]
    assert f"MMB LEAK(pid={pid})" not in delta, delta
    rejected = log.count("startup: rejected visible=false error=window layout constraints")
    assert rejected == {"show-error": 1, "retry": 2, "queued-retry": 1}.get(mode, 0)
    dimensions = re.search(r"startup: framebuffer width=(\d+) height=(\d+) stride=(\d+)", log)
    if dimensions:
        width, height, stride = map(int, dimensions.groups())
        assert stride >= width * 4
        assert size is None or tuple(size) == (width, height)
    else:
        assert size is not None, "older logs require --size WIDTHxHEIGHT from the recorded device metadata"
        width, height = size
    assert width > 0 and height > 0
    pixels = re.findall(r"yoffset=(\d+) expected_bgra=(\[[^\]]+\]) pixels=(\d+) bad_pixels=(\d+)", log)
    previous = re.findall(r"startup: previous_bgra=(\[[^\]]+\])", log)
    assert len(pixels) == len(previous) == (0 if mode == "show-error" else 2 if mode == "retry" else 1)
    for (offset, expected, count, bad), old in zip(pixels, previous):
        assert int(offset) in [0, height] and int(count) == width * height and bad == "0"
        assert json.loads(old) != json.loads(expected), "stale frame could pass"
    if mode == "show-error":
        assert "Error: window layout constraints" in log
    elif mode == "retry":
        assert "startup: same_component_retry=pass queued_task=true" in log
        assert {p[0] for p in pixels} == {"0", str(height)}, "retry did not exercise both pages"
    else:
        assert log.index("enter_loop_without_window") < log.index("deferred_window_created") < log.index("expected_bgra=")
        assert f"startup: deferred_start=pass mode={mode}" in log
    return {"mode": mode, "pid": pid, "exit_code": code, "rejected_shows": rejected,
            "verified_frames": len(pixels), "bad_pixels": 0, "active_tde_blocks": 3,
            "width": width, "height": height, "final_mmz_kib": int(summaries[1][0]), "forced_reclaims": 0, "passed": True}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base", type=Path, help="case prefix, e.g. evidence/run/round1/retry")
    parser.add_argument("--size", help="WIDTHxHEIGHT for historical logs without recorded dimensions")
    args = parser.parse_args()
    size = tuple(map(int, args.size.lower().split("x"))) if args.size else None
    if size is not None and len(size) != 2:
        parser.error("--size requires WIDTHxHEIGHT")
    print(json.dumps(check(args.base, size), indent=2))
