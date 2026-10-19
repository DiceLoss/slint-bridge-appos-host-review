#!/usr/bin/env python3
"""Launch a host-only Slint preview and exercise its native MCP interface."""
import argparse
import base64
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import urllib.request


class Client:
    def __init__(self, port):
        self.endpoint = f"http://127.0.0.1:{port}/mcp"
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def rpc(self, method, params):
        request = urllib.request.Request(self.endpoint, data=json.dumps(dict(
            jsonrpc="2.0", id=1, method=method, params=params)).encode(), headers={
                "Content-Type": "application/json", "Accept": "application/json, text/event-stream"})
        # Wide drags on rich device scenes can take longer than a simple query.
        timeout = 30 if method == "tools/call" and params.get("name") == "drag_element" else 10
        raw = self.opener.open(request, timeout=timeout).read().decode()
        if raw.startswith(("event:", "data:")):
            raw = next(line[5:].strip() for line in raw.splitlines() if line.startswith("data:"))
        response = json.loads(raw)
        if "error" in response:
            raise RuntimeError(response["error"])
        result = response["result"]
        if result.get("isError"):
            raise RuntimeError(result)
        return result

    def call(self, name, **args):
        return self.rpc("tools/call", dict(name=name, arguments=args))

    def data(self, name, **args):
        result = self.call(name, **args)
        return json.loads(next(b["text"] for b in result["content"] if b["type"] == "text"))

    def find(self, element_id):
        found = self.data("find_elements_by_id", windowHandle=self.window, elementsId=element_id)
        handles = found.get("elementHandles", [])
        if len(handles) != 1:
            raise AssertionError(f"Expected one {element_id}: {found}")
        return handles[0]

    def click(self, element_id):
        self.call("click_element", elementHandle=self.find(element_id))
        time.sleep(0.12)

    def set(self, element_id, value):
        self.call("set_element_value", elementHandle=self.find(element_id), value=str(value))
        time.sleep(0.05)

    def button(self, label):
        button = next(e for e in self.tree() if e.get("accessibleRole") == "Button"
                      and e.get("accessibleLabel") == label)
        self.call("click_element", elementHandle=button["handle"])

    def tree(self):
        return self.data("get_element_tree", elementHandle=self.root, maxElements=2500)["elements"]

    def expect_text(self, text):
        labels = [e.get("accessibleLabel", "") for e in self.tree()]
        assert any(text in label for label in labels), f"Missing text: {text}"

    def screenshot(self, output, name):
        result = self.call("take_screenshot", windowHandle=self.window, imageMimeType="image/png")
        output.joinpath(f"{name}.png").write_bytes(base64.b64decode(
            next(b["data"] for b in result["content"] if b["type"] == "image"), validate=True))
        output.joinpath(f"{name}.json").write_text(json.dumps(self.tree(), indent=2))

    def device_cards(self):
        # Native MCP searches omit cards clipped outside the current viewport.
        return [e for e in self.tree() if any(
            item.get("id") == "Devices::device-card" for item in e["typeNamesAndIds"])]

    def drag_horizontal(self, bounds, direction):
        left = bounds["absolutePosition"]["x"]
        right = left + bounds["size"]["width"]
        visible = [e for e in self.device_cards() if left + 16 < e["absolutePosition"]["x"]
                   + e["size"]["width"] / 2 < right - 16]
        source = max(visible, key=lambda e: e["absolutePosition"]["x"]) if direction == "left" else min(
            visible, key=lambda e: e["absolutePosition"]["x"])
        target = dict(x=left + 16 if direction == "left" else right - 16,
                      y=source["absolutePosition"]["y"] + source["size"]["height"] / 2)
        distance = abs(source["absolutePosition"]["x"] + source["size"]["width"] / 2 - target["x"])
        assert distance >= bounds["size"]["width"] * 0.6, "Expected a wide horizontal gesture"
        self.call("drag_element", elementHandle=source["handle"], target=target)
        return distance

    def rich_items(self):
        return [e for e in self.tree() if any(
            item.get("id") == "RichList::rich-item" for item in e["typeNamesAndIds"])]

    def drag_rich(self, bounds, axis, forward):
        coord, length = ("x", "width") if axis == "horizontal" else ("y", "height")
        low = bounds["absolutePosition"][coord]
        high = low + bounds["size"][length]
        visible = [e for e in self.rich_items() if low + 1 < e["absolutePosition"][coord]
                   + e["size"][length] / 2 < high - 1]
        source = (max if forward else min)(visible, key=lambda e: e["absolutePosition"][coord])
        target = {k: source["absolutePosition"][k] + source["size"][v] / 2
                  for k, v in [("x", "width"), ("y", "height")]}
        target[coord] = low + 12 if forward else high - 12
        distance = abs(source["absolutePosition"][coord] + source["size"][length] / 2 - target[coord])
        assert distance >= bounds["size"][length] * 0.6
        self.call("drag_element", elementHandle=source["handle"], target=target)
        return distance


