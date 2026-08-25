#!/usr/bin/env python3
"""Generate Tauri icon files locally with zero third-party dependencies.

Produces deterministic, solid-color-with-glyph PNG icons of the required
sizes, a Windows .ico that wraps the PNGs, and a macOS .icns container that
embeds PNG data. Run from the project root:

    npm run icons

Outputs into src-tauri/icons/. Safe to re-run at any time.
"""
from __future__ import annotations

import os
import struct
import zlib

OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")

BG = (24, 26, 32, 255)        # dark background
FG = (94, 234, 168, 255)      # mint green glyph
FG_DIM = (58, 122, 96, 255)   # dimmer green for secondary strokes


def _png_chunk(tag: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def render_icon(size: int) -> bytes:
    """Render a simple tree glyph: trunk + three branches (root/folders/chats)."""
    px = [[*BG] for _ in range(size * size)]

    def rect(x0f: float, y0f: float, x1f: float, y1f: float, color=FG) -> None:
        x0, y0 = int(x0f * size), int(y0f * size)
        x1, y1 = int(x1f * size), int(y1f * size)
        for y in range(max(0, y0), min(size, y1)):
            for x in range(max(0, x0), min(size, x1)):
                px[y * size + x][:] = color

    # rounded-ish background plate margin
    margin = max(2, size // 16)
    for y in range(size):
        for x in range(size):
            if x < margin or x >= size - margin or y < margin or y >= size - margin:
                px[y * size + x] = [0, 0, 0, 0]

    t = 0.10  # stroke thickness fraction
    # trunk
    rect(0.46, 0.30, 0.46 + t, 0.78)
    # root bar
    rect(0.28, 0.76, 0.72, 0.76 + t)
    # three branches
    rect(0.30, 0.30, 0.70, 0.30 + t)          # top bar
    rect(0.30, 0.30, 0.30 + t, 0.44, FG_DIM)  # left drop
    rect(0.45, 0.44, 0.55, 0.56, FG)          # middle node block
    rect(0.70 - t, 0.30, 0.70, 0.44, FG_DIM)  # right drop
    # leaf squares on drops
    rect(0.24, 0.42, 0.36, 0.54, FG)
    rect(0.64, 0.42, 0.76, 0.54, FG)

    raw = b"".join(b"\x00" + bytes(v for px4 in row for v in px4) for row in
                   (px[y * size:(y + 1) * size] for y in range(size)))
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + _png_chunk(b"IHDR", ihdr)
        + _png_chunk(b"IDAT", zlib.compress(raw, 9))
        + _png_chunk(b"IEND", b"")
    )


def write_ico(path: str, pngs: list[tuple[int, bytes]]) -> None:
    entries, offset = [], 6 + 16 * len(pngs)
    blobs = []
    for size, png in pngs:
        w = 0 if size >= 256 else size
        entries.append(struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(png), offset))
        blobs.append(png)
        offset += len(png)
    header = struct.pack("<HHH", 0, 1, len(pngs))
    with open(path, "wb") as f:
        f.write(header + b"".join(entries) + b"".join(blobs))


def write_icns(path: str, png_by_size: dict[int, bytes]) -> None:
    # OSType -> pixel size (ic07=128, ic08=256, ic09=512, ic10=1024->512@2x ...)
    types = {128: b"ic07", 256: b"ic08", 512: b"ic09", 1024: b"ic10"}
    chunks = []
    total = 8
    for size, ost in types.items():
        if size not in png_by_size:
            continue
        png = png_by_size[size]
        chunks.append(ost + struct.pack(">I", len(png) + 8) + png)
        total += len(png) + 8
    with open(path, "wb") as f:
        f.write(b"icns" + struct.pack(">I", total) + b"".join(chunks))


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    targets = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }
    rendered: dict[int, bytes] = {}
    for name, size in targets.items():
        data = render_icon(size)
        rendered[size] = data
        with open(os.path.join(OUT_DIR, name), "wb") as f:
            f.write(data)
    write_ico(os.path.join(OUT_DIR, "icon.ico"),
              [(32, rendered[32]), (128, rendered[128]), (256, rendered[256])])
    write_icns(os.path.join(OUT_DIR, "icon.icns"), rendered)
    print(f"Icons written to {os.path.abspath(OUT_DIR)}")


if __name__ == "__main__":
    main()
