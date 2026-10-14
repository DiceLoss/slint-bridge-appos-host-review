#!/usr/bin/env python3
"""Rebuild deterministic PNG list fixtures; requires CairoSVG."""
from pathlib import Path
import cairosvg

ROOT = Path(__file__).resolve().parent
PALETTES = {
    "blue": ("#e7f6fa", "#c9dff2", "#9acdcf"),
    "amber": ("#fff2df", "#e9d7eb", "#dfbfbc"),
}
for shape, (width, height) in {"row": (800, 112), "card": (244, 328)}.items():
    for name, (start, end, accent) in PALETTES.items():
        shadows = "".join(
            f'<rect x="{8-i}" y="{10-i}" width="{width-16+2*i}" height="{height-22+2*i}" rx="{12+i}" fill="#102c43" opacity="0.018"/>'
            for i in range(6, -1, -1)
        )
        svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<defs>
  <linearGradient id="paper" x2="1" y2="1"><stop stop-color="{start}"/><stop offset="1" stop-color="{end}"/></linearGradient>
  <radialGradient id="glow"><stop stop-color="white" stop-opacity="0.8"/><stop offset="1" stop-color="white" stop-opacity="0"/></radialGradient>
  <clipPath id="card"><rect x="8" y="6" width="{width-16}" height="{height-22}" rx="12"/></clipPath>
</defs>
{shadows}
<rect x="8" y="6" width="{width-16}" height="{height-22}" rx="12" fill="url(#paper)"/>
<g clip-path="url(#card)">
  <ellipse cx="{width*0.25}" cy="{height*0.1}" rx="{width*0.6}" ry="{height*0.9}" fill="url(#glow)"/>
  <circle cx="{width+25}" cy="{height*0.6}" r="{height*0.7}" fill="none" stroke="{accent}" stroke-width="12" opacity="0.25"/>
  <circle cx="{width+25}" cy="{height*0.6}" r="{height*0.9}" fill="none" stroke="white" stroke-width="2" opacity="0.55"/>
</g>
<rect x="8.5" y="6.5" width="{width-17}" height="{height-23}" rx="12" fill="none" stroke="white" stroke-opacity="0.7"/>
</svg>
'''
        source = ROOT / f"rich-{shape}-{name}.svg"
        source.write_text(svg)
        cairosvg.svg2png(bytestring=svg.encode(), write_to=str(source.with_suffix('.png')))
