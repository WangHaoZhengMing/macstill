"""Extracts the SF Symbols the app uses from a local SF Pro font into
icons/sf/<name>.svg, for local testing only (SF Symbols may not be
distributed or used off Apple platforms; the shippable set is make_icons.py).

  python extract_symbols.py <SF-Pro-Text-*.otf dir> <sf-symbols names.json>

Each SVG is a square frame 1.25 em tall around the symbol's text position
(centered on the cap height, like SF Symbols beside text), so every symbol
drawn into the same square shows at the same point size.
"""

import json
import os
import sys

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont

SYMBOLS = """
chevron.left chevron.right chevron.down chevron.up chevron.forward chevron.up.chevron.down
square.grid.2x2 list.bullet rectangle.split.3x1 squares.below.rectangle square.grid.3x1.below.line.grid.1x2
square.and.arrow.up tag ellipsis ellipsis.circle magnifyingglass xmark.circle.fill
menubar.dock.rectangle document arrow.down.circle hammer house icloud internaldrive externaldrive network
trash eject.fill clock sidebar.left xmark minus plus info.circle folder folder.fill app.badge
desktopcomputer laptopcomputer globe person.crop.circle arrow.up.arrow.down circle.fill checkmark
""".split()

WEIGHTS = {"regular": "SF-Pro-Text-Regular.otf", "medium": "SF-Pro-Text-Medium.otf", "semibold": "SF-Pro-Text-Semibold.otf"}

font_dir, names_json = sys.argv[1], sys.argv[2]
names = dict(json.load(open(names_json, encoding="utf-8")))
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "icons", "sf")
os.makedirs(out, exist_ok=True)

for weight, file in WEIGHTS.items():
    font = TTFont(os.path.join(font_dir, file))
    glyphs, cmap = font.getGlyphSet(), font.getBestCmap()
    upm = font["head"].unitsPerEm
    cap = font["OS/2"].sCapHeight
    for name in SYMBOLS:
        ch = names.get(name)
        if ch is None or ord(ch) not in cmap:
            print("missing", name)
            continue
        g = glyphs[cmap[ord(ch)]]
        bp = BoundsPen(glyphs)
        g.draw(bp)
        x0, y0, x1, y1 = bp.bounds
        side = max(1.25 * upm, x1 - x0 + 0.1 * upm, y1 - y0 + 0.1 * upm)
        cx, cy = (x0 + x1) / 2, cap / 2
        pen = SVGPathPen(glyphs)
        g.draw(pen)
        # Font y points up, SVG's down: flip about the frame.
        svg = (
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{cx - side / 2:.0f} {-(cy + side / 2):.0f} {side:.0f} {side:.0f}">'
            f'<path transform="scale(1,-1)" d="{pen.getCommands()}"/></svg>'
        )
        suffix = "" if weight == "regular" else "@" + weight
        with open(os.path.join(out, f"{name}{suffix}.svg"), "w") as f:
            f.write(svg)
print("done")
