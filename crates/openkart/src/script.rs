//! Game files: plain text, in blocks of the form
//!
//! ```text
//! kind name           # a block starts at the start of a line
//!     key words...    # its lines, indented
//! end
//! ```
//!
//! A `#` that starts a word starts a comment. A game is a folder of `*.kart` files (the game,
//! its tracks, racers, items, hazards and the physics), and every track names its `*.track`
//! file, built from OpenStreetMap by `tools/osm2track` and written in the same block form (see
//! [`crate::track`]).
//!
//! ```text
//! game pachino-gp      # the game: name, asset folders, credits, how many racers
//! physics              # the karts' numbers: top speed, grip, drift, items' boxes...
//! track sorsola        # a race: its track file, laps, route, scenery, baskets, pads
//! racer pachino        # a racer: model, kart, stats, how well the computer drives it
//! item pomodorata      # something found in a picnic basket
//! hazard ambulance     # something on the track that keeps to a timetable
//! ```
//!
//! Every name is declared before it is used, and every error names its file and line.

use std::fmt;

use serde::Serialize;

/// An error in a game file: where and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub file: String,
    pub line: usize,
    pub msg: String,
}

impl Error {
    pub fn new(file: &str, line: usize, msg: impl Into<String>) -> Error {
        Error {
            file: file.to_string(),
            line,
            msg: msg.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "{}:{}: {}", self.file, self.line, self.msg)
        } else {
            write!(f, "{}: {}", self.file, self.msg)
        }
    }
}

impl std::error::Error for Error {}

/// One line of a block: its number in the file and its words.
#[derive(Clone, Debug)]
pub struct Line {
    pub no: usize,
    pub words: Vec<String>,
}

impl Line {
    /// The words from `i` on, joined: free text such as names.
    pub fn rest(&self, i: usize) -> String {
        self.words.get(i..).map(|w| w.join(" ")).unwrap_or_default()
    }

    pub fn key(&self) -> &str {
        &self.words[0]
    }
}

/// `kind name ... end`.
#[derive(Clone, Debug)]
pub struct Block {
    pub file: String,
    pub no: usize,
    pub kind: String,
    pub name: String,
    pub lines: Vec<Line>,
}

impl Block {
    pub fn err(&self, l: &Line, msg: impl Into<String>) -> Error {
        Error::new(&self.file, l.no, msg)
    }

    pub fn here(&self, msg: impl Into<String>) -> Error {
        Error::new(&self.file, self.no, msg)
    }

    /// "racer pachino: unknown line ..." for a line the block does not take.
    pub fn unknown(&self, l: &Line) -> Error {
        self.err(
            l,
            format!(
                "{} {}: unknown line {:?}",
                self.kind,
                self.name,
                l.words.join(" ")
            ),
        )
    }

    /// Word `i` of a line, or an error saying the line is incomplete.
    pub fn word<'a>(&self, l: &'a Line, i: usize) -> Result<&'a str, Error> {
        l.words
            .get(i)
            .map(String::as_str)
            .ok_or_else(|| self.err(l, format!("{:?} is incomplete", l.words.join(" "))))
    }

    /// Word `i` of a line as a number.
    pub fn num(&self, l: &Line, i: usize) -> Result<f64, Error> {
        let w = self.word(l, i)?;
        w.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| {
                self.err(
                    l,
                    format!("{} wants a number, not {w:?}", l.words[..i].join(" ")),
                )
            })
    }

    /// Word `i` as a number within `lo..=hi`.
    pub fn num_in(&self, l: &Line, i: usize, lo: f64, hi: f64) -> Result<f64, Error> {
        let v = self.num(l, i)?;
        if v < lo || v > hi {
            return Err(self.err(
                l,
                format!("{} must be between {lo} and {hi}, not {v}", l.words[0]),
            ));
        }
        Ok(v)
    }

    /// Every word from `i` on as a number.
    pub fn nums(&self, l: &Line, i: usize) -> Result<Vec<f64>, Error> {
        (i..l.words.len()).map(|k| self.num(l, k)).collect()
    }
}

/// Splits a file into blocks. A line that starts at column 0 starts a block (or is its `end`);
/// the lines in between belong to it.
pub fn blocks(file: &str, text: &str) -> Result<Vec<Block>, Error> {
    let mut out: Vec<Block> = Vec::new();
    let mut open: Option<Block> = None;
    for (i, raw) in text.lines().enumerate() {
        let no = i + 1;
        let words = words(raw);
        if words.is_empty() {
            continue;
        }
        let top = !raw.starts_with([' ', '\t']);
        match (&mut open, top) {
            (Some(_), true) if words[0] == "end" => {
                if words.len() > 1 {
                    return Err(Error::new(file, no, "`end` takes nothing after it"));
                }
                out.push(open.take().unwrap());
            }
            (Some(b), true) => {
                return Err(Error::new(
                    file,
                    no,
                    format!(
                        "{:?} starts a new block, but {} {} (line {}) has no `end`",
                        words.join(" "),
                        b.kind,
                        b.name,
                        b.no
                    ),
                ));
            }
            (Some(b), false) => b.lines.push(Line { no, words }),
            (None, false) => {
                return Err(Error::new(
                    file,
                    no,
                    format!(
                        "{:?} is indented but is not inside a block",
                        words.join(" ")
                    ),
                ));
            }
            (None, true) => {
                if words[0] == "end" {
                    return Err(Error::new(file, no, "`end` without a block"));
                }
                open = Some(Block {
                    file: file.to_string(),
                    no,
                    kind: words[0].clone(),
                    name: words[1..].join(" "),
                    lines: Vec::new(),
                });
            }
        }
    }
    if let Some(b) = open {
        return Err(Error::new(
            file,
            b.no,
            format!("{} {} has no `end`", b.kind, b.name),
        ));
    }
    Ok(out)
}

