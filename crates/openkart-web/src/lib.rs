//! OpenKart in the browser. The page sends JSON requests and gets JSON answers through three
//! functions, with no generated glue (`web/js/engine.js` wraps them):
//!
//! ```js
//! const bytes = new TextEncoder().encode(JSON.stringify(request));
//! const ptr = wasm.kart_alloc(bytes.length);
//! new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
//! const out = wasm.kart_call(ptr, bytes.length);      // frees the request
//! const answer = JSON.parse(new TextDecoder().decode(
//!     new Uint8Array(wasm.memory.buffer, out, wasm.kart_out_len())));
//! ```
//!
//! Requests (`op`):
//!
//! * `load` `{files: [[name, text], ...]}`: compiles a game (its `*.kart` and `*.track` files).
//!   Answers `{game, physics, racers, items, hazards, tracks}`.
//! * `course` `{track}`: everything to draw a track: its samples, sections, buildings,
//!   landmarks, areas, minimap, scenery, pads, baskets and hazard crossings, and its `look`.
//! * `race` `{seed, track, entries: [{racer, human}], inputs?}`: starts a race (replaying
//!   `inputs`, a save, if given). Answers `{view}`.
//! * `step` `{inputs: [input per human], n?}`: `n` steps (default 1) with those inputs.
//!   Answers `{view, events}`. An input is `{steer: -100..100, gas, brake, drift, item}`.
//! * `view`; `save` (answers the replay: `{setup, inputs}`); `standings`.
//!
//! For the circuit designer, osm2track's code (see `tools/osm2track`):
//!
//! * `extract` `{docs: [overpass json, ...]}`: keeps an OpenStreetMap extract (Overpass JSON
//!   with `out geom`, several merged) and its street graph. Answers `{ways, drivable}`.
//! * `snap` `{lat, lon, within?}`: the street node nearest a click (`{snap: null}` if none
//!   within `within` metres, default 40). Answers `{snap: {node, lat, lon, way, name, metres}}`.
//! * `drive` `{from, to, against?}`: the shortest drive between two nodes as exact `route way`
//!   legs; a metre against a one-way street costs `against` metres (default 4).
//!   Answers `{drive: {spans: [{id, forward, from, to}], metres, against, line}}`.
//! * `tidy` `{spans, closed?}`: exact legs with the spurs taken out (where a leg drives back
//!   along the way the leg before came). Answers `{spans}`.
//! * `check` `{block}`: a `track ... end` block's route checked against the extract. Answers
//!   `{report: {closed, error, length, gaps, oneway, corners, problems, exact, line,
//!   sections, landmarks}}`.
//! * `bake` `{files, track}`: builds the track file for the track `track` of the game files
//!   `files` (`*.kart`) from the extract, as osm2track does, then loads the game with it, so
//!   `course` and `race` work on it. Answers `{game, text, file, length, sections, buildings,
//!   props, landmarks}`.

use std::cell::RefCell;
use std::sync::Arc;

use openkart::{load, Game, Input, Race, Replay, Setup};
use osm2track::{Graph, Osm};
use serde_json::{json, Value};

thread_local! {
    static GAME: RefCell<Option<Arc<Game>>> = const { RefCell::new(None) };
    static RACE: RefCell<Option<Race>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static EXTRACT: RefCell<Option<(Osm, Graph)>> = const { RefCell::new(None) };
}

/// Runs `f` on the extract loaded by `extract`.
fn with_extract<T>(f: impl FnOnce(&Osm, &Graph) -> Result<T, String>) -> Result<T, String> {
    EXTRACT.with(|x| match &*x.borrow() {
        Some((osm, g)) => f(osm, g),
        None => Err("load an extract first".into()),
    })
}

