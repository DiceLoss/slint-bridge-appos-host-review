#!/usr/bin/env python3
"""Replay the offline address/table extraction for the recorded F1 kernel.

This is a fixed-image investigation aid, not a generic symbol recovery tool or
a device admission check. It never accesses ADB or a live device.
"""

import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess


IMAGE_SHA256 = "754e2d6f05664522b16c6d60f5f6dc48d3e2eee1bf788938cf994a6823c86573"
FIT_SHA256 = "28573f4b908e29ef7cecc8b0eea7dec76f0fcd86138eb9034e3ee9d397d2820d"
BASE = 0xFFFFFFFF80000000


def fit_properties(blob):
    header = struct.unpack_from(">10I", blob)
    assert header[0] == 0xD00DFEED
    strings = blob[header[3] : header[3] + header[8]]
    offset = header[2]
    stack = []
    result = {}
    while offset < header[2] + header[9]:
        token = struct.unpack_from(">I", blob, offset)[0]
        offset += 4
        if token == 1:
            end = blob.index(b"\0", offset)
            stack.append(blob[offset:end].decode())
            offset = (end + 4) & ~3
        elif token == 2:
            stack.pop()
        elif token == 3:
            size, name_offset = struct.unpack_from(">II", blob, offset)
            offset += 8
            name = strings[name_offset : strings.index(b"\0", name_offset)].decode()
            value = blob[offset : offset + size]
            offset = (offset + size + 3) & ~3
            if "/".join(stack) == "/images/kernel" and name != "data":
                result[name] = (
                    hex(int.from_bytes(value, "big"))
                    if name in ("load", "entry")
                    else value.rstrip(b"\0").decode()
                )
        elif token == 9:
            break
        elif token != 4:
            raise ValueError(f"Unexpected FIT token {token}")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, default=Path("evidence/f1-display-20261019/unpacked/Image"))
    parser.add_argument("--fit", type=Path, default=Path("evidence/f1-display-20261019/kernel.mtd"))
    parser.add_argument("--output", type=Path, default=Path("evidence/f1-kernel-20261019"))
    parser.add_argument("--objdump", default="/opt/F1/bin/riscv64-unknown-linux-gnu-objdump")
    args = parser.parse_args()
    image = args.image.read_bytes()
    fit = args.fit.read_bytes()
    for data, expected, label in ((image, IMAGE_SHA256, "Image"), (fit, FIT_SHA256, "FIT")):
        actual = hashlib.sha256(data).hexdigest()
        if actual != expected:
            raise SystemExit(f"{label} SHA256 mismatch: {actual}; fixed addresses do not apply")
    args.output.mkdir(parents=True, exist_ok=True)

    def words(offset, count):
        return [hex(v) for v in struct.unpack_from(f"<{count}Q", image, offset)]

    commands = []
    for offset in range(0x569180, 0x5694B0, 48):
        command, function = struct.unpack_from("<I4xQ", image, offset)
        commands.append({
            "address": hex(BASE + offset),
            "command": hex(command),
            "function": hex(function),
            "description": image[offset + 16 : offset + 48].split(b"\0")[0].decode(),
        })
    assert struct.unpack_from("<I", image, 0x5694B0)[0] == 0xFFFF
    strings = {}
    for text in (b"aic_fb_ioctl", b"GE Bitblt FB error", b"DE underflow error", b"ge timeout", b"ge error status"):
        offset = image.index(text)
        strings[text.decode()] = {"offset": hex(offset), "address": hex(BASE + offset)}
    metadata = {
        "image_sha256": IMAGE_SHA256,
        "fit_sha256": FIT_SHA256,
        "image_length": len(image),
        "fit_kernel": fit_properties(fit),
        "image_header_text_offset": hex(struct.unpack_from("<Q", image, 8)[0]),
        "image_header_size": hex(struct.unpack_from("<Q", image, 16)[0]),
        "virtual_base": hex(BASE),
        "base_evidence": "runtime component_ops addresses match image pointer tables, bind code and absolute string references",
        "component_ops": {
            "0xffffffff8042fb00": words(0x42FB00, 2),
            "0xffffffff8042f388": words(0x42F388, 2),
            "0xffffffff8042fd48": words(0x42FD48, 2),
        },
        "fb_ops_at_0xffffffff805694e0": words(0x5694E0, 23),
        "fb_ioctl_switch_0x4600_to_0x4611": [
            hex(v | 0xFFFFFFFF00000000)
            for v in struct.unpack_from("<18I", image, 0x42D788)
        ],
        "aicfb_ioctl_commands": commands,
        "string_locations": strings,
        "objdump_version": subprocess.check_output([args.objdump, "--version"], text=True).splitlines()[0],
    }
    (args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    # Labels are analysis names, not recovered ELF symbols. Ranges include all
    # relevant branches so readers can inspect more than isolated instructions.
    ranges = {
        "head": (0, 0x180),
        "fb-core": (0x172CDE, 0x173218),
        "fb-driver": (0x1795F4, 0x17B420),
        "de-hardware": (0x17B420, 0x17BD16),
        "de-driver": (0x17BD16, 0x17D87A),
        "ge-normal": (0x1762F8, 0x178156),
    }
    for name, (start, end) in ranges.items():
        command = [
            args.objdump, "-D", "-b", "binary", "-m", "riscv:rv64",
            f"--adjust-vma={BASE:#x}",
            f"--start-address={BASE + start:#x}",
            f"--stop-address={BASE + end:#x}", str(args.image),
        ]
        with (args.output / f"{name}.disasm").open("w") as output:
            subprocess.run(command, check=True, stdout=output)
    print(f"Verified Image/FIT hashes; wrote metadata and {len(ranges)} disassembly ranges to {args.output}")


if __name__ == "__main__":
    main()