def exercise_rich(client, output, full=False):
    client.click("Showcase::nav-overview")
    client.click("Showcase::nav-rich-list")
    results = []
    for axis, number in [("vertical", 18), ("horizontal", 19)]:
        if axis == "horizontal":
            client.click("RichList::orientation-toggle")
        listing = client.find("RichList::listing")
        bounds = client.data("get_element_properties", elementHandle=listing)
        coord, length = ("x", "width") if axis == "horizontal" else ("y", "height")
        other = "y" if coord == "x" else "x"
        initial = client.rich_items()
        first = initial[0]["handle"]
        start = client.data("get_element_properties", elementHandle=first)["absolutePosition"]
        client.screenshot(output, f"{number}-rich-{axis}")
        selected = next(e["accessibleLabel"] for e in client.tree()
                        if e.get("accessibleLabel", "").startswith("Selected space:"))
        drags = []
        for forward in [True, False]:
            drags.append(client.drag_rich(bounds, axis, forward))
            time.sleep(0.2)
            client.expect_text(selected)
            pos = client.data("get_element_properties", elementHandle=first)["absolutePosition"]
            assert abs(pos[other] - start[other]) < 1, "List drifted on its locked axis"
            if forward:
                assert start[coord] - pos[coord] > bounds["size"][length] * 0.4, "Rich drag did not scroll"
        client.click("RichList::start-button")
        seen = set()
        positions = []
        if full:
            total = 25588 if axis == "horizontal" else 11992
            extent = total - bounds["size"][length]
            assert extent > 30 * bounds["size"][length]
            for forward in [True, False]:
                target = start[coord] - extent if forward else start[coord]
                for _ in range(60):
                    visible = client.rich_items()
                    seen.update(int(e["accessibleLabel"].split()[-1]) for e in visible)
                    pos = client.data("get_element_properties", elementHandle=first)["absolutePosition"]
                    assert abs(pos[other] - start[other]) < 1
                    positions.append(pos[coord])
                    if abs(pos[coord] - target) < 1:
                        break
                    client.call("scroll_element", elementHandle=listing,
                                **{"deltaX" if coord == "x" else "deltaY":
                                   (-1 if forward else 1) * bounds["size"][length] * 0.8})
                    time.sleep(0.08)
                else:
                    raise AssertionError(f"Rich {axis} boundary was not reached")
                client.call("scroll_element", elementHandle=listing,
                            **{"deltaX" if coord == "x" else "deltaY": -500 if forward else 500})
                time.sleep(0.08)
                pos = client.data("get_element_properties", elementHandle=first)["absolutePosition"]
                assert abs(pos[coord] - target) < 1, "Rich boundary did not clamp"
                client.screenshot(output, f"rich-{axis}-{'end' if forward else 'return'}")
                endpoint = next(e for e in client.rich_items()
                                if int(e["accessibleLabel"].split()[-1]) == (100 if forward else 1))
                client.call("click_element", elementHandle=endpoint["handle"])
                client.expect_text(f"Selected space: {100 if forward else 1}")
            assert seen == set(range(1, 101)), seen
        else:
            client.call("click_element", elementHandle=client.rich_items()[0]["handle"])
            client.expect_text("Selected space: 1")
        results.append(dict(axis=axis,viewport=bounds["size"],wide_drags=drags,
                            observed_ids=sorted(seen),positions=positions))
    client.click("RichList::orientation-toggle")
    client.click("RichList::start-button")
    output.joinpath("rich-list.json").write_text(json.dumps(results, indent=2))
    return results


