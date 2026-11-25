//! osm2track: builds an OpenKart track file from an OpenStreetMap extract and a route.
//!
//! ```sh
//! cargo run -p osm2track -- games/pachino-gp osm/sorsola.json [more.json ...] [--track sorsola]
//! ```
//!
//! See the library (`lib.rs`) for what it does. Map data © OpenStreetMap contributors (ODbL
//! 1.0): the track file it writes is a Derived Database under the same licence.

use std::path::{Path, PathBuf};

use openkart::script;
use openkart::track::Track;
use osm2track::{build, Osm};

const USAGE: &str = "usage: osm2track <game folder> <overpass.json>... [--track <id>]";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut dir = None;
    let mut inputs = Vec::new();
    let mut only = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--track" => {
                only = args.get(i + 1).cloned();
                i += 1;
            }
            "-h" | "--help" => {
                eprintln!("{USAGE}");
                return;
            }
            a if dir.is_none() => dir = Some(PathBuf::from(a)),
            a => inputs.push(PathBuf::from(a)),
        }
        i += 1;
    }
    let (Some(dir), false) = (dir, inputs.is_empty()) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    if let Err(e) = run(&dir, &inputs, only.as_deref()) {
        eprintln!("osm2track: {e}");
        std::process::exit(1);
    }
}

fn run(dir: &Path, inputs: &[PathBuf], only: Option<&str>) -> Result<(), String> {
    let mut files = Vec::new();
    let mut names: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "kart"))
        .collect();
    names.sort();
    for p in names {
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        files.push((p.file_name().unwrap().to_string_lossy().into_owned(), text));
    }
    let defs = script::compile(&files).map_err(|e| e.to_string())?;
    let osm = Osm::read(inputs)?;
    let mut done = 0;
    for t in &defs.tracks {
        if only.is_some_and(|o| o != t.id) {
            continue;
        }
        let track = build(&osm, t, defs.game.field.max(8))?;
        let out = dir.join(&t.osm);
        std::fs::write(&out, track.to_text()).map_err(|e| format!("{}: {e}", out.display()))?;
        // Check that the engine reads it back.
        let back = Track::parse(&t.osm, &std::fs::read_to_string(&out).unwrap())
            .map_err(|e| format!("the written file does not read back: {e}"))?;
        eprintln!(
            "{}: {} m, {} samples, {} sections, {} buildings ({} with porticoes), {} landmarks, {} props",
            out.display(),
            back.length.round(),
            back.samples.len(),
            back.sections.len(),
            back.buildings.len(),
            back.buildings
                .iter()
                .filter(|b| !b.portico.is_empty())
                .count(),
            back.landmarks.len(),
            back.props.len()
        );
        for s in &back.sections {
            eprintln!(
                "  {:>7.0} {:>7.0}  {:<36} {:<10} ways {:?}",
                s.from, s.to, s.name, s.highway, s.ways
            );
        }
        done += 1;
    }
    if done == 0 {
        return Err(format!("there is no track {:?}", only.unwrap_or("")));
    }
    Ok(())
}
