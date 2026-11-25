//! A track file (`*.track`): a closed road built from OpenStreetMap by `tools/osm2track`, in
//! the same block form as the game files. Coordinates are metres around the track's origin:
//! `x` east, `z` south (three.js's ground plane). The engine reads it; osm2track writes it
//! (both through this module, so the two always agree).
//!
//! ```text
//! track sorsola                 # where it came from: origin (lat lon of 0 0), spacing, licence
//! section Via Giuseppe Mazzini  # a stretch of the route: from, to (metres), highway, ways
//! samples                       # the centreline, one `p` every `spacing` metres:
//!     p x z width left right line    # road width, walls left and right of the centre, the
//! end                                # racing line (metres left of the centre)
//! checkpoints                   # metres from the start line
//! grid                          # start slots: metres from the line (behind it), metres left
//! minimap                       # a light copy of the centreline, for the map in the corner
//! building 148503172            # a footprint near the road: kind, name, height, levels,
//!                               # `foot x z ...`, `portico` (edges facing the track)
//! landmark gate Porta Maggiore  # at, dir, size
//! area hospital Policlinico ... # ground to draw: foot
//! props                         # the scenery, baked from `dress` lines:
//!     prop tree tree_oak.glb x z angle scale
//! end
//! ```
//!
//! The data is © OpenStreetMap contributors and the file is under the Open Database Licence.

use serde::Serialize;

use crate::script::{blocks, Block, Error, Line, Place};