def exercise_horizontal(client, output):
    client.click("Showcase::nav-devices")
    client.click("Devices::clear-filter")
    client.click("Devices::orientation-toggle")
    listing = client.find("Devices::horizontal-list")
    bounds = client.data("get_element_properties", elementHandle=listing)
    left = bounds["absolutePosition"]["x"]
    right = left + bounds["size"]["width"]

    initial = client.device_cards()
    client.expect_text("18 results")
    assert initial[0]["accessibleLabel"] == "Light panel 01"
    first = initial[0]["handle"]
    step = initial[1]["absolutePosition"]["x"] - initial[0]["absolutePosition"]["x"]
    expected_extent = 17 * step + initial[0]["size"]["width"] - bounds["size"]["width"]
    assert expected_extent > bounds["size"]["width"], "Expected a multi-screen list"
    seen = set()
    start = client.data("get_element_properties", elementHandle=first)["absolutePosition"]
    client.screenshot(output, "11-horizontal-start")
    results = []
    for direction in ("left", "right"):
        gestures = []
        recordings = []

        def swipe():
            # A long traversal can exceed the native recorder's 1024-event ring.
            client.call("start_event_recording", windowHandle=client.window)
            distance = client.drag_horizontal(bounds, direction)
            time.sleep(0.4)
            events = client.data("stop_event_recording", windowHandle=client.window)
            assert not int(events.get("droppedCount", 0)) and not int(events.get("unknownEventCount", 0))
            recordings.append(events)
            output.joinpath(f"horizontal-{direction}-events.json").write_text(json.dumps(recordings, indent=2))
            dialogs = client.data("find_elements_by_id", windowHandle=client.window,
                                  elementsId="Showcase::dialog-cancel")
            assert not dialogs.get("elementHandles"), "Dragging a card opened its details"
            position = client.data("get_element_properties", elementHandle=first)["absolutePosition"]
            assert abs(position["y"] - start["y"]) < 1, "Horizontal scrolling drifted vertically"
            gestures.append(dict(distance=distance, first_x=position["x"]))

        for _ in range(20):
            visible = client.device_cards()
            seen.update(e["accessibleLabel"] for e in visible)
            endpoint = next((e for e in visible if e["accessibleLabel"] == (
                "Room controller 18" if direction == "left" else "Light panel 01")), None)
            if endpoint is None:
                swipe()
                continue
            edge = client.data("get_element_properties", elementHandle=endpoint["handle"])
            position = edge["absolutePosition"]["x"] + (edge["size"]["width"] if direction == "left" else 0)
            if abs(position - (right if direction == "left" else left)) < 1:
                break
            swipe()
        else:
            raise AssertionError(f"Horizontal {direction} boundary was not reached")
        boundary = client.data("get_element_properties", elementHandle=first)["absolutePosition"]["x"]
        if direction == "left":
            assert abs(start["x"] - boundary - expected_extent) < 1, "Wrong horizontal scroll extent"
        swipe()
        assert abs(gestures[-1]["first_x"] - boundary) < 1, "Horizontal boundary did not clamp"
        results.append(dict(direction=direction, gestures=gestures, boundary_first_x=boundary,
                            recorded_events=sum(len(r["events"]) for r in recordings)))
        if direction == "left":
            client.screenshot(output, "12-horizontal-end")
            client.call("click_element", elementHandle=endpoint["handle"])
            client.expect_text("Selected: Room controller 18")
            client.click("Showcase::dialog-cancel")
    client.screenshot(output, "13-horizontal-return")
    client.set("Devices::search-input", "Room controller 18")
    filtered = client.device_cards()
    assert len(filtered) == 1 and filtered[0]["accessibleLabel"] == "Room controller 18"
    client.call("click_element", elementHandle=filtered[0]["handle"])
    client.expect_text("Selected: Room controller 18")
    client.click("Showcase::dialog-cancel")
    client.set("Devices::search-input", "no-such-device")
    assert not client.device_cards()
    client.expect_text("No matching devices")
    client.click("Devices::clear-filter")
    client.click("Devices::orientation-toggle")
    client.find("Devices::device-list")
    client.click("Showcase::nav-overview")
    result = dict(cards=18, observed_cards=sorted(seen), viewport_width=bounds["size"]["width"],
                  directions=results)
    output.joinpath("horizontal.json").write_text(json.dumps(result, indent=2))
    return result


