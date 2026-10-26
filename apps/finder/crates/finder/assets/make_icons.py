"""Generates the app's icons (SF Symbols-style line icons, tinted at runtime,
plus full-color file icons) into icons/. Run: python make_icons.py"""

import os

OUT = os.path.join(os.path.dirname(__file__), "icons")
S = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{}</svg>'
F = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="#000">{}</svg>'

LINE = {
    # navigation
    "chevron-left": S.format('<path d="M15 4.5 7.5 12l7.5 7.5" stroke-width="2"/>'),
    "chevron-right": S.format('<path d="M9 4.5 16.5 12 9 19.5" stroke-width="2"/>'),
    "chevron-down": S.format('<path d="M5 9l7 7 7-7" stroke-width="2"/>'),
    "chevron-up": S.format('<path d="M5 15l7-7 7 7" stroke-width="2"/>'),
    "disclosure": S.format('<path d="M9.5 6 15.5 12 9.5 18" stroke-width="2"/>'),
    # sidebar
    "applications": S.format('<path d="M12.6 3.5 5 18.5M11.4 3.5 19 18.5M3.5 13.5h17"/>'),
    "desktop": S.format('<rect x="2.8" y="4.5" width="18.4" height="15" rx="2.4"/><path d="M2.8 15.5h18.4" stroke-width="2.6"/>'),
    "document": S.format(
        '<path d="M6.5 2.8h7.2l4.8 4.8v12.6a1.6 1.6 0 0 1-1.6 1.6H6.5a1.6 1.6 0 0 1-1.6-1.6V4.4a1.6 1.6 0 0 1 1.6-1.6z"/>'
        '<path d="M13.5 2.9v3.4a1.6 1.6 0 0 0 1.6 1.6h3.3"/>'
    ),
    "download": S.format('<circle cx="12" cy="12" r="9.2"/><path d="M12 7v9.2M8.2 12.6 12 16.4l3.8-3.8"/>'),
    "hammer": S.format(
        '<path d="M13.5 9.5 4 19a1.8 1.8 0 0 0 2.6 2.6l9.5-9.5"/>'
        '<path d="M11 6.2 14.5 2.8c2.5-.4 4.7.4 6.4 2.2l-.6.6-2-.2-1.6 1.6.3 1.4 1 1L16.2 11.2l-1-1-1.4-.3-.6.6-2.8-2.8z"/>'
    ),
    "cloud": S.format('<path d="M7 19h10.5a4.5 4.5 0 0 0 .6-8.96A6 6 0 0 0 6.6 9.2 4.9 4.9 0 0 0 7 19z"/>'),
    "house": S.format('<path d="M3 11.2 12 3.6l9 7.6M5.4 9.4v10.2a.9.9 0 0 0 .9.9h3.8v-5.8h3.8v5.8h3.8a.9.9 0 0 0 .9-.9V9.4"/>'),
    "drive": S.format(
        '<path d="M5.4 6h13.2l2.8 6.6v4.6a1.6 1.6 0 0 1-1.6 1.6H4.2a1.6 1.6 0 0 1-1.6-1.6v-4.6z"/>'
        '<path d="M2.7 12.6h18.6M6.5 15.7h.01M9 15.7h.01M11.5 15.7h.01"/>'
    ),
    "external-drive": S.format('<rect x="2.6" y="7.5" width="18.8" height="9.4" rx="2.2"/><path d="M6 12.2h.01M15.5 12.2h3"/>'),
    "airdrop": S.format(
        '<circle cx="12" cy="11" r="1.4" fill="#000"/>'
        '<path d="M8.6 14.4a4.8 4.8 0 1 1 6.8 0M5.8 17.2a8.8 8.8 0 1 1 12.4 0M10.6 20.5 12 15.5l1.4 5"/>'
    ),
    "globe": S.format(
        '<circle cx="12" cy="12" r="9.2"/>'
        '<path d="M2.8 12h18.4M12 2.8c2.6 2.6 3.8 5.6 3.8 9.2s-1.2 6.6-3.8 9.2c-2.6-2.6-3.8-5.6-3.8-9.2S9.4 5.4 12 2.8zM4.4 7h15.2M4.4 17h15.2"/>'
    ),
    "trash": S.format(
        '<path d="M3.6 6h16.8M9 6V4.2a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1V6M5.6 6l1 14a1.8 1.8 0 0 0 1.8 1.6h7.2a1.8 1.8 0 0 0 1.8-1.6l1-14M10 9.8v8.2M14 9.8v8.2"/>'
    ),
    "eject": F.format('<path d="M12 4.5 19.5 13.5h-15z"/><rect x="4.5" y="16" width="15" height="2.6" rx="1"/>'),
    "checkmark": S.format('<path d="M5 12.5l4.5 4.5L19 7" stroke-width="2.2"/>'),
    "clock": S.format('<circle cx="12" cy="12" r="9.2"/><path d="M12 6.5V12l3.6 2.4"/>'),
    # toolbar
    "grid": S.format(
        '<rect x="3.5" y="3.5" width="7" height="7" rx="1.6"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.6"/>'
        '<rect x="3.5" y="13.5" width="7" height="7" rx="1.6"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.6"/>'
    ),
    "list": S.format(
        '<path d="M8.5 6h12M8.5 12h12M8.5 18h12"/>'
        '<circle cx="4" cy="6" r=".6" fill="#000"/><circle cx="4" cy="12" r=".6" fill="#000"/><circle cx="4" cy="18" r=".6" fill="#000"/>'
    ),
    "columns": S.format('<rect x="2.6" y="4" width="18.8" height="16" rx="2.2"/><path d="M9 4v16M15 4v16"/>'),
    "gallery": S.format('<rect x="2.6" y="3.4" width="18.8" height="12.6" rx="2"/><path d="M3.5 20h1M7.5 20h1M11.5 20h1M15.5 20h1M19.5 20h1" stroke-width="2.2"/>'),
    "group": S.format(
        '<rect x="2.8" y="4" width="4.6" height="3.4" rx=".8"/><rect x="9.7" y="4" width="4.6" height="3.4" rx=".8"/>'
        '<rect x="16.6" y="4" width="4.6" height="3.4" rx=".8"/><path d="M2.8 10.3h18.4" stroke-width="1.2"/>'
        '<rect x="2.8" y="13" width="4.6" height="3.4" rx=".8"/><rect x="9.7" y="13" width="4.6" height="3.4" rx=".8"/>'
        '<rect x="16.6" y="13" width="4.6" height="3.4" rx=".8"/><rect x="2.8" y="18.6" width="4.6" height="3" rx=".8"/>'
        '<rect x="9.7" y="18.6" width="4.6" height="3" rx=".8"/>'
    ),
    "share": S.format(
        '<path d="M12 2.8v12.4M8 6.6l4-3.8 4 3.8M8.4 9.6H6.6a1.8 1.8 0 0 0-1.8 1.8v8a1.8 1.8 0 0 0 1.8 1.8h10.8a1.8 1.8 0 0 0 1.8-1.8v-8a1.8 1.8 0 0 0-1.8-1.8h-1.8"/>'
    ),
    "tag": S.format(
        '<path d="M3 4.6v6a1.6 1.6 0 0 0 .5 1.1l9 9a1.6 1.6 0 0 0 2.2 0l6-6a1.6 1.6 0 0 0 0-2.2l-9-9A1.6 1.6 0 0 0 10.6 3H4.6A1.6 1.6 0 0 0 3 4.6z"/>'
        '<circle cx="7.6" cy="7.6" r="1.4"/>'
    ),
    "ellipsis": F.format('<circle cx="5" cy="12" r="1.9"/><circle cx="12" cy="12" r="1.9"/><circle cx="19" cy="12" r="1.9"/>'),
    "ellipsis-circle": S.format(
        '<circle cx="12" cy="12" r="9.2"/>'
        '<circle cx="7.8" cy="12" r=".5" fill="#000"/><circle cx="12" cy="12" r=".5" fill="#000"/><circle cx="16.2" cy="12" r=".5" fill="#000"/>'
    ),
    "search": S.format('<circle cx="10.2" cy="10.2" r="6.6" stroke-width="1.9"/><path d="M15.2 15.2 20.6 20.6" stroke-width="2.1"/>'),
    "xmark-circle": F.format(
        '<path fill-rule="evenodd" d="M12 2.5a9.5 9.5 0 1 0 0 19 9.5 9.5 0 0 0 0-19zM9 7.6l3 3 3-3 1.4 1.4-3 3 3 3-1.4 1.4-3-3-3 3L7.6 15l3-3-3-3z"/>'
    ),
    # traffic-light glyphs, shown while the pointer is over the group
    "tl-close": S.format('<path d="M7.5 7.5l9 9M16.5 7.5l-9 9" stroke-width="2.6"/>'),
    "tl-min": S.format('<path d="M6 12h12" stroke-width="2.8"/>'),
    "tl-max": F.format('<path d="M7 7h7.5L7 14.5zM17 17H9.5L17 9.5z"/>'),
}

