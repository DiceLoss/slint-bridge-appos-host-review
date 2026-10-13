#!/usr/bin/env python3
"""Profile an already verified NX5 showcase with trace enabled; never screenshots."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import sys
import time

TOOLS = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(TOOLS / "nx5"))
from deploy import Adb, managed_path  # noqa: E402

spec = importlib.util.spec_from_file_location("showcase_check", TOOLS / "check-showcase.py")
ui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ui)

BATCH = re.compile(r"presented=(\d+) fps=([\d.]+) flip_avg_ms=([\d.]+)")


def complete_batches(before, after):
    old = [int(row[0]) for row in BATCH.findall(before)]
    last = max(old, default=0)
    new = [(int(count), float(fps), float(flip)) for count, fps, flip in BATCH.findall(after)
           if int(count) > last]
    if any(fps <= 0 or flip < 0 for _, fps, flip in new):
        raise ValueError("Invalid presentation timing")
    if any(b[0] - a[0] != 150 for a, b in zip(new, new[1:])):
        raise ValueError("Missing or reordered completed-presentation batches")
    # The first batch straddles the phase start; its idle/setup time is not a sample.
    return new[1:]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--remote", required=True, help="directory with trace.log and phase")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--port", type=int, default=19315)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    adb = Adb(args.serial)
    log_path = managed_path(args.remote + "/trace.log")
    phase_path = managed_path(args.remote + "/phase")
    client = ui.Client(args.port)
    client.window, = client.data("list_windows")["windowHandles"]
    client.root = client.data("get_window_properties", windowHandle=client.window)["rootElementHandle"]
    results = []

    def mark(name):
        adb.shell("sh", "-c", 'printf "%s\\n" "$1" > "$2"', "phase", name, phase_path)

    def run(name, seconds, action=None, measure=True):
        mark(name)
        before = adb.read(log_path)
        began = time.monotonic()
        operations = 0
        while time.monotonic() - began < seconds:
            if action:
                action(operations)
                operations += 1
            else:
                time.sleep(0.1)
        elapsed = time.monotonic() - began
        after = adb.read(log_path)
        args.output.joinpath(name + ".before.log").write_text(before)
        args.output.joinpath(name + ".after.log").write_text(after)
        batches = complete_batches(before, after)
        result = dict(phase=name, elapsed_seconds=elapsed, operations=operations,
                      complete_150_frame_batches=batches)
        if measure:
            if len(batches) < 4:
                raise RuntimeError(f"Insufficient completed-presentation samples in {name}: {batches}")
            result.update(fps= len(batches) / sum(1 / row[1] for row in batches),
                          fps_min=min(row[1] for row in batches), fps_max=max(row[1] for row in batches),
                          frames=150 * len(batches))
        results.append(result)
        args.output.joinpath("phases.json").write_text(json.dumps(results, indent=2) + "\n")
        print(json.dumps(result), flush=True)

    client.click("Showcase::nav-controls")
    run("idle", 20, measure=False)
    client.click("Showcase::nav-motion")
    if any(e.get("accessibleLabel") == "Resume motion" for e in client.tree()):
        client.click("Motion::motion-toggle")
    run("motion-warmup", 8, measure=False)
    run("motion", 60)

    client.click("Showcase::nav-devices")
    client.click("Devices::clear-filter")
    listing = client.find("Devices::device-list")
    props = client.data("get_element_properties", elementHandle=listing)
    x = props["absolutePosition"]["x"] + props["size"]["width"] / 2
    y = props["absolutePosition"]["y"]
    height = props["size"]["height"]

    def scroll(index):
        # Six upward gestures, six downward gestures, covering the 18-row model.
        target = y + 16 if (index // 6) % 2 == 0 else y + height - 16
        client.call("drag_element", elementHandle=listing, target=dict(x=x, y=target))

    run("list-warmup", 8, scroll, measure=False)
    run("list", 60, scroll)

    client.click("Showcase::nav-media")
    slider = client.find("Media::alpha-slider")
    props = client.data("get_element_properties", elementHandle=slider)
    x = props["absolutePosition"]["x"]
    y = props["absolutePosition"]["y"] + props["size"]["height"] / 2
    width = props["size"]["width"]

    def blend(index):
        target = x + 12 if index % 2 == 0 else x + width - 12
        client.call("drag_element", elementHandle=slider, target=dict(x=target, y=y))

    run("blend-warmup", 8, blend, measure=False)
    run("blend", 60, blend)
    client.click("Media::cutout-button")
    run("cutout-warmup", 8, blend, measure=False)
    run("cutout", 60, blend)
    client.click("Media::cutout-button")
    client.click("Showcase::nav-controls")
    run("idle-after", 20, measure=False)
    mark("ready-for-physical")
    args.output.joinpath("resources.log").write_text(adb.read(args.remote + "/resources.log"))
    print("Automatic measurement complete; application remains running.", flush=True)


if __name__ == "__main__":
    main()
