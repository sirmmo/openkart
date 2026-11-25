#!/bin/sh
# Adds a circuit made with the circuit designer to a game and builds it: puts the track block
# into the game folder (as <id>.kart), runs osm2track on the extract (which joins the route,
# finds the walls and the racing line, and bakes the buildings, gates and scenery into
# <id>.track), then races four computer racers round it to show that it works.
#
#   sh tools/new-track.sh games/pachino-gp viali.kart viali-extract.json [more.json ...]
#
# The block is the designer's "Track block (.kart)" download, the extract its "Extract (.json)"
# (or one from tools/fetch-osm.sh). If the build fails the game folder is left as it was.
# Map data © OpenStreetMap contributors (ODbL 1.0).
set -e
ROOT=$(cd "$(dirname "$0")/.." && pwd)
GAME=${1:?usage: sh tools/new-track.sh <game folder> <track block .kart> <extract.json>...}
BLOCK=${2:?usage: sh tools/new-track.sh <game folder> <track block .kart> <extract.json>...}
shift 2
[ $# -gt 0 ] || { echo "give the extract (Overpass JSON) the track is built from" >&2; exit 2; }
[ -d "$GAME" ] || { echo "$GAME: there is no such game folder" >&2; exit 1; }
[ -f "$BLOCK" ] || { echo "$BLOCK: there is no such file" >&2; exit 1; }
for x in "$@"; do [ -f "$x" ] || { echo "$x: there is no such file" >&2; exit 1; }; done

ID=$(awk '/^track[ \t]/ { print $2; exit }' "$BLOCK")
[ -n "$ID" ] || { echo "$BLOCK: there is no \`track <id>\` line" >&2; exit 1; }
OSM=$(awk '/^track[ \t]/ { f = 1 } f && /^[ \t]+osm[ \t]/ { print $2; exit }' "$BLOCK")
[ -n "$OSM" ] || { echo "$BLOCK: track $ID has no \`osm <file>.track\` line" >&2; exit 1; }
OUT="$GAME/$ID.kart"
for k in "$GAME"/*.kart; do
  [ -f "$k" ] && [ "$k" != "$OUT" ] || continue
  if grep -q "^track[ \t][ \t]*$ID[ \t]*\$" "$k"; then
    echo "$k already has a track $ID: give the new one another id" >&2
    exit 1
  fi
done

# Keep what was there, to put it back if the build fails.
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
[ -f "$OUT" ] && cp "$OUT" "$TMP/kart"
[ -f "$GAME/$OSM" ] && cp "$GAME/$OSM" "$TMP/track"
restore() {
  if [ -f "$TMP/kart" ]; then cp "$TMP/kart" "$OUT"; else rm -f "$OUT"; fi
  if [ -f "$TMP/track" ]; then cp "$TMP/track" "$GAME/$OSM"; else rm -f "$GAME/$OSM"; fi
  echo "the game folder is as it was" >&2
}
if [ ! "$BLOCK" -ef "$OUT" ]; then cp "$BLOCK" "$OUT"; fi
echo "== $OUT: track $ID"

CARGO="cargo"
[ -x "$HOME/.cargo/bin/cargo" ] && CARGO="$HOME/.cargo/bin/cargo"
echo "== osm2track: the route, the walls, the racing line, the buildings and the scenery, baked"
if ! "$CARGO" run --quiet --release --manifest-path "$ROOT/Cargo.toml" -p osm2track -- "$GAME" "$@" --track "$ID"; then
  restore
  exit 1
fi
echo "== a race: four computer racers"
if ! "$CARGO" run --quiet --release --manifest-path "$ROOT/Cargo.toml" -p openkart --example race -- "$GAME" 1 "$ID"; then
  restore
  exit 1
fi
echo
echo "Done: $OUT and $GAME/$OSM. Play it: sh tools/site.sh, then play/?game=$(basename "$GAME")&track=$ID"