/// The designer's requests.
fn designer(op: &str, req: &Value) -> Result<Value, String> {
    match op {
        "extract" => {
            let docs: Vec<(String, Value)> = req["docs"]
                .as_array()
                .ok_or("docs: a list of Overpass JSON documents")?
                .iter()
                .enumerate()
                .map(|(i, d)| (format!("extract {}", i + 1), d.clone()))
                .collect();
            let osm = Osm::from_values(&docs)?;
            let graph = Graph::new(&osm);
            let (ways, drivable) = osm.counts();
            EXTRACT.with(|x| *x.borrow_mut() = Some((osm, graph)));
            Ok(json!({ "ways": ways, "drivable": drivable }))
        }
        "snap" => {
            let (lat, lon) = (
                req["lat"].as_f64().ok_or("lat")?,
                req["lon"].as_f64().ok_or("lon")?,
            );
            let within = req["within"].as_f64().unwrap_or(40.0);
            with_extract(|_, g| Ok(json!({ "snap": g.snap(lat, lon, within) })))
        }
        "drive" => {
            let (a, b) = (
                req["from"].as_u64().ok_or("from: a node")?,
                req["to"].as_u64().ok_or("to: a node")?,
            );
            with_extract(|_, g| {
                let d = g
                    .drive(a, b, req["against"].as_f64().unwrap_or(4.0).max(1.0))
                    .ok_or_else(|| format!("no street leads from node {a} to node {b}"))?;
                Ok(json!({ "drive": d }))
            })
        }
        "tidy" => {
            let spans: Vec<openkart::script::WaySpan> =
                serde_json::from_value(req["spans"].clone()).map_err(|e| format!("spans: {e}"))?;
            let closed = req["closed"].as_bool().unwrap_or(false);
            with_extract(|_, g| Ok(json!({ "spans": g.tidy(&spans, closed) })))
        }
        "check" => {
            let t = osm2track::track_block(req["block"].as_str().ok_or("block: the track block")?)?;
            with_extract(|osm, _| Ok(json!({ "report": osm2track::check(osm, &t) })))
        }
        "bake" => {
            let mut files: Vec<(String, String)> =
                serde_json::from_value(req["files"].clone()).map_err(|e| format!("files: {e}"))?;
            files.retain(|(n, _)| n.ends_with(".kart") || n.ends_with(".track"));
            let id = req["track"].as_str().ok_or("track: the track's id")?;
            let karts: Vec<(String, String)> = files
                .iter()
                .filter(|(n, _)| n.ends_with(".kart"))
                .cloned()
                .collect();
            let defs = openkart::script::compile(&karts).map_err(|e| e.to_string())?;
            let def = defs
                .tracks
                .iter()
                .find(|t| t.id == id)
                .ok_or_else(|| format!("there is no track {id:?}"))?;
            let track = with_extract(|osm, _| osm2track::build(osm, def, defs.game.field.max(8)))?;
            let text = track.to_text();
            files.retain(|(n, _)| *n != def.osm);
            files.push((def.osm.clone(), text.clone()));
            let g = load(&files).map_err(|e| e.to_string())?;
            GAME.with(|x| *x.borrow_mut() = Some(Arc::new(g)));
            RACE.with(|r| *r.borrow_mut() = None);
            Ok(json!({
                "game": defs.game, "text": text, "file": def.osm, "length": track.length,
                "sections": track.sections, "buildings": track.buildings.len(),
                "props": track.props.len(), "landmarks": track.landmarks.len(),
            }))
        }
        _ => Err(format!("unknown op {op:?}")),
    }
}

