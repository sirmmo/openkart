# The OpenKart engine

`crates/openkart` is a Rust library with no dependencies beyond serde. It knows nothing about
tomatoes or Bologna: a game's files say what races where.

| Module | What |
| --- | --- |
| `script` | Game files (`*.kart`) into definitions: `Defs` (game, physics, tracks, racers, items, hazards) |
| `track` | Track files (`*.track`), both ways, and geometry: where a point is on the track, the track at a distance |
| `course` | A track made ready to race: scenery (`dress`), boost pads, baskets, hazard crossings, racing-line curvature |
| `sim` | The race: karts, the computer's driving, items, hazards, laps and places |
| `det` | Deterministic arithmetic: polynomial sine and cosine, a seeded xorshift |

## Loading a game

```rust
let game = openkart::load_dir(Path::new("games/pachino-gp"))?;   // every *.kart and *.track
let game = openkart::load(&[("game.kart".into(), text), ...])?;   // or (name, text) pairs
```

Errors are `openkart::Error { file, line, msg }` and print as `file:line: msg`.

## Racing

```rust
let setup = Setup {
    seed: "42".into(),
    track: "sorsola".into(),
    entries: vec![                     // the grid, first slot first
        Entry { racer: "carota".into(), human: false },
        Entry { racer: "pachino".into(), human: true },   // input 0 of every step
        ...
    ],
};
let mut race = Race::new(Arc::new(game), &setup)?;
while !race.over() {
    race.step(&[Input { steer: 30, gas: true, ..Input::default() }]);   // one per human
    for event in race.take_events() { ... }
}
let view = race.view();          // what to draw
let results = race.standings();  // place, racer, time, best lap
```

* **The step** is 1/60 s (`sim::HZ`, `sim::DT`). A race starts with the countdown
  (`physics countdown`), then runs until every racer has finished, or the time limit.
* **An input** is `{ steer: -100 (right) .. 100 (left), gas, brake, drift, item }`. Steering
  is an integer so a saved input replays exactly. `item` uses the held item when it goes from
  off to on. `drift` held while steering at speed starts a drift; letting go releases its
  boost.
* **Computer racers** aim at the racing line ahead (looking further ahead the faster they go),
  each with a lane that wanders a little and moves out of the way of the racer just in front,
  plan their speed on the line's curvature with room to brake, drift through long corners,
  use items when someone is near ahead (or behind, for a slick), back off when stuck, and are
  put back on the road if they make no progress for six seconds. `skill` scales how hard they
  corner, how much they drift and how straight they keep. Behind the leading human (or the
  leader, in a race with none) they gain up to `catchup` on top speed; ahead of it they lose a
  little.
* **A human who finishes** is driven by the computer for the rest of the race.
* **Laps** count when a kart has passed every checkpoint in order and crosses the line again;
  places go to the finished by time, then by distance along the track.

## Determinism and replays

A race is its `Setup` and every step's human inputs: `race.save()` is a `Replay { setup,
inputs }` and `Race::replay(game, &replay)` plays it again exactly, natively or in the
browser. The engine uses only `+ - * /` and `sqrt` on `f64` (correctly rounded everywhere,
WebAssembly included), its own polynomial sine and cosine (`det::sin_cos`) for turning, a
seeded xorshift for lanes and items, and no hash-ordered iteration. Hazards keep to the race
clock.

## The track

`Track::locate(x, z, hint)` finds the nearest segment to a point (searching the segments around
`hint`, or all of them), and gives its distance from the start line `s` and from the centre
`d` (left positive). `Track::frame(s)` gives the centre, the direction, the road width, the
walls and the racing line at a distance; `Track::place` turns a place (`Via Roma 0.5`) into
metres. A kart off the road (`|d|` beyond the kerb) loses top speed; one at a wall is pushed
back along the normal, loses the speed it had into the wall and is turned off it.

## The view

`race.view()` (`sim::View`) is what a client draws each step:

```text
{ tick, time, phase: "countdown" | "race" | "over",
  karts: [{ racer, human, x, z, hx, hz, speed, s, d, lap, place, item, unpacking,
            drift (-1, 0, 1), tier (0, 1, 2: the drift's charge), boost, spin, splat, bump,
            off, time (at the finish), laps (lap times) }],
  shots: [{ item, x, z, vx, vz, aiming }],   slicks: [{ item, x, z }],
  baskets: [true, false, ...],               hazards: [{ id, x, z, hx, hz, moving }],
  order: [kart indices, first place first] }
```

Events (`race.take_events()`): `count {n}` (3, 2, 1, then 0: go), `pickup {kart}`,
`use {kart, item}`, `hit {kart, by, effect}`, `boost {kart, tier}` (a drift's), `pad {kart}`,
`wall {kart, hard}`, `bump {a, b}`, `lap {kart, lap}` (now driving that lap),
`finish {kart, place, time}`, `over`.

## In the browser

`crates/openkart-web` is the engine as WebAssembly, with no generated glue: the page writes a
JSON request into the module's memory and reads a JSON answer (see `web/js/engine.js`).

| Request | Answer |
| --- | --- |
| `{op: "load", files: [[name, text], ...]}` | `{game, physics, racers, items, hazards, tracks}` |
| `{op: "course", track}` | the course: `track` (samples, sections, buildings, landmarks, areas, minimap, grid), `props`, `pads`, `baskets`, `crossings`, `look`, `name` |
| `{op: "race", seed, track, entries, inputs?}` | `{view}` (with `inputs`, the replay played to its end) |
| `{op: "step", inputs, n?}` | `{view, events}` after `n` steps (default 1) |
| `{op: "view"}`, `{op: "save"}`, `{op: "standings"}` | the view, the replay, the results |

Any request can answer `{error}`. `tools/site.sh` lists each game's files in
`games/<id>/files.json` for the page to fetch.

The client (`web/`, three.js, no build step) is: `js/world.js` (the road, kerbs, pavements,
barriers, buildings with porticoes, gates, scenery, pads, baskets), `js/karts.js` (racers,
items, hazards, particles), `js/hud.js`, `js/input.js` (keyboard, gamepad, touch) and
`js/main.js` (the title, the race loop at 60 steps a second, results and replays). URL
parameters: `game`, `track`, `racer`, `seed`; `race=1` starts at once, `auto=1` lets the
computer drive your kart, `at=<seconds>` skips into the race, `cam=far` raises the camera.

## osm2track

`tools/osm2track` (a binary of the workspace) builds track files: see
[Game files](RULES.md#building-a-track). It uses the engine's own `Track` writer and
`course::dress`, and reads the file it wrote back through the engine, so the two always agree.