COLOR = {
    "folder": """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 100">
<defs>
<linearGradient id="back" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#4FB2E6"/><stop offset="1" stop-color="#3A9FD9"/></linearGradient>
<linearGradient id="front" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#6DCFF6"/><stop offset="1" stop-color="#48A9DD"/></linearGradient>
</defs>
<path d="M10 4h32c4 0 6 1.5 8.5 4.5L56 15h62a8 8 0 0 1 8 8v65a8 8 0 0 1-8 8H10a8 8 0 0 1-8-8V12a8 8 0 0 1 8-8z" fill="url(#back)"/>
<rect x="9" y="20" width="110" height="20" rx="4" fill="#fff"/>
<path d="M10 25h108a8 8 0 0 1 8 8v55a8 8 0 0 1-8 8H10a8 8 0 0 1-8-8V33a8 8 0 0 1 8-8z" fill="url(#front)"/>
<path d="M10 95.2h108" stroke="#2f88bd" stroke-opacity=".35"/>
</svg>""",
    "file": """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 128">
<path d="M10 3h55l32 32v84a6 6 0 0 1-6 6H10a6 6 0 0 1-6-6V9a6 6 0 0 1 6-6z" fill="#fff" stroke="#cfcfcf" stroke-width="2"/>
<path d="M65 3v26a6 6 0 0 0 6 6h26" fill="#f0f0f0" stroke="#cfcfcf" stroke-width="2"/>
</svg>""",
    "app": """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f4f6f9"/><stop offset="1" stop-color="#d9dde3"/></linearGradient></defs>
<rect x="10" y="10" width="108" height="108" rx="26" fill="url(#g)" stroke="#c4c9d0" stroke-width="1.5"/>
<g stroke="#b9c3cf" stroke-width="3" fill="none"><path d="M30 64h68M64 30v68"/><circle cx="64" cy="64" r="22"/><rect x="36" y="36" width="56" height="56" rx="6"/></g>
</svg>""",
    "disk": """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#e9e9eb"/><stop offset="1" stop-color="#b8b8bd"/></linearGradient></defs>
<rect x="8" y="34" width="112" height="60" rx="10" fill="url(#g)" stroke="#9a9aa0" stroke-width="2"/>
<circle cx="100" cy="64" r="4" fill="#5fd35f"/>
</svg>""",
}

os.makedirs(OUT, exist_ok=True)
for name, svg in {**LINE, **COLOR}.items():
    with open(os.path.join(OUT, name + ".svg"), "w") as f:
        f.write(svg)
print(len(LINE) + len(COLOR), "icons")
