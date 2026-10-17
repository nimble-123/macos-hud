#!/usr/bin/env bash
# Renders every TUI tab to PNG for reviews (e.g. on a phone via Remote Control).
# Output: target/screenshots/*.png. Needs uv; downloads Hack Nerd Font Mono once.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
out="$target/screenshots"
fonts="$HOME/Library/Caches/hud-fonts"
cols="${1:-115}"
rows="${2:-50}"

mkdir -p "$fonts"
if [[ ! -f "$fonts/HackNerdFontMono-Regular.ttf" ]]; then
  curl -fsSL -o "$fonts/Hack.zip" https://github.com/ryanoasis/nerd-fonts/releases/latest/download/Hack.zip
  unzip -oq "$fonts/Hack.zip" 'HackNerdFontMono-Regular.ttf' 'HackNerdFontMono-Bold.ttf' -d "$fonts"
  rm "$fonts/Hack.zip"
fi

rm -rf "$out" && mkdir -p "$out"
cargo run -q -p hud-tui --example screenshot -- "$out" "$cols" "$rows" >/dev/null

uv run -q --with resvg-py python -I - "$out" "$fonts" <<'PY'
import sys, pathlib, resvg_py
out, fonts = pathlib.Path(sys.argv[1]), sys.argv[2]
for svg in sorted(out.glob("*.svg")):
    png = resvg_py.svg_to_bytes(svg_path=str(svg), font_dirs=[fonts], skip_system_fonts=True)
    svg.with_suffix(".png").write_bytes(bytes(png))
    print(svg.with_suffix(".png"))
PY
