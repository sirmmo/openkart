//! osm2track: builds an OpenKart track file from OpenStreetMap data and a route.
//!
//! ```sh
//! cargo run -p osm2track -- games/pachino-gp osm/sorsola.json [more.json ...] [--track sorsola]
//! ```
//!
//! It reads the game's `*.kart` files, and for each `track` block (or the one named) joins the
//! `route` legs into one closed loop through the street graph of the extract (Overpass JSON with
//! `out geom`), rounds the corners, takes the road's width from its lanes or class, finds the
//! walls (buildings, and other stretches of the track running close by), a racing line, the
//! checkpoints, the grid, the minimap, the buildings near the road (with porticoes where they
//! face it), the landmarks and the hospital grounds, and writes the block's `osm` file next to
//! the game files.
//!
//! Map data © OpenStreetMap contributors (ODbL 1.0): the track file it writes is a Derived
//! Database under the same licence.

use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::path::{Path, PathBuf};

use openkart::script::{self, LandmarkSpec, Leg, TrackDef};
use openkart::track::{Area, Building, Landmark, Sample, Section, Track};
use serde_json::Value;

const DRIVABLE: [&str; 16] = [
    "motorway",
    "trunk",
    "primary",
    "secondary",
    "tertiary",
    "motorway_link",
    "trunk_link",
    "primary_link",
    "secondary_link",
    "tertiary_link",
    "unclassified",
    "residential",
    "living_street",
    "service",
    "pedestrian",
    "road",
];

/// Metres per degree of latitude.
const M: f64 = 111_320.0;

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
            "{}: {} m, {} samples, {} sections, {} buildings ({} with porticoes), {} landmarks",
            out.display(),
            back.length.round(),
            back.samples.len(),
            back.sections.len(),
            back.buildings.len(),
            back.buildings
                .iter()
                .filter(|b| !b.portico.is_empty())
                .count(),
            back.landmarks.len()
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

// ---------- the map ----------

#[derive(Clone, Debug)]
struct Way {
    id: u64,
    nodes: Vec<u64>,
    geom: Vec<(f64, f64)>,
    tags: BTreeMap<String, String>,
}

impl Way {
    fn tag(&self, k: &str) -> Option<&str> {
        self.tags.get(k).map(String::as_str)
    }
}

/// A relation known only by its bounds, or a tagged node (an outline of one point).
#[derive(Clone, Debug)]
struct Thing {
    outline: Vec<(f64, f64)>,
    tags: BTreeMap<String, String>,
}

struct Osm {
    ways: Vec<Way>,
    things: Vec<Thing>,
    pos: BTreeMap<u64, (f64, f64)>,
}

fn tags(v: &Value) -> BTreeMap<String, String> {
    v["tags"]
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default()
}

