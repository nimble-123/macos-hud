# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools>=4.50", "resvg-py>=0.5"]
# ///
"""Generate the README assets in docs/assets: logo, wordmark and social preview.

The overview showcase is rendered from the real view by scripts/gen-showcase.sh;
the social preview (1280 x 640, uploaded under Settings > General) crops it, so
render the showcase first.

Text is converted to outlines so GitHub renders it without web fonts. The fonts
(Chakra Petch and IBM Plex Mono, SIL OFL) are downloaded once into a cache dir.

Usage:
    uv run scripts/gen-readme-assets.py
"""
import base64
import urllib.request
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
import resvg_py

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "assets"
CACHE = Path.home() / "Library" / "Caches" / "hud-readme-fonts"
FONT_URLS = {
    "ChakraPetch-SemiBold.ttf": "https://github.com/google/fonts/raw/main/ofl/chakrapetch/ChakraPetch-SemiBold.ttf",
    "ChakraPetch-Regular.ttf": "https://github.com/google/fonts/raw/main/ofl/chakrapetch/ChakraPetch-Regular.ttf",
    "IBMPlexMono-Regular.ttf": "https://github.com/google/fonts/raw/main/ofl/ibmplexmono/IBMPlexMono-Regular.ttf",
}


def font(name):
    path = CACHE / name
    if not path.exists():
        CACHE.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve(FONT_URLS[name], path)
    return TTFont(path)


OUT.mkdir(parents=True, exist_ok=True)

CHAKRA = font("ChakraPetch-SemiBold.ttf")
CHAKRA_REGULAR = font("ChakraPetch-Regular.ttf")
PLEX = font("IBMPlexMono-Regular.ttf")


def text_path(font, text, x, y, size):
    """Return (svg path d, advance width) for text with baseline at y."""
    gs = font.getGlyphSet()
    cmap = font.getBestCmap()
    upm = font["head"].unitsPerEm
    scale = size / upm
    pen = SVGPathPen(gs)
    cx = 0.0
    for ch in text:
        name = cmap.get(ord(ch))
        if name is None:
            raise SystemExit(f"missing glyph {ch!r}")
        tp = TransformPen(pen, (scale, 0, 0, -scale, x + cx * scale, y))
        gs[name].draw(tp)
        cx += font["hmtx"][name][0]
    return pen.getCommands(), cx * scale


PALETTES = {
    "dark": dict(bg="#161B22", chrome="#1F252E", border="#2E3640", ink="#E6E8E3", muted="#AEB5BD",
                 soft="#8F98A3", rule="#3A424D", focus="#E870C6", ok="#5CC98A", adv="#4CC0D6",
                 cau="#E8B23A", warn="#F0675B"),
    "light": dict(bg="#F2F3F1", chrome="#E6E8E4", border="#C4C8C2", ink="#1C2128", muted="#47505C",
                  soft="#5A6470", rule="#C4C8C2", focus="#B02A8A", ok="#24774A", adv="#13738A",
                  cau="#8F6200", warn="#B42F25"),
}

