# OpenKart

An arcade kart racer on real streets. The race track comes from **OpenStreetMap**, with the
buildings, porticoes and plane trees around it; the racers, items and hazards come from
plain-text game files; the engine is a Rust library, deterministic at 60 steps a second, so a
race is a seed plus each player's inputs and replays exactly.

The first game is **Pachino Grand Prix** (`games/pachino-gp`): Capitan Pachino, the tomato
pirate of [Capitan Pachino 64](https://sirmmo.github.io/pachino64/), races his friend Captain
Carota and two bosses, King Steak and Don Prosciutto, round **Il Giro di Sant'Orsola**: 2.3 km
in Bologna, down the Viali from Porta San Vitale to Porta Maggiore under the plane trees, out
along the porticoes of Via Mazzini, up Via Albertoni and Via Massarenti, then through the
grounds of the Policlinico Sant'Orsola-Malpighi between its pavilions, where ambulances cross on
a timetable. Three laps. And **Tra le Porte**: 1.6 km through the university quarter, from
Porta San Vitale along Via San Vitale and Via Zamboni to Porta San Donato and back down the
Viali, drawn with the circuit designer.

Drift through corners for a boost; drive through picnic baskets for an item: a **pomodorata**
(a tomato thrown forward that splats the windscreen), an **olive-oil slick** (dropped behind:
karts spin), a **peperoncino** (a boost), a **parmesan wheel** (rolls ahead along the road) or a
**sushi roll** (aims, then rolls in a straight line).

**Play:** https://ingmmo.com/openkart/play/ · **Documentation:** https://ingmmo.com/openkart/

## Controls

| | Keyboard | Gamepad |
| --- | --- | --- |
| Steer | ← → or A D | left stick |
| Accelerate | ↑ or W | A, right trigger |
| Brake, reverse | ↓ or S | B, left trigger |
| Drift (hold while turning) | Space | right shoulder |
| Use the item | X, E or Enter | X, left shoulder |
| Pause | Esc or P | Start |

On a touch screen the kart accelerates by itself, with buttons to steer, drift, brake and use
the item.

## How it works

* **Game files** (`*.kart`) declare the game, the physics, tracks, racers, items and hazards,
  in `kind name ... end` blocks: see [docs/RULES.md](docs/RULES.md). Errors name the file and
  line.
* **Track files** (`*.track`) are built by `tools/osm2track` from an OpenStreetMap extract and
  the track's `route` (street names, or OSM way ids with a direction and a span): one closed
  loop with rounded corners, its width from lanes or road class, walls at the buildings, a
  racing line, checkpoints, a grid, a minimap, the buildings near the road with their heights
  and porticoes, the city gates, the hospital's grounds and pavilions, and the scenery of the
  `dress` lines, baked.
* **The engine** (`crates/openkart`): arcade physics on the track ribbon (speed, steering,
  grip, weight, drift with boost, off-road, walls, kart-to-kart bumps), laps and places,
  computer racers on the racing line with per-racer skill and mild catch-up, items and hazards.
  See [docs/ENGINE.md](docs/ENGINE.md).
* **The browser game** is the engine as WebAssembly (`crates/openkart-web`, JSON messages, no
  generated glue) and a three.js page (`web/`, no build step). A game with several tracks has a
  picker on its title screen.
* **The circuit designer** (`designer/`) draws new tracks on OpenStreetMap: see below.

## Designing a circuit

The circuit designer (`designer/`, published at `designer/` beside the game) is a map of
OpenStreetMap (Leaflet, the standard tiles). Pan to a neighbourhood, click **Load streets
here** (its streets, buildings and gates come from the Overpass API, a box of 3 × 3 km at
most), then click points along the streets in the order of the race: each is joined to the one
before by the shortest drive, and the last back to the first. It checks the circuit as you go
(its length, whether it closes, gaps between legs, one-way streets driven the wrong way, very
sharp corners), lets you set the name, laps, width and start line, scenery (plane trees, lamp
posts, cones) and city gates, previews it in 3D with the game's renderer, and exports the
`track` block (with exact `route way` lines) and the extract. Then:

```sh
sh tools/new-track.sh games/pachino-gp mytrack.kart mytrack-extract.json
```

adds the block to the game, runs osm2track (which bakes the buildings, gates and scenery into
`mytrack.track`) and races four computer racers round it. The routing, the checks and the bake
in the page are osm2track's own code, compiled to WebAssembly. Step by step:
[docs/DESIGNER.md](docs/DESIGNER.md).

## Development

```sh
cargo test --workspace                                  # files, races, replays, items, routes
cargo run --release --example race -- games/pachino-gp 7     # four computer racers, seed 7
sh tools/fetch-osm.sh 44.4850,11.3500,44.4990,11.3720 osm/sorsola.json
cargo run -p osm2track -- games/pachino-gp osm/sorsola.json  # rebuild sorsola.track
sh tools/fetch-osm.sh 44.4920,11.3475,44.5005,11.3610 osm/porte.json
sh tools/new-track.sh games/pachino-gp games/pachino-gp/porte.kart osm/porte.json  # rebuild porte.track and race it
sh tools/site.sh && python3 -m http.server -d _site            # docs on :8000, game in play/, designer in designer/
```

| Path | What |
| --- | --- |
| `crates/openkart/` | The engine: `script.rs` (game files), `track.rs` (track files, geometry), `course.rs` (scenery, pads, baskets), `sim.rs` (the race), `det.rs` |
| `crates/openkart/tests/` | Races on the real track: files load, four racers finish, replays agree, every item works |
| `crates/openkart-web/` | The engine as WebAssembly: `load`, `course`, `race`, `step`, `save` messages |
| `tools/osm2track/` | OpenStreetMap extract + route → track file; its library also routes, checks and bakes for the designer |
| `tools/fetch-osm.sh` | Downloads an extract from the Overpass API for a bounding box |
| `tools/new-track.sh` | Adds a designed track block to a game, builds it and races it |
| `designer/` | The circuit designer: `index.html`, `designer.js`, Leaflet in `vendor/` |
| `games/pachino-gp/` | Pachino Grand Prix: `game.kart`, `racers.kart`, `items.kart`, `sorsola.kart` and `porte.kart`, and their `.track` files (built) |
| `web/` | The page: `js/main.js`, `world.js`, `karts.js`, `hud.js`, `input.js`, `engine.js`; `models/` |
| `site/`, `docs/` | The documentation site |
| `tools/site.sh` | Builds `_site/`: the documentation at the root, the game in `play/`, the designer in `designer/` |

Still to come: more tracks, sound, online play.

## Credits and licences

The code is GPL-3.0-or-later (`LICENSE`). The track files (`games/*/*.track`) are built from
OpenStreetMap data, **© OpenStreetMap contributors**, and are under the
[Open Database Licence 1.0](https://opendatacommons.org/licenses/odbl/1-0/) (see
https://www.openstreetmap.org/copyright). The game shows the attribution on its title screen and
during races. The models are CC0, and Capitan Pachino is from Capitan Pachino 64. Everything,
licence by licence: [CREDITS.md](CREDITS.md).
