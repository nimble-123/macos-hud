# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools>=4.50"]
# ///
"""Generate the README assets in docs/assets: logo, wordmark and overview showcase.

Text is converted to outlines so GitHub renders it without web fonts. The fonts
(Chakra Petch, IBM Plex Mono, both SIL OFL) are downloaded once into a cache dir.

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
    "IBMPlexMono-Regular.ttf": "https://github.com/google/fonts/raw/main/ofl/ibmplexmono/IBMPlexMono-Regular.ttf",
    "IBMPlexMono-Medium.ttf": "https://github.com/google/fonts/raw/main/ofl/ibmplexmono/IBMPlexMono-Medium.ttf",
}


def font(name):
    path = CACHE / name
    if not path.exists():
        CACHE.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve(FONT_URLS[name], path)
    return TTFont(path)


OUT.mkdir(parents=True, exist_ok=True)

MONO = font("IBMPlexMono-Regular.ttf")
MONO_MED = font("IBMPlexMono-Medium.ttf")
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

# ---------------------------------------------------------------- showcase
FS = 13.0                     # font size
CW = FS * 0.6                 # monospace advance
LH = 22.0                     # line height
COLS = 96
PADX, TOP = 28.0, 64.0        # text origin inside window
W = PADX * 2 + COLS * CW
ROWS = 17
H = TOP + ROWS * LH + 20


def col(c):
    return PADX + c * CW


def base(r):
    return TOP + r * LH


def glyph(kind, c, r, color):
    """Status symbols drawn as shapes, centered in one character cell."""
    cx, cy, s = col(c) + CW / 2, base(r) - FS * 0.36, FS * 0.34
    if kind == "ok":
        return f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{s:.1f}" fill="{color}"/>'
    if kind == "none":
        return f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{s - 0.6:.1f}" fill="none" stroke="{color}" stroke-width="1.2"/>'
    if kind == "adv":
        return (f'<path d="M{cx:.1f} {cy - s * 1.15:.1f}L{cx + s * 1.15:.1f} {cy:.1f}L{cx:.1f} {cy + s * 1.15:.1f}'
                f'L{cx - s * 1.15:.1f} {cy:.1f}Z" fill="{color}"/>')
    if kind == "cau":
        return (f'<path d="M{cx:.1f} {cy - s * 1.1:.1f}L{cx + s * 1.2:.1f} {cy + s:.1f}L{cx - s * 1.2:.1f} {cy + s:.1f}Z" '
                f'fill="{color}"/>')
    if kind == "warn":
        k = s * 0.95
        return (f'<path d="M{cx - k:.1f} {cy - k:.1f}L{cx + k:.1f} {cy + k:.1f}M{cx + k:.1f} {cy - k:.1f}L{cx - k:.1f} {cy + k:.1f}" '
                f'stroke="{color}" stroke-width="2.4" stroke-linecap="round"/>')
    if kind == "spin":  # braille-like spinner: 3 of 6 dots
        dots = [(0, 0), (1, 0), (0, 1)]
        out = []
        for dx, dy in [(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)]:
            on = (dx, dy) in dots
            out.append(f'<circle cx="{col(c) + 2.2 + dx * 3.4:.1f}" cy="{base(r) - 9.6 + dy * 3.4:.1f}" r="1.25" '
                       f'fill="{color}" opacity="{1 if on else 0.25}"/>')
        return "".join(out)
    if kind == "enter":
        x0, y0 = col(c) + 1.5, base(r) - 8.5
        return (f'<path d="M{x0 + 5.5:.1f} {y0:.1f}V{y0 + 5:.1f}H{x0:.1f}M{x0 + 2.5:.1f} {y0 + 2.5:.1f}L{x0:.1f} {y0 + 5:.1f}'
                f'L{x0 + 2.5:.1f} {y0 + 7.5:.1f}" fill="none" stroke="{color}" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>')
    raise ValueError(kind)


def spark(values, c, r, color):
    out = []
    bw = CW - 1.6
    for i, v in enumerate(values):
        h = 2 + v * 2.2
        out.append(f'<rect x="{col(c + i) + 0.8:.1f}" y="{base(r) - h:.1f}" width="{bw:.1f}" height="{h:.1f}" rx="0.6" fill="{color}"/>')
    return "".join(out)


def showcase(theme):
    p = PALETTES[theme]
    els = []

    def t(s, c, r, color, font=MONO):
        d, _ = text_path(font, s, col(c), base(r), FS)
        els.append(f'<path d="{d}" fill="{color}"/>')

    def rule(r):
        y = base(r) - FS * 0.35
        els.append(f'<line x1="{PADX:.1f}" y1="{y:.1f}" x2="{W - PADX:.1f}" y2="{y:.1f}" stroke="{p["rule"]}" stroke-width="1"/>')

    # tabs
    tabs = ["Overview", "Workspaces", "Updates", "AI", "Log", "Settings"]
    c = 0
    for i, name in enumerate(tabs):
        t(name, c, 0, p["focus"] if i == 0 else p["muted"], MONO_MED if i == 0 else MONO)
        if i == 0:
            y = base(0) + 6
            els.append(f'<line x1="{col(c):.1f}" y1="{y:.1f}" x2="{col(c + len(name)):.1f}" y2="{y:.1f}" '
                       f'stroke="{p["focus"]}" stroke-width="2" stroke-linecap="round"/>')
        c += len(name) + 4
    t("14:32", COLS - 5, 0, p["soft"])
    rule(1)

    # system
    t("CPU", 0, 2, p["muted"]); t("23 %", 4, 2, p["ink"])
    els.append(spark([1, 2, 2, 3, 5, 4, 2, 1, 1, 2, 3, 2, 2, 3], 11, 2, p["soft"]))
    t("Memory", 30, 2, p["muted"]); t("61 %", 37, 2, p["ink"])
    els.append(spark([5, 5, 5, 6, 6, 6, 5, 5, 5, 5, 6, 6, 5, 5], 43, 2, p["soft"]))
    t("Processes", 62, 2, p["muted"]); t("512", 72, 2, p["ink"])

    # attention
    t("Needs attention", 0, 4, p["ink"], MONO_MED)
    rows = [
        ("adv", "adv", "3 repos behind upstream", "api · web-shop · infra-modules"),
        ("cau", "cau", "2 repos skipped", "dotfiles: local changes · notes: ahead 1"),
        ("warn", "warn", "1 repo diverged", "legacy-portal ↑2 ↓5"),
        ("adv", "adv", "11 updates available", "brew 1 · npm 10 (2 major, manual only)"),
    ]
    for i, (g, color, label, detail) in enumerate(rows):
        r = 5 + i
        els.append(glyph(g, 0, r, p[color]))
        t(label, 2, r, p["ink"])
        t(detail, 34, r, p["muted"])

    # ai
    t("AI today", 0, 10, p["muted"])
    t("1.24 M tokens", 11, 10, p["ink"])
    t("≈ $4.10 API equivalent", 27, 10, p["ink"])
    t("Claude Code · estimate", 54, 10, p["soft"])

    # recent ok
    els.append(glyph("ok", 0, 12, p["ok"]))
    t("Fast-forwarded api to origin/main, 3 commits", 2, 12, p["ink"])
    t("just now", COLS - 8, 12, p["soft"])

    rule(13)
    els.append(glyph("spin", 0, 14, p["adv"]))
    t("npm install -g wrangler@4.151.1", 2, 14, p["ink"])
    t("running 00:12", COLS - 13, 14, p["soft"])

    # footer keys
    keys = [("?", "help"), ("tab", "switch"), (None, "action"), ("r", "refresh"), ("f", "fast-forward"), ("q", "quit")]
    c = 0
    for k, label in keys:
        if k is None:
            els.append(glyph("enter", c, 16, p["focus"]))
            c += 2
        else:
            t(k, c, 16, p["focus"], MONO_MED)
            c += len(k) + 1
        t(label, c, 16, p["soft"])
        c += len(label) + 3

    # window chrome
    title_d, tw = text_path(MONO_MED, "hud", 0, 0, 12)
    tx = (W - tw) / 2
    chrome = (
        f'<rect x="0.5" y="0.5" width="{W - 1:.1f}" height="{H - 1:.1f}" rx="10" fill="{p["bg"]}" stroke="{p["border"]}"/>'
        f'<path d="M0.5 32V10.5a10 10 0 0 1 10-10h{W - 21:.1f}a10 10 0 0 1 10 10V32Z" fill="{p["chrome"]}"/>'
        f'<line x1="0.5" y1="32" x2="{W - 0.5:.1f}" y2="32" stroke="{p["border"]}"/>'
        + "".join(f'<circle cx="{20 + i * 18}" cy="16.5" r="5.5" fill="{p["border"]}"/>' for i in range(3))
        + f'<g transform="translate({tx:.1f} 21)"><path d="{title_d}" fill="{p["soft"]}"/></g>'
    )
    desc = ("hud overview tab: CPU, memory and process totals with sparklines; only items needing attention: "
            "3 repos behind, 2 skipped, 1 diverged, 11 updates; AI tokens today with an estimated API-equivalent cost; "
            "a completed fast-forward; a running npm install; key hints in the footer.")
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W:.0f} {H:.0f}" width="{W:.0f}" height="{H:.0f}" '
            f'role="img" aria-labelledby="t d"><title id="t">hud overview (design preview)</title><desc id="d">{desc}</desc>'
            f'{chrome}{"".join(els)}</svg>\n')


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


(OUT / "showcase-dark.svg").write_text(showcase("dark"))
(OUT / "showcase-light.svg").write_text(showcase("light"))
(OUT / "logo.svg").write_text(logo())
(OUT / "wordmark.svg").write_text(wordmark("light"))
(OUT / "wordmark-dark.svg").write_text(wordmark("dark"))
print("written", sorted(x.name for x in OUT.iterdir()))
