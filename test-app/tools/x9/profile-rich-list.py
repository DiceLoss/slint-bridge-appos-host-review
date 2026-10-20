#!/usr/bin/env python3
"""Drive native MCP while an external X9 resource observer records each phase."""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import time

spec = importlib.util.spec_from_file_location("showcase_check", Path(__file__).resolve().parents[1] / "check-showcase.py")
ui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ui)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--remote", required=True)
    parser.add_argument("--port", type=int, default=19316)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seconds", type=float, default=60)
    args = parser.parse_args()
    if not args.remote.startswith("/tmp/slint-bridge-x9-perf-") or args.seconds < 50:
        parser.error("use a dedicated X9 performance directory and at least fifty seconds")
    args.output.mkdir(parents=True, exist_ok=False)
    client = ui.Client(args.port)
    client.window, = client.data("list_windows")["windowHandles"]
    properties = client.data("get_window_properties", windowHandle=client.window)
    client.root = properties["rootElementHandle"]
    assert properties["size"] == dict(width=1280, height=800)
    results = []

    def mark(name):
        phase = args.remote + "/phase"
        temporary = args.remote + "/phase.next"
        command = (f"printf '%s\\n' {shlex.quote(name)} > {shlex.quote(temporary)}"
                   f" && mv {shlex.quote(temporary)} {shlex.quote(phase)}")
        subprocess.run(["adb", "-s", args.serial, "shell", command], check=True, timeout=10,
                       stdout=subprocess.DEVNULL)

    def run(name, seconds, action=None):
        mark(name)
        start = time.monotonic()
        moves = []
        while time.monotonic() - start < seconds:
            if action:
                moves.append(action())
            else:
                time.sleep(0.1)
        elapsed = time.monotonic() - start
        mark("between")
        result = dict(phase=name, host_seconds=elapsed, gestures=len(moves), movements=moves)
        results.append(result)
        (args.output / "phases.json").write_text(json.dumps(results, indent=2) + "\n")
        print(json.dumps({k: v for k, v in result.items() if k != "movements"}), flush=True)

    try:
        client.click("Showcase::nav-controls")
        run("controls-warmup", 5)
        run("controls-idle", 15)
        client.click("Showcase::nav-rich-list")
        for axis in ("vertical", "horizontal"):
            if axis == "horizontal":
                client.click("RichList::orientation-toggle")
            client.click("RichList::start-button")
            listing = client.find("RichList::listing")
            bounds = client.data("get_element_properties", elementHandle=listing)
            length = "width" if axis == "horizontal" else "height"
            viewport = bounds["size"][length]
            extent = (25588 if axis == "horizontal" else 11992) - viewport
            client.call("scroll_element", elementHandle=listing,
                        **{"deltaX" if axis == "horizontal" else "deltaY": -4 * viewport})
            time.sleep(1)
            forward = True

            def scroll():
                nonlocal forward
                offset = client.rich_offset(bounds, axis)
                if offset > extent - 2 * viewport:
                    forward = False
                elif offset < 2 * viewport:
                    forward = True
                began = time.monotonic()
                pointer_distance = client.drag_rich(bounds, axis, forward)
                end_offset = client.rich_offset(bounds, axis)
                distance = abs(end_offset - offset)
                assert distance > 0.4 * viewport, (axis, "insufficient movement", distance)
                assert 1 < end_offset < extent - 1, (axis, "reached an endpoint", end_offset)
                return dict(seconds=time.monotonic() - began, forward=forward,
                            content_distance=distance, pointer_distance=pointer_distance,
                            offset_before=offset, offset_after=end_offset, viewport=viewport)

            run(f"rich-{axis}-warmup", 10, scroll)
            run(f"rich-{axis}", args.seconds, scroll)
            client.expect_text("Selected space: None")
            run(f"rich-{axis}-idle", 10)
        client.click("Showcase::nav-controls")
        run("controls-after", 15)
    finally:
        mark("done")


if __name__ == "__main__":
    main()