/// A line's words, without its comment.
fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in line.split_whitespace() {
        if w.starts_with('#') {
            break;
        }
        out.push(w.to_string());
    }
    out
}

// ---------- the game's definitions ----------

/// The `game` block.
#[derive(Clone, Debug, Serialize)]
pub struct GameDef {
    pub id: String,
    pub name: String,
    pub text: String,
    /// (kind, folder): where the client finds the models of racers, karts, items, props...
    pub assets: Vec<(String, String)>,
    /// Lines for the credits screen.
    pub credits: Vec<String>,
    /// Racers in a race (the player and the computer's).
    pub field: usize,
    /// The track raced when none is chosen.
    pub first: String,
}

/// The karts' numbers, shared by every racer (each racer's stats scale them).
#[derive(Clone, Debug, Serialize)]
pub struct Physics {
    /// Top speed, m/s, for a racer of speed 3.
    pub top: f64,
    /// Acceleration from standstill, m/s², for accel 3.
    pub accel: f64,
    pub brake: f64,
    /// Rolling drag with no pedal, per second.
    pub coast: f64,
    /// Steering, radians per second at full lock.
    pub turn: f64,
    /// Sideways grip: how fast the kart's travel follows its nose, per second.
    pub grip: f64,
    /// Grip while drifting (lower: the kart slides out).
    pub drift_grip: f64,
    /// Seconds of drifting for the first and second boost.
    pub drift_charge: (f64, f64),
    /// Seconds of boost those give.
    pub drift_boost: (f64, f64),
    /// Top speed times this while boosting.
    pub boost_power: f64,
    /// Top speed times this off the road (on the pavement and verges).
    pub offroad: f64,
    /// Sideways acceleration the computer racers plan corners with, m/s².
    pub corner: f64,
    /// The most a computer racer behind the leading human gains on top speed (0.05: 5%).
    pub catchup: f64,
    /// Seconds a picnic basket takes to come back.
    pub basket_respawn: f64,
    /// Seconds a racer unpacks a basket before the item can be used.
    pub unpack: f64,
    /// Seconds of the countdown.
    pub countdown: f64,
    /// Seconds a race may last before everyone still out is classified as not finishing.
    pub time_limit: f64,
    /// Seconds of boost from a boost pad.
    pub pad_boost: f64,
    /// Kart radius for collisions, metres.
    pub radius: f64,
}

impl Default for Physics {
    fn default() -> Self {
        Physics {
            top: 30.0,
            accel: 14.0,
            brake: 28.0,
            coast: 0.5,
            turn: 2.2,
            grip: 9.0,
            drift_grip: 2.2,
            drift_charge: (0.9, 1.9),
            drift_boost: (0.7, 1.4),
            boost_power: 1.3,
            offroad: 0.55,
            corner: 20.0,
            catchup: 0.05,
            basket_respawn: 4.0,
            unpack: 1.0,
            countdown: 3.0,
            time_limit: 900.0,
            pad_boost: 1.0,
            radius: 1.0,
        }
    }
}

/// A place on a track: metres from the start line, or a fraction of one of its sections
/// (`Via Giuseppe Mazzini 0.5`: half way along the stretch of the route on that street).
#[derive(Clone, Debug, Serialize, PartialEq)]
pub enum Place {
    Metres(f64),
    In(String, f64),
}

/// One leg of a route: every OSM way of a street (by its name), or a list of way ids.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub enum Leg {
    /// Every drivable way with this name.
    Street(String),
    /// These ways, one after the other, each driven in whichever direction closes the loop.
    Ways(Vec<u64>),
    /// One way, optionally one way round and only between two of its nodes: the exact form,
    /// as a circuit designer writes it.
    Way(WaySpan),
}

/// `route way 617896282 forward from 5840652725 to 5840652728`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct WaySpan {
    pub id: u64,
    /// `Some(true)`: in the order of the way's nodes; `Some(false)`: against it.
    pub forward: Option<bool>,
    /// Enter the way at this node, and leave it at that one.
    pub from: Option<u64>,
    pub to: Option<u64>,
}

/// Which side of the road scenery goes on.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
    Both,
}

