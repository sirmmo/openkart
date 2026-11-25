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

use std::cell::RefCell;
use std::sync::Arc;

use openkart::{load, Game, Input, Race, Replay, Setup};
use serde_json::{json, Value};

thread_local! {
    static GAME: RefCell<Option<Arc<Game>>> = const { RefCell::new(None) };
    static RACE: RefCell<Option<Race>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
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
}
