#!/usr/bin/env python3
"""The app icons of Mist's own apps (Files, Player, Settings), written as SVG.
   native/assets/icons/make.py            -> hicolor/scalable/apps/mist-*.svg next to this file
One family: the macOS icon grid (a squircle of 824 px on a 1024 canvas, a soft shadow under it), Mist's dusk
colours, a lit glass rim and one plain symbol each. No text and no filters beyond a drop shadow, so the
files render the same in resvg (the dock's tiles), librsvg and Qt."""
import math
from pathlib import Path

OUT = Path(__file__).parent / "hicolor/scalable/apps"
C, R = 512.0, 412.0  # centre, half the squircle's side


def squircle(cx=C, cy=C, r=R, n=5.0, steps=192):
    # superellipse |x|^n + |y|^n = r^n: the iOS / macOS tile outline
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, s = math.cos(t), math.sin(t)
        x = cx + r * math.copysign(abs(c) ** (2 / n), c)
        y = cy + r * math.copysign(abs(s) ** (2 / n), s)
        pts.append(f"{x:.1f},{y:.1f}")
    return "M" + " L".join(pts) + " Z"


TILE = squircle()


def frame(name, bg_stops, body, extra_defs=""):
    stops = "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in bg_stops)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <title>{name}</title>
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0.35" y2="1">{stops}</linearGradient>
    <linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.30"/>
      <stop offset="0.45" stop-color="#ffffff" stop-opacity="0.04"/>
      <stop offset="1" stop-color="#ffffff" stop-opacity="0"/>
    </linearGradient>
    <linearGradient id="rim" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.55"/>
      <stop offset="0.5" stop-color="#ffffff" stop-opacity="0.08"/>
      <stop offset="1" stop-color="#ffffff" stop-opacity="0.22"/>
    </linearGradient>
    <filter id="drop" x="-10%" y="-10%" width="120%" height="125%">
      <feDropShadow dx="0" dy="14" stdDeviation="16" flood-color="#000000" flood-opacity="0.32"/>
    </filter>
    <filter id="lift" x="-20%" y="-20%" width="140%" height="150%">
      <feDropShadow dx="0" dy="18" stdDeviation="22" flood-color="#0b0614" flood-opacity="0.45"/>
    </filter>
    <clipPath id="tile"><path d="{TILE}"/></clipPath>
    {extra_defs}
  </defs>
  <path d="{TILE}" fill="url(#bg)" filter="url(#drop)"/>
  <g clip-path="url(#tile)">
{body}
    <path d="M100,100 H924 V560 Q512,470 100,560 Z" fill="url(#sheen)"/>
  </g>
  <path d="{TILE}" fill="none" stroke="url(#rim)" stroke-width="5"/>