/// Room for a request of `len` bytes.
#[no_mangle]
pub extern "C" fn kart_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Answers the request at `ptr` (and frees it); the answer is at the returned pointer, for
/// [`kart_out_len`] bytes, until the next call.
///
/// # Safety
/// `ptr` must come from [`kart_alloc`] with `len` bytes written.
#[no_mangle]
pub unsafe extern "C" fn kart_call(ptr: *mut u8, len: usize) -> *const u8 {
    let bytes = Vec::from_raw_parts(ptr, len, len.max(1));
    let answer = match std::str::from_utf8(&bytes)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(t).ok())
    {
        Some(req) => handle(&req).unwrap_or_else(|e| json!({ "error": e })),
        None => json!({ "error": "the request is not JSON" }),
    };
    OUT.with(|o| {
        let mut o = o.borrow_mut();
        *o = answer.to_string().into_bytes();
        o.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn kart_out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}

fn game() -> Result<Arc<Game>, String> {
    GAME.with(|g| g.borrow().clone())
        .ok_or_else(|| "load a game first".to_string())
}

/// Answers one request (also usable natively, for tests).
pub fn handle(req: &Value) -> Result<Value, String> {
    let op = req["op"].as_str().ok_or("`op` is missing")?;
    if matches!(op, "extract" | "snap" | "drive" | "tidy" | "check" | "bake") {
        return designer(op, req);
    }
    match op {
        "load" => {
            let files: Vec<(String, String)> =
                serde_json::from_value(req["files"].clone()).map_err(|e| format!("files: {e}"))?;
            let g = load(&files).map_err(|e| e.to_string())?;
            let d = &g.defs;
            let tracks: Vec<Value> = d
                .tracks
                .iter()
                .zip(&g.courses)
                .map(|(t, c)| {
                    json!({
                        "id": t.id, "name": t.name, "text": t.text, "laps": t.laps,
                        "length": c.track.length, "attribution": c.track.attribution,
                        "licence": c.track.licence,
                    })
                })
                .collect();
            let answer = json!({
                "game": d.game, "physics": d.physics, "racers": d.racers, "items": d.items,
                "hazards": d.hazards, "tracks": tracks,
            });
            GAME.with(|x| *x.borrow_mut() = Some(Arc::new(g)));
            RACE.with(|r| *r.borrow_mut() = None);
            Ok(answer)
        }
        "course" => {
            let g = game()?;
            let id = req["track"].as_str().unwrap_or(&g.defs.game.first);
            let (i, c) = g
                .course(id)
                .ok_or_else(|| format!("there is no track {id:?}"))?;
            let look: serde_json::Map<String, Value> = g.defs.tracks[i]
                .look
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            let mut v = serde_json::to_value(&**c).map_err(|e| e.to_string())?;
            v["look"] = Value::Object(look);
            v["name"] = json!(g.defs.tracks[i].name);
            Ok(v)
        }
        "race" => {
            let g = game()?;
            let setup: Setup = serde_json::from_value(json!({
                "seed": req["seed"].as_str().unwrap_or("1"),
                "track": req["track"].as_str().unwrap_or(&g.defs.game.first),
                "entries": req["entries"],
            }))
            .map_err(|e| format!("entries: {e}"))?;
            let race = match req.get("inputs").filter(|i| !i.is_null()) {
                Some(i) => {
                    let inputs: Vec<Vec<Input>> =
                        serde_json::from_value(i.clone()).map_err(|e| format!("inputs: {e}"))?;
                    Race::replay(g, &Replay { setup, inputs })?
                }
                None => Race::new(g, &setup)?,
            };
            let view = race.view();
            RACE.with(|r| *r.borrow_mut() = Some(race));
            Ok(json!({ "view": view }))
        }
        _ => RACE.with(|r| {
            let mut r = r.borrow_mut();
            let race = r.as_mut().ok_or("start a race first")?;
            match op {
                "step" => {
                    let inputs: Vec<Input> = match req.get("inputs").filter(|i| !i.is_null()) {
                        Some(i) => {
                            serde_json::from_value(i.clone()).map_err(|e| format!("inputs: {e}"))?
                        }
                        None => Vec::new(),
                    };
                    let n = req["n"].as_u64().unwrap_or(1).min(60 * 600);
                    for _ in 0..n {
                        race.step(&inputs);
                    }
                    let events = race.take_events();
                    Ok(json!({ "view": race.view(), "events": events }))
                }
                "view" => Ok(json!({ "view": race.view() })),
                "save" => serde_json::to_value(race.save()).map_err(|e| e.to_string()),
                "standings" => Ok(json!({ "standings": race.standings() })),
                _ => Err(format!("unknown op {op:?}")),
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Value {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/pachino-gp");
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| openkart::is_game_file(n))
            .collect();
        names.sort();
        Value::Array(
            names
                .into_iter()
                .map(|n| json!([n, std::fs::read_to_string(dir.join(&n)).unwrap()]))
                .collect(),
        )
    }

    #[test]
    fn messages_race_and_replay() {
        let a = handle(&json!({ "op": "load", "files": files() })).unwrap();
        assert_eq!(a["game"]["name"], "Pachino Grand Prix");
        let c = handle(&json!({ "op": "course", "track": "sorsola" })).unwrap();
        assert!(c["track"]["samples"].as_array().unwrap().len() > 500);
        assert_eq!(c["look"]["sky"], "9fc8e8");
        let entries = json!([
            { "racer": "pachino", "human": true },
            { "racer": "carota" }, { "racer": "kingsteak" }, { "racer": "prosciutto" }
        ]);
        let r =
            handle(&json!({ "op": "race", "seed": "w", "track": "sorsola", "entries": entries }))
                .unwrap();
        assert_eq!(r["view"]["phase"], "countdown");
        let s =
            handle(&json!({ "op": "step", "n": 400, "inputs": [{ "steer": 10, "gas": true }] }))
                .unwrap();
        assert_eq!(s["view"]["phase"], "race");
        assert!(s["view"]["karts"][0]["speed"].as_f64().unwrap() > 5.0);
        let save = handle(&json!({ "op": "save" })).unwrap();
        let again = handle(&json!({
            "op": "race", "seed": save["setup"]["seed"], "track": "sorsola",
            "entries": save["setup"]["entries"], "inputs": save["inputs"],
        }))
        .unwrap();
        assert_eq!(again["view"], s["view"]);
        assert!(handle(&json!({ "op": "fly" })).unwrap_err().contains("fly"));
    }

    /// A 240 x 160 m block of four streets near Bologna, as Overpass JSON.
    fn block_extract() -> Value {
        let (lat0, lon0): (f64, f64) = (44.49, 11.36);
        let k = lat0.to_radians().cos();
        let ll = |x: f64, z: f64| json!({ "lat": lat0 - z / 111_320.0, "lon": lon0 + x / (111_320.0 * k) });
        let corners = [
            (1u64, 0.0, 0.0),
            (2, 120.0, 0.0),
            (3, 240.0, 0.0),
            (4, 240.0, -160.0),
            (5, 0.0, -160.0),
        ];
        let node = |id: u64| {
            let (_, x, z) = corners.iter().find(|c| c.0 == id).unwrap();
            ll(*x, *z)
        };
        let way = |id: u64, nodes: &[u64], name: &str| {
            json!({ "type": "way", "id": id, "nodes": nodes,
                "geometry": nodes.iter().map(|n| node(*n)).collect::<Vec<_>>(),
                "tags": { "highway": "residential", "name": name } })
        };
        json!({ "elements": [
            way(10, &[1, 2, 3], "Via Sud"),
            way(11, &[3, 4], "Via Est"),
            way(12, &[4, 5], "Via Nord"),
            way(13, &[5, 1], "Via Ovest"),
        ]})
    }

    #[test]
    fn designer_messages_route_check_and_bake() {
        assert!(handle(&json!({ "op": "snap", "lat": 44.49, "lon": 11.36 }))
            .unwrap_err()
            .contains("extract"));
        let a = handle(&json!({ "op": "extract", "docs": [block_extract()] })).unwrap();
        assert_eq!(a["drivable"], 4);
        // A click near the south-east corner, and the drive there from the south-west one.
        let s = handle(
            &json!({ "op": "snap", "lat": 44.49 + 3.0 / 111_320.0, "lon": 11.36 + 0.00301 }),
        )
        .unwrap();
        assert_eq!(s["snap"]["node"], 3);
        let d = handle(&json!({ "op": "drive", "from": 1, "to": 3 })).unwrap();
        assert_eq!(
            d["drive"]["spans"],
            json!([{ "id": 10, "forward": true, "from": 1, "to": 3 }])
        );
        // Round the block and back to the start, with a spur up Via Est and down again.
        let mut spans = Vec::new();
        for (a, b) in [(1, 3), (3, 4), (4, 3), (3, 4), (4, 5), (5, 1)] {
            let d = handle(&json!({ "op": "drive", "from": a, "to": b, "against": 1 })).unwrap();
            spans.extend(d["drive"]["spans"].as_array().unwrap().clone());
        }
        let t = handle(&json!({ "op": "tidy", "spans": spans, "closed": true })).unwrap();
        let legs = t["spans"].as_array().unwrap();
        assert_eq!(legs.len(), 4, "{legs:?}");
        let lines: String = legs
            .iter()
            .map(|l| {
                format!(
                    "    route way {} {} from {} to {}\n",
                    l["id"],
                    if l["forward"] == true {
                        "forward"
                    } else {
                        "backward"
                    },
                    l["from"],
                    l["to"]
                )
            })
            .collect();
        let block = format!("track square\n    name The Square\n    osm square.track\n{lines}    start Via Est 0.5\n    dress tree tree_oak.glb every 20 along Via Nord\nend\n");
        let c = handle(&json!({ "op": "check", "block": block })).unwrap();
        let r = &c["report"];
        assert_eq!(r["closed"], true, "{r}");
        assert!(
            (r["length"].as_f64().unwrap() - 800.0).abs() < 30.0,
            "{}",
            r["length"]
        );
        assert_eq!(r["exact"].as_array().unwrap().len(), 4);
        assert!(r["problems"].as_array().unwrap().is_empty(), "{r}");
        // A gap: Via Sud then Via Nord.
        let bad = handle(&json!({ "op": "check", "block": "track g\n    osm g.track\n    route way 10\n    route way 12\nend\n" })).unwrap();
        assert_eq!(bad["report"]["closed"], false);
        assert_eq!(bad["report"]["gaps"].as_array().unwrap().len(), 2);
        // Baked with the game's files: the course is there to draw, and it races.
        let mut files: Vec<Value> = files().as_array().unwrap().clone();
        files.push(json!(["square.kart", block]));
        let b = handle(&json!({ "op": "bake", "files": files, "track": "square" })).unwrap();
        assert!(b["text"]
            .as_str()
            .unwrap()
            .contains("OpenStreetMap contributors"));
        assert!(b["props"].as_u64().unwrap() > 0);
        let course = handle(&json!({ "op": "course", "track": "square" })).unwrap();
        assert!(course["track"]["samples"].as_array().unwrap().len() > 300);
        let entries = json!([{ "racer": "pachino" }, { "racer": "carota" }, { "racer": "kingsteak" }, { "racer": "prosciutto" }]);
        let r =
            handle(&json!({ "op": "race", "seed": "sq", "track": "square", "entries": entries }))
                .unwrap();
        assert_eq!(r["view"]["karts"].as_array().unwrap().len(), 4);
    }
}
