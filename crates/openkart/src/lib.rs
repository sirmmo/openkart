//! OpenKart: an arcade kart racing engine. Tracks are built from OpenStreetMap; racers, items,
//! hazards and the physics come from plain-text game files.
//!
//! * [`script`] reads game files (`*.kart`) into definitions.
//! * [`track`] reads and writes track files (`*.track`), and finds things on them.
//! * [`course`] prepares a track for racing: scenery placement, baskets, pads, hazards.
//! * [`sim`] races: a fixed 60 Hz step, deterministic, so a race is its seed and each
//!   player's inputs, tick by tick.
//!
//! ```no_run
//! let game = openkart::load_dir(std::path::Path::new("games/pachino-gp")).unwrap();
//! let setup = openkart::Setup::computer("demo", &game, "sorsola");
//! let mut race = openkart::Race::new(std::sync::Arc::new(game), &setup).unwrap();
//! while !race.over() {
//!     race.step(&[]);
//! }
//! ```

pub mod course;
pub mod det;
pub mod script;
pub mod sim;
pub mod track;

use std::sync::Arc;

pub use course::Course;
pub use script::{Defs, Error};
pub use sim::{Entry, Event, Input, Race, Replay, Setup};
pub use track::Track;

/// A compiled game: its definitions, and each track's course.
#[derive(Clone, Debug)]
pub struct Game {
    pub defs: Defs,
    /// One per `defs.tracks`, in the same order.
    pub courses: Vec<Arc<Course>>,
}

impl Game {
    pub fn course(&self, track: &str) -> Option<(usize, &Arc<Course>)> {
        let i = self.defs.track(track)?;
        Some((i, &self.courses[i]))
    }
}

/// Is this the name of a game file (a `*.kart`) or a track file (a `*.track`)?
pub fn is_game_file(name: &str) -> bool {
    name.ends_with(".kart") || name.ends_with(".track")
}

/// Compiles a game from its files: `(name, text)` for every `*.kart` and `*.track`.
pub fn load(files: &[(String, String)]) -> Result<Game, Error> {
    let karts: Vec<(String, String)> = files
        .iter()
        .filter(|(n, _)| n.ends_with(".kart"))
        .cloned()
        .collect();
    let defs = script::compile(&karts)?;
    let mut courses = Vec::new();
    for t in &defs.tracks {
        let (name, text) = files
            .iter()
            .find(|(n, _)| n == &t.osm || n.rsplit('/').next() == Some(t.osm.as_str()))
            .ok_or_else(|| {
                Error::new(
                    &t.osm,
                    0,
                    format!(
                        "track {} reads {}, which is not among the game's files (build it with tools/osm2track)",
                        t.id, t.osm
                    ),
                )
            })?;
        let track = Track::parse(name, text)?;
        courses.push(Arc::new(Course::new(&defs, t, track)?));
    }
    Ok(Game { defs, courses })
}

/// Loads every `*.kart` and `*.track` file of a folder.
pub fn load_dir(dir: &std::path::Path) -> Result<Game, Error> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| Error::new(&dir.display().to_string(), 0, e.to_string()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_game_file)
        })
        .collect();
    names.sort();
    let mut files = Vec::new();
    for p in names {
        let text = std::fs::read_to_string(&p)
            .map_err(|e| Error::new(&p.display().to_string(), 0, e.to_string()))?;
        files.push((p.file_name().unwrap().to_string_lossy().into_owned(), text));
    }
    load(&files)
}
