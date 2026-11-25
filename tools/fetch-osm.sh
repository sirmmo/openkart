#!/bin/sh
# Downloads an OpenStreetMap extract for osm2track from the Overpass API: every highway and
# building way with its geometry, hospital grounds, and historic gates and landmarks (as
# relations with bounds, and nodes) in a bounding box.
#   sh tools/fetch-osm.sh SOUTH,WEST,NORTH,EAST OUT.json
#   sh tools/fetch-osm.sh 44.4850,11.3500,44.4990,11.3720 osm/sorsola.json
# Map data © OpenStreetMap contributors, ODbL 1.0. Please keep to public map data and a small
# box: the public Overpass servers are shared.
set -e
BBOX=${1:?give the box: south,west,north,east}
OUT=${2:?give the output file}
URL=${OVERPASS_URL:-https://overpass-api.de/api/interpreter}
QUERY="[out:json][timeout:120];
(
  way[\"highway\"]($BBOX);
  way[\"building\"]($BBOX);
  way[\"amenity\"=\"hospital\"]($BBOX);
  way[\"historic\"]($BBOX);
);
out geom;
(
  relation[\"historic\"]($BBOX);
  relation[\"building\"]($BBOX);
  node[\"historic\"]($BBOX);
);
out tags bb;"
mkdir -p "$(dirname "$OUT")"
for wait in 0 20 60 120; do
  sleep "$wait"
  if curl -sS -A openkart-dev --max-time 180 -o "$OUT" "$URL" --data-urlencode "data=$QUERY" && head -c 1 "$OUT" | grep -q '{'; then
    echo "$OUT: $(wc -c < "$OUT") bytes"
    exit 0
  fi
  echo "the Overpass server is busy; trying again" >&2
done
echo "could not download the extract" >&2
exit 1
