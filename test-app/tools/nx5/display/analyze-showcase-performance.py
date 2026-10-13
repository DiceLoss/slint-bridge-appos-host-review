#!/usr/bin/env python3
"""Summarize completed-frame batches, resource samples and matched evdev edges."""
import argparse
import json
import math
from pathlib import Path
import re
import statistics


def distribution(values):
    ordered = sorted(values)
    return dict(count=len(values), minimum=min(values), median=statistics.median(values),
                p95=ordered[math.ceil(len(ordered) * .95) - 1], maximum=max(values))


def resources(text):
    hz = int(re.search(r"ticks_per_second=(\d+)", text)[1])
    rows = []
    for line in text.splitlines():
        if not line.startswith("sample "):
            continue
        row = dict(token.split("=", 1) for token in line.split()[1:])
        rows.append({key: value if key == "phase" else int(value) for key, value in row.items()})
    result = {}
    for phase in dict.fromkeys(row["phase"] for row in rows):
        samples = [row for row in rows if row["phase"] == phase]
        if len(samples) < 2:
            continue
        cpu = [(b["cpu_ticks"] - a["cpu_ticks"]) * 100_000_000 / hz / (b["mono_us"] - a["mono_us"])
               for a, b in zip(samples, samples[1:])]
        result[phase] = dict(samples=len(samples), rss_kib=distribution([r["rss_kib"] for r in samples]),
                             process_cumulative_hwm_kib=max(r["hwm_kib"] for r in samples),
                             available_kib_min=min(r["available_kib"] for r in samples),
                             cpu_percent_one_core=distribution(cpu))
    return result


def touch(raw, recording):
    if int(recording.get("droppedCount", 0)) or int(recording.get("unknownEventCount", 0)):
        raise ValueError("MCP recording is incomplete")
    held = pending = False
    axes = re.search(r"^ready .*\baxis_x=(-?\d+) axis_y=(-?\d+)", raw, re.MULTILINE)
    x, y = map(int, axes.groups()) if axes else (0, 0)
    edges = []
    for line in raw.splitlines():
        match = re.fullmatch(r"event kernel_us=(\d+) read_us=(\d+) type=(\d+) code=(\d+) value=(-?\d+)", line)
        if not match:
            continue
        kernel, received, kind, code, value = map(int, match.groups())
        if (kind, code) == (0, 3):
            raise ValueError("evdev SYN_DROPPED: incomplete capture")
        if (kind, code) == (3, 0): x = value
        if (kind, code) == (3, 1): y = value
        if (kind, code) == (1, 330): pending = value != 0
        if (kind, code) == (0, 0) and pending != held:
            edges.append(dict(kind="pointerPressed" if pending else "pointerReleased",
                              kernel_us=kernel, observer_us=received, x=x, y=y))
            held = pending
    events = [e for e in recording.get("events", [])
              if "pointerPressed" in e["event"] or "pointerReleased" in e["event"]]
    if not edges or len(edges) != len(events) or held:
        raise ValueError(f"Incomplete/unmatched physical edges: evdev={len(edges)} Slint={len(events)} held={held}")
    phase = "motion"
    for edge, event in zip(edges, events):
        if edge["kind"] not in event["event"]:
            raise ValueError("evdev/Slint edge order mismatch")
        position = event["event"][edge["kind"]]["position"]
        # This capture is specifically for the unrotated 1024x600 NX5 axes.
        if abs(edge["x"] - position.get("x", 0)) > 2 or abs(edge["y"] - position.get("y", 0)) > 2:
            raise ValueError("evdev/Slint coordinate mismatch")
        edge.update(delay_ms=int(event["timestampMs"]) - edge["kernel_us"] / 1000,
                    result=event.get("result", "Unspecified"), phase=phase,
                    navigation=12 <= edge["x"] <= 162)
        if edge["delay_ms"] < -1:
            raise ValueError("evdev and Slint clocks do not match")
        # Navigation applies on release. Keep the navigation gesture in the old scene.
        if edge["kind"] == "pointerReleased" and 12 <= edge["x"] <= 162:
            for name, top in [("overview", 102), ("controls", 158), ("list", 214),
                              ("motion", 270), ("blend", 326), ("lab", 382)]:
                if top <= edge["y"] <= top + 49:
                    phase = name
    phases = {}
    controls = {}
    for name in dict.fromkeys(edge["phase"] for edge in edges):
        values = [e["delay_ms"] for e in edges if e["phase"] == name]
        phases[name] = distribution(values)
        phases[name]["maximum_with_1ms_quantization_margin"] = max(values) + 1
        values = [e["delay_ms"] for e in edges if e["phase"] == name and not e["navigation"]]
        if values:
            controls[name] = distribution(values)
            controls[name]["maximum_with_1ms_quantization_margin"] = max(values) + 1
    return dict(contacts=len(edges) // 2, edges=edges, phases=phases,
                page_controls=controls,
                overall=distribution([e["delay_ms"] for e in edges]),
                accepted_events=sum(e.get("result") == "Accepted" for e in recording.get("events", [])),
                total_events=len(recording.get("events", [])), dropped=0,
                scope="Kernel SYN_REPORT to completed Slint dispatch, including callbacks; 1 ms MCP timestamp precision, not LCD scanout latency")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phases", type=Path, required=True)
    parser.add_argument("--resources", type=Path, required=True)
    parser.add_argument("--touch-raw", type=Path)
    parser.add_argument("--touch-mcp", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if bool(args.touch_raw) != bool(args.touch_mcp):
        parser.error("provide both physical touch files")
    result = dict(phases=json.loads(args.phases.read_text()), resources=resources(args.resources.read_text()))
    if args.touch_raw:
        result["physical_touch"] = touch(args.touch_raw.read_text(), json.loads(args.touch_mcp.read_text()))
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    for phase in result["phases"]:
        if "fps" in phase:
            memory = result["resources"][phase["phase"]]
            print(f'{phase["phase"]}: {phase["fps"]:.2f} FPS, {phase["frames"]} completed frames, '
                  f'RSS {memory["rss_kib"]["minimum"]}..{memory["rss_kib"]["maximum"]} KiB')
    if "physical_touch" in result:
        print(json.dumps(result["physical_touch"]["phases"], indent=2))


if __name__ == "__main__":
    main()
