#!/usr/bin/env python3
"""Regenerate Strawberry app icons from scripts/strawberry.svg.

Requires rsvg-convert. Produces src-tauri/icons/{32x32,128x128,128x128@2x,icon}.png
plus icon.ico / icon.icns, and refreshes src/assets/strawberry-icon.png
used by the in-app brand chip.
"""
import pathlib
import shutil
import struct
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent
SVG = ROOT / "scripts" / "strawberry.svg"
OUT = ROOT / "src-tauri" / "icons"
ASSETS = ROOT / "src" / "assets"

SIZES = {512: "icon.png", 256: "128x128@2x.png", 128: "128x128.png", 32: "32x32.png"}


def rasterize() -> dict[int, bytes]:
    pngs: dict[int, bytes] = {}
    for size in SIZES:
        out = subprocess.run(
            ["rsvg-convert", "-w", str(size), "-h", str(size), str(SVG)],
            check=True,
            capture_output=True,
        ).stdout
        pngs[size] = out
        (OUT / SIZES[size]).write_bytes(out)
        print(f"  ✓ {SIZES[size]}")
    return pngs


def write_ico(pngs: dict[int, bytes]) -> None:
    entries, blobs, offset = [], [], 6 + 16 * 3
    for size in (32, 128, 256):
        png = pngs[size]
        b = 0 if size >= 256 else size
        entries.append(struct.pack("<BBBBHHII", b, b, 0, 0, 1, 32, len(png), offset))
        blobs.append(png)
        offset += len(png)
    (OUT / "icon.ico").write_bytes(struct.pack("<HHH", 0, 1, 3) + b"".join(entries) + b"".join(blobs))
    print("  ✓ icon.ico")


def write_icns(pngs: dict[int, bytes]) -> None:
    chunks, total = [], 8
    for size, ostype in ((128, b"ic07"), (256, b"ic08"), (512, b"ic09")):
        png = pngs[size]
        chunks.append(ostype + struct.pack(">I", len(png) + 8) + png)
        total += len(png) + 8
    (OUT / "icon.icns").write_bytes(b"icns" + struct.pack(">I", total) + b"".join(chunks))
    print("  ✓ icon.icns")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    ASSETS.mkdir(parents=True, exist_ok=True)
    pngs = rasterize()
    write_ico(pngs)
    write_icns(pngs)
    shutil.copy(OUT / "128x128.png", ASSETS / "strawberry-icon.png")
    print("  ✓ src/assets/strawberry-icon.png")


if __name__ == "__main__":
    main()
