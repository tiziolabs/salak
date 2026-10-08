#!/bin/sh
# Renders the bitmap icons from icons/salak.svg, their only source.
#
# Requires rsvg-convert and icotool (Debian / Ubuntu: librsvg2-bin icoutils).
# Run from anywhere: sh scripts/render-icons.sh
set -eu
cd "$(dirname "$0")/.."

svg=icons/salak.svg
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

png() {
    rsvg-convert --width "$1" --height "$1" --output "$2" "$svg"
}

png 512 icons/icon.png
png 128 icons/128x128.png
# Sizes of the Windows icon, as Explorer and the taskbar ask for them.
for size in 16 24 32 48 64 128 256; do
    png "$size" "$tmp/$size.png"
done
# Every size is stored as PNG, which Windows Vista and later read: a fraction
# of the size of bitmaps.
icotool --create --output icons/icon.ico \
    --raw="$tmp"/16.png --raw="$tmp"/24.png --raw="$tmp"/32.png \
    --raw="$tmp"/48.png --raw="$tmp"/64.png --raw="$tmp"/128.png \
    --raw="$tmp"/256.png

# Copies installed by the Linux application and shown by the Tauri one.
cp icons/128x128.png crates/salak-gtk/data/icons/com.tiziolabs.salak.png
cp "$svg" crates/salak-gtk/data/icons/com.tiziolabs.salak.svg
cp "$svg" crates/salak-tauri/ui/logo.svg
