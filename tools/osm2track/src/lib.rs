//! osm2track: builds an OpenKart track file from OpenStreetMap data and a route, and checks
//! routes for the circuit designer.
//!
//! The binary (`cargo run -p osm2track -- games/pachino-gp osm/sorsola.json [--track id]`)
//! reads a game's `*.kart` files, and for each `track` block (or the one named) joins the
//! `route` legs into one closed loop through the street graph of the extract (Overpass JSON with
//! `out geom`), rounds the corners, takes the road's width from its lanes or class, finds the
//! walls (buildings, and other stretches of the track running close by), a racing line, the
//! checkpoints, the grid, the minimap, the buildings near the road (with porticoes where they
//! face it), the landmarks and the hospital grounds, bakes the scenery of the `dress` lines,
//! and writes the block's `osm` file next to the game files.
//!
//! The library is the same code, also compiled to WebAssembly in `openkart-web` for the
//! circuit designer: [`Osm`] (an extract), [`build`] (a track), [`check`] (what is wrong with
//! a route, and its exact form) and [`Graph`] (the street graph: snapping a click to a node,
//! and the shortest drive between two nodes).
//!
//! Map data © OpenStreetMap contributors (ODbL 1.0): the track file it writes is a Derived
//! Database under the same licence.

use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::path::PathBuf;

use openkart::script::{LandmarkSpec, Leg, TrackDef, WaySpan};
use openkart::track::{Area, Building, Landmark, Sample, Section, Track};
use serde::Serialize;
use serde_json::Value;