/// `dress tree plane_tree every 12 along Viale Giambattista Ercolani`: scenery set along the
/// road every so many metres, on the pavement beyond the kerb.
#[derive(Clone, Debug, Serialize)]
pub struct Dress {
    pub kind: String,
    pub model: String,
    pub every: f64,
    /// A section name, or every section.
    pub along: Option<String>,
    pub side: Side,
    /// Metres beyond the road's edge.
    pub offset: f64,
    /// The model's scale: each copy gets one between these (the same every bake).
    pub scale: (f64, f64),
    /// Which way the copies face.
    pub face: Face,
    #[serde(skip)]
    pub line: usize,
}

/// Which way scenery faces: towards the road, along it, or any way.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Face {
    Road,
    Along,
    Random,
}

/// `landmark gate Porta San Vitale`: something osm2track looks up by name in the map
/// (or places at `at lat lon` if the map has no such thing).
#[derive(Clone, Debug, Serialize)]
pub struct LandmarkSpec {
    pub kind: String,
    pub name: String,
    pub at: Option<(f64, f64)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TrackDef {
    pub id: String,
    /// The file and line it is declared at (for errors found when the track file is read).
    #[serde(skip)]
    pub file: String,
    #[serde(skip)]
    pub no: usize,
    pub name: String,
    pub text: String,
    /// The track file, next to the game files.
    pub osm: String,
    pub laps: u32,
    /// The route, leg by leg, joined by osm2track into one closed loop.
    pub route: Vec<Leg>,
    /// Where the start line is.
    pub start: Place,
    /// Road width: times this, and at least this (metres).
    pub widen: f64,
    pub min_width: f64,
    /// Pavement between the road and the walls, metres.
    pub verge: f64,
    /// Buildings this close to the road come with the track, metres.
    pub scenery: f64,
    pub dress: Vec<Dress>,
    pub landmarks: Vec<LandmarkSpec>,
    /// A row of baskets every so many metres (the first this far after the start).
    pub baskets_every: f64,
    pub baskets_first: f64,
    /// Baskets in each row, across the road.
    pub baskets_across: usize,
    /// Boost pads.
    pub pads: Vec<Place>,
    #[serde(skip)]
    pub pad_lines: Vec<usize>,
    /// Hazards that run on this track.
    pub hazards: Vec<String>,
    /// Look hints for the client: sky, ground, palette... (the engine passes them on).
    pub look: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RacerDef {
    pub id: String,
    pub name: String,
    pub text: String,
    /// The driver's model and the kart's, in the `racers` and `karts` asset folders.
    pub model: String,
    pub kart: String,
    pub colour: String,
    /// Stats from 1 to 5.
    pub speed: f64,
    pub accel: f64,
    pub grip: f64,
    pub weight: f64,
    /// How well the computer drives this racer, 0 to 1.
    pub skill: f64,
    /// Look hints for the client (`scale`, `lift`, `turn`, `eyes`, `crown`...).
    pub look: Vec<(String, String)>,
}

/// What an item does when used.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    /// Thrown forward, homing a little on the racer ahead.
    Throw,
    /// Dropped behind.
    Drop,
    /// Used on oneself.
    Boost,
    /// Rolls ahead along the road.
    Roll,
    /// Waits, aims at the nearest racer ahead, then rolls in a straight line.
    Aim,
}

/// What happens to a racer hit by an item or a hazard.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    /// The windscreen is splattered: slower, and the player sees a blur.
    Splat,
    /// The kart spins round and slows down.
    Spin,
    /// Knocked sideways, losing most of its speed.
    Bump,
    /// Faster (on oneself).
    Boost,
}

#[derive(Clone, Debug, Serialize)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    pub text: String,
    pub kind: ItemKind,
    pub model: String,
    pub effect: Effect,
    /// Seconds the effect lasts.
    pub time: f64,
    /// Top speed times this while the effect lasts (splat, boost).
    pub power: f64,
    /// m/s, for things that move.
    pub speed: f64,
    /// Seconds before it is gone.
    pub life: f64,
    /// Seconds an `aim` item aims.
    pub aim: f64,
    /// Metres: how close counts as a hit.
    pub radius: f64,
    /// Chance by place in the race: first, then on to last (as many numbers as you like; the
    /// field is spread over them).
    pub odds: Vec<f64>,
    /// Racers it can hit before it is gone (a slick: a few; a tomato: one).
    pub hits: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct HazardDef {
    pub id: String,
    #[serde(skip)]
    pub file: String,
    #[serde(skip)]
    pub no: usize,
    pub name: String,
    pub text: String,
    pub model: String,
    /// Where it crosses the road.
    pub at: Place,
    /// It starts crossing every `every` seconds, `offset` seconds into the race.
    pub every: f64,
    pub offset: f64,
    pub speed: f64,
    pub length: f64,
    pub width: f64,
    /// Metres beyond the walls it starts and ends.
    pub margin: f64,
    pub effect: Effect,
    pub time: f64,
}

/// Everything a game's files declare.
#[derive(Clone, Debug, Serialize)]
pub struct Defs {
    pub game: GameDef,
    pub physics: Physics,
    pub tracks: Vec<TrackDef>,
    pub racers: Vec<RacerDef>,
    pub items: Vec<ItemDef>,
    pub hazards: Vec<HazardDef>,
}

