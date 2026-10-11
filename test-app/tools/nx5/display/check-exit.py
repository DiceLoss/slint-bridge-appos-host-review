#!/usr/bin/env python3
"""Check one NX5 run-regression.sh case for active TDE use and clean exit."""
import argparse
import json
from pathlib import Path
import re


def check(base):
    def read(suffix):
        return Path(str(base) + suffix).read_text()

    pid = int(read(".pid"))
    code = int(read(".rc"))
    read(".done")
    log = read(".log")
    before = read(".kernel-before")
    after = read(".kernel")
    # Keep an overlapping tail to tolerate retirement of older ring-buffer
    # lines, but reject a wrap/clear that could hide any of this run's output.
    marker = "\n".join(before.splitlines()[-20:]) + "\n"
    offset = after.find(marker)
    assert marker.strip() and offset >= 0, "kernel log has no pre-run overlap"
    delta = after[offset + len(marker):]
    leaks = [line for line in delta.splitlines()
             if f"MMB LEAK(pid={pid})" in line]
    initial = read(".initial.mmz")
    live = read(".live.mmz")
    final = read(".mmz")
    names = re.findall(r'name="(bridge_tde_[^"]+)"', live)
    assert sorted(names) == ["bridge_tde_sourc", "bridge_tde_sourc", "bridge_tde_targe"], names
    for state in [initial, final]:
        assert "bridge_tde_" not in state and 'name="vou-wbc"' not in state, state
        assert "used=4900KB" in state and "block_number=2" in state, state
    assert not leaks, "kernel forced MMZ reclamation:\n" + "\n".join(leaks)
    assert "slint-bridge: TDE enabled;" in log
    assert not re.search(r"errors=[1-9]|VO_Disable failed|hide after event-loop failure", log), log
    fault = base.name == "fault"
    assert code == (1 if fault else 0), f"unexpected exit code {code}"
    if fault:
        injected = log.index("nx5-regression: injected evdev read EIO")
        observed = log.index("display: observation result=0")
        error = log.index("Error: read evdev input: Input/output error (os error 5)")
        assert injected < observed < error, "WBC must finish before the EIO is returned"
        assert 'name="vou-wbc"' in read(".before.mmz")
        assert re.search(r"^frame=\d+ ", log[injected:observed], re.M), "no WBC frames after EIO"
    return {"pid": pid, "exit_code": code, "active_tde_blocks": len(names),
            "forced_reclaims": leaks, "final_mmz_kib": 4900, "passed": True}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base", type=Path, help="case prefix, e.g. evidence/run/pattern")
    args = parser.parse_args()
    print(json.dumps(check(args.base), indent=2))