def exercise(client, output):
    client.window, = client.data("list_windows")["windowHandles"]
    client.root = client.data("get_window_properties", windowHandle=client.window)["rootElementHandle"]
    client.click("Showcase::nav-overview")
    client.expect_text("The interface, in motion")
    client.call("start_event_recording", windowHandle=client.window)
    client.screenshot(output, "01-overview")
    client.click("Showcase::nav-clock")
    client.expect_text("Time, simply")
    clock = client.find("Clock::clock-time")
    before = client.data("get_element_properties", elementHandle=clock)["accessibleLabel"]
    time.sleep(1.2)
    after = client.data("get_element_properties", elementHandle=clock)["accessibleLabel"]
    assert before != after, "Clock must follow wall time"
    client.screenshot(output, "20-clock")
    client.click("Showcase::nav-controls")
    client.set("Controls::station-input", "MCP Studio")
    client.set("Controls::level-slider", 32)
    client.click("Controls::power-switch")
    client.click("Controls::notifications-check")
    client.click("Controls::apply-button")
    client.expect_text("MCP Studio · Balanced · 32% · Off")
    client.expect_text("Alerts Off")
    time.sleep(3.1)  # Let the short confirmation toast dismiss before visual QA.
    client.screenshot(output, "02-controls")
    client.click("Controls::reset-button")
    client.screenshot(output, "07-reset-dialog")
    client.click("Showcase::dialog-cancel")
    client.expect_text("MCP Studio · Balanced · 32% · Off")
    client.click("Controls::keyboard-button")
    client.button("5")
    client.expect_text("MCP Studio5│")
    client.button("⌫")
    client.expect_text("MCP Studio│")
    client.click("Showcase::nav-devices")
    client.expect_text("Station keyboard")  # Modal must block navigation beneath it.
    client.screenshot(output, "08-keyboard")
    client.click("Showcase::dialog-cancel")

    client.click("Showcase::nav-devices")
    client.set("Devices::search-input", "workshop")
    client.click("Devices::online-filter")
    client.expect_text("No matching devices")
    client.click("Devices::clear-filter")
    client.click("Devices::search-keyboard")
    client.button("L")
    client.button("I")
    client.click("Showcase::dialog-confirm")
    client.expect_text("6 results")
    client.click("Devices::clear-filter")
    client.screenshot(output, "03-devices")
    # Select the visible first row by its semantic label, retaining stable model IDs.
    row = next(e for e in client.tree() if e.get("accessibleRole") == "Button"
               and e.get("accessibleLabel") == "Light panel 01")
    client.call("click_element", elementHandle=row["handle"])
    client.expect_text("No network connection")
    client.screenshot(output, "09-device-dialog")
    client.click("Showcase::dialog-cancel")
    listing = client.find("Devices::device-list")
    first = next(e for e in client.tree() if e.get("accessibleRole") == "Button"
                 and e.get("accessibleLabel") == "Light panel 01")
    before = first["absolutePosition"]["y"]
    position = client.data("get_element_properties", elementHandle=listing)["absolutePosition"]
    client.call("drag_element", elementHandle=listing,
                target=dict(x=position["x"] + 80, y=position["y"] + 15))
    time.sleep(0.3)
    after = client.data("get_element_properties", elementHandle=first["handle"])
    assert after["absolutePosition"]["y"] < before, "List did not scroll on drag"

    client.click("Showcase::nav-motion")
    client.click("Motion::motion-toggle")
    ticks = next(e["accessibleLabel"] for e in client.tree() if e.get("accessibleLabel", "").startswith("UI ticks "))
    time.sleep(0.2)
    client.expect_text(ticks)
    client.click("Motion::transition-button")
    time.sleep(0.9)
    client.screenshot(output, "04-motion")
    client.click("Motion::motion-toggle")

    client.click("Showcase::nav-media")
    client.set("Media::alpha-slider", 40)
    client.expect_text("40%")
    client.screenshot(output, "05-layers")
    client.click("Media::cutout-button")
    client.expect_text("no video source connected")
    client.screenshot(output, "10-transparent")
    client.click("Media::cutout-button")

    exercise_scaling(client, output)
    exercise_rich(client, output)
    client.click("Showcase::nav-lab")
    for label in ("Tap 1", "Tap 2", "Tap 3"):
        client.button(label)
    pad = client.find("Lab::drag-pad")
    client.call("click_element", elementHandle=pad)
    client.expect_text("Drags 0")
    props = client.data("get_element_properties", elementHandle=pad)
    position = props["absolutePosition"]
    client.call("drag_element", elementHandle=pad,
                target=dict(x=position.get("x", 0) + 45, y=position.get("y", 0) + 30))
    client.expect_text("Drags 1")
    client.expect_text("6 / 6 exercises completed")
    client.screenshot(output, "06-lab")
    events = client.data("stop_event_recording", windowHandle=client.window)
    assert int(events.get("droppedCount", 0)) == 0, "MCP recording dropped events"
    assert int(events.get("unknownEventCount", 0)) == 0, "MCP recorded unknown events"
    assert len(events.get("events", [])) > 100, "Expected a full interaction recording"
    output.joinpath("events.json").write_text(json.dumps(events, indent=2))
    client.click("Lab::clear-journal")
    client.expect_text("Your next action will appear here")
    client.click("Showcase::nav-controls")
    client.expect_text("MCP Studio · Balanced · 32% · Off")
    client.click("Controls::reset-button")
    client.click("Showcase::dialog-confirm")
    client.expect_text("NX5 Studio")
    client.expect_text("No profile applied yet")
    client.click("Showcase::nav-overview")
    return exercise_horizontal(client, output)


