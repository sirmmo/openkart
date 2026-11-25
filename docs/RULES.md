# OpenKart game files

A game is a folder of plain-text files: `*.kart` files (the game, its physics, tracks,
racers, items and hazards) and the `*.track` files its tracks name, which
`tools/osm2track` builds from OpenStreetMap. Every file is a list of blocks:

```text
kind name
    key values...
end
```

A block starts at the start of a line; its lines are indented; `end` closes it. A `#` that
starts a word starts a comment, so colours are written without one: `colour e8352b`. Blocks may
come in any file and any order (a track may name a hazard declared further down). Every error
names its file and line:

```text
sorsola.kart:51: track sorsola: pad: the track has no section "Via Roma" (it has Viale Giambattista Ercolani, ...)
racers.kart:12: racer pachino: unknown line "sped 3"
```

The engine reads everything that changes the race. Lines marked *client* are hints for the
browser client (models, colours, looks): the engine checks and passes them on.

## game

```text
game pachino-gp
    name Pachino Grand Prix
    text Capitan Pachino has left the Garden Isle for a road race...   # several lines join
    assets racers models/racers     # client: where each kind of model is, under the page
    assets karts models/karts
    assets items models/items
    assets props models/props
    assets hazards models/karts
    field 4                         # racers in a race: the player and the computer's
    first sorsola                   # the track raced when none is chosen
    credit Food, karts and props: Kenney (kenney.nl), CC0      # client: the credits
end
```

There is one `game` block. A game needs at least one track, one item and `field` racers.

## physics

The karts' numbers, shared by every racer; each racer's stats scale them. All are optional
(these are the defaults).

```text
physics
    top 30              # top speed, m/s, for a racer of speed 3
    accel 14            # m/s² from a standstill, for accel 3
    brake 28
    coast 0.5           # rolling drag with no pedal, per second
    turn 2.2            # steering at full lock, radians a second
    grip 9              # how fast the kart's travel follows its nose, per second
    drift-grip 2.2      # the same while drifting: lower, so the kart slides out
    drift-charge 0.7 1.5    # seconds of drift for the first and the second boost
    drift-boost 0.7 1.4     # seconds of boost they give
    boost-power 1.3     # top speed times this while boosting
    offroad 0.55        # top speed times this off the road (pavements, verges)
    corner 20           # sideways m/s² the computer racers plan corners with
    catchup 0.05        # most a computer racer behind the leading human gains (5%)
    basket-respawn 4    # seconds a picnic basket takes to come back
    unpack 1            # seconds a racer unpacks a basket before using the item
    countdown 3
    time-limit 900      # seconds before everyone still racing is out
    pad-boost 1         # seconds of boost from a boost pad
    radius 1            # kart radius for collisions, metres
end
```

A racer's stats (1 to 5, 3 is average) change these: **speed** +3.5% top speed a point;
**accel** +9% acceleration a point; **grip** +5% steering and +10% grip a point; **weight**
+15% mass a point (heavier karts push lighter ones about and gain a little top speed, but
accelerate a little slower).

## racer

```text
racer kingsteak
    name King Steak
    text Ruler of Mount Grill, in his crown.
    model meat-raw.glb      # the driver, in the `racers` folder
    kart kart-oopi.glb      # the kart, in the `karts` folder
    colour a8322d           # client: minimap and lists
    speed 4                 # stats, 1 to 5
    accel 2
    grip 2
    weight 5
    skill 0.84              # how well the computer drives it, 0 to 1
    look scale 2.6          # client: the driver's size,
    look lift 0.75          #   its height in the seat (metres),
    look stand 1            #   stood up (a model lying flat),
    look turn 90            #   turned (degrees),
    look eyes 0.66 angry    #   googly eyes at that height (0 to 1), angry or not,
    look crown 3f8cff       #   and a golden crown with a gem of that colour
end
```

The client hides a node called `character` in the kart model and seats the driver there.

## item

What the picnic baskets on the road hold. A racer who drives through a basket unpacks an item
(if it holds none); the item is drawn with the odds for the racer's place.