impl Defs {
    pub fn track(&self, id: &str) -> Option<usize> {
        self.tracks.iter().position(|t| t.id == id)
    }
    pub fn racer(&self, id: &str) -> Option<usize> {
        self.racers.iter().position(|r| r.id == id)
    }
    pub fn item(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|r| r.id == id)
    }
    pub fn hazard(&self, id: &str) -> Option<usize> {
        self.hazards.iter().position(|r| r.id == id)
    }
}

/// The block kinds of game files.
pub const KINDS: [&str; 6] = ["game", "physics", "track", "racer", "item", "hazard"];

/// Compiles a game's `*.kart` files (name, text). Track files are not read here.
pub fn compile(files: &[(String, String)]) -> Result<Defs, Error> {
    let mut all = Vec::new();
    for (name, text) in files {
        all.extend(blocks(name, text)?);
    }
    for b in &all {
        if !KINDS.contains(&b.kind.as_str()) {
            return Err(b.here(format!(
                "unknown block {:?} (blocks are {})",
                b.kind,
                KINDS.join(", ")
            )));
        }
        if b.name.is_empty() && b.kind != "physics" {
            return Err(b.here(format!("{} wants a name: `{} <id>`", b.kind, b.kind)));
        }
    }
    let games: Vec<&Block> = all.iter().filter(|b| b.kind == "game").collect();
    let gb = match games.as_slice() {
        [g] => *g,
        [] => {
            return Err(Error::new(
                files.first().map(|f| f.0.as_str()).unwrap_or("game"),
                0,
                "there is no `game` block",
            ))
        }
        [_, g, ..] => return Err(g.here("there is more than one `game` block")),
    };
    let mut defs = Defs {
        game: game(gb)?,
        physics: Physics::default(),
        tracks: Vec::new(),
        racers: Vec::new(),
        items: Vec::new(),
        hazards: Vec::new(),
    };
    let mut seen_physics = false;
    for kind in ["physics", "item", "hazard", "racer", "track"] {
        for b in all.iter().filter(|b| b.kind == kind) {
            match kind {
                "physics" => {
                    if seen_physics {
                        return Err(b.here("there is more than one `physics` block"));
                    }
                    seen_physics = true;
                    physics(&mut defs.physics, b)?;
                }
                "item" => {
                    if defs.item(&b.name).is_some() {
                        return Err(dup(b));
                    }
                    let it = item(b)?;
                    defs.items.push(it);
                }
                "hazard" => {
                    if defs.hazard(&b.name).is_some() {
                        return Err(dup(b));
                    }
                    let h = hazard(b)?;
                    defs.hazards.push(h);
                }
                "racer" => {
                    if defs.racer(&b.name).is_some() {
                        return Err(dup(b));
                    }
                    let r = racer(b)?;
                    defs.racers.push(r);
                }
                _ => {
                    if defs.track(&b.name).is_some() {
                        return Err(dup(b));
                    }
                    let t = track(&defs, b)?;
                    defs.tracks.push(t);
                }
            }
        }
    }
    if defs.tracks.is_empty() {
        return Err(gb.here("the game has no `track`"));
    }
    if defs.racers.len() < defs.game.field.max(1) {
        return Err(gb.here(format!(
            "a race has {} racers (`field`), but only {} are declared",
            defs.game.field,
            defs.racers.len()
        )));
    }
    if defs.items.is_empty() {
        return Err(gb.here("the game has no `item` for the picnic baskets"));
    }
    if defs.game.first.is_empty() {
        defs.game.first = defs.tracks[0].id.clone();
    } else if defs.track(&defs.game.first).is_none() {
        return Err(gb.here(format!("first: there is no track {:?}", defs.game.first)));
    }
    Ok(defs)
}

fn dup(b: &Block) -> Error {
    b.here(format!("{} {} is declared twice", b.kind, b.name))
}

fn add_text(to: &mut String, l: &Line) {
    if !to.is_empty() {
        to.push(' ');
    }
    to.push_str(&l.rest(1));
}

/// Hex without `#` (which starts a comment): `colour e8352b`.
fn colour(b: &Block, l: &Line) -> Result<String, Error> {
    let w = b.word(l, 1)?;
    if w.len() != 6 || !w.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(b.err(
            l,
            format!("a colour is six hex digits with no `#` (e8352b), not {w:?}"),
        ));
    }
    Ok(format!("#{}", w.to_lowercase()))
}

fn game(b: &Block) -> Result<GameDef, Error> {
    let mut g = GameDef {
        id: b.name.clone(),
        name: b.name.clone(),
        text: String::new(),
        assets: Vec::new(),
        credits: Vec::new(),
        field: 4,
        first: String::new(),
    };
    for l in &b.lines {
        match l.key() {
            "name" => g.name = l.rest(1),
            "text" => add_text(&mut g.text, l),
            "assets" => {
                let kind = b.word(l, 1)?.to_string();
                let dir = b.word(l, 2)?.trim_end_matches('/').to_string();
                if g.assets.iter().any(|(k, _)| *k == kind) {
                    return Err(b.err(l, format!("assets {kind} is given twice")));
                }
                g.assets.push((kind, dir));
            }
            "credit" => g.credits.push(l.rest(1)),
            "field" => g.field = b.num_in(l, 1, 1.0, 12.0)? as usize,
            "first" => g.first = b.word(l, 1)?.to_string(),
            _ => return Err(b.unknown(l)),
        }
    }
    Ok(g)
}

