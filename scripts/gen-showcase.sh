#!/usr/bin/env bash
# Renders the README showcase from the real view with sample data.
# Output: docs/assets/showcase-{dark,light}.png. Needs uv (see tui-screenshots.sh).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
shots="${CARGO_TARGET_DIR:-$root/target}/screenshots"

"$root/scripts/tui-screenshots.sh" 115 38 >/dev/null
cp "$shots/01-overview-en-dark.png" "$root/docs/assets/showcase-dark.png"
cp "$shots/01-overview-en-light.png" "$root/docs/assets/showcase-light.png"
ls -l "$root"/docs/assets/showcase-*.png
