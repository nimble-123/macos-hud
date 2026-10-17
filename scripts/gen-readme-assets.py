# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools>=4.50"]
# ///
"""Generate the README assets in docs/assets: logo and wordmark.

The overview showcase is rendered from the real view by scripts/gen-showcase.sh.

Text is converted to outlines so GitHub renders it without web fonts. The fonts
(Chakra Petch, SIL OFL) are downloaded once into a cache dir.

Usage:
    uv run scripts/gen-readme-assets.py
"""
import urllib.request
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "assets"
CACHE = Path.home() / "Library" / "Caches" / "hud-readme-fonts"
FONT_URLS = {
    "ChakraPetch-SemiBold.ttf": "https://github.com/google/fonts/raw/main/ofl/chakrapetch/ChakraPetch-SemiBold.ttf",
}


def font(name):
    path = CACHE / name
    if not path.exists():
        CACHE.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve(FONT_URLS[name], path)
    return TTFont(path)


OUT.mkdir(parents=True, exist_ok=True)

CHAKRA = font("ChakraPetch-SemiBold.ttf")


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


(OUT / "logo.svg").write_text(logo())
(OUT / "wordmark.svg").write_text(wordmark("light"))
(OUT / "wordmark-dark.svg").write_text(wordmark("dark"))
print("written", sorted(x.name for x in OUT.iterdir()))