fn physics(p: &mut Physics, b: &Block) -> Result<(), Error> {
    for l in &b.lines {
        let pos = |i| b.num_in(l, i, 0.0, 1e6);
        match l.key() {
            "top" => p.top = pos(1)?,
            "accel" => p.accel = pos(1)?,
            "brake" => p.brake = pos(1)?,
            "coast" => p.coast = pos(1)?,
            "turn" => p.turn = pos(1)?,
            "grip" => p.grip = pos(1)?,
            "drift-grip" => p.drift_grip = pos(1)?,
            // drift-charge 0.9 1.9: seconds of drift for the first and second boost
            "drift-charge" => p.drift_charge = (pos(1)?, pos(2)?),
            "drift-boost" => p.drift_boost = (pos(1)?, pos(2)?),
            "boost-power" => p.boost_power = pos(1)?,
            "offroad" => p.offroad = b.num_in(l, 1, 0.05, 1.0)?,
            "corner" => p.corner = pos(1)?,
            "catchup" => p.catchup = b.num_in(l, 1, 0.0, 0.5)?,
            "basket-respawn" => p.basket_respawn = pos(1)?,
            "unpack" => p.unpack = pos(1)?,
            "countdown" => p.countdown = b.num_in(l, 1, 0.0, 10.0)?,
            "time-limit" => p.time_limit = b.num_in(l, 1, 10.0, 7200.0)?,
            "pad-boost" => p.pad_boost = pos(1)?,
            "radius" => p.radius = b.num_in(l, 1, 0.2, 5.0)?,
            _ => return Err(b.unknown(l)),
        }
    }
    if p.top <= 0.0 || p.accel <= 0.0 || p.turn <= 0.0 || p.grip <= 0.0 {
        return Err(b.here("physics: top, accel, turn and grip must be more than 0"));
    }
    if p.drift_charge.0 > p.drift_charge.1 {
        return Err(b.here("physics: drift-charge's first number is the shorter drift"));
    }
    Ok(())
}

/// A place: `120` (metres from the start line) or `Via Giuseppe Mazzini 0.5`.
pub fn place(b: &Block, l: &Line, from: usize) -> Result<Place, Error> {
    let words = &l.words[from.min(l.words.len())..];
    match words {
        [] => Err(b.err(
            l,
            format!(
                "{:?} wants a place: metres from the start, or a street and a fraction",
                l.words.join(" ")
            ),
        )),
        [m] => Ok(Place::Metres(b.num(l, from).map_err(|_| {
            b.err(
                l,
                format!(
                    "{m:?} is not a place: give metres, or a street and a fraction (Via Roma 0.5)"
                ),
            )
        })?)),
        [.., last] => {
            let f = last.parse::<f64>().ok().filter(|f| (0.0..=1.0).contains(f));
            match f {
                Some(f) => Ok(Place::In(words[..words.len() - 1].join(" "), f)),
                None => Err(b.err(
                    l,
                    format!(
                        "{:?}: a place on a street ends with a fraction from 0 to 1, not {last:?}",
                        words.join(" ")
                    ),
                )),
            }
        }
    }
}