def exercise_scaling(client, output):
    from PIL import Image

    client.click("Showcase::nav-scaling")
    sizes = []
    colors = [(0.25, 0.25, (217, 75, 64)), (0.75, 0.25, (21, 158, 138)),
              (0.25, 0.75, (57, 122, 208)), (0.75, 0.75, (232, 180, 66))]
    for number, zoom in enumerate([50, 125, 200, 100], 14):
        if zoom == 200:
            slider = client.find("Scaling::zoom-slider")
            props = client.data("get_element_properties", elementHandle=slider)
            client.call("drag_element", elementHandle=slider, target=dict(
                x=props["absolutePosition"]["x"] + props["size"]["width"] - 1,
                y=props["absolutePosition"]["y"] + props["size"]["height"] / 2))
        elif zoom == 100:
            client.click("Scaling::zoom-reset")
        else:
            client.set("Scaling::zoom-slider", zoom)
        name = f"{number:02}-scaling-{zoom}"
        client.screenshot(output, name)
        pixels = Image.open(output / (name + ".png")).convert("RGBA")
        for kind in ["png", "svg"]:
            picture = client.data("get_element_properties", elementHandle=client.find(f"Scaling::{kind}-image"))
            viewport = client.data("get_element_properties", elementHandle=client.find(f"Scaling::{kind}-viewport"))
            assert abs(picture["size"]["width"] - 256 * zoom / 100) < 0.1
            assert abs(picture["size"]["height"] - 160 * zoom / 100) < 0.1
            x, y = picture["absolutePosition"]["x"], picture["absolutePosition"]["y"]
            w, h = picture["size"]["width"], picture["size"]["height"]
            vx, vy = viewport["absolutePosition"]["x"], viewport["absolutePosition"]["y"]
            vw, vh = viewport["size"]["width"], viewport["size"]["height"]
            for fx, fy, color in colors:
                px, py = round(x + fx * w), round(y + fy * h)
                assert vx <= px < vx + vw and vy <= py < vy + vh
                assert pixels.getpixel((px, py)) == (*color, 255), (kind, zoom, px, py)
            if zoom <= 125:
                assert pixels.getpixel((round(x + 2), round(y + 2))) == (220, 229, 235, 255)
            if zoom == 200:
                # Larger panels may fit an enlarged image on one axis. Check
                # clipping only where the actual image exceeds its viewport.
                if w > vw:
                    assert x < vx
                    assert pixels.getpixel((round(vx - 4), round(y + h / 4)))[:3] != colors[0][2]
                    assert pixels.getpixel((round(vx + vw + 4), round(y + h / 4)))[:3] != colors[1][2]
                else:
                    assert vx <= x and x + w <= vx + vw
                if h > vh:
                    assert y < vy
                    assert pixels.getpixel((round(x + w / 4), round(vy - 4)))[:3] != colors[0][2]
                    assert pixels.getpixel((round(x + w / 4), round(vy + vh + 4)))[:3] != colors[2][2]
                else:
                    assert vy <= y and y + h <= vy + vh
            sizes.append(dict(format=kind, zoom=zoom, image=picture, viewport=viewport))
    output.joinpath("scaling.json").write_text(json.dumps(sizes, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/showcase"))
    parser.add_argument("--output", type=Path, default=Path("target/showcase-check"))
    parser.add_argument("--rich-only", action="store_true", help="fully traverse the 100-item rich list in both directions")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    client = Client(port)
    with args.output.joinpath("preview.log").open("w") as log:
        process = subprocess.Popen([str(args.binary.resolve()), "--preview", "--seconds=180"],
                                   env=dict(os.environ, SLINT_MCP_PORT=str(port)), stdout=log, stderr=log)
        try:
            for _ in range(100):
                if process.poll() is not None:
                    raise RuntimeError("Preview exited; see preview.log")
                try:
                    client.rpc("initialize", dict(protocolVersion="2025-06-18", capabilities={},
                                                   clientInfo=dict(name="showcase-check", version="1")))
                    break
                except (OSError, ValueError):
                    time.sleep(0.1)
            else:
                raise RuntimeError("Preview MCP did not start")
            if args.rich_only:
                client.window, = client.data("list_windows")["windowHandles"]
                client.root = client.data("get_window_properties", windowHandle=client.window)["rootElementHandle"]
                rich = exercise_rich(client, args.output, full=True)
                summary = dict(result="passed", scope="host native Slint MCP; no device or evdev validation",
                               pages=1, screenshots=6, rich_list=rich)
            else:
                horizontal = exercise(client, args.output)
                summary = dict(result="passed", scope="host native Slint MCP; no device or evdev validation",
                               pages=9, screenshots=20, horizontal=horizontal,
                               runtime_image_scales=[50, 125, 200, 100])
            args.output.joinpath("summary.json").write_text(json.dumps(summary, indent=2) + "\n")
            print(json.dumps(summary, indent=2))
        finally:
            process.terminate()
            process.wait(timeout=10)


if __name__ == "__main__":
    main()
