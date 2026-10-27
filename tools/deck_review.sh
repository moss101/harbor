#!/bin/zsh
# Render sample decks to images for design review, without PowerPoint.
#
#   tools/deck_review.sh <themes|all> <motion> <outdir> [rtl]
#
# Builds the gallery deck for each theme, reads it back through the REAL
# scene reader, draws it with the dev SVG renderer, rasterises with macOS
# Quick Look, and writes per-theme contact sheets
# (<outdir>/<theme>-sheet-N.png, 6 slides each) plus <outdir>/compare-N.png
# (one tile per theme for slide N = 1,3,4,7). Text wrapping in the SVG is
# an estimate; geometry, colour, layering and gradients are exact.
set -e
export PATH="$HOME/.cargo/bin:$PATH"
here="$(cd "$(dirname "$0")/.." && pwd)"
themes="$1"; motion="$2"; out="$3"; rtl="$4"
rm -rf "$out"; mkdir -p "$out"
cd "$here/core"
cargo run -q -p harbor_artifacts --example deck_gallery -- "$themes" "$motion" "$out" $rtl > "$out/list.txt"
for f in $(cat "$out/list.txt"); do
  id=$(basename "$f" .pptx)
  cargo run -q -p harbor_render --example scene_svg -- "$f" "$out/$id-svg" > /dev/null
  ( cd "$out/$id-svg" && qlmanage -t -s 1600 -o . $id-*.svg > /dev/null 2>&1 ) &
done
wait
pkill qlmanage 2>/dev/null || true
for f in $(cat "$out/list.txt"); do
  id=$(basename "$f" .pptx)
  python3 "$here/tools/deck_sheet.py" slides "$out/$id-svg" "$id" "$out/$id-sheet.png" 6 > /dev/null
done
for n in 1 3 4 7; do python3 "$here/tools/deck_sheet.py" compare "$out" $n "$out/compare-$n.png" > /dev/null; done
ls "$out"/*.png