```text
item pomodorata
    name Pomodorata
    text A ripe tomato, thrown forward...
    kind throw              # throw, drop, boost, roll or aim
    model tomato.glb        # client: in the `items` folder, or wheel:<colour> for a wheel
    speed 34                # m/s (a thrown item adds the thrower's speed)
    life 2.5                # seconds before it is gone
    radius 1.9              # metres: how close counts as a hit
    effect splat 2.6        # splat, spin, bump or boost, and for how many seconds
    power 0.72              # top speed times this while the effect lasts (splat, boost)
    hits 1                  # racers it can hit before it is gone
    aim 0.7                 # an aim item aims this many seconds
    odds 3 3 2 2            # weights by place, first place first
end
```

Kinds:

* **throw**: flies forward, turning a little towards the nearest racer ahead.
* **drop**: left on the road behind the kart (a slick: give it `hits` and a long `life`).
* **boost**: used at once, on oneself (`effect boost`, `power`).
* **roll**: rolls ahead along the road, keeping its distance from the centre.
* **aim**: stops, turns to the nearest racer ahead for `aim` seconds, then rolls in a straight
  line until it hits something, a wall, or its `life` ends.

Effects: **splat** slows the racer (`power`) and the player sees sauce on the windscreen;
**spin** spins the kart round and slows it; **bump** knocks it aside and takes most of its
speed. A racer who was just hit cannot be hit again for a moment.

`odds` may have any number of weights: the field is spread over them (with four racers and four
weights, each place has its own). The game's items: the pomodorata (a tomato), the olive-oil
slick, the peperoncino (a boost), the parmesan wheel (rolls) and the sushi roll (aims).

## hazard

Something that keeps to a timetable on the track: it crosses the road again and again.

```text
hazard ambulance
    name Ambulanza
    model ambulance.glb             # client: in the `hazards` folder
    cross Policlinico Sant'Orsola-Malpighi 0.45    # where it crosses (a place, see below)
    every 7 offset 2                # starts a crossing every 7 s, the first 2 s into the race
    speed 9                         # m/s
    size 5.2 2.4                    # length and width, metres
    margin 7                        # it starts and ends this far beyond the walls
    effect bump 1.2
end
```

It drives across from the right of the road to the left, then back on the next crossing.
Its schedule depends only on the race clock, so it is the same in every race.

## track

```text
track sorsola
    name Il Giro di Sant'Orsola
    text Bologna, between the Viali and the Policlinico...
    osm sorsola.track       # the track file, next to the game files
    laps 3
    route Piazza di Porta San Vitale        # the route, leg by leg: see below
    route Viale Giambattista Ercolani
    ...
    start Viale Giambattista Ercolani 0.3   # where the start line is (a place)
    widen 1.25              # road width from the map, times this
    min-width 8.5           # and at least this
    verge 5                 # pavement between the road and the walls, metres
    scenery 60              # buildings this close to the road come with the track
    landmark gate Porta San Vitale          # looked up by name in the map
    landmark gate Porta Maggiore at 44.49007 11.35728   # or placed, if the map lacks it
    dress tree tree_oak.glb every 13 offset 2 scale 6.5 9 face random along Viale Giambattista Ercolani
    baskets every 430 first 230 across 4    # a row of picnic baskets every 430 m
    pad Via Giuseppe Mazzini 0.35           # a boost pad on the racing line
    hazard ambulance
    look sky 9fc8e8         # client: sky, ground and the buildings' palette
    look ground c9b79a
    look palette c8553d d9a441 b5452f e0b36a
    look start gantry
end
```

`route`, `start`, `widen`, `min-width`, `verge`, `scenery`, `landmark` and `dress` are read by
`tools/osm2track` when it builds the track file; the engine reads the rest (and checks that the
places exist).

### Places

A place on a track is metres from the start line (`pad 300`), or a street of the route and a
fraction of the way along it (`pad Via Giuseppe Mazzini 0.35`). If the route takes a street
more than once, the fraction runs over all of it. Unnamed ways inside a hospital take the
hospital's name; other unnamed legs are called `Leg <first way id>`.

### Routes

A route is a list of legs, joined into one closed loop. Each leg follows on from the one
before, and the last leads back to the first.

```text
route Via Giuseppe Mazzini          # every drivable way with this name
route ways 617896269 617896271      # these ways, in this order, either way round
route way 617896282 forward from 5840652725 to 5840652728   # the exact form
```

`route way <id> [forward|backward] [from <node>] [to <node>]` is the exact form, one OSM way
per line, as a circuit designer writes it: `forward` drives the way in the order of its nodes,
`backward` against it; `from` and `to` take only the part of the way between those two nodes
(entering at `from`, leaving at `to`; their order gives the direction). Without them, a way
is driven whichever way round closes the loop, and may be entered and left at any of its
nodes where the legs before and after meet it.

