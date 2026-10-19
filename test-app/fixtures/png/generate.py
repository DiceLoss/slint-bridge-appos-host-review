#!/usr/bin/env python3
"""Reproduce odd-width RGB/RGBA fixtures with all five PNG row filters."""
from pathlib import Path
import struct
import zlib


def chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))


def paeth(left, above, corner):
    prediction = left + above - corner
    errors = [abs(prediction - value) for value in (left, above, corner)]
    return (left, above, corner)[errors.index(min(errors))]


for channels in (3, 4):
    width, height = 257, 129
    previous = bytes(width * channels)
    filtered = bytearray()
    for y in range(height):
        row = bytes((x * 31 + y * 17 + channel * 73) & 255 if channel < 3
                    else (0, 1, 127, 254, 255)[(x + y) % 5]
                    for x in range(width) for channel in range(channels))
        method = (3, 4, 0, 1, 2)[y % 5]
        filtered.append(method)
        for i, value in enumerate(row):
            left = row[i - channels] if i >= channels else 0
            above = previous[i]
            corner = previous[i - channels] if i >= channels else 0
            prediction = (0, left, above, (left + above) // 2, paeth(left, above, corner))[method]
            filtered.append((value - prediction) & 255)
        previous = row
    encoded = zlib.compress(filtered, 9)
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8,
          2 if channels == 3 else 6, 0, 0, 0))
    # Splitting the zlib header and trailer across chunks exercises IDAT joining.
    for part in (encoded[:1], encoded[1:-3], encoded[-3:]):
        png += chunk(b'IDAT', part)
    png += chunk(b'IEND', b'')
    (Path(__file__).parent / ('rgb-filters.png' if channels == 3 else 'rgba-filters.png')).write_bytes(png)