impl Osm {
    fn read(paths: &[PathBuf]) -> Result<Osm, String> {
        let mut ways = BTreeMap::new();
        let mut things = Vec::new();
        let mut pos = BTreeMap::new();
        for p in paths {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let v: Value = serde_json::from_str(&text)
                .map_err(|e| format!("{}: not Overpass JSON: {e}", p.display()))?;
            let els = v["elements"]
                .as_array()
                .ok_or_else(|| format!("{}: there is no `elements` list", p.display()))?;
            for e in els {
                let id = e["id"].as_u64().unwrap_or(0);
                match e["type"].as_str() {
                    Some("way") => {
                        let nodes: Vec<u64> = e["nodes"]
                            .as_array()
                            .map(|a| a.iter().filter_map(|n| n.as_u64()).collect())
                            .unwrap_or_default();
                        let geom: Vec<(f64, f64)> = e["geometry"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .map(|g| {
                                        (
                                            g["lat"].as_f64().unwrap_or(0.0),
                                            g["lon"].as_f64().unwrap_or(0.0),
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if geom.len() != nodes.len() || geom.is_empty() {
                            continue;
                        }
                        for (n, g) in nodes.iter().zip(&geom) {
                            pos.insert(*n, *g);
                        }
                        ways.insert(
                            id,
                            Way {
                                id,
                                nodes,
                                geom,
                                tags: tags(e),
                            },
                        );
                    }
                    Some("relation") => {
                        let b = &e["bounds"];
                        if let (Some(a), Some(o), Some(c), Some(d)) = (
                            b["minlat"].as_f64(),
                            b["minlon"].as_f64(),
                            b["maxlat"].as_f64(),
                            b["maxlon"].as_f64(),
                        ) {
                            things.push(Thing {
                                outline: vec![(a, o), (a, d), (c, d), (c, o)],
                                tags: tags(e),
                            });
                        }
                    }
                    Some("node") => {
                        if let (Some(a), Some(o)) = (e["lat"].as_f64(), e["lon"].as_f64()) {
                            if e.get("tags").is_some() {
                                things.push(Thing {
                                    outline: vec![(a, o)],
                                    tags: tags(e),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(Osm {
            ways: ways.into_values().collect(),
            things,
            pos,
        })
    }

    fn drivable(&self) -> impl Iterator<Item = &Way> {
        self.ways
            .iter()
            .filter(|w| w.tag("highway").is_some_and(|h| DRIVABLE.contains(&h)))
    }
}

/// Local metres: x east, z south, around an origin (equirectangular).
#[derive(Clone, Copy)]
struct Proj {
    lat0: f64,
    lon0: f64,
    k: f64,
}

impl Proj {
    fn new(lat0: f64, lon0: f64) -> Proj {
        Proj {
            lat0,
            lon0,
            k: lat0.to_radians().cos(),
        }
    }
    fn xz(&self, (lat, lon): (f64, f64)) -> (f64, f64) {
        ((lon - self.lon0) * M * self.k, -(lat - self.lat0) * M)
    }
}

// ---------- the route ----------

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct St {
    leg: usize,
    node: u64,
    /// Whether the loop has moved along this leg yet (a leg may not be skipped).
    moved: bool,
}

#[derive(PartialEq)]
struct Q(f64, St);
impl Eq for Q {}
impl PartialOrd for Q {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Q {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
    }
}

/// One step of the loop: from a node, along a way.
#[derive(Clone, Debug)]
struct Step {
    from: u64,
    way: usize,
    leg: usize,
}

fn leg_name(l: &Leg) -> String {
    match l {
        Leg::Street(s) => s.clone(),
        Leg::Ways(w) => format!(
            "ways {}",
            w.iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

/// The way indices of each leg.
fn leg_ways(osm: &Osm, t: &TrackDef) -> Result<Vec<Vec<usize>>, String> {
    let mut out = Vec::new();
    for leg in &t.route {
        let v: Vec<usize> = match leg {
            Leg::Street(name) => osm
                .ways
                .iter()
                .enumerate()
                .filter(|(_, w)| {
                    w.tag("name") == Some(name.as_str())
                        && w.tag("highway").is_some_and(|h| DRIVABLE.contains(&h))
                })
                .map(|(i, _)| i)
                .collect(),
            Leg::Ways(ids) => {
                let mut v = Vec::new();
                for id in ids {
                    match osm.ways.iter().position(|w| w.id == *id) {
                        Some(i) => v.push(i),
                        None => {
                            return Err(format!(
                                "track {}: route: way {id} is not in the extract",
                                t.id
                            ))
                        }
                    }
                }
                v
            }
        };
        if v.is_empty() {
            let Leg::Street(name) = leg else {
                unreachable!()
            };
            let last = name.split_whitespace().last().unwrap_or("");
            let mut like: BTreeSet<&str> = BTreeSet::new();
            for w in osm.drivable() {
                if let Some(n) = w.tag("name") {
                    if n.contains(last) {
                        like.insert(n);
                    }
                }
            }
            return Err(format!(
                "track {}: route {name:?}: no road in the extract has that name{}",
                t.id,
                if like.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (did you mean {}?)",
                        like.into_iter().take(5).collect::<Vec<_>>().join(", ")
                    )
                }
            ));
        }
        out.push(v);
    }
    Ok(out)
}

/// Joins the legs into the cheapest closed loop: leg after leg, each on its own ways,
/// switching to the next leg where they meet. Driving against a one-way street costs four
/// times as much, so dual carriageways are taken the right way round.
fn route(osm: &Osm, t: &TrackDef, proj: &Proj) -> Result<Vec<Step>, String> {
    // The search runs on sub-legs: a street is one, and a list of ways is one per way, in
    // order, so the loop follows the listed ways one after the other.
    let mut legs: Vec<Vec<usize>> = Vec::new();
    let mut parent: Vec<usize> = Vec::new();
    let mut label: Vec<String> = Vec::new();
    for (i, ws) in leg_ways(osm, t)?.into_iter().enumerate() {
        match &t.route[i] {
            Leg::Street(name) => {
                legs.push(ws);
                parent.push(i);
                label.push(name.clone());
            }
            Leg::Ways(_) => {
                for w in ws {
                    legs.push(vec![w]);
                    parent.push(i);
                    label.push(format!("way {}", osm.ways[w].id));
                }
            }
        }
    }
    let k = legs.len();
    if k < 2 {
        return Err(format!(
            "track {}: a route needs at least two legs that close a loop",
            t.id
        ));
    }
    let node_sets: Vec<BTreeSet<u64>> = legs
        .iter()
        .map(|ws| {
            ws.iter()
                .flat_map(|&w| osm.ways[w].nodes.iter().copied())
                .collect()
        })
        .collect();
    for i in 0..k {
        let j = (i + 1) % k;
        if node_sets[i].is_disjoint(&node_sets[j]) {
            return Err(format!(
                "track {}: route: {} and {} do not meet (they share no node), so the loop breaks there",
                t.id, label[i], label[j]
            ));
        }
    }
    let dist = |a: u64, b: u64| {
        let (ax, az) = proj.xz(osm.pos[&a]);
        let (bx, bz) = proj.xz(osm.pos[&b]);
        (ax - bx).hypot(az - bz)
    };
    // Per leg: node -> (next node, way, cost).
    type Adj = BTreeMap<u64, Vec<(u64, usize, f64)>>;
    let mut adj: Vec<Adj> = vec![BTreeMap::new(); k];
    for (l, ws) in legs.iter().enumerate() {
        for &wi in ws {
            let w = &osm.ways[wi];
            let one = match (w.tag("oneway"), w.tag("junction")) {
                (Some("yes" | "true" | "1"), _) | (_, Some("roundabout" | "circular")) => 1,
                (Some("-1"), _) => -1,
                _ => 0,
            };
            for p in w.nodes.windows(2) {
                let c = dist(p[0], p[1]);
                let (fw, bw) = match one {
                    1 => (c, c * 4.0),
                    -1 => (c * 4.0, c),
                    _ => (c, c),
                };
                adj[l].entry(p[0]).or_default().push((p[1], wi, fw));
                adj[l].entry(p[1]).or_default().push((p[0], wi, bw));
            }
        }
    }
    let closers: Vec<u64> = node_sets[k - 1]
        .intersection(&node_sets[0])
        .copied()
        .collect();
    let mut best: Option<(f64, Vec<Step>)> = None;
    for &j in &closers {
        // Dijkstra from (first leg, j) back to (last leg, j).
        let start = St {
            leg: 0,
            node: j,
            moved: false,
        };
        let goal = St {
            leg: k - 1,
            node: j,
            moved: true,
        };
        let mut d: BTreeMap<St, f64> = BTreeMap::new();
        let mut prev: BTreeMap<St, (St, Option<usize>)> = BTreeMap::new();
        let mut heap = BinaryHeap::new();
        d.insert(start, 0.0);
        heap.push(Q(0.0, start));
        let mut found = None;
        while let Some(Q(c, u)) = heap.pop() {
            if c > d[&u] {
                continue;
            }
            if u == goal {
                found = Some(c);
                break;
            }
            let mut next: Vec<(St, f64, Option<usize>)> = Vec::new();
            if let Some(es) = adj[u.leg].get(&u.node) {
                for &(v, w, ec) in es {
                    next.push((
                        St {
                            leg: u.leg,
                            node: v,
                            moved: true,
                        },
                        ec,
                        Some(w),
                    ));
                }
            }
            if u.moved && u.leg + 1 < k && node_sets[u.leg + 1].contains(&u.node) {
                next.push((
                    St {
                        leg: u.leg + 1,
                        node: u.node,
                        moved: false,
                    },
                    0.5,
                    None,
                ));
            }
            for (v, ec, w) in next {
                let nc = c + ec;
                if nc < *d.get(&v).unwrap_or(&f64::MAX) {
                    d.insert(v, nc);
                    prev.insert(v, (u, w));
                    heap.push(Q(nc, v));
                }
            }
        }
        let Some(cost) = found else { continue };
        if best.as_ref().is_some_and(|(b, _)| *b <= cost) {
            continue;
        }
        let mut steps = Vec::new();
        let mut u = goal;
        while u != start {
            let (p, w) = prev[&u];
            if let Some(w) = w {
                steps.push(Step {
                    from: p.node,
                    way: w,
                    leg: parent[p.leg],
                });
            }
            u = p;
        }
        steps.reverse();
        if steps.len() > 2 {
            best = Some((cost, steps));
        }
    }
    best.map(|b| b.1).ok_or_else(|| {
        format!(
            "track {}: the legs meet, but no loop runs through all of them in order",
            t.id
        )
    })
}

// ---------- shaping ----------

fn width_of(w: &Way, t: &TrackDef) -> f64 {
    let tag_w = w
        .tag("width")
        .and_then(|v| v.trim_end_matches(" m").parse::<f64>().ok());
    let lanes = w.tag("lanes").and_then(|v| v.parse::<f64>().ok());
    let base = match (tag_w, lanes) {
        (Some(x), _) => x,
        (None, Some(l)) => l * 3.2 + 1.0,
        _ => match w.tag("highway").unwrap_or("") {
            "motorway" | "trunk" | "primary" => 10.0,
            "secondary" => 9.0,
            "tertiary" => 8.0,
            "residential" | "unclassified" => 7.0,
            _ => 6.0,
        },
    };
    (base * t.widen).max(t.min_width)
}

#[derive(Clone, Copy, Debug)]
struct Pt {
    x: f64,
    z: f64,
    w: f64,
    leg: usize,
    way: usize,
}

/// Points every `step` metres (about) along a closed polyline; attributes from the point
/// before.
fn resample(pts: &[Pt], step: f64) -> Vec<Pt> {
    let n = pts.len();
    let mut total = 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        total += (b.x - a.x).hypot(b.z - a.z);
    }
    let count = (total / step).round().max(8.0) as usize;
    let step = total / count as f64;
    let mut out = Vec::with_capacity(count);
    let mut i = 0;
    let mut seg_start = 0.0;
    for c in 0..count {
        let s = c as f64 * step;
        loop {
            let (a, b) = (pts[i % n], pts[(i + 1) % n]);
            let len = (b.x - a.x).hypot(b.z - a.z);
            if s <= seg_start + len || i >= n - 1 {
                let f = if len > 0.0 {
                    ((s - seg_start) / len).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                out.push(Pt {
                    x: a.x + (b.x - a.x) * f,
                    z: a.z + (b.z - a.z) * f,
                    ..a
                });
                break;
            }
            seg_start += len;
            i += 1;
        }
    }
    out
}

/// Drops points where the loop turns back on itself (more than about 120 degrees): a short
/// driveway that doubles back to a junction becomes a chord across it.
fn despike(pts: &mut Vec<Pt>) {
    loop {
        let n = pts.len();
        let mut drop = None;
        for i in 0..n {
            let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            let (ux, uz) = (b.x - a.x, b.z - a.z);
            let (wx, wz) = (c.x - b.x, c.z - b.z);
            let l = ux.hypot(uz) * wx.hypot(wz);
            if l > 1e-9 && (ux * wx + uz * wz) / l < -0.5 {
                drop = Some(i);
                break;
            }
        }
        match drop {
            Some(i) if pts.len() > 8 => {
                pts.remove(i);
            }
            _ => return,
        }
    }
}

/// On a loop sampled every metre: where the way 8 m ahead turns back on the way 8 m behind
/// (a driveway that doubles back to a junction), the points in between go, and the corner
/// becomes a chord.
fn unkink(pts: &mut Vec<Pt>) {
    let k = 8;
    loop {
        let n = pts.len();
        let mut worst: Option<(f64, usize)> = None;
        for i in 0..n {
            let (a, b, c) = (pts[(i + n - k) % n], pts[i], pts[(i + k) % n]);
            let (ux, uz) = (b.x - a.x, b.z - a.z);
            let (wx, wz) = (c.x - b.x, c.z - b.z);
            let l = ux.hypot(uz) * wx.hypot(wz);
            if l < 1e-9 {
                continue;
            }
            let dot = (ux * wx + uz * wz) / l;
            if dot < -0.25 && worst.is_none_or(|w| dot < w.0) {
                worst = Some((dot, i));
            }
        }
        let Some((_, i)) = worst else { return };
        if n < 64 {
            return;
        }
        // Remove i-k/2 ..= i+k/2 (wrapping).
        let mut idx: Vec<usize> = (0..=k).map(|o| (i + n + o - k / 2) % n).collect();
        idx.sort_unstable();
        for j in idx.into_iter().rev() {
            pts.remove(j);
        }
        *pts = resample(pts, 1.0);
    }
}

/// Moving averages on a closed loop: rounds the corners.
fn smooth(pts: &mut [Pt], half: usize, passes: usize) {
    let n = pts.len();
    for _ in 0..passes {
        let old = pts.to_vec();
        for i in 0..n {
            let (mut x, mut z) = (0.0, 0.0);
            for o in 0..=2 * half {
                let p = old[(i + n + o - half) % n];
                x += p.x;
                z += p.z;
            }
            let c = (2 * half + 1) as f64;
            pts[i].x = x / c;
            pts[i].z = z / c;
        }
    }
}

fn smooth_vals(v: &mut [f64], half: usize, passes: usize) {
    let n = v.len();
    for _ in 0..passes {
        let old = v.to_vec();
        for i in 0..n {
            let mut s = 0.0;
            for o in 0..=2 * half {
                s += old[(i + n + o - half) % n];
            }
            v[i] = s / (2 * half + 1) as f64;
        }
    }
}

/// A grid of cells for finding things near a point.
struct Grid {
    cell: f64,
    map: BTreeMap<(i64, i64), Vec<usize>>,
}

impl Grid {
    fn new(cell: f64) -> Grid {
        Grid {
            cell,
            map: BTreeMap::new(),
        }
    }
    fn key(&self, x: f64, z: f64) -> (i64, i64) {
        (
            (x / self.cell).floor() as i64,
            (z / self.cell).floor() as i64,
        )
    }
    fn add(&mut self, x: f64, z: f64, i: usize) {
        let k = self.key(x, z);
        self.map.entry(k).or_default().push(i);
    }
    fn near(&self, x: f64, z: f64, r: f64) -> Vec<usize> {
        let (a, b) = self.key(x - r, z - r);
        let (c, d) = self.key(x + r, z + r);
        let mut out = Vec::new();
        for i in a..=c {
            for j in b..=d {
                if let Some(v) = self.map.get(&(i, j)) {
                    out.extend(v);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Where the ray from `p` along `dir` crosses segment `a`-`b`: its distance, if it does.
fn ray_hit(p: (f64, f64), dir: (f64, f64), a: (f64, f64), b: (f64, f64)) -> Option<f64> {
    let (ex, ez) = (b.0 - a.0, b.1 - a.1);
    let den = dir.0 * ez - dir.1 * ex;
    if den.abs() < 1e-12 {
        return None;
    }
    let (wx, wz) = (a.0 - p.0, a.1 - p.1);
    let t = (wx * ez - wz * ex) / den;
    let u = (wx * dir.1 - wz * dir.0) / den;
    (t >= 0.0 && (0.0..=1.0).contains(&u)).then_some(t)
}

fn inside(p: (f64, f64), poly: &[(f64, f64)]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            c = !c;
        }
    }
    c
}

fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (ex, ez) = (b.0 - a.0, b.1 - a.1);
    let l2 = (ex * ex + ez * ez).max(1e-12);
    let t = (((p.0 - a.0) * ex + (p.1 - a.1) * ez) / l2).clamp(0.0, 1.0);
    (p.0 - a.0 - ex * t).hypot(p.1 - a.1 - ez * t)
}

/// Twice the signed area (the shoelace sum).
fn area2(poly: &[(f64, f64)]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum()
}

/// Drops repeated and nearly straight-through points.
fn simplify(poly: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let mut v = poly;
    if v.len() > 1 && v[0] == v[v.len() - 1] {
        v.pop();
    }
    loop {
        let n = v.len();
        if n <= 3 {
            return v;
        }
        let mut drop = None;
        for i in 0..n {
            let (a, b, c) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            let (ux, uz) = (b.0 - a.0, b.1 - a.1);
            let (wx, wz) = (c.0 - b.0, c.1 - b.1);
            let lu = ux.hypot(uz);
            let lw = wx.hypot(wz);
            if lu < 0.4 || lw < 0.4 {
                drop = Some(i);
                break;
            }
            let cross = (ux * wz - uz * wx) / (lu * lw);
            let dot = (ux * wx + uz * wz) / (lu * lw);
            if cross.abs() < 0.04 && dot > 0.0 {
                drop = Some(i);
                break;
            }
        }
        match drop {
            Some(i) => {
                v.remove(i);
            }
            None => return v,
        }
    }
}

/// A building's kind for the client, and its levels when the map does not say.
fn building_kind(tags: &BTreeMap<String, String>) -> Option<(&'static str, f64)> {
    let b = tags.get("building").map(String::as_str)?;
    if tags.get("historic").map(String::as_str) == Some("city_gate") {
        return None;
    }
    Some(match b {
        "hospital" => ("hospital", 4.0),
        "church" | "chapel" | "cathedral" | "basilica" => ("church", 4.0),
        "bell_tower" | "tower" => ("tower", 8.0),
        "roof" | "kiosk" | "shelter" | "carport" => ("roof", 1.0),
        "garage" | "garages" | "shed" | "service" | "hut" => ("shed", 1.0),
        _ => ("house", 4.0),
    })
}

fn centroid(poly: &[(f64, f64)]) -> (f64, f64) {
    let n = poly.len() as f64;
    (
        poly.iter().map(|p| p.0).sum::<f64>() / n,
        poly.iter().map(|p| p.1).sum::<f64>() / n,
    )
}

fn build(osm: &Osm, t: &TrackDef, slots: usize) -> Result<Track, String> {
    // The origin: the middle of the route's ways.
    let legs = leg_ways(osm, t)?;
    let (mut a, mut o, mut c, mut d) = (90.0f64, 180.0f64, -90.0f64, -180.0f64);
    for &w in legs.iter().flatten() {
        for &(lat, lon) in &osm.ways[w].geom {
            a = a.min(lat);
            o = o.min(lon);
            c = c.max(lat);
            d = d.max(lon);
        }
    }
    let lat0 = ((a + c) / 2.0 * 1e5).round() / 1e5;
    let lon0 = ((o + d) / 2.0 * 1e5).round() / 1e5;
    let proj = Proj::new(lat0, lon0);
    let steps = route(osm, t, &proj)?;
    // The loop as points; corners rounded; widths eased.
    let mut pts: Vec<Pt> = steps
        .iter()
        .map(|s| {
            let (x, z) = proj.xz(osm.pos[&s.from]);
            Pt {
                x,
                z,
                w: width_of(&osm.ways[s.way], t),
                leg: s.leg,
                way: s.way,
            }
        })
        .collect();
    pts.dedup_by(|b, a| (a.x - b.x).hypot(a.z - b.z) < 0.05);
    despike(&mut pts);
    let mut fine = resample(&pts, 1.0);
    unkink(&mut fine);
    smooth(&mut fine, 7, 3);
    let mut widths: Vec<f64> = fine.iter().map(|p| p.w).collect();
    smooth_vals(&mut widths, 10, 3);
    for (p, w) in fine.iter_mut().zip(&widths) {
        p.w = *w;
    }
    let mut pts = resample(&fine, 2.0);
    let names: Vec<String> = t
        .route
        .iter()
        .enumerate()
        .map(|(i, l)| section_name(osm, &proj, l, &legs[i]))
        .collect();
    let mut track = Track {
        id: t.id.clone(),
        name: t.name.clone(),
        attribution: "© OpenStreetMap contributors".into(),
        licence: "ODbL 1.0 (https://opendatacommons.org/licenses/odbl/1-0/)".into(),
        origin: (lat0, lon0),
        ..Track::default()
    };
    let set_samples = |track: &mut Track, pts: &[Pt]| {
        track.samples = pts
            .iter()
            .map(|p| Sample {
                x: p.x,
                z: p.z,
                w: p.w,
                ..Sample::default()
            })
            .collect();
        track.finish();
        track.spacing = track.length / pts.len() as f64;
        track.sections = sections(track, pts, &names, osm);
    };
    set_samples(&mut track, &pts);
    // The start line at sample 0.
    let s0 = track
        .place(&t.start)
        .map_err(|m| format!("track {}: start: {m}", t.id))?;
    let (i0, _) = track.seg(s0);
    pts.rotate_left(i0);
    set_samples(&mut track, &pts);
    let n = pts.len();
    let pos: Vec<(f64, f64)> = pts.iter().map(|p| (p.x, p.z)).collect();
    let mut sgrid = Grid::new(25.0);
    for (i, p) in pos.iter().enumerate() {
        sgrid.add(p.0, p.1, i);
    }
    let near_track = |p: (f64, f64), r: f64| -> Option<(usize, f64)> {
        let mut best: Option<(usize, f64)> = None;
        for i in sgrid.near(p.0, p.1, r) {
            let dd = (pos[i].0 - p.0).hypot(pos[i].1 - p.1);
            if dd <= r && best.is_none_or(|b| dd < b.1) {
                best = Some((i, dd));
            }
        }
        best
    };
    // Buildings near the road; those standing on it are left out.
    let hospital_areas: Vec<(&Way, Vec<(f64, f64)>)> = osm
        .ways
        .iter()
        .filter(|w| w.tag("amenity") == Some("hospital") && w.nodes.len() > 3)
        .map(|w| (w, w.geom.iter().map(|g| proj.xz(*g)).collect()))
        .collect();
    let mut buildings: Vec<(Building, Vec<(f64, f64)>)> = Vec::new();
    let mut dropped = 0;
    for w in &osm.ways {
        let Some((kind, default_levels)) = building_kind(&w.tags) else {
            continue;
        };
        if w.nodes.len() < 4 {
            continue;
        }
        let poly = simplify(w.geom.iter().map(|g| proj.xz(*g)).collect());
        if poly.len() < 3 || area2(&poly).abs() < 8.0 {
            continue;
        }
        let mid = centroid(&poly);
        let in_hospital = hospital_areas.iter().any(|(_, a)| inside(mid, a));
        let name = w.tag("name").unwrap_or("").to_string();
        let pavilion = kind == "hospital"
            || (in_hospital
                && (name.starts_with("Padiglione") || name.contains("Pronto Soccorso")));
        let reach = if pavilion { t.scenery * 2.5 } else { t.scenery };
        if !poly.iter().any(|p| near_track(*p, reach).is_some()) {
            continue;
        }
        let mut on_road = false;
        'edges: for i in 0..poly.len() {
            let (pa, pb) = (poly[i], poly[(i + 1) % poly.len()]);
            let len = (pb.0 - pa.0).hypot(pb.1 - pa.1);
            let k = len.ceil().max(1.0) as usize;
            for j in 0..=k {
                let f = j as f64 / k as f64;
                let q = (pa.0 + (pb.0 - pa.0) * f, pa.1 + (pb.1 - pa.1) * f);
                if let Some((si, dd)) = near_track(q, 20.0) {
                    if dd < pts[si].w / 2.0 + 0.8 {
                        on_road = true;
                        break 'edges;
                    }
                }
            }
        }
        if !on_road {
            on_road = sgrid
                .near(mid.0, mid.1, 80.0)
                .iter()
                .any(|&i| inside(pos[i], &poly));
        }
        if on_road {
            dropped += 1;
            continue;
        }
        let mut poly = poly;
        if area2(&poly) < 0.0 {
            poly.reverse();
        }
        let levels = w
            .tag("building:levels")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(default_levels);
        let height = w
            .tag("height")
            .and_then(|v| v.trim_end_matches(" m").parse::<f64>().ok())
            .unwrap_or(match kind {
                "church" => 15.0,
                "tower" => 26.0,
                "roof" => 3.5,
                "shed" => 3.2,
                _ => levels * 3.4 + 1.2,
            });
        buildings.push((
            Building {
                id: w.id,
                kind: if pavilion { "pavilion" } else { kind }.to_string(),
                name,
                height: (height * 10.0).round() / 10.0,
                levels: levels.round() as u32,
                foot: poly.iter().map(|p| [p.0, p.1]).collect(),
                portico: Vec::new(),
            },
            poly,
        ));
    }
    // Walls: the nearest building edge either side, or the end of the pavement; and where
    // another stretch of the track runs alongside, half way to it.
    let mut egrid = Grid::new(20.0);
    let mut edges: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for (_, poly) in &buildings {
        for i in 0..poly.len() {
            let (pa, pb) = (poly[i], poly[(i + 1) % poly.len()]);
            let len = (pb.0 - pa.0).hypot(pb.1 - pa.1);
            let k = (len / 10.0).ceil().max(1.0) as usize;
            for j in 0..=k {
                let f = j as f64 / k as f64;
                egrid.add(
                    pa.0 + (pb.0 - pa.0) * f,
                    pa.1 + (pb.1 - pa.1) * f,
                    edges.len(),
                );
            }
            edges.push((pa, pb));
        }
    }
    for i in 0..n {
        let sm = &track.samples[i];
        let p = (sm.x, sm.z);
        let nrm = (sm.tz, -sm.tx);
        let half = sm.w / 2.0;
        let maxd = half + t.verge;
        let mut walls = [maxd, maxd];
        for (si, side) in [1.0, -1.0].iter().enumerate() {
            let dir = (nrm.0 * side, nrm.1 * side);
            for e in egrid.near(p.0 + dir.0 * maxd / 2.0, p.1 + dir.1 * maxd / 2.0, maxd) {
                if let Some(h) = ray_hit(p, dir, edges[e].0, edges[e].1) {
                    walls[si] = walls[si].min((h - 0.3).max(half + 0.6));
                }
            }
        }
        for j in sgrid.near(p.0, p.1, 45.0) {
            let di = (j as f64 - i as f64).abs();
            let ds = (di * track.spacing).min(track.length - di * track.spacing);
            if ds < 40.0 {
                continue;
            }
            let (vx, vz) = (pos[j].0 - p.0, pos[j].1 - p.1);
            if (vx * sm.tx + vz * sm.tz).abs() > 2.5 {
                continue;
            }
            let lateral = vx * nrm.0 + vz * nrm.1;
            let inner = lateral.abs() - pts[j].w / 2.0;
            if inner > half + 1.0 {
                let si = if lateral > 0.0 { 0 } else { 1 };
                walls[si] = walls[si].min((half + inner) / 2.0);
            }
        }
        track.samples[i].left = walls[0];
        track.samples[i].right = walls[1];
    }
    // The racing line: pulled straight between its neighbours, inside the road.
    let mut off = vec![0.0f64; n];
    let kk = 6;
    for _ in 0..600 {
        for i in 0..n {
            let q = |j: usize, off: &[f64]| {
                let s = &track.samples[j];
                (s.x + s.tz * off[j], s.z - s.tx * off[j])
            };
            let a = q((i + n - kk) % n, &off);
            let b = q((i + kk) % n, &off);
            let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            let s = &track.samples[i];
            let want = (mid.0 - s.x) * s.tz - (mid.1 - s.z) * s.tx;
            let room = (s.w / 2.0 - 1.4).max(0.0);
            off[i] = (off[i] + 0.6 * (want - off[i])).clamp(-room, room);
        }
    }
    smooth_vals(&mut off, 3, 2);
    for (i, o) in off.iter().enumerate() {
        track.samples[i].line = *o;
    }
    // Checkpoints, grid, minimap.
    track.checkpoints = (1..16).map(|k| track.length * k as f64 / 16.0).collect();
    let w0 = track.samples[0].w;
    track.grid = (0..slots)
        .map(|k| {
            let (row, col) = (k / 2, k % 2);
            let behind = 7.0 + row as f64 * 7.0 + col as f64 * 3.5;
            let left = if col == 0 { w0 / 4.0 } else { -w0 / 4.0 };
            (behind, left)
        })
        .collect();
    track.minimap = track
        .samples
        .iter()
        .step_by(4)
        .map(|s| [s.x, s.z])
        .collect();
    // Porticoes: long house fronts that face the road across the pavement.
    for (b, poly) in &mut buildings {
        if b.kind != "house" || b.height < 7.0 {
            continue;
        }
        for i in 0..poly.len() {
            let (pa, pb) = (poly[i], poly[(i + 1) % poly.len()]);
            let (ex, ez) = (pb.0 - pa.0, pb.1 - pa.1);
            let len = ex.hypot(ez);
            if len < 6.0 {
                continue;
            }
            // Outward, for a positive shoelace sum.
            let out = (ez / len, -ex / len);
            let m = ((pa.0 + pb.0) / 2.0, (pa.1 + pb.1) / 2.0);
            let Some((si, _)) = near_track(m, t.verge + 30.0) else {
                continue;
            };
            let s = &track.samples[si];
            let v = (s.x - m.0, s.z - m.1);
            let facing = v.0 * out.0 + v.1 * out.1;
            let parallel = (out.0 * s.tz - out.1 * s.tx).abs();
            if facing > 1.0 && facing < s.w / 2.0 + t.verge + 6.0 && parallel > 0.85 {
                b.portico.push(i);
            }
        }
    }
    // Landmarks.
    for spec in &t.landmarks {
        track
            .landmarks
            .push(landmark(osm, &proj, spec).map_err(|m| format!("track {}: {m}", t.id))?);
    }
    // Hospital grounds the track runs through or by.
    for (w, poly) in &hospital_areas {
        if poly.iter().any(|p| near_track(*p, 80.0).is_some()) {
            let poly = simplify(poly.clone());
            track.areas.push(Area {
                kind: "hospital".into(),
                name: w.tag("name").unwrap_or("").to_string(),
                foot: poly.iter().map(|p| [p.0, p.1]).collect(),
            });
        }
    }
    track.buildings = buildings.into_iter().map(|b| b.0).collect();
    let route: Vec<String> = t.route.iter().map(leg_name).collect();
    let mut used: Vec<u64> = Vec::new();
    for s in &steps {
        let id = osm.ways[s.way].id;
        if used.last() != Some(&id) {
            used.push(id);
        }
    }
    let mut notes = vec![
        format!(
            "{}: built by tools/osm2track from OpenStreetMap data.",
            t.name
        ),
        "Map data © OpenStreetMap contributors, under the Open Database Licence (ODbL) 1.0:".into(),
        "https://www.openstreetmap.org/copyright. This file is a Derived Database of it and is"
            .into(),
        "also under the ODbL 1.0 (https://opendatacommons.org/licenses/odbl/1-0/).".into(),
        "Do not edit it by hand: change the track block and run tools/osm2track again.".into(),
        format!("Route: {}.", route.join(" / ")),
        "OSM ways, in order:".into(),
    ];
    for chunk in used.chunks(10) {
        notes.push(format!(
            "  {}",
            chunk
                .iter()
                .map(|w| w.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    notes.push(format!(
        "{} buildings near the road; {} left out because they stand on it.",
        track.buildings.len(),
        dropped
    ));
    track.notes = notes;
    Ok(track)
}

/// A leg's section name: its street, or the name its ways share, or the hospital it runs
/// through, or "Leg <first way>".
fn section_name(osm: &Osm, proj: &Proj, leg: &Leg, ways: &[usize]) -> String {
    if let Leg::Street(s) = leg {
        return s.clone();
    }
    let names: BTreeSet<&str> = ways
        .iter()
        .filter_map(|&w| osm.ways[w].tag("name"))
        .collect();
    if names.len() == 1 && ways.iter().all(|&w| osm.ways[w].tag("name").is_some()) {
        return names.into_iter().next().unwrap().to_string();
    }
    let w = &osm.ways[ways[ways.len() / 2]];
    let mid = proj.xz(w.geom[w.geom.len() / 2]);
    for a in osm
        .ways
        .iter()
        .filter(|w| w.tag("amenity") == Some("hospital"))
    {
        let poly: Vec<(f64, f64)> = a.geom.iter().map(|g| proj.xz(*g)).collect();
        if poly.len() > 3 && inside(mid, &poly) {
            return a.tag("name").unwrap_or("Hospital").to_string();
        }
    }
    format!("Leg {}", osm.ways[ways[0]].id)
}

/// Runs of samples on the same leg.
fn sections(track: &Track, pts: &[Pt], names: &[String], osm: &Osm) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let n = pts.len();
    let mut i = 0;
    while i < n {
        let leg = pts[i].leg;
        let mut j = i;
        let mut ways: Vec<u64> = Vec::new();
        let mut hw: BTreeMap<String, usize> = BTreeMap::new();
        while j < n && pts[j].leg == leg {
            let w = &osm.ways[pts[j].way];
            if !ways.contains(&w.id) {
                ways.push(w.id);
            }
            *hw.entry(w.tag("highway").unwrap_or("").to_string())
                .or_default() += 1;
            j += 1;
        }
        let to = if j == n {
            track.length
        } else {
            track.samples[j].s
        };
        out.push(Section {
            name: names[leg].clone(),
            from: track.samples[i].s,
            to,
            highway: hw
                .into_iter()
                .max_by_key(|(_, c)| *c)
                .map(|(h, _)| h)
                .unwrap_or_default(),
            ways,
        });
        i = j;
    }
    out
}

/// A landmark from the map: by name (a gate or a building first), facing along the nearest
/// road.
fn landmark(osm: &Osm, proj: &Proj, spec: &LandmarkSpec) -> Result<Landmark, String> {
    let mut outline: Option<Vec<(f64, f64)>> = None;
    let mut levels = None;
    let mut score_best = 0;
    let candidates = osm
        .ways
        .iter()
        .filter(|w| w.tag("highway").is_none())
        .map(|w| (w.geom.clone(), &w.tags))
        .chain(osm.things.iter().map(|t| (t.outline.clone(), &t.tags)));
    for (g, tags) in candidates {
        if tags.get("name").map(String::as_str) != Some(spec.name.as_str()) {
            continue;
        }
        let score = match (tags.get("historic"), tags.get("building")) {
            (Some(_), Some(_)) => 3,
            (_, Some(_)) => 2,
            _ => 1,
        };
        if score > score_best {
            score_best = score;
            outline = Some(g);
            levels = tags
                .get("building:levels")
                .and_then(|v| v.parse::<f64>().ok());
        }
    }
    let mut pts: Vec<(f64, f64)> = match (outline, spec.at) {
        (Some(o), _) => o.iter().map(|g| proj.xz(*g)).collect(),
        (None, Some(at)) => vec![proj.xz(at)],
        (None, None) => {
            return Err(format!(
                "landmark {} {:?}: nothing in the extract has that name (give `at <lat> <lon>`)",
                spec.kind, spec.name
            ))
        }
    };
    if pts.len() > 1 && pts[0] == pts[pts.len() - 1] {
        pts.pop();
    }
    let c = centroid(&pts);
    let mut best: Option<(f64, (f64, f64))> = None;
    for w in osm.drivable() {
        if w.tag("highway") == Some("service") {
            continue;
        }
        for g in w.geom.windows(2) {
            let (a, b) = (proj.xz(g[0]), proj.xz(g[1]));
            let dd = seg_dist(c, a, b);
            if dd < 60.0 && best.is_none_or(|x| dd < x.0) {
                best = Some((dd, (b.0 - a.0, b.1 - a.1)));
            }
        }
    }
    let dir = best.map(|b| b.1).unwrap_or((1.0, 0.0));
    let l = dir.0.hypot(dir.1).max(1e-9);
    let (dx, dz) = (dir.0 / l, dir.1 / l);
    let (mut w, mut d) = (12.0f64, 10.0f64);
    if pts.len() > 2 {
        let (mut a0, mut a1, mut c0, mut c1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in &pts {
            let (vx, vz) = (p.0 - c.0, p.1 - c.1);
            let along = vx * dx + vz * dz;
            let across = vx * dz - vz * dx;
            a0 = a0.min(along);
            a1 = a1.max(along);
            c0 = c0.min(across);
            c1 = c1.max(across);
        }
        d = (a1 - a0).max(4.0);
        w = (c1 - c0).max(4.0);
    }
    let h = match spec.kind.as_str() {
        "gate" => 13.0f64.max(levels.unwrap_or(0.0) * 4.0),
        _ => 12.0,
    };
    Ok(Landmark {
        kind: spec.kind.clone(),
        name: spec.name.clone(),
        x: c.0,
        z: c.1,
        dx,
        dz,
        size: [w, d, h],
    })
}