osm2track finds the cheapest loop that runs through the legs in order (driving against a
one-way street costs four times as much, so a dual carriageway is taken on the right side),
and says which legs do not meet if there is none:

```text
osm2track: track sorsola: route: way 617896282 and Via Giuseppe Massarenti do not meet (they share no node), so the loop breaks there
```

### Scenery

`dress <kind> <model> every <metres>` sets copies of a model from the `props` folder along the
road, on the pavement beyond the kerb, wherever it is wide enough (not where a building stands
close), and not by the start line. Options: `side left|right|both`, `offset <metres beyond
the road's edge>` (default 1.5), `scale <min> [max]` (each copy gets a scale between them),
`face road|along|random` (default road), and `along <street>` last (the rest of the line).
osm2track bakes the copies into the track file (see `props` below), so the client draws them
as they are; if a track file has no `props`, the engine places them when it loads.

## Track files

A `*.track` file is written by `tools/osm2track`; do not edit it by hand. It is in the same
block form, and its data is © OpenStreetMap contributors under the Open Database Licence: the
file is a Derived Database under the ODbL 1.0 (the code that reads it is GPL). Coordinates are
metres around the `origin`: `x` east, `z` south (an equirectangular projection).

```text
track sorsola
    name Il Giro di Sant'Orsola
    attribution © OpenStreetMap contributors
    licence ODbL 1.0 (https://opendatacommons.org/licenses/odbl/1-0/)
    origin 44.4896400 11.3648500        # latitude and longitude of (0, 0)
    spacing 2                           # metres between samples
    length 2317.06
end
section Via Giuseppe Mazzini            # a stretch of the route
    from 331.85 to 759.66               # metres from the start line
    highway primary
    ways 149955205
end
samples                                 # the centreline, start line first, one every 2 m:
    p -556.4 -103.2 13.6 18.4 20.6 1.2  #   x z, road width, walls left and right of the
end                                     #   centre, racing line (metres left of the centre)
checkpoints
    at 144.82 289.63 ...                # metres from the start line, in order
end
grid
    slot 7 3.4                          # metres behind the line, metres left of the centre
end
minimap
    at x z x z ...                      # a light copy of the centreline
end
area hospital Policlinico Sant'Orsola-Malpighi
    foot x z x z ...                    # ground to draw (the hospital's lawns)
end
landmark gate Porta Maggiore
    at -601.26 -47.34
    dir 0.9291 0.3699                   # the way its passage runs
    size 16.76 16.32 13                 # width (across dir), depth, height
end
props                                   # the baked scenery:
    prop tree tree_oak.glb -560.2 -80.4 2.718 7.41   # kind model x z angle scale
end
building 148503517
    kind pavilion                       # house, pavilion, hospital, church, tower, roof, shed
    name Padiglione 5 - Nuove patologie
    height 14.8
    levels 4
    foot x z x z ...                    # the footprint, with a positive shoelace sum
    portico 0 3                         # edges (point i to i+1) with a portico
end
```

* **Walls** are the nearest building front either side of each sample, the end of the
  pavement (`verge`) where there is none, or half way to another stretch of the track that runs
  alongside. Buildings that stand on the road are left out.
* **The racing line** is pulled straight between its neighbours inside the road, which cuts
  the corners; the computer racers follow it, and plan their speed on its curvature.
* **Porticoes** are put on the long fronts of houses that face the track across the pavement
  (in Bologna most have one).
* **Landmarks** face along the nearest road that is not the track's own (the road through a
  gate, not the ring road beside it).
* **props** angle is radians about the vertical: 0 faces +z (south), and a model's +z faces
  `(sin angle, cos angle)`.

## Building a track

```sh
sh tools/fetch-osm.sh 44.4850,11.3500,44.4990,11.3720 osm/sorsola.json   # south,west,north,east
cargo run -p osm2track -- games/pachino-gp osm/sorsola.json [more.json ...] [--track sorsola]
```

osm2track reads any Overpass JSON with `out geom` (ways with `nodes` and `geometry`; relations
with bounds and nodes with tags are used for landmarks), several files merged. It prints the
sections it made, with the OSM ways of each, and checks that the engine reads the file back.
The extract itself is not kept in the repository: rebuilding with a newer one gives today's
map.
