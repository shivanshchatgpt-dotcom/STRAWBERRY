#!/usr/bin/env python3
"""Generate Strawberry brand icons — pure Python stdlib, zero dependencies.

Draws a strawberry glyph (gradient berry body, mint crown, seed dots) on a
dark rounded plate, renders once at high resolution, and downsamples to every
size Tauri needs: PNGs (32/128/256/512), Windows .ico, macOS .icns.

Run from the project root:
    npm run icons
Outputs into src-tauri/icons/. Safe to re-run.
"""
from __future__ import annotations

import math
import os
import struct
import zlib

OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")

MASTER = 1024  # render resolution (downsampled for every target)

# palette
PLATE = (18, 12, 17, 255)        # dark berry-tinted plate
PLATE_EDGE = (0, 0, 0, 0)        # outside rounded corners
BERRY_TOP = (251, 113, 133)      # #fb7185
BERRY_BOT = (190, 18, 60)        # #be123c
LEAF_A = (74, 222, 128)          # #4ade80
LEAF_B = (22, 163, 74)           # #16a34a
SEED = (255, 228, 230)           # #ffe4e6


def _lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


def _inside_body(x: float, y: float, S: float) -> bool:
    """Strawberry body: round shoulders + tapering point."""
    cx, top = 0.5 * S, 0.44 * S
    r = 0.215 * S
    # upper round part
    if (x - cx) ** 2 + (y - top) ** 2 <= r * r:
        return True
    # lower taper (y from top..tip)
    tip = 0.865 * S
    if top <= y <= tip:
        t = (y - top) / (tip - top)
        hw = r * (1.0 - t) ** 1.28
        return abs(x - cx) <= hw
    return False


def _inside_leaf(x: float, y: float, S: float) -> bool:
    """Three-leaf crown above the body."""
    cy = 0.355 * S
    spikes = (
        (0.500 * S, 0.205 * S, 0.440 * S, 0.560 * S),  # center apex + base span
        (0.335 * S, 0.265 * S, 0.275 * S, 0.445 * S),  # left
        (0.665 * S, 0.265 * S, 0.555 * S, 0.725 * S),  # right
    )
    for apex_x, apex_y, bx0, bx1 in spikes:
        if y < apex_y or y > cy:
            continue
        t = (y - apex_y) / (cy - apex_y)
        half = (bx1 - bx0) / 2 * t
        mid = (bx0 + bx1) / 2
        if abs(x - mid) <= half:
            return True
    return False


def _seed_hits(x: float, y: float, S: float) -> bool:
    pts = (
        (0.415, 0.505), (0.585, 0.515), (0.500, 0.585),
        (0.435, 0.660), (0.565, 0.660), (0.500, 0.735),
        (0.465, 0.445), (0.545, 0.445),
    )
    rr = 0.0135 * S
    for fx, fy in pts:
        dx, dy = x - fx * S, y - fy * S
        if dx * dx + dy * dy <= rr * rr:
            return True
    return False


def _highlight(x: float, y: float, S: float) -> float:
    """Soft top-left sheen on the berry (0..0.35 extra lightness)."""
    dx, dy = (x - 0.40 * S), (y - 0.40 * S)
    d = math.sqrt(dx * dx + dy * dy) / (0.30 * S)
    if d >= 1.0:
        return 0.0
    return 0.30 * (1.0 - d) ** 2


def render_master() -> list[list[tuple[int, int, int, int]]]:
    S = MASTER
    img = [[PLATE for _ in range(S)] for _ in range(S)]

    # rounded-plate mask
    m = 0.045 * S
    rad = 0.20 * S

    def on_plate(px: int, py: int) -> bool:
        x, y = px + 0.5, py + 0.5
        if x < m or x >= S - m or y < m or y >= S - m:
            return False
        # corner check
        for cx, cy in ((m + rad, m + rad), (S - m - rad, m + rad),
                       (m + rad, S - m - rad), (S - m - rad, S - m - rad)):
            if (x < cx if cx < S / 2 else x > cx) and (y < cy if cy < S / 2 else y > cy):
                if (x - cx) ** 2 + (y - cy) ** 2 > rad * rad:
                    return False
        return True

    for py in range(S):
        for px in range(S):
            if not on_plate(px, py):
                img[py][px] = PLATE_EDGE
                continue
            x, y = px + 0.5, py + 0.5

            if _inside_leaf(x, y, S):
                # slight two-tone: outer leaves darker
                shade = LEAF_B if x < 0.36 * S or x > 0.64 * S else LEAF_A
                img[py][px] = (*shade, 255)
                continue

            if _inside_body(x, y, S):
                t = max(0.0, min(1.0, (y - 0.30 * S) / (0.58 * S)))
                c = tuple(int(_lerp(BERRY_TOP[i], BERRY_BOT[i], t)) for i in range(3))
                h = _highlight(x, y, S)
                c = tuple(min(255, int(v + (255 - v) * h)) for v in c)
                if _seed_hits(x, y, S):
                    c = SEED
                img[py][px] = (*c, 255)
    return img


def downsample(master: list[list[tuple[int, int, int, int]]], target: int) -> bytes:
    f = MASTER // target
    assert MASTER % target == 0, "target must divide MASTER"
    rows = []
    for ty in range(target):
        row = bytearray()
        for tx in range(target):
            r = g = b = a = 0
            for sy in range(f):
                src = master[ty * f + sy]
                for sx in range(f):
                    pr, pg, pb, pa = src[tx * f + sx]
                    r += pr; g += pg; b += pb; a += pa
            n = f * f
            row += bytes((r // n, g // n, b // n, a // n))
        rows.append(bytes(row))
    raw = b"".join(b"\x00" + r for r in rows)
    ihdr = struct.pack(">IIBBBBB", target, target, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + _png_chunk(b"IHDR", ihdr)
        + _png_chunk(b"IDAT", zlib.compress(raw, 9))
        + _png_chunk(b"IEND", b"")
    )


def _png_chunk(tag: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def write_ico(path: str, pngs: list[tuple[int, bytes]]) -> None:
    entries, offset = [], 6 + 16 * len(pngs)
    blobs = []
    for size, png in pngs:
        w = 0 if size >= 256 else size
        entries.append(struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(png), offset))
        blobs.append(png)
        offset += len(png)
    with open(path, "wb") as f:
        f.write(struct.pack("<HHH", 0, 1, len(pngs)) + b"".join(entries) + b"".join(blobs))


def write_icns(path: str, png_by_size: dict[int, bytes]) -> None:
    types = {128: b"ic07", 256: b"ic08", 512: b"ic09", 1024: b"ic10"}
    chunks, total = [], 8
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
    print("Rendering strawberry master (this takes a few seconds)…")
    master = render_master()
    rendered = {}
    for name, size in (("32x32.png", 32), ("128x128.png", 128),
                       ("128x128@2x.png", 256), ("icon.png", 512)):
        data = downsample(master, size)
        rendered[size] = data
        with open(os.path.join(OUT_DIR, name), "wb") as f:
            f.write(data)
        print(f"  ✓ {name} ({size}px)")
    write_ico(os.path.join(OUT_DIR, "icon.ico"),
              [(32, rendered[32]), (128, rendered[128]), (256, rendered[256])])
    print("  ✓ icon.ico")
    write_icns(os.path.join(OUT_DIR, "icon.icns"),
               {128: rendered[128], 256: rendered[256], 512: rendered[512]})
    print(f"Icons written to {os.path.abspath(OUT_DIR)}")


if __name__ == "__main__":
    main()
