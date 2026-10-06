#!/usr/bin/env python3
"""brow product icon generator (Phase 5).

Renders the brow brand icon programmatically so every shipped artifact
(.deb, .rpm, AppImage, MSI, portable zip) carries the same bitmap set and
the assets stay reproducible from source — no binary blobs in git without
a generator.

Design: dark navy rounded tile, a privacy shield arc in teal, and a bold
lowercase "b" — the browser that keeps you inside a safe perimeter.

Usage:
    python3 generate_icons.py [--out DIR]

Outputs (into --out, default: script directory):
    brow_512.png  brow_256.png  brow_128.png  brow_64.png  brow.ico
"""

from __future__ import annotations

import argparse
import os

from PIL import Image, ImageDraw, ImageFont

TILE_BG_TOP = (16, 24, 38)      # #101826 deep navy
TILE_BG_BOTTOM = (23, 37, 56)   # #172538
SHIELD = (45, 212, 191)         # #2DD4BF teal
LETTER = (237, 244, 255)        # #EDF4FF near-white
FONT_PATH = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"

SIZES = (512, 256, 128, 64)


def rounded_tile(size: int, radius_ratio: float = 0.22) -> tuple[Image.Image, int]:
    """Vertical-gradient rounded square, anti-aliased via 4x supersampling."""
    scale = 4
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    grad = Image.new("RGBA", (s, s))
    gd = ImageDraw.Draw(grad)
    top, bot = TILE_BG_TOP, TILE_BG_BOTTOM
    for y in range(s):
        t = y / max(1, s - 1)
        c = tuple(int(top[i] + (bot[i] - top[i]) * t) for i in range(3)) + (255,)
        gd.line([(0, y), (s, y)], fill=c)
    mask = Image.new("L", (s, s), 0)
    md = ImageDraw.Draw(mask)
    md.rounded_rectangle([0, 0, s - 1, s - 1], radius=int(s * radius_ratio), fill=255)
    img.paste(grad, (0, 0), mask)
    return img, scale


def draw_emblem(size: int) -> Image.Image:
    img, scale = rounded_tile(size)
    s = size * scale
    d = ImageDraw.Draw(img)

    # Shield arc: a thick open ring suggesting a perimeter around the mark.
    cx, cy = s * 0.50, s * 0.52
    r = s * 0.335
    width = max(2, int(s * 0.052))
    # 300-degree arc, opening at the top — like an unfinished shield.
    d.arc(
        [cx - r, cy - r, cx + r, cy + r],
        start=210,
        end=510,
        fill=SHIELD,
        width=width,
    )
    # Shield top: a chevron capping the ring opening.
    ch = s * 0.085
    ch_w = s * 0.17
    left = (cx - ch_w, cy - r + ch * 0.15)
    apex = (cx, cy - r - ch * 0.55)
    right = (cx + ch_w, cy - r + ch * 0.15)
    d.line([left, apex, right], fill=SHIELD, width=width, joint="curve")

    # The glyph: bold lowercase "b", optically centered.
    font = ImageFont.truetype(FONT_PATH, int(s * 0.42))
    text = "b"
    bbox = d.textbbox((0, 0), text, font=font)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
    tx = cx - tw / 2 - bbox[0]
    ty = cy - th / 2 - bbox[1]
    # Subtle depth: a 1-2px teal shadow under the near-white glyph.
    shadow_off = max(1, s // 256)
    d.text((tx + shadow_off, ty + shadow_off), text, font=font, fill=SHIELD)
    d.text((tx, ty), text, font=font, fill=LETTER)

    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.dirname(os.path.abspath(__file__)))
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)

    rendered = {}
    for size in SIZES:
        img = draw_emblem(size)
        name = f"brow_{size}.png"
        img.save(os.path.join(args.out, name))
        rendered[size] = img
        print(f"wrote {name}")

    # Windows ICO embeds every raster size for crisp Explorer/MSI icons.
    ico_path = os.path.join(args.out, "brow.ico")
    rendered[256].save(
        ico_path,
        format="ICO",
        sizes=[(s, s) for s in (256, 128, 64, 48, 32, 16)],
    )
    print("wrote brow.ico")


if __name__ == "__main__":
    main()