fn track(defs: &Defs, b: &Block) -> Result<TrackDef, Error> {
    let mut t = TrackDef {
        id: b.name.clone(),
        file: b.file.clone(),
        no: b.no,
        name: b.name.clone(),
        text: String::new(),
        osm: String::new(),
        laps: 3,
        route: Vec::new(),
        start: Place::Metres(0.0),
        widen: 1.0,
        min_width: 7.0,
        verge: 5.0,
        scenery: 60.0,
        dress: Vec::new(),
        landmarks: Vec::new(),
        baskets_every: 0.0,
        baskets_first: 200.0,
        baskets_across: 4,
        pads: Vec::new(),
        pad_lines: Vec::new(),
        hazards: Vec::new(),
        look: Vec::new(),
    };
    for l in &b.lines {
        match l.key() {
            "name" => t.name = l.rest(1),
            "text" => add_text(&mut t.text, l),
            "osm" => t.osm = b.word(l, 1)?.to_string(),
            "laps" => t.laps = b.num_in(l, 1, 1.0, 99.0)? as u32,
            "route" => {
                let rest = &l.words[1..];
                if rest.is_empty() {
                    return Err(b.err(l, "route wants a street name or OSM way ids"));
                }
                if rest.iter().all(|w| w.chars().all(|c| c.is_ascii_digit())) {
                    t.route.push(Leg::Ways(
                        rest.iter().map(|w| w.parse::<u64>().unwrap_or(0)).collect(),
                    ));
                } else if rest[0] == "ways" {
                    let ids: Result<Vec<u64>, _> = rest[1..].iter().map(|w| w.parse()).collect();
                    match ids {
                        Ok(ids) if !ids.is_empty() => t.route.push(Leg::Ways(ids)),
                        _ => {
                            return Err(b.err(l, "route ways wants OSM way ids: route ways 123 456"))
                        }
                    }
                } else if rest[0] == "way" {
                    t.route.push(Leg::Way(way_span(b, l)?));
                } else {
                    t.route.push(Leg::Street(l.rest(1)));
                }
            }
            "start" => t.start = place(b, l, 1)?,
            "widen" => t.widen = b.num_in(l, 1, 0.2, 5.0)?,
            "min-width" => t.min_width = b.num_in(l, 1, 2.0, 60.0)?,
            "verge" => t.verge = b.num_in(l, 1, 0.0, 50.0)?,
            "scenery" => t.scenery = b.num_in(l, 1, 0.0, 500.0)?,
            // dress tree plane_tree.glb every 12 [along Viale ...] [side left] [offset 2]
            "dress" => {
                let kind = b.word(l, 1)?.to_string();
                let model = b.word(l, 2)?.to_string();
                if b.word(l, 3)? != "every" {
                    return Err(b.err(
                        l,
                        "dress: `dress <kind> <model> every <metres> [side left|right|both] [offset <m>] [scale <min> [max]] [face road|along|random] [along <street>]`",
                    ));
                }
                let every = b.num_in(l, 4, 1.0, 10000.0)?;
                let mut d = Dress {
                    kind,
                    model,
                    every,
                    along: None,
                    side: Side::Both,
                    offset: 1.5,
                    scale: (1.0, 1.0),
                    face: Face::Road,
                    line: l.no,
                };
                let mut i = 5;
                while i < l.words.len() {
                    match l.words[i].as_str() {
                        "side" => {
                            d.side = match b.word(l, i + 1)? {
                                "left" => Side::Left,
                                "right" => Side::Right,
                                "both" => Side::Both,
                                w => {
                                    return Err(
                                        b.err(l, format!("side is left, right or both, not {w:?}"))
                                    )
                                }
                            };
                            i += 2;
                        }
                        "offset" => {
                            d.offset = b.num_in(l, i + 1, -50.0, 50.0)?;
                            i += 2;
                        }
                        // scale 6.5 9: between the two; scale 8: just that
                        "scale" => {
                            let lo = b.num_in(l, i + 1, 0.001, 1000.0)?;
                            let hi = l
                                .words
                                .get(i + 2)
                                .and_then(|w| w.parse::<f64>().ok())
                                .filter(|v| *v >= lo);
                            d.scale = (lo, hi.unwrap_or(lo));
                            i += if hi.is_some() { 3 } else { 2 };
                        }
                        "face" => {
                            d.face = match b.word(l, i + 1)? {
                                "road" => Face::Road,
                                "along" => Face::Along,
                                "random" => Face::Random,
                                w => {
                                    return Err(b.err(
                                        l,
                                        format!("face is road, along or random, not {w:?}"),
                                    ))
                                }
                            };
                            i += 2;
                        }
                        // `along` takes the rest of the line: a street name.
                        "along" => {
                            let s = l.rest(i + 1);
                            if s.is_empty() {
                                return Err(b.err(l, "along wants a street name"));
                            }
                            d.along = Some(s);
                            break;
                        }
                        w => {
                            return Err(b.err(
                                l,
                                format!(
                                    "dress: unknown word {w:?} (side, offset, scale, face, along)"
                                ),
                            ))
                        }
                    }
                }
                t.dress.push(d);
            }
            // landmark gate Porta Maggiore [at 44.4900 11.3572]
            "landmark" => {
                let kind = b.word(l, 1)?.to_string();
                let words = &l.words[2..];
                let (name, at) = match words.iter().position(|w| w == "at") {
                    Some(i) => {
                        let lat = b.num(l, 2 + i + 1)?;
                        let lon = b.num(l, 2 + i + 2)?;
                        (words[..i].join(" "), Some((lat, lon)))
                    }
                    None => (words.join(" "), None),
                };
                t.landmarks.push(LandmarkSpec { kind, name, at });
            }
            // baskets every 420 [first 250] [across 4]
            "baskets" => {
                let mut i = 1;
                while i < l.words.len() {
                    match l.words[i].as_str() {
                        "every" => t.baskets_every = b.num_in(l, i + 1, 20.0, 1e5)?,
                        "first" => t.baskets_first = b.num_in(l, i + 1, 0.0, 1e5)?,
                        "across" => t.baskets_across = b.num_in(l, i + 1, 1.0, 8.0)? as usize,
                        w => {
                            return Err(b.err(
                                l,
                                format!("baskets: unknown word {w:?} (every, first, across)"),
                            ))
                        }
                    }
                    i += 2;
                }
            }
            "pad" => {
                t.pads.push(place(b, l, 1)?);
                t.pad_lines.push(l.no);
            }
            "hazard" => {
                let id = b.word(l, 1)?;
                if defs.hazard(id).is_none() {
                    return Err(b.err(
                        l,
                        format!("there is no hazard {id:?} (declare it before the track)"),
                    ));
                }
                t.hazards.push(id.to_string());
            }
            "look" => t
                .look
                .push((b.word(l, 1)?.to_string(), l.rest(2).to_string())),
            _ => return Err(b.unknown(l)),
        }
    }
    if t.osm.is_empty() {
        return Err(b.here(format!(
            "track {}: `osm <file>.track` says which track file to read",
            t.id
        )));
    }
    if t.route.is_empty() {
        return Err(b.here(format!("track {}: there is no `route`", t.id)));
    }
    Ok(t)
}

