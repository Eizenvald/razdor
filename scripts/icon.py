#!/usr/bin/env python3
"""Builds every icon file from the master picture `data/icon/razdor.png` (1024×1024 RGBA).

    python3 -I scripts/icon.py    # rebuild everything from the master

To change the icon: replace data/icon/razdor.png (and the hand-tuned small sizes, if any), run
the script, commit data/icon/.

Outputs, all in data/icon/ (committed, so a build needs no Python):
  razdor-{16,24,32,48,64,128,256,512}.png  downscaled (Lanczos); a hand-tuned
                                           razdor-<n>.override.png (a tighter crop that reads
                                           better small: 16, 24, 32) wins over the downscale
  razdor.ico     Windows: 16–256, the 256 stored as PNG (build.rs embeds it in Razdor.exe)
  razdor.icns    macOS: ic07–ic10 (128, 256, 512, 1024) as PNG
  window-{16,32,64}.rgba  raw RGBA for the game window's icon (macroquad's Conf::icon)

Needs Pillow. The icon is Razdor's own art; nothing in it comes from the game.
"""

import io
import struct
import sys
from pathlib import Path

from PIL import Image

DIR = Path(__file__).resolve().parent.parent / "data" / "icon"
MASTER = DIR / "razdor.png"
PNG_SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
ICNS = [(b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024)]
WINDOW = [16, 32, 64]


def scaled(master, size):
    override = DIR / f"razdor-{size}.override.png"
    if override.is_file():
        img = Image.open(override).convert("RGBA")
        if img.size != (size, size):
            sys.exit(f"{override.name}: must be {size}×{size}")
        return img
    return master.resize((size, size), Image.LANCZOS)


def png_bytes(img):
    buf = io.BytesIO()
    img.save(buf, "PNG", optimize=True)
    return buf.getvalue()


def write_ico(images):
    entries, blobs = [], []
    offset = 6 + 16 * len(images)
    for img in images:
        w, h = img.size
        if w >= 256:
            blob = png_bytes(img)
        else:
            # BMP form (BITMAPINFOHEADER + BGRA bottom-up + 1-bit AND mask): the small sizes
            # stay readable by every Windows shell.
            bgra = img.transpose(Image.FLIP_TOP_BOTTOM).tobytes("raw", "BGRA")
            mask_row = ((w + 31) // 32) * 4
            header = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, len(bgra) + mask_row * h, 0, 0, 0, 0)
            blob = header + bgra + bytes(mask_row * h)
        entries.append(struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(blob), offset))
        blobs.append(blob)
        offset += len(blob)
    data = struct.pack("<HHH", 0, 1, len(images)) + b"".join(entries) + b"".join(blobs)
    (DIR / "razdor.ico").write_bytes(data)


def write_icns(master):
    chunks = b""
    for kind, size in ICNS:
        blob = png_bytes(master if size == master.width else master.resize((size, size), Image.LANCZOS))
        chunks += kind + struct.pack(">I", 8 + len(blob)) + blob
    (DIR / "razdor.icns").write_bytes(b"icns" + struct.pack(">I", 8 + len(chunks)) + chunks)


def main():
    if not MASTER.is_file():
        sys.exit(f"no master picture: {MASTER}")
    master = Image.open(MASTER).convert("RGBA")
    if master.size != (1024, 1024):
        sys.exit(f"{MASTER.name}: must be 1024×1024, is {master.width}×{master.height}")
    images = {s: scaled(master, s) for s in PNG_SIZES}
    for s, img in images.items():
        img.save(DIR / f"razdor-{s}.png", optimize=True)
    write_ico([images[s] for s in ICO_SIZES])
    write_icns(master)
    for s in WINDOW:
        (DIR / f"window-{s}.rgba").write_bytes(images[s].tobytes("raw", "RGBA"))
    print("icons written to", DIR)


if __name__ == "__main__":
    main()