# ---------------------------------------------------------------- logo
def logo():
    """Flight path vector (the HUD 'bird') on a cockpit panel, with a horizon line."""
    s = 112
    m = "#E870C6"
    cx, cy, r = 56, 60, 11
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {s} {s}" width="{s}" height="{s}" role="img" aria-labelledby="lt"><title id="lt">hud</title>
<rect width="{s}" height="{s}" rx="24" fill="#161B22"/>
<rect x="0.75" y="0.75" width="{s - 1.5}" height="{s - 1.5}" rx="23.25" fill="none" stroke="#2E3640" stroke-width="1.5"/>
<path d="M18 68H38M74 68H94" stroke="#8F98A3" stroke-width="2" stroke-linecap="round"/>
<path d="M26 75H34M78 75H86" stroke="#8F98A3" stroke-width="2" stroke-linecap="round" opacity="0.5"/>
<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{m}" stroke-width="4.5"/>
<path d="M{cx - r - 17} {cy}H{cx - r}M{cx + r} {cy}H{cx + r + 17}M{cx} {cy - r}V{cy - r - 12}" stroke="{m}" stroke-width="4.5" stroke-linecap="round"/>
</svg>
'''


def wordmark(theme):
    p = PALETTES[theme]
    d, w = text_path(CHAKRA, "hud", 0, 56, 64)
    pad = 4
    W = w + pad * 2
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W:.0f} 80" width="{W:.0f}" height="80" role="img" '
            f'aria-labelledby="wt"><title id="wt">hud</title>'
            f'<g transform="translate({pad} 0)"><path d="{d}" fill="{p["ink"]}"/></g>'
            f'<rect x="{pad}" y="68" width="{w:.1f}" height="4" rx="2" fill="{p["focus"]}"/></svg>\n')


# ---------------------------------------------------------------- social preview
def social_preview():
    """Brand and claim on the left, the real overview bleeding off the right edge.

    Text stays inside GitHub's 40 px safe border; only the screenshot crosses it.
    """
    p = PALETTES["dark"]
    W, H, x0 = 1280, 640, 72
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" '
             f'viewBox="0 0 {W} {H}" width="{W}" height="{H}">',
             f'<rect width="{W}" height="{H}" fill="{p["bg"]}"/>']

    # Overview screenshot in a window frame, cropped by the canvas edges.
    shot = base64.b64encode((OUT / "showcase-dark.png").read_bytes()).decode()
    sx, sy, sw = 640, 96, 980
    sh = sw * 1332 / 1848
    parts.append(f'<clipPath id="win"><rect x="{sx}" y="{sy}" width="{sw}" height="{sh:.0f}" rx="12"/></clipPath>'
                 f'<image x="{sx}" y="{sy}" width="{sw}" height="{sh:.0f}" clip-path="url(#win)" '
                 f'xlink:href="data:image/png;base64,{shot}"/>'
                 f'<rect x="{sx + 0.75}" y="{sy + 0.75}" width="{sw}" height="{sh:.0f}" rx="12" fill="none" '
                 f'stroke="{p["border"]}" stroke-width="1.5"/>')

    # Logo and wordmark on one line.
    parts.append(logo().replace('<svg xmlns="http://www.w3.org/2000/svg" ', f'<svg x="{x0}" y="96" ', 1)
                 .replace('width="112" height="112"', 'width="88" height="88"', 1))
    d, w = text_path(CHAKRA, "hud", x0 + 112, 168, 88)
    parts.append(f'<path d="{d}" fill="{p["ink"]}"/>'
                 f'<rect x="{x0 + 112}" y="180" width="{w:.1f}" height="5" rx="2.5" fill="{p["focus"]}"/>')

    def lines(font, rows, y, size, leading, fill, width):
        nonlocal parts
        for i, row in enumerate(rows):
            d, w = text_path(font, row, x0, y + i * leading, size)
            if w > width:
                raise SystemExit(f"{row!r} is {w:.0f} px wide, more than {width}")
            parts.append(f'<path d="{d}" fill="{fill}"/>')

    column = sx - x0 - 40
    lines(CHAKRA, ["A heads-up display", "for your developer machine"], 288, 38, 50, p["ink"], column)
    lines(CHAKRA_REGULAR, ["System load, Git workspaces, package updates", "and AI usage in one terminal view."],
          410, 24, 36, p["muted"], column)

    # Install command in a quiet box.
    d_prompt, wp = text_path(PLEX, "$ ", x0 + 20, 551, 20)
    d_cmd, wc = text_path(PLEX, "brew install nimble-123/tap/hud", x0 + 20 + wp, 551, 20)
    parts.append(f'<rect x="{x0 + 0.75}" y="516.75" width="{wp + wc + 40:.0f}" height="50" rx="6" fill="{p["chrome"]}" '
                 f'stroke="{p["border"]}" stroke-width="1.5"/>'
                 f'<path d="{d_prompt}" fill="{p["focus"]}"/><path d="{d_cmd}" fill="{p["ink"]}"/>')
    parts.append("</svg>")
    svg = "".join(parts)
    png = resvg_py.svg_to_bytes(svg_string=svg, skip_system_fonts=True)
    (OUT / "social-preview.png").write_bytes(bytes(png))


(OUT / "logo.svg").write_text(logo())
(OUT / "wordmark.svg").write_text(wordmark("light"))
(OUT / "wordmark-dark.svg").write_text(wordmark("dark"))
social_preview()
print("written", sorted(x.name for x in OUT.iterdir()))
