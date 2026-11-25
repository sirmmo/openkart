# Credits

## Code: GPL-3.0-or-later

OpenKart's engine, tools, client and documentation. See `LICENSE`.

* [three.js](https://threejs.org) r160 (`web/vendor/`), MIT: `web/vendor/three-LICENSE.txt`.
* [Leaflet](https://leafletjs.com) 1.9.4 (`designer/vendor/leaflet/`), the circuit designer's
  map, BSD 2-Clause: `designer/vendor/leaflet/LICENSE`.
* Fonts loaded from Google Fonts: Alfa Slab One and Barlow Condensed, SIL Open Font Licence.

## Map data: ODbL 1.0

The track files (`games/*/*.track`) are Derived Databases of
[OpenStreetMap](https://www.openstreetmap.org) data, **© OpenStreetMap contributors**, available
under the [Open Database Licence 1.0](https://opendatacommons.org/licenses/odbl/1-0/). The
streets, buildings, heights, the city gates (Porta San Vitale, Porta Maggiore, Porta San
Donato, the Torresotto di San Vitale) and the grounds and pavilions of the Policlinico
Sant'Orsola-Malpighi in Il Giro di Sant'Orsola and Tra le Porte come from it. Built with
`tools/osm2track` from an Overpass API extract (`tools/fetch-osm.sh`, or the circuit designer's).
If you publish a track file, or a game made with one, keep this attribution and the ODbL.

The circuit designer shows OpenStreetMap's standard tiles from tile.openstreetmap.org (©
OpenStreetMap contributors), as a plain map under the
[tile usage policy](https://operations.osmfoundation.org/policies/tiles/), and asks the public
Overpass API for one box of streets at a time, when its user clicks "Load streets here".

## Models

| Folder | Models | From | Licence |
| --- | --- | --- | --- |
| `web/models/racers/` | `pachino.glb` (Capitan Pachino) | [Capitan Pachino 64](https://github.com/sirmmo/pachino64) | the model of Capitan Pachino 64's author, who also made OpenKart; ask before reusing it elsewhere |
| `web/models/racers/` | `carrot.glb`, `meat-raw.glb`, `whole-ham.glb` and `Textures/colormap.png` | Kenney, [Food Kit](https://kenney.nl/assets/food-kit) (via Capitan Pachino 64) | CC0 (`LICENSE-food.txt`) |
| `web/models/items/` | `tomato.glb`, `bottle-oil.glb`, `paprika.glb`, `maki-salmon.glb`, `Textures/colormap.png` | Kenney, Food Kit | CC0 (`LICENSE.txt`) |
| `web/models/karts/` | `kart-oobi.glb`, `kart-oodi.glb`, `kart-ooli.glb`, `kart-oopi.glb`, `ambulance.glb`, `Textures/colormap.png` | Kenney, [Car Kit](https://kenney.nl/assets/car-kit) | CC0 (`LICENSE.txt`) |
| `web/models/props/` | `cone.glb`, `Textures/colormap.png` | Kenney, Car Kit | CC0 (`LICENSE-car-kit.txt`) |
| `web/models/props/` | `lightPostModern.glb` (street lamps) | Kenney, [Racing Kit](https://kenney.nl/assets/racing-kit) | CC0 (`LICENSE-racing-kit.txt`) |
| `web/models/props/` | `tree_oak.glb` (the plane trees) | Kenney, [Nature Kit](https://kenney.nl/assets/nature-kit) | CC0 (`LICENSE-nature-kit.txt`) |

The buildings, porticoes, city gates, picnic baskets, parmesan wheel, oil slicks, barriers,
start gantry and boost pads are drawn by the client from the track data, with textures it paints
itself. The googly eyes and golden crowns follow Capitan Pachino 64.

## Sound

The engines, tyres, items, siren, jingles and the music ("Tarantella di Sant'Orsola", written for
OpenKart) are synthesised in the browser by `web/js/audio.js` and `web/js/music.js` (GPL, as the
code). The basket's coin and the "hurt" sound follow Capitan Pachino 64's synth. Four short
samples are Kenney's, CC0:

| File | From | Licence |
| --- | --- | --- |
| `web/sounds/tick_001.ogg`, `select_002.ogg`, `drop_002.ogg` | Kenney, [Interface Sounds](https://kenney.nl/assets/interface-sounds) | CC0 (`LICENSE-interface-sounds.txt`) |
| `web/sounds/card-shove-1.ogg` | Kenney, [Casino Audio](https://kenney.nl/assets/casino-audio) | CC0 (`LICENSE-casino-audio.txt`) |

## Characters

Capitan Pachino, Captain Carota, King Steak and Don Prosciutto are characters of Capitan
Pachino 64. OpenKart is an original game: its names, items and look are its own.