/// The highways a circuit may use.
pub const DRIVABLE: [&str; 16] = [
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

/// An OpenStreetMap extract: ways with their geometry, and relations and tagged nodes (for
/// landmarks).
pub struct Osm {
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
    /// Overpass JSON files, merged.
    pub fn read(paths: &[PathBuf]) -> Result<Osm, String> {
        let mut texts = Vec::new();
        for p in paths {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            texts.push((p.display().to_string(), text));
        }
        Osm::parse(&texts)
    }

    /// Overpass JSON documents (name, text), merged.
    pub fn parse(texts: &[(String, String)]) -> Result<Osm, String> {
        let mut docs = Vec::new();
        for (name, text) in texts {
            let v: Value = serde_json::from_str(text)
                .map_err(|e| format!("{name}: not Overpass JSON: {e}"))?;
            docs.push((name.clone(), v));
        }
        Osm::from_values(&docs)
    }

    /// Overpass JSON documents already parsed (name, document), merged.
    pub fn from_values(docs: &[(String, Value)]) -> Result<Osm, String> {
        let mut ways = BTreeMap::new();
        let mut things = Vec::new();
        let mut pos = BTreeMap::new();
        for (name, v) in docs {
            let els = v["elements"]
                .as_array()
                .ok_or_else(|| format!("{name}: there is no `elements` list"))?;
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

    /// How many ways, and how many of them are drivable.
    pub fn counts(&self) -> (usize, usize) {
        (self.ways.len(), self.drivable().count())
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
    to: u64,
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
        Leg::Way(w) => {
            let mut s = format!("way {}", w.id);
            match w.forward {
                Some(true) => s.push_str(" forward"),
                Some(false) => s.push_str(" backward"),
                None => {}
            }
            if let Some(n) = w.from {
                s.push_str(&format!(" from {n}"));
            }
            if let Some(n) = w.to {
                s.push_str(&format!(" to {n}"));
            }
            s
        }
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
            Leg::Way(span) => {
                let Some(i) = osm.ways.iter().position(|w| w.id == span.id) else {
                    return Err(format!(
                        "track {}: route: way {} is not in the extract",
                        t.id, span.id
                    ));
                };
                for n in [span.from, span.to].into_iter().flatten() {
                    if !osm.ways[i].nodes.contains(&n) {
                        return Err(format!(
                            "track {}: route: node {n} is not on way {}",
                            t.id, span.id
                        ));
                    }
                }
                vec![i]
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

/// A route cut into sub-legs for the search: a street is one, a list of ways is one per way,
/// in order (so the loop follows the listed ways one after the other), a `route way` line is
/// one.
struct SubLegs<'a> {
    ways: Vec<Vec<usize>>,
    /// The route leg each came from.
    parent: Vec<usize>,
    label: Vec<String>,
    /// For a `route way` line: the direction, and the nodes it enters and leaves by.
    spans: Vec<Option<&'a WaySpan>>,
    /// Where each may be entered and left.
    entry: Vec<BTreeSet<u64>>,
    exit: Vec<BTreeSet<u64>>,
}

fn sub_legs<'a>(osm: &Osm, t: &'a TrackDef) -> Result<SubLegs<'a>, String> {
    let mut sl = SubLegs {
        ways: Vec::new(),
        parent: Vec::new(),
        label: Vec::new(),
        spans: Vec::new(),
        entry: Vec::new(),
        exit: Vec::new(),
    };
    for (i, ws) in leg_ways(osm, t)?.into_iter().enumerate() {
        match &t.route[i] {
            Leg::Street(name) => {
                sl.ways.push(ws);
                sl.parent.push(i);
                sl.label.push(name.clone());
                sl.spans.push(None);
            }
            Leg::Ways(_) => {
                for w in ws {
                    sl.ways.push(vec![w]);
                    sl.parent.push(i);
                    sl.label.push(format!("way {}", osm.ways[w].id));
                    sl.spans.push(None);
                }
            }
            Leg::Way(span) => {
                sl.ways.push(ws);
                sl.parent.push(i);
                sl.label.push(leg_name(&t.route[i]));
                sl.spans.push(Some(span));
            }
        }
    }
    for l in 0..sl.ways.len() {
        let nodes: BTreeSet<u64> = sl.ways[l]
            .iter()
            .flat_map(|&w| osm.ways[w].nodes.iter().copied())
            .collect();
        let only = |n: Option<u64>| match n {
            Some(n) => BTreeSet::from([n]),
            None => nodes.clone(),
        };
        sl.entry.push(only(sl.spans[l].and_then(|s| s.from)));
        sl.exit.push(only(sl.spans[l].and_then(|s| s.to)));
    }
    Ok(sl)
}

/// Where a `route way` line runs on its way: the node indices `lo..=hi` and the direction.
/// On a closed way (a roundabout) the node it closes at is both the first and the last, so the
/// span is read whichever way agrees with the direction (or is the shorter).
fn span_range(w: &Way, sp: &WaySpan) -> Result<(usize, usize, Option<bool>), String> {
    let at = |n: Option<u64>| -> Vec<usize> {
        n.map(|n| {
            w.nodes
                .iter()
                .enumerate()
                .filter(|(_, x)| **x == n)
                .map(|(i, _)| i)
                .collect()
        })
        .unwrap_or_default()
    };
    let (fs, ts) = (at(sp.from), at(sp.to));
    let last = w.nodes.len() - 1;
    let dir = sp.forward;
    match (fs.is_empty(), ts.is_empty()) {
        (false, false) => {
            let mut best: Option<(usize, usize)> = None;
            for &f in &fs {
                for &to in &ts {
                    if f == to || dir.is_some_and(|d| d != (f < to)) {
                        continue;
                    }
                    if best.is_none_or(|(a, b)| f.abs_diff(to) < a.abs_diff(b)) {
                        best = Some((f, to));
                    }
                }
            }
            let (f, to) = best.ok_or_else(|| "from and to go the other way".to_string())?;
            Ok((f.min(to), f.max(to), Some(f < to)))
        }
        (false, true) => Ok(match dir {
            Some(false) => (0, *fs.iter().max().unwrap(), dir),
            _ => (*fs.iter().min().unwrap(), last, dir),
        }),
        (true, false) => Ok(match dir {
            Some(false) => (*ts.iter().min().unwrap(), last, dir),
            _ => (0, *ts.iter().max().unwrap(), dir),
        }),
        (true, true) => Ok((0, last, dir)),
    }
}

/// Joins the legs into the cheapest closed loop: leg after leg, each on its own ways,
/// switching to the next leg where they meet. Driving against a one-way street costs four
/// times as much, so dual carriageways are taken the right way round.
fn route(osm: &Osm, t: &TrackDef, proj: &Proj) -> Result<Vec<Step>, String> {
    let sl = sub_legs(osm, t)?;
    let SubLegs {
        ways: legs,
        parent,
        label,
        spans,
        entry,
        exit,
    } = &sl;
    let k = legs.len();
    if k < 2 {
        return Err(format!(
            "track {}: a route needs at least two legs that close a loop",
            t.id
        ));
    }
    for i in 0..k {
        let j = (i + 1) % k;
        if exit[i].is_disjoint(&entry[j]) {
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
            // A `route way` line: only between its nodes, only its way round.
            let (lo, hi, dir) = match spans[l] {
                Some(sp) => span_range(w, sp)
                    .map_err(|m| format!("track {}: route {}: {m}", t.id, label[l]))?,
                None => (0, w.nodes.len() - 1, None),
            };
            for p in w.nodes[lo..=hi].windows(2) {
                let c = dist(p[0], p[1]);
                let (fw, bw) = match one {
                    1 => (c, c * 4.0),
                    -1 => (c * 4.0, c),
                    _ => (c, c),
                };
                if dir != Some(false) {
                    adj[l].entry(p[0]).or_default().push((p[1], wi, fw));
                }
                if dir != Some(true) {
                    adj[l].entry(p[1]).or_default().push((p[0], wi, bw));
                }
            }
        }
    }
    let closers: Vec<u64> = exit[k - 1].intersection(&entry[0]).copied().collect();
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
            if u.moved
                && u.leg + 1 < k
                && exit[u.leg].contains(&u.node)
                && entry[u.leg + 1].contains(&u.node)
            {
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
                    to: u.node,
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
            "track {}: the legs meet, but no loop runs through all of them in order (check the directions and spans of `route way` lines)",
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

/// The loop before its start line is placed: the projection, the steps, the points every
/// 2 m (corners rounded, widths eased) and a track with those samples and the sections, with
/// `s = 0` where the loop's first step starts.
struct Shaped {
    proj: Proj,
    steps: Vec<Step>,
    pts: Vec<Pt>,
    names: Vec<String>,
    track: Track,
}

/// The origin for a route: the middle of its ways.
fn origin(osm: &Osm, legs: &[Vec<usize>]) -> (f64, f64) {
    let (mut a, mut o, mut c, mut d) = (90.0f64, 180.0f64, -90.0f64, -180.0f64);
    for &w in legs.iter().flatten() {
        for &(lat, lon) in &osm.ways[w].geom {
            a = a.min(lat);
            o = o.min(lon);
            c = c.max(lat);
            d = d.max(lon);
        }
    }
    (
        ((a + c) / 2.0 * 1e5).round() / 1e5,
        ((o + d) / 2.0 * 1e5).round() / 1e5,
    )
}

fn set_samples(track: &mut Track, pts: &[Pt], names: &[String], osm: &Osm) {
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
    track.sections = sections(track, pts, names, osm);
}

fn shape(osm: &Osm, t: &TrackDef) -> Result<Shaped, String> {
    let legs = leg_ways(osm, t)?;
    let (lat0, lon0) = origin(osm, &legs);
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
    let pts = resample(&fine, 2.0);
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
    set_samples(&mut track, &pts, &names, osm);
    Ok(Shaped {
        proj,
        steps,
        pts,
        names,
        track,
    })
}

/// Builds a track: the loop, its walls, racing line, buildings, landmarks and baked scenery.
/// `slots` is the number of grid slots.
pub fn build(osm: &Osm, t: &TrackDef, slots: usize) -> Result<Track, String> {
    let Shaped {
        proj,
        steps,
        mut pts,
        names,
        mut track,
    } = shape(osm, t)?;
    // The start line at sample 0.
    let s0 = track
        .place(&t.start)
        .map_err(|m| format!("track {}: start: {m}", t.id))?;
    let (i0, _) = track.seg(s0);
    pts.rotate_left(i0);
    set_samples(&mut track, &pts, &names, osm);
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
    // Ease the walls: a sliding minimum, then a mean, over 2 samples either side. Every wall
    // stays where it was or comes closer, so nothing new opens up, and barriers do not jump.
    for side in 0..2 {
        let w: Vec<f64> = track
            .samples
            .iter()
            .map(|s| if side == 0 { s.left } else { s.right })
            .collect();
        let mins: Vec<f64> = (0..n)
            .map(|i| {
                (0..5)
                    .map(|o| w[(i + n + o - 2) % n])
                    .fold(f64::MAX, f64::min)
            })
            .collect();
        for i in 0..n {
            let mean = (0..5).map(|o| mins[(i + n + o - 2) % n]).sum::<f64>() / 5.0;
            let s = &mut track.samples[i];
            let v = mean.max(s.w / 2.0 + 0.6);
            if side == 0 {
                s.left = v;
            } else {
                s.right = v;
            }
        }
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
    let route_ways: BTreeSet<u64> = steps.iter().map(|s| osm.ways[s.way].id).collect();
    for spec in &t.landmarks {
        track.landmarks.push(
            landmark(osm, &proj, spec, &route_ways).map_err(|m| format!("track {}: {m}", t.id))?,
        );
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
    // The bake: the scenery of the `dress` lines, placed once and written into the file.
    track.props = openkart::course::dress(t, &track).map_err(|e| e.to_string())?;
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
        // Legs on the same street (or in the same hospital) next to each other: one section.
        if let Some(last) = out.last_mut() {
            if last.name == names[leg] {
                last.to = to;
                for w in ways {
                    if !last.ways.contains(&w) {
                        last.ways.push(w);
                    }
                }
                i = j;
                continue;
            }
        }
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
fn landmark(
    osm: &Osm,
    proj: &Proj,
    spec: &LandmarkSpec,
    route_ways: &BTreeSet<u64>,
) -> Result<Landmark, String> {
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
    // Which way it faces: along the nearest road that is not the track's (the road through a
    // gate, not the ring road beside it).
    for w in osm.drivable() {
        if w.tag("highway") == Some("service") || route_ways.contains(&w.id) {
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
        // Extents along a turned axis overstate an outline (most of all a relation's bounds):
        // keep the outline's area.
        let k = (area2(&pts).abs() / 2.0 / ((a1 - a0) * (c1 - c0)).max(1e-6))
            .sqrt()
            .min(1.0);
        d = ((a1 - a0) * k).max(4.0);
        w = ((c1 - c0) * k).max(4.0);
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

// ---------- the circuit designer ----------

/// Metres between two points given as (lat, lon), near each other.
fn metres(a: (f64, f64), b: (f64, f64)) -> f64 {
    let k = ((a.0 + b.0) / 2.0).to_radians().cos();
    ((a.0 - b.0) * M).hypot((a.1 - b.1) * M * k)
}

/// One way's one-way rule: 1 in the order of its nodes, -1 against it, 0 both ways.
fn oneway(w: &Way) -> i32 {
    match (w.tag("oneway"), w.tag("junction")) {
        (Some("yes" | "true" | "1"), _) | (_, Some("roundabout" | "circular")) => 1,
        (Some("-1"), _) => -1,
        _ => 0,
    }
}

/// A stretch of a drive on one way, one way round: from node index `start` to `end` of the
/// way (in the direction of travel).
#[derive(Clone, Debug, PartialEq)]
struct Run {
    way: usize,
    start: usize,
    end: usize,
    metres: f64,
}

impl Run {
    fn forward(&self) -> bool {
        self.end > self.start
    }
}

/// Edges of a drive (way, node index from, node index to, metres), the indices next to each
/// other, into runs: a new run wherever the way, the direction or the place on the way
/// changes.
fn group_runs(edges: &[(usize, usize, usize, f64)]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for &(way, i, j, m) in edges {
        if let Some(r) = runs.last_mut() {
            if r.way == way && r.end == i && r.forward() == (j > i) {
                r.end = j;
                r.metres += m;
                continue;
            }
        }
        runs.push(Run {
            way,
            start: i,
            end: j,
            metres: m,
        });
    }
    runs
}

/// A run as an exact `route way` leg.
fn run_span(nodes: &[u64], id: u64, r: &Run) -> WaySpan {
    WaySpan {
        id,
        forward: Some(r.forward()),
        from: Some(nodes[r.start]),
        to: Some(nodes[r.end]),
    }
}

/// The loop's steps as runs on its ways. A loop that starts part way along a way, and comes
/// back to it, is one run there.
fn loop_runs(osm: &Osm, steps: &[Step]) -> Vec<Run> {
    let mut edges: Vec<(usize, usize, usize, f64)> = Vec::new();
    for s in steps {
        let w = &osm.ways[s.way];
        let mut best: Option<(usize, usize)> = None;
        for i in 0..w.nodes.len() - 1 {
            let e = if w.nodes[i] == s.from && w.nodes[i + 1] == s.to {
                (i, i + 1)
            } else if w.nodes[i] == s.to && w.nodes[i + 1] == s.from {
                (i + 1, i)
            } else {
                continue;
            };
            // On a way that meets itself, the edge that follows on from the last one.
            let follows = edges.last().is_some_and(|l| l.0 == s.way && l.2 == e.0);
            if best.is_none() || follows {
                best = Some(e);
            }
        }
        if let Some((i, j)) = best {
            edges.push((s.way, i, j, metres(osm.pos[&s.from], osm.pos[&s.to])));
        }
    }
    let mut runs = group_runs(&edges);
    if runs.len() > 2 {
        let (first, last) = (&runs[0], &runs[runs.len() - 1]);
        if first.way == last.way && last.end == first.start && first.forward() == last.forward() {
            let last = runs.pop().unwrap();
            runs[0].start = last.start;
            runs[0].metres += last.metres;
        }
    }
    runs
}

/// Two legs of a route that do not meet.
#[derive(Clone, Debug, Serialize)]
pub struct Gap {
    /// The route legs (0 is the first `route` line): the gap is after `after` and before
    /// `before`.
    pub after: usize,
    pub before: usize,
    pub what: String,
    /// How far apart they are at their nearest, and the point half way.
    pub metres: f64,
    pub at: [f64; 2],
}

/// A stretch driven against a one-way street.
#[derive(Clone, Debug, Serialize)]
pub struct Against {
    pub way: u64,
    pub name: String,
    pub metres: f64,
    pub at: [f64; 2],
}

/// A very sharp corner of the loop as it will be built.
#[derive(Clone, Debug, Serialize)]
pub struct Corner {
    /// Degrees the road turns within 15 m either side, and its tightest radius there.
    pub turn: f64,
    pub radius: f64,
    /// Metres from the loop's first point, the section, and where.
    pub s: f64,
    pub name: String,
    pub at: [f64; 2],
}

/// A stretch of the route, as `start`, `pad` and `dress ... along` name it.
#[derive(Clone, Debug, Serialize)]
pub struct Stretch {
    pub name: String,
    pub from: f64,
    pub to: f64,
}

/// What [`check`] finds out about a track block's route.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    /// A loop runs through every leg in order.
    pub closed: bool,
    /// Why not, when it does not.
    pub error: Option<String>,
    /// The loop's length as it will be built (corners rounded), metres.
    pub length: f64,
    pub gaps: Vec<Gap>,
    pub oneway: Vec<Against>,
    pub corners: Vec<Corner>,
    /// Problems with the rest of the block: the start, `dress ... along`, landmarks.
    pub problems: Vec<String>,
    /// The loop as exact `route` lines (`way 123 forward from 4 to 5`): one per stretch of
    /// one way.
    pub exact: Vec<String>,
    /// The loop as it will be built, every 4 m or so: latitude, longitude, metres from its
    /// first point (where `s` in `sections` and `corners` counts from).
    pub line: Vec<[f64; 3]>,
    pub sections: Vec<Stretch>,
    /// The city gates in the extract near the loop: (landmark kind, name).
    pub landmarks: Vec<(String, String)>,
}

/// Turns sharper than this within 15 m either side are reported.
const SHARP: f64 = 110.0;

/// Checks a track block's route against an extract, for the circuit designer: whether its
/// legs close a loop (and where they do not meet), how long the loop is, where it drives
/// against a one-way street, its very sharp corners, whether its start, scenery and landmarks
/// are found, and the loop as exact `route way` lines.
pub fn check(osm: &Osm, t: &TrackDef) -> Report {
    let mut r = Report::default();
    let sl = match sub_legs(osm, t) {
        Ok(sl) => sl,
        Err(e) => {
            r.error = Some(e);
            return r;
        }
    };
    let k = sl.ways.len();
    if k < 2 {
        r.error = Some("a route needs at least two legs that close a loop".into());
        return r;
    }
    for i in 0..k {
        let j = (i + 1) % k;
        if !sl.exit[i].is_disjoint(&sl.entry[j]) {
            continue;
        }
        // Where they come closest.
        let (mut m, mut pa, mut pb) = (f64::MAX, (0.0, 0.0), (0.0, 0.0));
        for a in &sl.exit[i] {
            for b in &sl.entry[j] {
                let d = metres(osm.pos[a], osm.pos[b]);
                if d < m {
                    (m, pa, pb) = (d, osm.pos[a], osm.pos[b]);
                }
            }
        }
        r.gaps.push(Gap {
            after: sl.parent[i],
            before: sl.parent[j],
            what: format!("{} and {} do not meet", sl.label[i], sl.label[j]),
            metres: (m * 10.0).round() / 10.0,
            at: [(pa.0 + pb.0) / 2.0, (pa.1 + pb.1) / 2.0],
        });
    }
    if let Some(g) = r.gaps.first() {
        r.error = Some(format!(
            "{} ({:.0} m apart), so the loop breaks there",
            g.what, g.metres
        ));
        return r;
    }
    let sh = match shape(osm, t) {
        Ok(sh) => sh,
        Err(e) => {
            r.error = Some(e);
            return r;
        }
    };
    r.closed = true;
    let tr = &sh.track;
    r.length = (tr.length * 10.0).round() / 10.0;
    let ll = |x: f64, z: f64| {
        let lat = sh.proj.lat0 - z / M;
        let lon = sh.proj.lon0 + x / (M * sh.proj.k);
        [(lat * 1e6).round() / 1e6, (lon * 1e6).round() / 1e6]
    };
    for s in tr.samples.iter().step_by(2) {
        let [a, b] = ll(s.x, s.z);
        r.line.push([a, b, (s.s * 10.0).round() / 10.0]);
    }
    r.sections = tr
        .sections
        .iter()
        .map(|s| Stretch {
            name: s.name.clone(),
            from: s.from,
            to: s.to,
        })
        .collect();
    // The loop, exactly.
    let runs = loop_runs(osm, &sh.steps);
    for run in &runs {
        let w = &osm.ways[run.way];
        r.exact
            .push(leg_name(&Leg::Way(run_span(&w.nodes, w.id, run))));
        let one = oneway(w);
        if (one == 1 && !run.forward()) || (one == -1 && run.forward()) {
            let mid = w.geom[(run.start + run.end) / 2];
            r.oneway.push(Against {
                way: w.id,
                name: w.tag("name").unwrap_or("").to_string(),
                metres: run.metres.round(),
                at: [mid.0, mid.1],
            });
        }
    }
    // Very sharp corners: where the road turns by more than SHARP degrees within 15 m.
    let n = tr.samples.len();
    let kk = ((15.0 / tr.spacing).round() as usize).max(2);
    let turn = |i: usize| {
        let a = &tr.samples[(i + n - kk) % n];
        let b = &tr.samples[(i + kk) % n];
        (a.tx * b.tx + a.tz * b.tz)
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    };
    let radius = |i: usize| {
        let p = |j: usize| {
            let s = &tr.samples[j % n];
            (s.x, s.z)
        };
        let (a, b, c) = (p(i + n - 3), p(i), p(i + 3));
        let (ab, bc, ca) = (
            (b.0 - a.0).hypot(b.1 - a.1),
            (c.0 - b.0).hypot(c.1 - b.1),
            (a.0 - c.0).hypot(a.1 - c.1),
        );
        let cross = ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).abs();
        if cross < 1e-9 {
            f64::INFINITY
        } else {
            ab * bc * ca / (2.0 * cross)
        }
    };
    let sharp: Vec<bool> = (0..n).map(|i| turn(i) > SHARP).collect();
    if sharp.iter().any(|x| !x) {
        let first = (0..n).find(|&i| !sharp[i]).unwrap();
        let mut i = 0;
        while i < n {
            let at = (first + i) % n;
            if !sharp[at] {
                i += 1;
                continue;
            }
            let mut best = (turn(at), radius(at), at);
            while i < n && sharp[(first + i) % n] {
                let j = (first + i) % n;
                let (t2, r2) = (turn(j), radius(j));
                if t2 > best.0 {
                    best = (t2, best.1.min(r2), j);
                } else {
                    best.1 = best.1.min(r2);
                }
                i += 1;
            }
            let s = &tr.samples[best.2];
            r.corners.push(Corner {
                turn: best.0.round(),
                radius: (best.1 * 10.0).round() / 10.0,
                s: (s.s * 10.0).round() / 10.0,
                name: tr
                    .section_at(s.s)
                    .map(|x| x.name.clone())
                    .unwrap_or_default(),
                at: ll(s.x, s.z),
            });
        }
    }
    // The rest of the block.
    if let Err(m) = tr.place(&t.start) {
        r.problems.push(format!("start: {m}"));
    }
    for p in &t.pads {
        if let Err(m) = tr.place(p) {
            r.problems.push(format!("pad: {m}"));
        }
    }
    if let Err(e) = openkart::course::dress(t, tr) {
        r.problems.push(e.msg);
    }
    let route_ways: BTreeSet<u64> = sh.steps.iter().map(|s| osm.ways[s.way].id).collect();
    for spec in &t.landmarks {
        if let Err(m) = landmark(osm, &sh.proj, spec, &route_ways) {
            r.problems.push(m);
        }
    }
    // City gates the loop passes, within 150 m.
    let mut seen = BTreeSet::new();
    let near = |g: &[(f64, f64)]| {
        g.iter().any(|p| {
            let (x, z) = sh.proj.xz(*p);
            tr.samples
                .iter()
                .step_by(4)
                .any(|s| (s.x - x).hypot(s.z - z) < 150.0)
        })
    };
    let things = osm
        .ways
        .iter()
        .filter(|w| w.tag("highway").is_none())
        .map(|w| (&w.geom, &w.tags))
        .chain(osm.things.iter().map(|t| (&t.outline, &t.tags)));
    for (g, tags) in things {
        let (Some(name), Some("city_gate")) =
            (tags.get("name"), tags.get("historic").map(String::as_str))
        else {
            continue;
        };
        if !seen.contains(name) && near(g) {
            seen.insert(name.clone());
            r.landmarks.push(("gate".to_string(), name.clone()));
        }
    }
    r
}

/// A lone `track ... end` block, as the designer writes it, compiled with a stand-in game (its
/// `hazard` lines left out: the stand-in has none).
pub fn track_block(block: &str) -> Result<TrackDef, String> {
    let body: String = block
        .lines()
        .map(|l| {
            let hazard =
                l.starts_with([' ', '\t']) && l.split_whitespace().next() == Some("hazard");
            if hazard {
                "\n".to_string()
            } else {
                format!("{l}\n")
            }
        })
        .collect();
    let text = format!(
        "game designer\n    field 1\nend\nracer r\n    model r.glb\n    kart k.glb\nend\nitem i\n    kind drop\nend\n{body}"
    );
    let defs = openkart::script::compile(&[("track.kart".into(), text)]).map_err(|e| {
        // Lines of the block, not of the stand-in.
        let line = e.line.saturating_sub(10);
        format!("line {line}: {}", e.msg)
    })?;
    defs.tracks
        .into_iter()
        .next()
        .ok_or_else(|| "there is no `track` block".to_string())
}

/// The drivable street graph of an extract, for routing between the points a designer clicks.
pub struct Graph {
    /// Drivable ways: id, nodes, name.
    ways: Vec<(u64, Vec<u64>, String)>,
    pos: BTreeMap<u64, (f64, f64)>,
    /// node -> the pieces of street from it.
    adj: BTreeMap<u64, Vec<Edge>>,
}

/// A piece of street from one node to the next: (next node, way, index of the node on the
/// way, index of the next node, metres, against a one-way street).
type Edge = (u64, usize, usize, usize, f64, bool);

/// A click snapped to the street graph.
#[derive(Clone, Debug, Serialize)]
pub struct Snap {
    pub node: u64,
    pub lat: f64,
    pub lon: f64,
    pub way: u64,
    pub name: String,
    /// From the click to the street, metres.
    pub metres: f64,
}

/// The shortest drive between two nodes, as exact `route way` legs.
#[derive(Clone, Debug, Serialize)]
pub struct Drive {
    pub spans: Vec<WaySpan>,
    pub metres: f64,
    /// Metres of it against one-way streets.
    pub against: f64,
    /// [lat, lon] along it.
    pub line: Vec<[f64; 2]>,
}

impl Graph {
    pub fn new(osm: &Osm) -> Graph {
        let mut g = Graph {
            ways: Vec::new(),
            pos: BTreeMap::new(),
            adj: BTreeMap::new(),
        };
        for w in osm.drivable() {
            // Areas (a pedestrian square drawn as a closed outline) are not streets.
            if w.tag("area") == Some("yes") {
                continue;
            }
            let wi = g.ways.len();
            let one = oneway(w);
            for i in 0..w.nodes.len() - 1 {
                let (a, b) = (w.nodes[i], w.nodes[i + 1]);
                let m = metres(w.geom[i], w.geom[i + 1]);
                g.adj
                    .entry(a)
                    .or_default()
                    .push((b, wi, i, i + 1, m, one == -1));
                g.adj
                    .entry(b)
                    .or_default()
                    .push((a, wi, i + 1, i, m, one == 1));
            }
            for (n, p) in w.nodes.iter().zip(&w.geom) {
                g.pos.insert(*n, *p);
            }
            g.ways.push((
                w.id,
                w.nodes.clone(),
                w.tag("name").unwrap_or("").to_string(),
            ));
        }
        g
    }

    /// The node of a street nearest a click, within `within` metres: the nearer end of the
    /// nearest piece of street.
    pub fn snap(&self, lat: f64, lon: f64, within: f64) -> Option<Snap> {
        let k = lat.to_radians().cos();
        let xz = |p: (f64, f64)| ((p.1 - lon) * M * k, (p.0 - lat) * M);
        let mut best: Option<(f64, u64, usize)> = None;
        for (wi, (_, nodes, _)) in self.ways.iter().enumerate() {
            for p in nodes.windows(2) {
                let (a, b) = (xz(self.pos[&p[0]]), xz(self.pos[&p[1]]));
                let d = seg_dist((0.0, 0.0), a, b);
                if d <= within && best.is_none_or(|x| d < x.0) {
                    let n = if a.0.hypot(a.1) <= b.0.hypot(b.1) {
                        p[0]
                    } else {
                        p[1]
                    };
                    best = Some((d, n, wi));
                }
            }
        }
        let (d, node, wi) = best?;
        let p = self.pos[&node];
        Some(Snap {
            node,
            lat: p.0,
            lon: p.1,
            way: self.ways[wi].0,
            name: self.ways[wi].2.clone(),
            metres: (d * 10.0).round() / 10.0,
        })
    }

    /// Exact legs with the spurs taken out: where one leg drives back along the way the
    /// leg before came (to reach a point clicked on a side street, or on the far side of a
    /// junction), the two are cut back to where they part. On a `closed` loop the last leg
    /// and the first are tidied too.
    pub fn tidy(&self, spans: &[WaySpan], closed: bool) -> Vec<WaySpan> {
        let mut v: Vec<WaySpan> = spans.to_vec();
        loop {
            let n = v.len();
            if n < 2 {
                return v;
            }
            let pairs = if closed { n } else { n - 1 };
            let mut changed = false;
            for i in 0..pairs {
                let j = (i + 1) % n;
                let (a, b) = (&v[i], &v[j]);
                if a.id != b.id || a.forward == b.forward || a.to != b.from {
                    continue;
                }
                let Some(nodes) = self.ways.iter().find(|w| w.0 == a.id).map(|w| &w.1) else {
                    continue;
                };
                let idx = |n: Option<u64>| n.and_then(|n| nodes.iter().position(|x| *x == n));
                let (Some(a0), Some(x), Some(b1)) = (idx(a.from), idx(a.to), idx(b.to)) else {
                    continue;
                };
                // a runs a0 -> x, b runs x -> b1, back the other way.
                let back = a0.abs_diff(x);
                let along = x.abs_diff(b1);
                if along < back {
                    v[i].to = b.to;
                    v.remove(j);
                } else if along > back {
                    v[j].from = a.from;
                    v.remove(i);
                } else {
                    let (hi, lo) = (i.max(j), i.min(j));
                    v.remove(hi);
                    v.remove(lo);
                }
                changed = true;
                break;
            }
            if !changed {
                return v;
            }
        }
    }

    /// The cheapest drive from node `a` to node `b`, as exact `route way` legs. A metre
    /// against a one-way street costs `against` metres (osm2track's own is 4; 1 takes the
    /// streets as closed for the race, both ways alike).
    pub fn drive(&self, a: u64, b: u64, against: f64) -> Option<Drive> {
        let wrong = against;
        if !self.adj.contains_key(&a) || !self.adj.contains_key(&b) {
            return None;
        }
        let mut d: BTreeMap<u64, f64> = BTreeMap::new();
        let mut prev: BTreeMap<u64, (u64, usize)> = BTreeMap::new();
        let mut heap = BinaryHeap::new();
        // The heap orders by cost, then by state: the node goes in `leg`.
        let st = |n: u64| St {
            leg: 0,
            node: n,
            moved: false,
        };
        d.insert(a, 0.0);
        heap.push(Q(0.0, st(a)));
        while let Some(Q(c, u)) = heap.pop() {
            let u = u.node;
            if c > d[&u] {
                continue;
            }
            if u == b {
                break;
            }
            for (ei, e) in self.adj[&u].iter().enumerate() {
                let nc = c + if e.5 { e.4 * wrong } else { e.4 };
                if nc < *d.get(&e.0).unwrap_or(&f64::MAX) {
                    d.insert(e.0, nc);
                    prev.insert(e.0, (u, ei));
                    heap.push(Q(nc, st(e.0)));
                }
            }
        }
        if a != b && !prev.contains_key(&b) {
            return None;
        }
        let mut edges = Vec::new();
        let mut against = 0.0;
        let mut u = b;
        while u != a {
            let (p, ei) = prev[&u];
            let e = self.adj[&p][ei];
            edges.push((e.1, e.2, e.3, e.4));
            if e.5 {
                against += e.4;
            }
            u = p;
        }
        edges.reverse();
        let runs = group_runs(&edges);
        let mut line = vec![[self.pos[&a].0, self.pos[&a].1]];
        for &(w, _, j, _) in &edges {
            let n = self.ways[w].1[j];
            line.push([self.pos[&n].0, self.pos[&n].1]);
        }
        Some(Drive {
            spans: runs
                .iter()
                .map(|r| run_span(&self.ways[r.way].1, self.ways[r.way].0, r))
                .collect(),
            metres: edges.iter().map(|e| e.3).sum::<f64>().round(),
            against: against.round(),
            line,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 200 x 120 m block (nodes 1-4 at the corners, 5 half way along the north side) and its
    /// four streets, two of them one-way; one building beside the south street.
    fn extract() -> Osm {
        let (lat0, lon0): (f64, f64) = (44.49, 11.36);
        let k = lat0.to_radians().cos();
        let ll = |x: f64, z: f64| (lat0 - z / M, lon0 + x / (M * k));
        let pts = [
            (1, 0.0, 0.0),
            (2, 200.0, 0.0),
            (3, 200.0, -120.0),
            (5, 100.0, -120.0),
            (4, 0.0, -120.0),
            (10, 40.0, 7.0),
            (11, 80.0, 7.0),
            (12, 80.0, 30.0),
            (13, 40.0, 30.0),
            (30, 0.0, 300.0),
            (31, 300.0, 300.0),
            (32, 300.0, 318.0),
            (33, 0.0, 318.0),
            (40, 500.0, 0.0),
            (41, 520.0, 0.0),
            (42, 520.0, 20.0),
            (43, 500.0, 20.0),
        ];
        let node = |id: u64| {
            let (_, x, z) = pts.iter().find(|p| p.0 == id).unwrap();
            let (lat, lon) = ll(*x, *z);
            serde_json::json!({ "lat": lat, "lon": lon })
        };
        let way = |id: u64, nodes: &[u64], tags: serde_json::Value| {
            serde_json::json!({ "type": "way", "id": id, "nodes": nodes,
                "geometry": nodes.iter().map(|n| node(*n)).collect::<Vec<_>>(), "tags": tags })
        };
        let doc = serde_json::json!({ "elements": [
            way(100, &[1, 2], serde_json::json!({ "highway": "primary", "name": "Via Sud", "lanes": "2" })),
            way(101, &[2, 3], serde_json::json!({ "highway": "residential", "name": "Via Est" })),
            way(102, &[3, 5, 4], serde_json::json!({ "highway": "service", "oneway": "yes" })),
            way(103, &[1, 4], serde_json::json!({ "highway": "tertiary", "name": "Via Ovest" })),
            way(200, &[10, 11, 12, 13, 10], serde_json::json!({ "building": "yes", "building:levels": "3" })),
            // A long thin block far to the south: a hairpin at either end.
            way(110, &[30, 31], serde_json::json!({ "highway": "residential", "name": "Via Lunga" })),
            way(111, &[31, 32], serde_json::json!({ "highway": "residential" })),
            way(112, &[32, 33], serde_json::json!({ "highway": "residential", "name": "Via Lunga" })),
            way(113, &[33, 30], serde_json::json!({ "highway": "residential" })),
            // A roundabout, away to the east.
            way(120, &[40, 41, 42, 43, 40], serde_json::json!({ "highway": "tertiary", "junction": "roundabout" })),
        ]});
        Osm::parse(&[("test.json".into(), doc.to_string())]).unwrap()
    }

    fn track_def(route: &str) -> TrackDef {
        let text = format!(
            "game g\n    field 2\nend\nracer a\n    model a.glb\n    kart k.glb\nend\nracer b\n    model a.glb\n    kart k.glb\nend\nitem i\n    kind drop\nend\ntrack t\n    osm t.track\n{route}\n    start Via Sud 0.5\n    dress tree t.glb every 20 side right scale 2 3 face random along Via Sud\nend\n"
        );
        openkart::script::compile(&[("t.kart".into(), text)])
            .unwrap()
            .tracks
            .remove(0)
    }

    #[test]
    fn streets_and_way_spans_make_the_same_loop() {
        let osm = extract();
        let by_name = track_def(
            "    route Via Sud\n    route Via Est\n    route ways 102\n    route Via Ovest",
        );
        let a = build(&osm, &by_name, 8).unwrap();
        assert!((a.length - 640.0).abs() < 25.0, "{}", a.length);
        // Via Sud twice: the start line is half way along it.
        assert_eq!(a.sections.len(), 5);
        // The exact form, as a designer writes it: the north street only from node 3 to 4.
        let exact = track_def(
            "    route way 100\n    route way 101 forward\n    route way 102 from 3 to 4\n    route way 103 backward",
        );
        let b = build(&osm, &exact, 8).unwrap();
        assert!((a.length - b.length).abs() < 1.0);
        // Against the way's nodes it cannot close.
        let wrong = track_def("    route way 100\n    route way 101 backward\n    route ways 102\n    route Via Ovest");
        assert!(build(&osm, &wrong, 8).unwrap_err().contains("no loop"));
        let bad =
            track_def("    route way 100\n    route way 102 from 9 to 4\n    route Via Ovest");
        assert!(build(&osm, &bad, 8).unwrap_err().contains("node 9"));
        // The building beside Via Sud is kept, with its front as a wall; the trees are baked.
        assert_eq!(a.buildings.len(), 1);
        assert!(a.samples.iter().any(|s| s.right < s.w / 2.0 + 5.0));
        assert!(!a.props.is_empty() && a.props.iter().all(|p| p.scale >= 2.0 && p.scale <= 3.0));
        let back = Track::parse("t.track", &a.to_text()).unwrap();
        assert_eq!(back.props.len(), a.props.len());
    }

    #[test]
    fn the_check_gives_the_exact_loop_and_it_builds_the_same() {
        let osm = extract();
        let by_name = track_def(
            "    route Via Sud\n    route Via Est\n    route ways 102\n    route Via Ovest",
        );
        let r = check(&osm, &by_name);
        assert!(r.closed && r.error.is_none(), "{r:?}");
        assert!((r.length - 640.0).abs() < 25.0, "{}", r.length);
        assert!(r.gaps.is_empty() && r.oneway.is_empty() && r.problems.is_empty());
        // Four ways, four exact lines, the right way round the one-way street.
        assert_eq!(r.exact.len(), 4, "{:?}", r.exact);
        assert!(r.exact.contains(&"way 102 forward from 3 to 4".to_string()));
        assert!(r
            .exact
            .contains(&"way 103 backward from 4 to 1".to_string()));
        let lines: String = r.exact.iter().map(|l| format!("    route {l}\n")).collect();
        let exact = track_def(lines.trim_end());
        let a = build(&osm, &by_name, 8).unwrap();
        let b = build(&osm, &exact, 8).unwrap();
        assert!(
            (a.length - b.length).abs() < 0.5,
            "{} {}",
            a.length,
            b.length
        );
        assert_eq!(check(&osm, &exact).exact, r.exact);
        // The line to draw goes round the block, and the sections name its streets.
        assert!(r.line.len() > 100);
        assert!(r.sections.iter().any(|s| s.name == "Via Est"));
        // A rectangle's square corners are not very sharp.
        assert!(r.corners.is_empty(), "{:?}", r.corners);
    }

    #[test]
    fn the_check_finds_gaps_wrong_ways_hairpins_and_bad_places() {
        let osm = extract();
        // Via Sud and the north street share no node.
        let gap = check(
            &osm,
            &track_def("    route way 100\n    route way 102\n    route Via Ovest"),
        );
        assert!(!gap.closed);
        assert_eq!(gap.gaps.len(), 1);
        assert_eq!((gap.gaps[0].after, gap.gaps[0].before), (0, 1));
        assert!((gap.gaps[0].metres - 120.0).abs() < 1.0, "{:?}", gap.gaps);
        assert!(gap.error.unwrap().contains("do not meet"));
        // Round the block the other way: against the one-way street.
        let back = check(
            &osm,
            &track_def("    route way 100 backward\n    route way 103 forward\n    route way 102 backward\n    route way 101 backward"),
        );
        assert!(back.closed, "{back:?}");
        assert_eq!(back.oneway.len(), 1);
        assert_eq!(back.oneway[0].way, 102);
        assert!((back.oneway[0].metres - 200.0).abs() < 1.0);
        // The long thin block turns back on itself at both ends. Its start is not on it.
        let thin = check(&osm, &track_def("    route ways 110 111 112 113"));
        assert!(thin.closed, "{thin:?}");
        assert_eq!(thin.corners.len(), 2, "{:?}", thin.corners);
        assert!(thin
            .corners
            .iter()
            .all(|c| c.turn > 110.0 && c.radius < 15.0));
        assert!(thin.problems.iter().any(|p| p.starts_with("start:")));
        assert!(thin.problems.iter().any(|p| p.contains("Via Sud")));
    }

    #[test]
    fn the_graph_snaps_clicks_and_drives_the_right_way_round() {
        let osm = extract();
        let g = Graph::new(&osm);
        // A click 6 m off Via Est, near its north end, snaps to node 3.
        let (lat0, lon0): (f64, f64) = (44.49, 11.36);
        let k = lat0.to_radians().cos();
        let s = g
            .snap(lat0 + 110.0 / M, lon0 + 206.0 / (M * k), 30.0)
            .unwrap();
        assert_eq!((s.node, s.way), (3, 101));
        assert!((s.metres - 6.0).abs() < 0.5);
        assert!(g
            .snap(lat0 - 200.0 / M, lon0 + 100.0 / (M * k), 30.0)
            .is_none());
        // Node 1 to 3: along Via Sud and Via Est.
        let d = g.drive(1, 3, 4.0).unwrap();
        let lines: Vec<String> = d
            .spans
            .iter()
            .map(|s| leg_name(&Leg::Way(s.clone())))
            .collect();
        assert_eq!(
            lines,
            ["way 100 forward from 1 to 2", "way 101 forward from 2 to 3"]
        );
        assert!((d.metres - 320.0).abs() < 1.0 && d.against == 0.0);
        // 4 to 3 the long way round rather than 200 m against the one-way street...
        let d = g.drive(4, 3, 4.0).unwrap();
        assert_eq!(d.spans.len(), 3);
        assert_eq!(d.against, 0.0);
        // ...but 100 m against it beats 540 m round.
        let d = g.drive(5, 3, 4.0).unwrap();
        assert_eq!(d.spans.len(), 1);
        assert_eq!(d.against, 100.0);
        assert_eq!(d.line.len(), 2);
        // With the streets closed for the race, 200 m against it beats 440 m round.
        let d = g.drive(4, 3, 1.0).unwrap();
        assert_eq!((d.spans.len(), d.against), (1, 200.0));
        assert!(g.drive(1, 30, 4.0).is_none());
        // Out along Via Sud and straight back: all spur, nothing is left.
        let there = g.drive(1, 2, 4.0).unwrap().spans;
        let back = g.drive(2, 1, 4.0).unwrap().spans;
        let all: Vec<WaySpan> = there.iter().chain(&back).cloned().collect();
        assert!(g.tidy(&all, false).is_empty());
        // Up Via Est and back, then home along Via Sud, as a closed loop: the spur up Via Est
        // goes, then the out-and-back along Via Sud that is left.
        let a = g.drive(1, 3, 4.0).unwrap().spans;
        let b = g.drive(3, 2, 4.0).unwrap().spans;
        let mut legs: Vec<WaySpan> = a.iter().chain(&b).cloned().collect();
        legs.push(WaySpan {
            id: 100,
            forward: Some(false),
            from: Some(2),
            to: Some(1),
        });
        let t = g.tidy(&legs, true);
        assert!(t.is_empty(), "{t:?}");
        // Along the north street from 3 to 4, then back to 5: one leg, from 3 to 5.
        let legs = vec![
            WaySpan {
                id: 102,
                forward: Some(true),
                from: Some(3),
                to: Some(4),
            },
            WaySpan {
                id: 102,
                forward: Some(false),
                from: Some(4),
                to: Some(5),
            },
        ];
        let t = g.tidy(&legs, false);
        assert_eq!(
            t,
            vec![WaySpan {
                id: 102,
                forward: Some(true),
                from: Some(3),
                to: Some(5)
            }]
        );
    }

    #[test]
    fn spans_on_a_roundabout_read_the_way_they_go() {
        let osm = extract();
        let w = osm.ways.iter().find(|w| w.id == 120).unwrap();
        let span = |forward, from, to| WaySpan {
            id: 120,
            forward: Some(forward),
            from: Some(from),
            to: Some(to),
        };
        // Node 40 is both the first node and the last.
        assert_eq!(
            span_range(w, &span(true, 42, 40)).unwrap(),
            (2, 4, Some(true))
        );
        assert_eq!(
            span_range(w, &span(true, 40, 41)).unwrap(),
            (0, 1, Some(true))
        );
        assert_eq!(
            span_range(w, &span(false, 41, 40)).unwrap(),
            (0, 1, Some(false))
        );
        assert_eq!(
            span_range(w, &span(false, 40, 43)).unwrap(),
            (3, 4, Some(false))
        );
        assert!(span_range(w, &span(false, 41, 42)).is_err());
        // A drive round it, from 42 past 40 to 41, is two legs that meet at 40.
        let g = Graph::new(&osm);
        let d = g.drive(42, 41, 4.0).unwrap();
        let lines: Vec<String> = d
            .spans
            .iter()
            .map(|s| leg_name(&Leg::Way(s.clone())))
            .collect();
        assert_eq!(
            lines,
            [
                "way 120 forward from 42 to 40",
                "way 120 forward from 40 to 41"
            ]
        );
    }

    #[test]
    fn a_designer_block_compiles_alone() {
        let t = track_block("track mine\n    osm mine.track\n    route way 100 forward from 1 to 2\n    route Via Est\n    hazard ambulance\nend\n").unwrap();
        assert_eq!(t.id, "mine");
        assert_eq!(t.route.len(), 2);
        let e = track_block("track mine\n    osm mine.track\n    route way x\nend\n").unwrap_err();
        assert!(e.starts_with("line 3:"), "{e}");
    }
}