/// `route way <id> [forward|backward] [from <node>] [to <node>]`.
fn way_span(b: &Block, l: &Line) -> Result<WaySpan, Error> {
    let usage = "route way <id> [forward|backward] [from <node>] [to <node>]";
    let id = b
        .word(l, 2)?
        .parse::<u64>()
        .map_err(|_| b.err(l, format!("{usage}: the way id is a number")))?;
    let mut w = WaySpan {
        id,
        forward: None,
        from: None,
        to: None,
    };
    let mut i = 3;
    while i < l.words.len() {
        let node = |k: usize| -> Result<u64, Error> {
            b.word(l, k)?
                .parse::<u64>()
                .map_err(|_| b.err(l, format!("{usage}: a node id is a number")))
        };
        match l.words[i].as_str() {
            "forward" => w.forward = Some(true),
            "backward" => w.forward = Some(false),
            "from" => {
                w.from = Some(node(i + 1)?);
                i += 1;
            }
            "to" => {
                w.to = Some(node(i + 1)?);
                i += 1;
            }
            x => return Err(b.err(l, format!("{usage}: unknown word {x:?}"))),
        }
        i += 1;
    }
    Ok(w)
}

fn racer(b: &Block) -> Result<RacerDef, Error> {
    let mut r = RacerDef {
        id: b.name.clone(),
        name: b.name.clone(),
        text: String::new(),
        model: String::new(),
        kart: String::new(),
        colour: "#ffffff".into(),
        speed: 3.0,
        accel: 3.0,
        grip: 3.0,
        weight: 3.0,
        skill: 0.8,
        look: Vec::new(),
    };
    for l in &b.lines {
        let stat = || b.num_in(l, 1, 1.0, 5.0);
        match l.key() {
            "name" => r.name = l.rest(1),
            "text" => add_text(&mut r.text, l),
            "model" => r.model = b.word(l, 1)?.to_string(),
            "kart" => r.kart = b.word(l, 1)?.to_string(),
            "colour" | "color" => r.colour = colour(b, l)?,
            "speed" => r.speed = stat()?,
            "accel" => r.accel = stat()?,
            "grip" => r.grip = stat()?,
            "weight" => r.weight = stat()?,
            "skill" => r.skill = b.num_in(l, 1, 0.0, 1.0)?,
            "look" => r.look.push((b.word(l, 1)?.to_string(), l.rest(2))),
            _ => return Err(b.unknown(l)),
        }
    }
    if r.model.is_empty() || r.kart.is_empty() {
        return Err(b.here(format!(
            "racer {}: `model` (the driver) and `kart` are needed",
            r.id
        )));
    }
    Ok(r)
}

fn effect(b: &Block, l: &Line, i: usize) -> Result<Effect, Error> {
    Ok(match b.word(l, i)? {
        "splat" => Effect::Splat,
        "spin" => Effect::Spin,
        "bump" => Effect::Bump,
        "boost" => Effect::Boost,
        w => {
            return Err(b.err(
                l,
                format!("effect is splat, spin, bump or boost, not {w:?}"),
            ))
        }
    })
}

fn item(b: &Block) -> Result<ItemDef, Error> {
    let mut it = ItemDef {
        id: b.name.clone(),
        name: b.name.clone(),
        text: String::new(),
        kind: ItemKind::Throw,
        model: String::new(),
        effect: Effect::Bump,
        time: 1.0,
        power: 1.0,
        speed: 40.0,
        life: 3.0,
        aim: 0.6,
        radius: 1.6,
        odds: vec![1.0],
        hits: 1,
    };
    let mut kind_given = false;
    for l in &b.lines {
        match l.key() {
            "name" => it.name = l.rest(1),
            "text" => add_text(&mut it.text, l),
            "kind" => {
                kind_given = true;
                it.kind = match b.word(l, 1)? {
                    "throw" => ItemKind::Throw,
                    "drop" => ItemKind::Drop,
                    "boost" => ItemKind::Boost,
                    "roll" => ItemKind::Roll,
                    "aim" => ItemKind::Aim,
                    w => {
                        return Err(b.err(
                            l,
                            format!("kind is throw, drop, boost, roll or aim, not {w:?}"),
                        ))
                    }
                }
            }
            "model" => it.model = b.word(l, 1)?.to_string(),
            // effect spin 1.2
            "effect" => {
                it.effect = effect(b, l, 1)?;
                if l.words.len() > 2 {
                    it.time = b.num_in(l, 2, 0.0, 60.0)?;
                }
            }
            "power" => it.power = b.num_in(l, 1, 0.05, 5.0)?,
            "speed" => it.speed = b.num_in(l, 1, 0.0, 500.0)?,
            "life" => it.life = b.num_in(l, 1, 0.1, 600.0)?,
            "aim" => it.aim = b.num_in(l, 1, 0.0, 10.0)?,
            "radius" => it.radius = b.num_in(l, 1, 0.1, 20.0)?,
            "hits" => it.hits = b.num_in(l, 1, 1.0, 100.0)? as u32,
            "odds" => {
                let v = b.nums(l, 1)?;
                if v.is_empty() || v.iter().any(|x| *x < 0.0) {
                    return Err(b.err(l, "odds wants numbers of 0 or more, first place first"));
                }
                it.odds = v;
            }
            _ => return Err(b.unknown(l)),
        }
    }
    if !kind_given {
        return Err(b.here(format!(
            "item {}: `kind` (throw, drop, boost, roll or aim) is needed",
            it.id
        )));
    }
    if (it.kind == ItemKind::Boost) != (it.effect == Effect::Boost) {
        return Err(b.here(format!(
            "item {}: a `kind boost` item has `effect boost`, and only it does",
            it.id
        )));
    }
    Ok(it)
}

