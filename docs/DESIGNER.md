# Designing a circuit

The **circuit designer** ([designer/](designer/), or `designer/index.html` in the repository)
draws a new OpenKart track on a map of OpenStreetMap: you pick the streets, it checks the loop
as you go, you dress it with scenery, and it gives you two files and one command. The command,
`tools/new-track.sh`, adds the track to a game and builds it with `tools/osm2track`, which bakes
the buildings, the city gates and the scenery into the track file.

The designer runs osm2track's own code in the browser (compiled to WebAssembly with the
engine): the routing between the points you click, the checks, the exact `route way` lines it
writes, and the bake for the 3D preview are the ones the command line uses, so what the designer
says about a circuit is what osm2track will build.

## Step by step

### 1. Find the streets

Pan and zoom the map (OpenStreetMap's own tiles) to where the race goes, then click **Load
streets here**. The designer asks the [Overpass API](https://overpass-api.de) for the streets
of the box on the screen, with what osm2track uses around them: buildings (with their heights
and levels), city gates and other historic things, hospital grounds. The box can be 3 × 3 km at
most (the Overpass servers are shared); load several boxes side by side if the circuit is
bigger. A box saved before, as a file, works too (**or an extract**).

### 2. Draw the circuit

With **Click points**, click near the streets in the order of the race: each point snaps to the
nearest street, and is joined to the one before by the shortest drive along the streets; the
last point is joined back to the first, so the circuit is always a loop. Drives that go into a
side street and straight back out (to reach a point clicked on the far side of a junction) are
cut away. With **Pick streets**, each click adds a whole street, from where the circuit is to
its far end, towards the click.

Tick **Streets closed for the race** to route both ways along one-way streets (they are still
reported). **Undo** (Ctrl+Z) takes back the last point, **Reverse** races the other way round,
and the ✕ beside a leg removes it (hover over a leg to see it on the map).

### 3. Read the checks

They update after every click:

* the length of a lap and of the race (1.5 to 3 km races best);
* whether the legs close into one loop, and where two legs do not meet (with how far apart
  they are, and a **Bridge** button that joins them by the shortest drive);
* stretches driven against a one-way street;
* very sharp corners: where the road, as it will be built with its corners rounded, turns by
  more than 110° within 30 m (a hairpin), with its radius;
* whether the start line, the boost pads, the scenery's streets and the gates are found.

Click a check to see the place on the map.

### 4. Set up the track

Its id (the file names: `<id>.kart`, `<id>.track`), name, text and laps; the road's width (the
map's widths times this, and at least that), the pavement between the road and the walls, how
far from the road buildings come with the track, and how often a row of picnic baskets lies
across it. **Set the start line** and click the circuit; by default it is at the first point.
**Add a boost pad** the same way.

### 5. Dress it

Choose plane trees, lamp posts or cones from the palette, how often (metres) and on which side,
and along which street of the circuit (or all of it), then **Add this scenery**. City gates
near the circuit are listed (and ticked) as the loop passes them; to place one the map does not
have, give its name, click **Place it on the map** and click where it stands.

### 6. Export

The panel shows the `track ... end` block as it will be written. Download:

* **Track block (.kart)**: the block, with every `route way` line exact (one per stretch of one
  OpenStreetMap way, with its direction and the nodes it enters and leaves by);
* **Extract (.json)**: the streets you loaded (Overpass JSON), so the track can be built
  offline and again later.

**Bake and preview in 3D** builds the track file in the browser and flies round it with the
game's own renderer; **Track file (.track)** downloads what it built.

### 7. Build it

In the OpenKart folder, with the two files:

```sh
sh tools/new-track.sh games/pachino-gp mytrack.kart mytrack-extract.json
```

It copies the block into the game as `games/pachino-gp/mytrack.kart`, runs osm2track (the
route, walls, racing line, buildings, gates and scenery, baked into `mytrack.track`), then races
four computer racers round it and prints the result. If the build fails, the game folder is left
as it was. Then `sh tools/site.sh`: the game's title screen has a picker with every track of the
game (`play/?track=mytrack` starts on the new one).

The designer remembers the circuit you are drawing in your browser; it comes back when you load
its streets again. **Import…** takes a block it wrote before.

## The block it writes

The second track of Pachino Grand Prix, `games/pachino-gp/porte.kart`, was drawn with the
designer: eight points, from Porta San Vitale along Via San Vitale, up Via Petroni to Piazza
Verdi, along Via Zamboni to Porta San Donato and back down Viale Filopanti. Shortened:

```text
track porte
    name Tra le Porte
    osm porte.track
    laps 3
    route way 23183655 forward from 250734851 to 7888475359
    route way 1490523111 forward from 7888475359 to 251846404
    ...
    route way 1490523112 forward from 13396522100 to 250734851
    start Viale Quirico Filopanti 0.65
    widen 1.25
    min-width 8.5
    landmark gate Porta San Donato
    dress tree tree_oak.glb every 13 offset 2 scale 6.5 9 face random along Viale Quirico Filopanti
    baskets every 400 first 200 across 4
    pad Via Zamboni 0.24
    ...
end
```

Every line is described in [Game files](RULES.md#track). The exact `route way` lines need the
extract the designer used (or a newer one where those ways and nodes still exist); street names
(`route Via Zamboni`) are shorter to write by hand but less exact.

## In the engine

The designer's requests to the WebAssembly engine (`crates/openkart-web`), on top of the
game's own (see [Engine](ENGINE.md#in-the-browser)):

| Request | Answer |
| --- | --- |
| `{op: "extract", docs: [overpass json, ...]}` | `{ways, drivable}`: keeps the extract and its street graph |
| `{op: "snap", lat, lon, within?}` | `{snap: {node, lat, lon, way, name, metres}}`, or `null` |
| `{op: "drive", from, to, against?}` | `{drive: {spans, metres, against, line}}`: the shortest drive as exact legs |
| `{op: "tidy", spans, closed?}` | `{spans}`: the legs without spurs |
| `{op: "check", block}` | `{report: {closed, error, length, gaps, oneway, corners, problems, exact, line, sections, landmarks}}` |
| `{op: "bake", files, track}` | `{game, text, file, length, sections, buildings, props, landmarks}`; the game is loaded with it |

They are osm2track's library (`tools/osm2track/src/lib.rs`): `Osm`, `Graph::snap`,
`Graph::drive`, `Graph::tidy`, `check`, `track_block` and `build`.

## Map data and the servers it uses

The map tiles are OpenStreetMap's standard tiles from `tile.openstreetmap.org`, shown as a
plain Leaflet map (no prefetching, no bulk downloads), as the
[tile usage policy](https://operations.osmfoundation.org/policies/tiles/) asks. Streets come
from the public Overpass API, one box per click. Both are © OpenStreetMap contributors, under
the [Open Database Licence](https://www.openstreetmap.org/copyright); so are the extracts and
the track files made from them.

For tests, the designer takes URL flags: `extract=<url>` loads an extract file instead of
asking Overpass, `tiles=none` leaves the background blank, `points=lat,lon;lat,lon;...` clicks
those points, and `closed=1`, `id=`, `name=`, `text=`, `laps=`, `start=lat,lon`,
`pads=lat,lon;...`, `dress=tree@<street>;lamp;...` and `bake=1` do what the panel does.