</svg>
"""


def files():
    # a folder of frosted glass: the back with its tab, a sheet of paper, the lit front
    defs = """
    <linearGradient id="back" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#7cc4ff"/><stop offset="1" stop-color="#2f7fe0"/>
    </linearGradient>
    <linearGradient id="front" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#bfe4ff"/><stop offset="0.12" stop-color="#8fd0ff"/><stop offset="1" stop-color="#3d8cf0"/>
    </linearGradient>
    <linearGradient id="paper" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#e3ecf7"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.5" cy="0.95" r="0.75">
      <stop offset="0" stop-color="#6fb8ff" stop-opacity="0.35"/><stop offset="1" stop-color="#6fb8ff" stop-opacity="0"/>
    </radialGradient>"""
    body = """
    <rect x="100" y="100" width="824" height="824" fill="url(#glow)"/>
    <g filter="url(#lift)">
      <path d="M236,318 Q236,286 268,286 H430 Q450,286 464,300 L500,336 H756 Q788,336 788,368 V700 H236 Z" fill="url(#back)"/>
      <rect x="292" y="356" width="440" height="260" rx="18" fill="url(#paper)" transform="rotate(-4 512 486)"/>
      <path d="M220,452 Q220,420 252,420 H772 Q804,420 804,452 L788,712 Q786,744 754,744 H270 Q238,744 236,712 Z" fill="url(#front)"/>
      <path d="M252,421 H772" stroke="#ffffff" stroke-opacity="0.85" stroke-width="5" stroke-linecap="round"/>
      <rect x="452" y="520" width="120" height="16" rx="8" fill="#ffffff" fill-opacity="0.55"/>
    </g>"""
    return frame("Mist Files", [(0, "#1d3f78"), (0.55, "#173061"), (1, "#0e1b3a")], body, defs)


def player():
    # a dusk screen (Mist's dunes under the sunset) behind a lit glass play button
    defs = """
    <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#2a1740"/><stop offset="0.55" stop-color="#7a3550"/><stop offset="1" stop-color="#f08a4b"/>
    </linearGradient>
    <radialGradient id="sun" cx="0.5" cy="0.78" r="0.42">
      <stop offset="0" stop-color="#ffd59a" stop-opacity="0.95"/><stop offset="0.35" stop-color="#ff9a55" stop-opacity="0.55"/>
      <stop offset="1" stop-color="#ff9a55" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="dune" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#3b1f2a"/><stop offset="1" stop-color="#12090f"/>
    </linearGradient>
    <linearGradient id="btn" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.38"/><stop offset="1" stop-color="#ffffff" stop-opacity="0.12"/>
    </linearGradient>"""
    body = f"""
    <rect x="100" y="100" width="824" height="824" fill="url(#sky)"/>
    <circle cx="512" cy="700" r="330" fill="url(#sun)"/>
    <path d="M100,720 Q300,640 520,700 T924,668 V924 H100 Z" fill="url(#dune)"/>
    <path d="M100,800 Q340,730 600,790 T924,770 V924 H100 Z" fill="#0b0609" fill-opacity="0.7"/>
    <g filter="url(#lift)">
      <circle cx="512" cy="472" r="178" fill="url(#btn)"/>
      <circle cx="512" cy="472" r="176" fill="none" stroke="#ffffff" stroke-opacity="0.55" stroke-width="5"/>
      <path d="M466,382 Q466,356 489,369 L622,452 Q644,466 622,480 L489,563 Q466,576 466,550 Z" fill="#ffffff"/>
    </g>
    <rect x="232" y="842" width="560" height="14" rx="7" fill="#ffffff" fill-opacity="0.22"/>
    <rect x="232" y="842" width="236" height="14" rx="7" fill="#ffffff" fill-opacity="0.9"/>"""
    return frame("Mist Player", [(0, "#2a1740"), (1, "#f08a4b")], body, defs)


def gear(cx, cy, r_out, r_in, teeth, tooth_w=0.42, round_=10):
    # a gear outline: `teeth` flat-topped teeth between r_in and r_out
    pts = []
    for i in range(teeth):
        a0 = 2 * math.pi * i / teeth
        step = 2 * math.pi / teeth
        half = step * tooth_w / 2
        for a, r in ((a0 - half * 1.25, r_in), (a0 - half, r_out), (a0 + half, r_out), (a0 + half * 1.25, r_in)):
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return "M" + " L".join(f"{x:.1f},{y:.1f}" for x, y in pts) + " Z"


def settings():
    # a machined gear of brushed metal, a warm Mist ring at its hub
    defs = """
    <linearGradient id="metal" x1="0" y1="0" x2="0.4" y2="1">
      <stop offset="0" stop-color="#f4f6fa"/><stop offset="0.5" stop-color="#b9c0cc"/><stop offset="1" stop-color="#7d8696"/>
    </linearGradient>
    <linearGradient id="inner" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#5d6575"/><stop offset="1" stop-color="#2c313b"/>
    </linearGradient>
    <linearGradient id="ring" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#ffc07a"/><stop offset="1" stop-color="#e2683a"/>
    </linearGradient>"""
    g = gear(512, 500, 300, 248, 12)
    body = f"""
    <g filter="url(#lift)">
      <path d="{g}" fill="url(#metal)" stroke="#ffffff" stroke-opacity="0.5" stroke-width="3" stroke-linejoin="round"/>
      <circle cx="512" cy="500" r="196" fill="url(#inner)"/>
      <circle cx="512" cy="500" r="196" fill="none" stroke="#000000" stroke-opacity="0.25" stroke-width="6"/>
      <circle cx="512" cy="500" r="118" fill="none" stroke="url(#ring)" stroke-width="34"/>
      <circle cx="512" cy="500" r="58" fill="url(#metal)"/>
    </g>"""
    return frame("Mist Settings", [(0, "#4a5263"), (0.6, "#2b303b"), (1, "#181b22")], body, defs)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for name, f in (("mist-files", files), ("mist-player", player), ("mist-settings", settings)):
        (OUT / f"{name}.svg").write_text(f(), encoding="utf-8")
        print(OUT / f"{name}.svg")


main()