fn hazard(b: &Block) -> Result<HazardDef, Error> {
    let mut h = HazardDef {
        id: b.name.clone(),
        file: b.file.clone(),
        no: b.no,
        name: b.name.clone(),
        text: String::new(),
        model: String::new(),
        at: Place::Metres(0.0),
        every: 10.0,
        offset: 0.0,
        speed: 10.0,
        length: 5.0,
        width: 2.4,
        margin: 4.0,
        effect: Effect::Bump,
        time: 1.0,
    };
    let mut at = false;
    for l in &b.lines {
        match l.key() {
            "name" => h.name = l.rest(1),
            "text" => add_text(&mut h.text, l),
            "model" => h.model = b.word(l, 1)?.to_string(),
            "cross" => {
                h.at = place(b, l, 1)?;
                h.no = l.no;
                at = true;
            }
            // every 9 [offset 3]
            "every" => {
                h.every = b.num_in(l, 1, 0.5, 3600.0)?;
                if l.words.get(2).map(String::as_str) == Some("offset") {
                    h.offset = b.num_in(l, 3, 0.0, 3600.0)?;
                } else if l.words.len() > 2 {
                    return Err(b.err(l, "every <seconds> [offset <seconds>]"));
                }
            }
            "speed" => h.speed = b.num_in(l, 1, 0.1, 200.0)?,
            "size" => {
                h.length = b.num_in(l, 1, 0.1, 50.0)?;
                h.width = b.num_in(l, 2, 0.1, 50.0)?;
            }
            "margin" => h.margin = b.num_in(l, 1, 0.0, 100.0)?,
            "effect" => {
                h.effect = effect(b, l, 1)?;
                if l.words.len() > 2 {
                    h.time = b.num_in(l, 2, 0.0, 60.0)?;
                }
            }
            _ => return Err(b.unknown(l)),
        }
    }
    if !at {
        return Err(b.here(format!(
            "hazard {}: `cross <place>` says where it crosses the road",
            h.id
        )));
    }
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> Result<Defs, Error> {
        compile(&[("t.kart".into(), text.into())])
    }

    const MIN: &str = "game g\n    field 1\nend\ntrack t\n    osm t.track\n    route Via Roma\nend\nracer r\n    model a.glb\n    kart k.glb\nend\nitem i\n    kind drop\n    effect spin 1\nend\n";

    #[test]
    fn a_minimal_game_compiles() {
        let d = one(MIN).unwrap();
        assert_eq!(d.tracks[0].route, vec![Leg::Street("Via Roma".into())]);
        assert_eq!(d.game.first, "t");
    }

    #[test]
    fn errors_name_the_file_and_line() {
        let e = one("game g\n    nme x\nend\n").unwrap_err();
        assert_eq!(e.to_string(), "t.kart:2: game g: unknown line \"nme x\"");
        let e = one("game g\nracer r\nend\n").unwrap_err();
        assert!(e.to_string().contains("t.kart:2:"), "{e}");
        assert!(e.to_string().contains("has no `end`"), "{e}");
        let e = one(&MIN.replace("kind drop", "kind lob")).unwrap_err();
        assert!(e.msg.contains("not \"lob\""), "{e}");
        let e = one(&MIN.replace("route Via Roma", "route Via Roma\n    start Via Roma 7"))
            .unwrap_err();
        assert!(e.msg.contains("fraction"), "{e}");
    }

    #[test]
    fn places_and_routes() {
        let d = one(&MIN.replace(
            "route Via Roma",
            "route Via Roma\n    route 12 34\n    route ways 5\n    route way 7 backward from 1 to 2\n    start Via Roma 0.25\n    pad 300",
        ))
        .unwrap();
        let t = &d.tracks[0];
        assert_eq!(t.route[1], Leg::Ways(vec![12, 34]));
        assert_eq!(t.route[2], Leg::Ways(vec![5]));
        assert_eq!(
            t.route[3],
            Leg::Way(WaySpan {
                id: 7,
                forward: Some(false),
                from: Some(1),
                to: Some(2)
            })
        );
        assert_eq!(t.start, Place::In("Via Roma".into(), 0.25));
        assert_eq!(t.pads, vec![Place::Metres(300.0)]);
    }
}
