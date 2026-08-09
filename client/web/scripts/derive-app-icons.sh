#!/usr/bin/env bash
# Derive every app-icon raster the web client ships, from the reference PNG.
#
# The reference plates are the approved app icon artwork and the source of the
# whole colour palette. They are NOT in this repo (binary, omitted from the
# design-mirror pull) -- see client/web/src/design/brand/README.md for where to
# get them. Point SRC_DIR at a directory holding:
#
#   light-1024.png   dark-1024.png
#
# Then:  client/web/scripts/derive-app-icons.sh <SRC_DIR>
#
# Requires ImageMagick (`brew install imagemagick`). Outputs are committed;
# this script is how you regenerate them, not part of the build.

set -euo pipefail

SRC_DIR="${1:-}"
if [[ -z "$SRC_DIR" ]]; then
  echo "usage: $0 <dir containing light-1024.png and dark-1024.png>" >&2
  exit 2
fi

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEB="$(dirname "$HERE")"
BRAND="$WEB/src/design/brand"
PUBLIC="$WEB/public"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

for v in light dark; do
  src="$SRC_DIR/$v-1024.png"
  [[ -f "$src" ]] || { echo "missing $src" >&2; exit 1; }

  # The reference plates are opaque: the squircle is drawn on an opaque white
  # surround, so the corners are white, not transparent. Flood-filling inward
  # from each corner lifts exactly that surround out and leaves the plate's own
  # squircle edge -- which a geometric mask could not do, because the corner is
  # a superellipse, not a circular arc.
  #
  # The fuzz has to clear the light plate's smallest separation from white
  # (blue channel, 231 vs 248 = 6.7%) without reaching it. 4% sits inside that
  # and is verified not to eat the bird's own cream chest, which is enclosed by
  # the plate and so unreachable from a corner anyway.
  magick "$src" -alpha set -fuzz 4% \
    -fill none -floodfill +0+0 white \
    -fill none -floodfill +1023+0 white \
    -fill none -floodfill +0+1023 white \
    -fill none -floodfill +1023+1023 white \
    "$WORK/$v-cut.png"
done

emit() { # emit <variant> <size> <dest>
  # color-type=6 is forced because ImageMagick's PNG encoder picks the type
  # from actual pixel content: a fully-opaque small render would otherwise be
  # written as 3-channel RGB with no alpha channel at all.
  # compression-level 9 is lossless and free (~5%); no lossy palette step --
  # quantising to 256 colours saves ~44% but this is brand artwork.
  magick "$WORK/$1-cut.png" -filter Lanczos -resize "$2x$2" \
    -strip -colorspace sRGB -define png:color-type=6 \
    -define png:compression-level=9 "$3"
}

# Nav rail brand mark. The design system's own NavRail sets this at 26px with
# border-radius 22.37%; 2x and 3x are for hidpi, wired up as a srcset.
for v in light dark; do
  for s in 26 52 78; do emit "$v" "$s" "$BRAND/app-icon-$v-$s.png"; done
done

# Browser tab. Light plate, because it is the design system's default (the
# brand card and its NavRail both reach for the light plate). Checked rather
# than assumed: at true 16px on both a white and a dark tab strip, both plates
# stay legible -- the bird fills the tile, so the gorget and beak carry the
# read and the plate colour barely participates. This is a consistency call,
# not a legibility one.
for s in 16 32 48; do emit light "$s" "$PUBLIC/favicon-$s.png"; done

# Installed PWA icon (manifest, purpose "any").
for s in 192 512; do emit light "$s" "$PUBLIC/app-icon-$s.png"; done

echo "derived:"
ls -1 "$BRAND"/app-icon-*.png "$PUBLIC"/favicon-*.png "$PUBLIC"/app-icon-*.png \
  | sed "s|$WEB/|  client/web/|"
