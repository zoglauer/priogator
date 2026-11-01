#!/usr/bin/env python3
"""Draw the app icon (assets/icon.png, 1024x1024) without any libraries.

Three columns of task bars on a dark rounded square; the top bar of each
column is the pinned deadline task.
"""

import math
import struct
import sys
import zlib
from pathlib import Path

SIZE = 1024


def rounded_rect_coverage(px, py, x0, y0, x1, y1, r):
    """Anti-aliased coverage (0..1) of pixel centre (px, py) by a rounded rectangle."""
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    bx, by = (x1 - x0) / 2 - r, (y1 - y0) / 2 - r
    qx, qy = abs(px - cx) - bx, abs(py - cy) - by
    outside = math.hypot(max(qx, 0), max(qy, 0))
    inside = min(max(qx, qy), 0)
    d = outside + inside - r
    return min(max(0.5 - d, 0.0), 1.0)


def main(out: Path):
    pixels = [[0.0, 0.0, 0.0, 0.0] for _ in range(SIZE * SIZE)]

    def draw(x0, y0, x1, y1, r, color, gradient_to=None):
        for y in range(max(int(y0) - 1, 0), min(int(y1) + 2, SIZE)):
            t = (y - y0) / max(y1 - y0, 1)
            rgb = color
            if gradient_to:
                rgb = [c + (g - c) * t for c, g in zip(color, gradient_to)]
            for x in range(max(int(x0) - 1, 0), min(int(x1) + 2, SIZE)):
                a = rounded_rect_coverage(x + 0.5, y + 0.5, x0, y0, x1, y1, r) * rgb[3]
                if a <= 0:
                    continue
                p = pixels[y * SIZE + x]
                # "over" compositing with straight alpha
                out_a = a + p[3] * (1 - a)
                for i in range(3):
                    p[i] = (rgb[i] * a + p[i] * p[3] * (1 - a)) / out_a
                p[3] = out_a

    # Background (macOS icon grid: 824 px body, 100 px margin).
    draw(100, 100, 924, 924, 185, [62, 78, 112, 1.0], [30, 40, 66, 1.0])

    deadline = [235, 87, 74, 1.0]
    colours = [
        [86, 150, 255, 1.0],
        [72, 190, 120, 1.0],
        [250, 170, 60, 1.0],
        [176, 110, 220, 1.0],
    ]
    bars = [  # per column: colour index of each priority bar
        [0, 1, 0, 2],
        [3, 0, 1],
        [2, 3, 1, 0],
    ]
    col_w, gap = 196, 34
    left = (SIZE - (3 * col_w + 2 * gap)) / 2
    for c, column in enumerate(bars):
        x0 = left + c * (col_w + gap)
        x1 = x0 + col_w
        draw(x0, 230, x1, 800, 30, [255, 255, 255, 0.10])
        draw(x0 + 18, 252, x1 - 18, 316, 16, deadline)
        draw(x0 + 18, 340, x1 - 18, 344, 2, [255, 255, 255, 0.35])
        y = 366
        for i in column:
            draw(x0 + 18, y, x1 - 18, y + 64, 16, colours[i])
            y += 86

    raw = bytearray()
    for y in range(SIZE):
        raw.append(0)  # PNG filter: none
        for x in range(SIZE):
            r, g, b, a = pixels[y * SIZE + x]
            raw += bytes((round(r), round(g), round(b), round(a * 255)))

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(png)


if __name__ == "__main__":
    main(Path(sys.argv[1]) if len(sys.argv) > 1 else Path("assets/icon.png"))