#[derive(Clone, Debug, Serialize, Default)]
pub struct Sample {
    pub x: f64,
    pub z: f64,
    /// Road width.
    pub w: f64,
    /// Distance from the centre to the wall on the left, and on the right.
    pub left: f64,
    pub right: f64,
    /// The racing line, metres left of the centre.
    pub line: f64,
    /// Metres from the start line.
    pub s: f64,
    /// Unit tangent (the direction of the race); its left normal is `(tz, -tx)`.
    pub tx: f64,
    pub tz: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Section {
    pub name: String,
    pub from: f64,
    pub to: f64,
    pub highway: String,
    pub ways: Vec<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Building {
    pub id: u64,
    pub kind: String,
    pub name: String,
    pub height: f64,
    pub levels: u32,
    /// Counter-clockwise seen from above (x east, z south), not closed.
    pub foot: Vec<[f64; 2]>,
    /// Edges (from point i to i+1) that face the track and have a portico.
    pub portico: Vec<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Landmark {
    pub kind: String,
    pub name: String,
    pub x: f64,
    pub z: f64,
    /// Which way it faces (a unit vector): a gate's passage runs this way.
    pub dx: f64,
    pub dz: f64,
    /// Width (across `dir`), depth (along it), height.
    pub size: [f64; 3],
}

/// A piece of scenery, baked: a model from the game's `props` folder, where it stands, which
/// way it faces (radians about the vertical; 0 faces +z, south) and its scale.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Prop {
    pub kind: String,
    pub model: String,
    pub x: f64,
    pub z: f64,
    pub angle: f64,
    pub scale: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Area {
    pub kind: String,
    pub name: String,
    pub foot: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub attribution: String,
    pub licence: String,
    /// Latitude and longitude of (0, 0).
    pub origin: (f64, f64),
    pub spacing: f64,
    pub length: f64,
    pub samples: Vec<Sample>,
    pub sections: Vec<Section>,
    pub checkpoints: Vec<f64>,
    /// (metres behind the line, metres left of the centre)
    pub grid: Vec<(f64, f64)>,
    pub minimap: Vec<[f64; 2]>,
    pub buildings: Vec<Building>,
    pub landmarks: Vec<Landmark>,
    pub areas: Vec<Area>,
    /// The scenery, baked by osm2track from the `dress` lines (sent to clients as the
    /// course's `props`).
    #[serde(skip)]
    pub props: Vec<Prop>,
    /// Free comment lines written at the top of the file.
    #[serde(skip)]
    pub notes: Vec<String>,
}

/// Where something is on the track.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Loc {
    /// The segment from sample `i` to `i + 1`.
    pub i: usize,
    /// Metres from the start line, and left of the centre.
    pub s: f64,
    pub d: f64,
}

fn r2(v: f64) -> String {
    let v = (v * 100.0).round() / 100.0;
    if v == 0.0 {
        "0".into()
    } else {
        format!("{v}")
    }
}

impl Track {
    // ---------- reading and writing ----------

    /// Reads a track file.
    pub fn parse(file: &str, text: &str) -> Result<Track, Error> {
        let mut t = Track::default();
        let mut seen_track = false;
        for b in blocks(file, text)? {
            match b.kind.as_str() {
                "track" => {
                    seen_track = true;
                    t.id = b.name.clone();
                    for l in &b.lines {
                        match l.key() {
                            "name" => t.name = l.rest(1),
                            "attribution" => t.attribution = l.rest(1),
                            "licence" => t.licence = l.rest(1),
                            "origin" => t.origin = (b.num(l, 1)?, b.num(l, 2)?),
                            "spacing" => t.spacing = b.num_in(l, 1, 0.1, 100.0)?,
                            "length" | "source" | "route" | "built" => {}
                            _ => return Err(b.unknown(l)),
                        }
                    }
                }
                "section" => {
                    let mut s = Section {
                        name: b.name.clone(),
                        from: 0.0,
                        to: 0.0,
                        highway: String::new(),
                        ways: Vec::new(),
                    };
                    for l in &b.lines {
                        match l.key() {
                            "from" => {
                                s.from = b.num(l, 1)?;
                                if b.word(l, 2)? != "to" {
                                    return Err(b.err(l, "from <m> to <m>"));
                                }
                                s.to = b.num(l, 3)?;
                            }
                            "highway" => s.highway = b.word(l, 1)?.to_string(),
                            "ways" => {
                                for k in 1..l.words.len() {
                                    s.ways.push(b.num(l, k)? as u64);
                                }
                            }
                            _ => return Err(b.unknown(l)),
                        }
                    }
                    t.sections.push(s);
                }
                "samples" => {
                    for l in &b.lines {
                        if l.key() != "p" || l.words.len() != 7 {
                            return Err(b.err(l, "a sample is `p x z width left right line`"));
                        }
                        let v = b.nums(l, 1)?;
                        t.samples.push(Sample {
                            x: v[0],
                            z: v[1],
                            w: v[2],
                            left: v[3],
                            right: v[4],
                            line: v[5],
                            ..Sample::default()
                        });
                    }
                }
                "checkpoints" => {
                    for l in &b.lines {
                        if l.key() != "at" {
                            return Err(b.unknown(l));
                        }
                        t.checkpoints.extend(b.nums(l, 1)?);
                    }
                }
                "grid" => {
                    for l in &b.lines {
                        if l.key() != "slot" || l.words.len() != 3 {
                            return Err(b.err(l, "a grid slot is `slot <metres behind> <metres left>`"));
                        }
                        t.grid.push((b.num(l, 1)?, b.num(l, 2)?));
                    }
                }
                "minimap" => {
                    for l in &b.lines {
                        if l.key() != "at" {
                            return Err(b.unknown(l));
                        }
                        t.minimap.extend(pairs(&b, l)?);
                    }
                }
                "building" => t.buildings.push(building(&b)?),
                "props" => {
                    for l in &b.lines {
                        if l.key() != "prop" || l.words.len() != 7 {
                            return Err(b.err(l, "a prop is `prop <kind> <model> x z angle scale`"));
                        }
                        t.props.push(Prop {
                            kind: l.words[1].clone(),
                            model: l.words[2].clone(),
                            x: b.num(l, 3)?,
                            z: b.num(l, 4)?,
                            angle: b.num(l, 5)?,
                            scale: b.num(l, 6)?,
                        });
                    }
                }
                "landmark" => t.landmarks.push(landmark(&b)?),
                "area" => {
                    let (kind, name) = split_kind(&b);
                    let mut foot = Vec::new();
                    for l in &b.lines {
                        match l.key() {
                            "foot" => foot.extend(pairs(&b, l)?),
                            _ => return Err(b.unknown(l)),
                        }
                    }
                    t.areas.push(Area { kind, name, foot });
                }
                k => {
                    return Err(b.here(format!(
                        "unknown block {k:?} in a track file (track, section, samples, checkpoints, grid, minimap, building, landmark, area, props)"
                    )))
                }
            }
        }
        if !seen_track {
            return Err(Error::new(file, 0, "there is no `track` block"));
        }
        if t.samples.len() < 8 {
            return Err(Error::new(file, 0, "the track has fewer than 8 samples"));
        }
        if t.spacing <= 0.0 {
            return Err(Error::new(file, 0, "the track has no `spacing`"));
        }
        t.finish();
        Ok(t)
    }

    /// Fills in what is derived: distances and tangents.
    pub fn finish(&mut self) {
        let n = self.samples.len();
        let mut s = 0.0;
        for i in 0..n {
            self.samples[i].s = s;
            let j = (i + 1) % n;
            let (dx, dz) = (
                self.samples[j].x - self.samples[i].x,
                self.samples[j].z - self.samples[i].z,
            );
            s += (dx * dx + dz * dz).sqrt();
        }
        self.length = s;
        for i in 0..n {
            let a = &self.samples[(i + n - 1) % n];
            let b = &self.samples[(i + 1) % n];
            let (dx, dz) = (b.x - a.x, b.z - a.z);
            let l = (dx * dx + dz * dz).sqrt().max(1e-9);
            self.samples[i].tx = dx / l;
            self.samples[i].tz = dz / l;
        }
    }

    /// Writes the track file.
    pub fn to_text(&self) -> String {
        let mut o = String::new();
        for n in &self.notes {
            o.push_str(&format!("# {n}\n"));
        }
        o.push_str(&format!("track {}\n", self.id));
        o.push_str(&format!("    name {}\n", self.name));
        o.push_str(&format!("    attribution {}\n", self.attribution));
        o.push_str(&format!("    licence {}\n", self.licence));
        o.push_str(&format!(
            "    origin {:.7} {:.7}\n",
            self.origin.0, self.origin.1
        ));
        o.push_str(&format!("    spacing {}\n", r2(self.spacing)));
        o.push_str(&format!("    length {}\n", r2(self.length)));
        o.push_str("end\n\n");
        for s in &self.sections {
            o.push_str(&format!("section {}\n", s.name));
            o.push_str(&format!("    from {} to {}\n", r2(s.from), r2(s.to)));
            o.push_str(&format!("    highway {}\n", s.highway));
            let ways: Vec<String> = s.ways.iter().map(|w| w.to_string()).collect();
            o.push_str(&format!("    ways {}\n", ways.join(" ")));
            o.push_str("end\n");
        }
        o.push_str("\nsamples\n    # x z width left right line\n");
        for p in &self.samples {
            o.push_str(&format!(
                "    p {} {} {} {} {} {}\n",
                r2(p.x),
                r2(p.z),
                r2(p.w),
                r2(p.left),
                r2(p.right),
                r2(p.line)
            ));
        }
        o.push_str("end\n\ncheckpoints\n");
        let cps: Vec<String> = self.checkpoints.iter().map(|c| r2(*c)).collect();
        o.push_str(&format!("    at {}\nend\n\ngrid\n", cps.join(" ")));
        for (s, d) in &self.grid {
            o.push_str(&format!("    slot {} {}\n", r2(*s), r2(*d)));
        }
        o.push_str("end\n\nminimap\n");
        for c in self.minimap.chunks(8) {
            let v: Vec<String> = c
                .iter()
                .map(|p| format!("{} {}", r2(p[0]), r2(p[1])))
                .collect();
            o.push_str(&format!("    at {}\n", v.join(" ")));
        }
        o.push_str("end\n");
        for a in &self.areas {
            o.push_str(&format!("\narea {} {}\n", a.kind, a.name));
            for c in a.foot.chunks(8) {
                let v: Vec<String> = c
                    .iter()
                    .map(|p| format!("{} {}", r2(p[0]), r2(p[1])))
                    .collect();
                o.push_str(&format!("    foot {}\n", v.join(" ")));
            }
            o.push_str("end\n");
        }
        for l in &self.landmarks {
            o.push_str(&format!("\nlandmark {} {}\n", l.kind, l.name));
            o.push_str(&format!("    at {} {}\n", r2(l.x), r2(l.z)));
            o.push_str(&format!("    dir {:.4} {:.4}\n", l.dx, l.dz));
            o.push_str(&format!(
                "    size {} {} {}\n",
                r2(l.size[0]),
                r2(l.size[1]),
                r2(l.size[2])
            ));
            o.push_str("end\n");
        }
        if !self.props.is_empty() {
            o.push_str("\nprops\n    # kind model x z angle scale\n");
            for p in &self.props {
                o.push_str(&format!(
                    "    prop {} {} {} {} {:.3} {:.3}\n",
                    p.kind,
                    p.model,
                    r2(p.x),
                    r2(p.z),
                    p.angle,
                    p.scale
                ));
            }
            o.push_str("end\n");
        }
        o.push('\n');
        for b in &self.buildings {
            o.push_str(&format!("building {}\n", b.id));
            o.push_str(&format!("    kind {}\n", b.kind));
            if !b.name.is_empty() {
                o.push_str(&format!("    name {}\n", b.name));
            }
            o.push_str(&format!(
                "    height {}\n    levels {}\n",
                r2(b.height),
                b.levels
            ));
            let v: Vec<String> = b
                .foot
                .iter()
                .map(|p| format!("{} {}", r2(p[0]), r2(p[1])))
                .collect();
            o.push_str(&format!("    foot {}\n", v.join(" ")));
            if !b.portico.is_empty() {
                let v: Vec<String> = b.portico.iter().map(|p| p.to_string()).collect();
                o.push_str(&format!("    portico {}\n", v.join(" ")));
            }
            o.push_str("end\n");
        }
        o
    }

    // ---------- geometry ----------

    pub fn n(&self) -> usize {
        self.samples.len()
    }

    /// `s` brought into `0..length`.
    pub fn wrap(&self, s: f64) -> f64 {
        let l = self.length;
        let mut s = s % l;
        if s < 0.0 {
            s += l;
        }
        s
    }

    /// The segment that holds `s`, and how far along it (0..1).
    pub fn seg(&self, s: f64) -> (usize, f64) {
        let s = self.wrap(s);
        let n = self.n();
        // Samples are evenly spaced, so start from the estimate and walk.
        let mut i = ((s / self.length) * n as f64) as usize % n;
        for _ in 0..n {
            let a = self.samples[i].s;
            let b = if i + 1 == n {
                self.length
            } else {
                self.samples[i + 1].s
            };
            if s < a {
                i = (i + n - 1) % n;
            } else if s >= b {
                i = (i + 1) % n;
            } else {
                return (i, ((s - a) / (b - a).max(1e-9)).clamp(0.0, 1.0));
            }
        }
        (0, 0.0)
    }

    /// Point, tangent, road width, walls and racing line at `s`, interpolated.
    pub fn frame(&self, s: f64) -> Frame {
        let (i, t) = self.seg(s);
        let a = &self.samples[i];
        let b = &self.samples[(i + 1) % self.n()];
        let lerp = |x: f64, y: f64| x + (y - x) * t;
        let (tx, tz) = norm(lerp(a.tx, b.tx), lerp(a.tz, b.tz));
        Frame {
            x: lerp(a.x, b.x),
            z: lerp(a.z, b.z),
            tx,
            tz,
            w: lerp(a.w, b.w),
            left: lerp(a.left, b.left),
            right: lerp(a.right, b.right),
            line: lerp(a.line, b.line),
        }
    }

    /// The point `d` metres left of the centre at `s`.
    pub fn point(&self, s: f64, d: f64) -> (f64, f64) {
        let f = self.frame(s);
        (f.x + f.tz * d, f.z - f.tx * d)
    }

    /// Where a point is, searching the segments around `hint` (or all of them for `None`).
    pub fn locate(&self, x: f64, z: f64, hint: Option<usize>) -> Loc {
        let n = self.n();
        let (from, count) = match hint {
            Some(h) => ((h + n - 10) % n, 21.min(n)),
            None => (0, n),
        };
        let mut best = (f64::MAX, Loc::default());
        for k in 0..count {
            let i = (from + k) % n;
            let a = &self.samples[i];
            let b = &self.samples[(i + 1) % n];
            let (ex, ez) = (b.x - a.x, b.z - a.z);
            let len2 = (ex * ex + ez * ez).max(1e-9);
            let t = (((x - a.x) * ex + (z - a.z) * ez) / len2).clamp(0.0, 1.0);
            let (px, pz) = (a.x + ex * t, a.z + ez * t);
            let d2 = (x - px) * (x - px) + (z - pz) * (z - pz);
            if d2 < best.0 {
                let len = len2.sqrt();
                // Left of the segment is (ez, -ex).
                let d = ((x - a.x) * ez - (z - a.z) * ex) / len;
                let seg_end = if i + 1 == n { self.length } else { b.s };
                best = (
                    d2,
                    Loc {
                        i,
                        s: a.s + (seg_end - a.s) * t,
                        d,
                    },
                );
            }
        }
        best.1
    }

    /// The stretch `[from, to)` of a section, by name.
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// Metres from the start line of a place.
    pub fn place(&self, p: &Place) -> Result<f64, String> {
        match p {
            Place::Metres(m) => Ok(self.wrap(*m)),
            Place::In(name, f) => {
                // A street the route takes more than once: the fraction runs over all of it.
                let parts: Vec<&Section> =
                    self.sections.iter().filter(|s| s.name == *name).collect();
                if parts.is_empty() {
                    let names: Vec<&str> = self.sections.iter().map(|s| s.name.as_str()).collect();
                    return Err(format!(
                        "the track has no section {name:?} (it has {})",
                        names.join(", ")
                    ));
                }
                let total: f64 = parts.iter().map(|s| s.to - s.from).sum();
                let mut left = total * f;
                for s in &parts {
                    let len = s.to - s.from;
                    if left <= len {
                        return Ok(self.wrap(s.from + left));
                    }
                    left -= len;
                }
                Ok(self.wrap(parts[parts.len() - 1].to))
            }
        }
    }

    /// The section at `s`.
    pub fn section_at(&self, s: f64) -> Option<&Section> {
        let s = self.wrap(s);
        self.sections.iter().find(|x| s >= x.from && s < x.to)
    }

    /// Signed curvature (1/m, left turns positive) of the racing line around each sample.
    pub fn line_curvature(&self) -> Vec<f64> {
        let n = self.n();
        let pts: Vec<(f64, f64)> = self
            .samples
            .iter()
            .map(|p| (p.x + p.tz * p.line, p.z - p.tx * p.line))
            .collect();
        let k = 4;
        (0..n)
            .map(|i| {
                let a = pts[(i + n - k) % n];
                let b = pts[i];
                let c = pts[(i + k) % n];
                curvature(a, b, c)
            })
            .collect()
    }
}

/// The track at one place.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub x: f64,
    pub z: f64,
    pub tx: f64,
    pub tz: f64,
    pub w: f64,
    pub left: f64,
    pub right: f64,
    pub line: f64,
}

pub fn norm(x: f64, z: f64) -> (f64, f64) {
    let l = (x * x + z * z).sqrt();
    if l < 1e-12 {
        (1.0, 0.0)
    } else {
        (x / l, z / l)
    }
}

/// Curvature of the circle through three points; positive when the turn is to the left
/// (x east, z south: left of a heading (tx, tz) is (tz, -tx)).
pub fn curvature(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    let (abx, abz) = (b.0 - a.0, b.1 - a.1);
    let (bcx, bcz) = (c.0 - b.0, c.1 - b.1);
    let (acx, acz) = (c.0 - a.0, c.1 - a.1);
    // Turning left (towards (tz, -tx)) is a negative cross product in x-z.
    let cross = abx * bcz - abz * bcx;
    let den = ((abx * abx + abz * abz) * (bcx * bcx + bcz * bcz) * (acx * acx + acz * acz)).sqrt();
    if den < 1e-12 {
        0.0
    } else {
        -2.0 * cross / den
    }
}

fn split_kind(b: &Block) -> (String, String) {
    let mut it = b.name.splitn(2, ' ');
    let kind = it.next().unwrap_or("").to_string();
    let name = it.next().unwrap_or("").to_string();
    (kind, name)
}

fn pairs(b: &Block, l: &Line) -> Result<Vec<[f64; 2]>, Error> {
    let v = b.nums(l, 1)?;
    if v.len() % 2 != 0 {
        return Err(b.err(l, "points come in pairs: x z x z ..."));
    }
    Ok(v.chunks(2).map(|c| [c[0], c[1]]).collect())
}

fn building(b: &Block) -> Result<Building, Error> {
    let mut out = Building {
        id: b.name.parse().map_err(|_| {
            b.here(format!(
                "a building is named by its OSM id, not {:?}",
                b.name
            ))
        })?,
        kind: "house".into(),
        name: String::new(),
        height: 12.0,
        levels: 4,
        foot: Vec::new(),
        portico: Vec::new(),
    };
    for l in &b.lines {
        match l.key() {
            "kind" => out.kind = b.word(l, 1)?.to_string(),
            "name" => out.name = l.rest(1),
            "height" => out.height = b.num_in(l, 1, 0.5, 500.0)?,
            "levels" => out.levels = b.num_in(l, 1, 0.0, 200.0)? as u32,
            "foot" => out.foot.extend(pairs(b, l)?),
            "portico" => {
                for k in 1..l.words.len() {
                    out.portico.push(b.num(l, k)? as usize);
                }
            }
            _ => return Err(b.unknown(l)),
        }
    }
    if out.foot.len() < 3 {
        return Err(b.here(format!(
            "building {}: a footprint has at least 3 points",
            out.id
        )));
    }
    if let Some(p) = out.portico.iter().find(|p| **p >= out.foot.len()) {
        return Err(b.here(format!("building {}: there is no edge {p}", out.id)));
    }
    Ok(out)
}

fn landmark(b: &Block) -> Result<Landmark, Error> {
    let (kind, name) = split_kind(b);
    let mut out = Landmark {
        kind,
        name,
        x: 0.0,
        z: 0.0,
        dx: 1.0,
        dz: 0.0,
        size: [10.0, 10.0, 10.0],
    };
    for l in &b.lines {
        match l.key() {
            "at" => {
                out.x = b.num(l, 1)?;
                out.z = b.num(l, 2)?;
            }
            "dir" => {
                let (x, z) = norm(b.num(l, 1)?, b.num(l, 2)?);
                out.dx = x;
                out.dz = z;
            }
            "size" => out.size = [b.num(l, 1)?, b.num(l, 2)?, b.num(l, 3)?],
            _ => return Err(b.unknown(l)),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 100 x 50 rectangle, driven anticlockwise on the map (x east, z south).
    pub fn square() -> Track {
        let mut t = Track {
            id: "sq".into(),
            spacing: 2.0,
            ..Track::default()
        };
        let corners = [(0.0, 0.0), (100.0, 0.0), (100.0, -50.0), (0.0, -50.0)];
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            let len: f64 = ((b.0 - a.0) as f64).hypot(b.1 - a.1);
            let steps = (len / 2.0) as usize;
            for j in 0..steps {
                let f = j as f64 / steps as f64;
                t.samples.push(Sample {
                    x: a.0 + (b.0 - a.0) * f,
                    z: a.1 + (b.1 - a.1) * f,
                    w: 10.0,
                    left: 8.0,
                    right: 8.0,
                    ..Sample::default()
                });
            }
        }
        t.sections.push(Section {
            name: "Bottom".into(),
            from: 0.0,
            to: 100.0,
            highway: "primary".into(),
            ways: vec![1],
        });
        t.finish();
        t
    }

    #[test]
    fn locate_and_frame_agree() {
        let t = square();
        assert!((t.length - 300.0).abs() < 1e-9);
        // Heading east along z = 0, left is north, which is -z.
        let l = t.locate(30.0, -3.0, None);
        assert!(
            (l.s - 30.0).abs() < 1e-9 && (l.d - 3.0).abs() < 1e-9,
            "{l:?}"
        );
        let (x, z) = t.point(30.0, 3.0);
        assert!((x - 30.0).abs() < 1e-9 && (z + 3.0).abs() < 1e-9);
        assert_eq!(t.place(&Place::In("Bottom".into(), 0.5)).unwrap(), 50.0);
        assert!(t.place(&Place::In("Top".into(), 0.5)).is_err());
        // The square turns left at every corner.
        let c = curvature((98.0, 0.0), (100.0, 0.0), (100.0, -2.0));
        assert!(c > 0.0);
    }

    #[test]
    fn the_text_form_reads_back() {
        let mut t = square();
        t.buildings.push(Building {
            id: 7,
            kind: "house".into(),
            name: "Casa".into(),
            height: 12.5,
            levels: 4,
            foot: vec![[0.0, 10.0], [10.0, 10.0], [10.0, 20.0]],
            portico: vec![0],
        });
        t.grid.push((6.0, 2.5));
        t.checkpoints = vec![0.0, 150.0];
        let back = Track::parse("sq.track", &t.to_text()).unwrap();
        assert_eq!(back.samples.len(), t.samples.len());
        assert_eq!(back.buildings[0].portico, vec![0]);
        assert_eq!(back.grid, vec![(6.0, 2.5)]);
        assert!((back.length - t.length).abs() < 1e-6);
    }
}
