//! A track made ready for racing: where the scenery stands, the rows of picnic baskets, the
//! boost pads, the hazards' crossings and how fast each stretch of the racing line can be
//! taken. Worked out once, from the track file and the `track` block.

use serde::Serialize;

use crate::script::{Defs, Error, Side, TrackDef};
use crate::track::Track;

/// A piece of scenery from a `dress` line.
#[derive(Clone, Debug, Serialize)]
pub struct Prop {
    pub kind: String,
    pub model: String,
    pub x: f64,
    pub z: f64,
    /// The road's direction there.
    pub tx: f64,
    pub tz: f64,
}

/// A boost pad: on the racing line, across the road.
#[derive(Clone, Debug, Serialize)]
pub struct Pad {
    pub s: f64,
    pub d: f64,
    pub x: f64,
    pub z: f64,
    pub tx: f64,
    pub tz: f64,
}

/// A picnic basket's place.
#[derive(Clone, Debug, Serialize)]
pub struct BasketSpot {
    pub s: f64,
    pub d: f64,
    pub x: f64,
    pub z: f64,
}

/// Where a hazard crosses the road.
#[derive(Clone, Debug, Serialize)]
pub struct Crossing {
    pub hazard: usize,
    pub id: String,
    pub s: f64,
    /// From `-right - margin` to `left + margin`, metres left of the centre.
    pub from: f64,
    pub to: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Course {
    pub id: String,
    pub track: Track,
    pub laps: u32,
    pub props: Vec<Prop>,
    pub pads: Vec<Pad>,
    pub baskets: Vec<BasketSpot>,
    pub crossings: Vec<Crossing>,
    /// Curvature of the racing line at each sample (1/m, left positive).
    #[serde(skip)]
    pub curv: Vec<f64>,
}

impl Course {
    pub fn new(defs: &Defs, def: &TrackDef, track: Track) -> Result<Course, Error> {
        let at = |no: usize, m: String| Error::new(&def.file, no, format!("track {}: {m}", def.id));
        let mut props = Vec::new();
        for d in &def.dress {
            let sections: Vec<(f64, f64)> = match &d.along {
                Some(name) => {
                    let v: Vec<(f64, f64)> = track
                        .sections
                        .iter()
                        .filter(|s| s.name == *name)
                        .map(|s| (s.from, s.to))
                        .collect();
                    if v.is_empty() {
                        let names: Vec<&str> =
                            track.sections.iter().map(|s| s.name.as_str()).collect();
                        return Err(at(
                            d.line,
                            format!(
                                "dress along {name:?}: the route has no such street (it has {})",
                                names.join(", ")
                            ),
                        ));
                    }
                    v
                }
                None => vec![(0.0, track.length)],
            };
            for (from, to) in sections {
                let mut s = from + d.every / 2.0;
                while s < to {
                    // Keep the start line clear.
                    if s > 15.0 && s < track.length - 25.0 {
                        let f = track.frame(s);
                        for side in [1.0, -1.0] {
                            let wanted = match d.side {
                                Side::Both => true,
                                Side::Left => side > 0.0,
                                Side::Right => side < 0.0,
                            };
                            let wall = if side > 0.0 { f.left } else { f.right };
                            let off = f.w / 2.0 + d.offset;
                            // Only where the pavement is wide enough (no building in the way).
                            if wanted && off + 0.8 < wall {
                                let dd = side * off;
                                props.push(Prop {
                                    kind: d.kind.clone(),
                                    model: d.model.clone(),
                                    x: f.x + f.tz * dd,
                                    z: f.z - f.tx * dd,
                                    tx: f.tx,
                                    tz: f.tz,
                                });
                            }
                        }
                    }
                    s += d.every;
                }
            }
        }
        let mut pads = Vec::new();
        for (p, no) in def.pads.iter().zip(&def.pad_lines) {
            let s = track.place(p).map_err(|m| at(*no, format!("pad: {m}")))?;
            let f = track.frame(s);
            pads.push(Pad {
                s,
                d: f.line,
                x: f.x + f.tz * f.line,
                z: f.z - f.tx * f.line,
                tx: f.tx,
                tz: f.tz,
            });
        }
        let mut baskets = Vec::new();
        if def.baskets_every > 0.0 {
            let mut s = def.baskets_first;
            while s < track.length - 30.0 {
                let f = track.frame(s);
                let n = def.baskets_across.max(1);
                let span = (f.w - 2.5).max(1.0);
                for k in 0..n {
                    let d = if n == 1 {
                        0.0
                    } else {
                        -span / 2.0 + span * k as f64 / (n - 1) as f64
                    };
                    baskets.push(BasketSpot {
                        s,
                        d,
                        x: f.x + f.tz * d,
                        z: f.z - f.tx * d,
                    });
                }
                s += def.baskets_every;
            }
        }
        let mut crossings = Vec::new();
        for id in &def.hazards {
            let hi = defs.hazard(id).expect("checked by the script");
            let h = &defs.hazards[hi];
            let s = track
                .place(&h.at)
                .map_err(|m| Error::new(&h.file, h.no, format!("hazard {}: {m}", h.id)))?;
            let f = track.frame(s);
            crossings.push(Crossing {
                hazard: hi,
                id: h.id.clone(),
                s,
                from: -f.right - h.margin,
                to: f.left + h.margin,
            });
        }
        let curv = track.line_curvature();
        if track.grid.len() < defs.game.field {
            return Err(at(
                def.no,
                format!(
                    "the track file has {} grid slots but a race has {} racers",
                    track.grid.len(),
                    defs.game.field
                ),
            ));
        }
        Ok(Course {
            id: def.id.clone(),
            track,
            laps: def.laps,
            props,
            pads,
            baskets,
            crossings,
            curv,
        })
    }
}
