//! The race. A fixed step of 1/60 s; every step reads one [`Input`] per human player. Computer
//! racers, baskets and hazards follow the seed, so a race is `{ setup, inputs }` and
//! [`Race::replay`] plays it again exactly (see [`crate::det`]).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::course::Course;
use crate::det::{turn, Rng};
use crate::script::{Effect, ItemKind, Physics};
use crate::track::{norm, Loc};
use crate::Game;

/// Steps per second.
pub const HZ: u32 = 60;
pub const DT: f64 = 1.0 / HZ as f64;

/// One player's controls for one step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    /// -100 (full right) to 100 (full left).
    #[serde(default)]
    pub steer: i8,
    #[serde(default)]
    pub gas: bool,
    #[serde(default)]
    pub brake: bool,
    #[serde(default)]
    pub drift: bool,
    #[serde(default)]
    pub item: bool,
}

/// A racer on the grid: which, and whether a human (the `n`th input of every step) drives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub racer: String,
    #[serde(default)]
    pub human: bool,
}

/// How a race starts: the seed, the track and the grid (first entry in the first slot).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    pub seed: String,
    pub track: String,
    pub entries: Vec<Entry>,
}

impl Setup {
    /// A race of the game's first `field` racers, all driven by the computer.
    pub fn computer(seed: &str, game: &Game, track: &str) -> Setup {
        Setup {
            seed: seed.into(),
            track: track.into(),
            entries: game
                .defs
                .racers
                .iter()
                .take(game.defs.game.field)
                .map(|r| Entry {
                    racer: r.id.clone(),
                    human: false,
                })
                .collect(),
        }
    }
}

/// A saved race: the setup and every step's inputs (one per human).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Replay {
    pub setup: Setup,
    pub inputs: Vec<Vec<Input>>,
}

/// Something that happened in a step, for the client's sounds and effects.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    /// The countdown shows a number (3, 2, 1) or 0: go.
    Count {
        n: u32,
    },
    Pickup {
        kart: usize,
    },
    Use {
        kart: usize,
        item: String,
    },
    Hit {
        kart: usize,
        by: String,
        effect: Effect,
    },
    Boost {
        kart: usize,
        tier: u8,
    },
    Pad {
        kart: usize,
    },
    Wall {
        kart: usize,
        hard: bool,
    },
    Bump {
        a: usize,
        b: usize,
    },
    Lap {
        kart: usize,
        lap: u32,
    },
    Finish {
        kart: usize,
        place: usize,
        time: f64,
    },
    Over,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Countdown,
    Race,
    Over,
}

/// A racer's numbers, from its stats and the physics.
#[derive(Clone, Debug)]
struct Stats {
    top: f64,
    accel: f64,
    turn: f64,
    grip: f64,
    mass: f64,
    skill: f64,
}

#[derive(Clone, Debug, Default)]
struct Ai {
    lane: f64,
    lane_to: f64,
    lane_t: f64,
    hold: f64,
    stuck: f64,
    reverse: f64,
    best: f64,
    best_t: f64,
}

#[derive(Clone, Debug)]
pub struct Kart {
    pub racer: usize,
    /// Which input drives it, while it races.
    pub human: Option<usize>,
    pub x: f64,
    pub z: f64,
    pub hx: f64,
    pub hz: f64,
    pub vx: f64,
    pub vz: f64,
    pub loc: Loc,
    /// Metres along the track since the start line, laps included (negative on the grid).
    pub dist: f64,
    pub lap: u32,
    pub next_cp: usize,
    pub finish: Option<u64>,
    pub place: usize,
    pub lap_ticks: Vec<u64>,
    /// -1, 0 or 1: drifting right or left.
    pub drift: i8,
    pub charge: f64,
    drift_ready: bool,
    pub boost: f64,
    boost_power: f64,
    pub spin: f64,
    spin_dir: f64,
    pub splat: f64,
    splat_power: f64,
    pub bump: f64,
    immune: f64,
    pub item: Option<usize>,
    pub unpack: f64,
    prev: Input,
    pub off: bool,
    wall_t: f64,
    stats: Stats,
    ai: Ai,
}

impl Kart {
    /// Forward speed.
    pub fn speed(&self) -> f64 {
        self.vx * self.hx + self.vz * self.hz
    }
}

#[derive(Clone, Debug)]
pub struct Shot {
    pub item: usize,
    pub owner: usize,
    pub x: f64,
    pub z: f64,
    pub vx: f64,
    pub vz: f64,
    pub loc: Loc,
    pub age: f64,
    pub aiming: f64,
    pub target: Option<usize>,
    pub hits: u32,
}

#[derive(Clone, Debug)]
pub struct Slick {
    pub item: usize,
    pub owner: usize,
    pub x: f64,
    pub z: f64,
    pub age: f64,
    pub hits: u32,
}

/// What the client draws each step.
#[derive(Clone, Debug, Serialize)]
pub struct View {
    pub tick: u64,
    /// Seconds since the start (negative in the countdown).
    pub time: f64,
    pub phase: Phase,
    pub karts: Vec<KartView>,
    pub shots: Vec<ThingView>,
    pub slicks: Vec<ThingView>,
    /// Whether each basket (in the course's order) is there.
    pub baskets: Vec<bool>,
    pub hazards: Vec<HazardView>,
    /// Kart indices, first place first.
    pub order: Vec<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct KartView {
    pub racer: String,
    pub human: bool,
    pub x: f64,
    pub z: f64,
    pub hx: f64,
    pub hz: f64,
    pub speed: f64,
    pub s: f64,
    pub d: f64,
    /// The lap being driven (1 to laps), or laps when finished.
    pub lap: u32,
    pub place: usize,
    pub item: Option<String>,
    pub unpacking: bool,
    pub drift: i8,
    /// 0, 1 or 2: the boost a drift released now would give.
    pub tier: u8,
    pub boost: bool,
    pub spin: bool,
    pub splat: f64,
    pub bump: bool,
    pub off: bool,
    /// Race time at the finish, in seconds.
    pub time: Option<f64>,
    pub laps: Vec<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ThingView {
    pub item: String,
    pub x: f64,
    pub z: f64,
    pub vx: f64,
    pub vz: f64,
    pub aiming: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct HazardView {
    pub id: String,
    pub x: f64,
    pub z: f64,
    pub hx: f64,
    pub hz: f64,
    pub moving: bool,
}

/// The final order.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Standing {
    pub place: usize,
    pub kart: usize,
    pub racer: String,
    /// Seconds, or `None` for a racer that did not finish.
    pub time: Option<f64>,
    pub best_lap: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Race {
    pub game: Arc<Game>,
    pub course: Arc<Course>,
    pub setup: Setup,
    pub tick: u64,
    pub karts: Vec<Kart>,
    pub shots: Vec<Shot>,
    pub slicks: Vec<Slick>,
    /// The tick each basket is back (0: it is there).
    pub baskets: Vec<u64>,
    /// Every step's human inputs, for [`Replay`].
    pub inputs: Vec<Vec<Input>>,
    pub events: Vec<Event>,
    humans: usize,
    rng: Rng,
    start_tick: u64,
    over: bool,
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

fn sign(v: f64) -> f64 {
    if v < 0.0 {
        -1.0
    } else {
        1.0
    }
}

impl Race {
    pub fn new(game: Arc<Game>, setup: &Setup) -> Result<Race, String> {
        let (_, course) = game
            .course(&setup.track)
            .ok_or_else(|| format!("there is no track {:?}", setup.track))?;
        let course = course.clone();
        let tr = &course.track;
        if setup.entries.is_empty() {
            return Err("a race needs at least one racer".into());
        }
        if setup.entries.len() > tr.grid.len() {
            return Err(format!(
                "the track has {} grid slots, not {}",
                tr.grid.len(),
                setup.entries.len()
            ));
        }
        let p = &game.defs.physics;
        let mut rng = Rng::new(&setup.seed);
        let mut karts = Vec::new();
        let mut humans = 0;
        for (slot, e) in setup.entries.iter().enumerate() {
            let ri = game
                .defs
                .racer(&e.racer)
                .ok_or_else(|| format!("there is no racer {:?}", e.racer))?;
            let r = &game.defs.racers[ri];
            let (behind, left) = tr.grid[slot];
            let s = tr.wrap(-behind);
            let f = tr.frame(s);
            let (x, z) = tr.point(s, left);
            let human = if e.human {
                humans += 1;
                Some(humans - 1)
            } else {
                None
            };
            let stats = Stats {
                top: p.top * (1.0 + 0.035 * (r.speed - 3.0) + 0.01 * (r.weight - 3.0)),
                accel: p.accel * (1.0 + 0.09 * (r.accel - 3.0) - 0.02 * (r.weight - 3.0)),
                turn: p.turn * (1.0 + 0.05 * (r.grip - 3.0)),
                grip: p.grip * (1.0 + 0.1 * (r.grip - 3.0)),
                mass: 1.0 + 0.15 * (r.weight - 3.0),
                skill: r.skill,
            };
            let lane = rng.range(-1.0, 1.0);
            karts.push(Kart {
                racer: ri,
                human,
                x,
                z,
                hx: f.tx,
                hz: f.tz,
                vx: 0.0,
                vz: 0.0,
                loc: tr.locate(x, z, None),
                dist: -behind,
                lap: 0,
                next_cp: 0,
                finish: None,
                place: slot,
                lap_ticks: Vec::new(),
                drift: 0,
                charge: 0.0,
                drift_ready: true,
                boost: 0.0,
                boost_power: 1.0,
                spin: 0.0,
                spin_dir: 1.0,
                splat: 0.0,
                splat_power: 1.0,
                bump: 0.0,
                immune: 0.0,
                item: None,
                unpack: 0.0,
                prev: Input::default(),
                off: false,
                wall_t: 0.0,
                stats,
                ai: Ai {
                    lane,
                    lane_to: lane,
                    lane_t: rng.range(1.0, 4.0),
                    best: -behind,
                    ..Ai::default()
                },
            });
        }
        let start_tick = (p.countdown * HZ as f64).round() as u64;
        let n_baskets = course.baskets.len();
        Ok(Race {
            game,
            course,
            setup: setup.clone(),
            tick: 0,
            karts,
            shots: Vec::new(),
            slicks: Vec::new(),
            baskets: vec![0; n_baskets],
            inputs: Vec::new(),
            events: Vec::new(),
            humans,
            rng,
            start_tick,
            over: false,
        })
    }

    /// Plays a saved race again.
    pub fn replay(game: Arc<Game>, r: &Replay) -> Result<Race, String> {
        let mut race = Race::new(game, &r.setup)?;
        for i in &r.inputs {
            race.step(i);
        }
        Ok(race)
    }

    pub fn save(&self) -> Replay {
        Replay {
            setup: self.setup.clone(),
            inputs: self.inputs.clone(),
        }
    }

    pub fn humans(&self) -> usize {
        self.humans
    }

    pub fn over(&self) -> bool {
        self.over
    }

    pub fn phase(&self) -> Phase {
        if self.over {
            Phase::Over
        } else if self.tick < self.start_tick {
            Phase::Countdown
        } else {
            Phase::Race
        }
    }

    /// Seconds since the start.
    pub fn time(&self) -> f64 {
        (self.tick as f64 - self.start_tick as f64) * DT
    }

    fn physics(&self) -> &Physics {
        &self.game.defs.physics
    }

    /// One step. `inputs` has one entry per human (missing ones count as no input).
    pub fn step(&mut self, inputs: &[Input]) {
        let mut logged: Vec<Input> = inputs.iter().copied().take(self.humans).collect();
        logged.resize(self.humans, Input::default());
        self.inputs.push(logged.clone());
        if self.over {
            self.tick += 1;
            return;
        }
        // The countdown.
        if self.tick < self.start_tick {
            let left = self.start_tick - self.tick;
            if left % HZ as u64 == 0 {
                self.events.push(Event::Count {
                    n: (left / HZ as u64) as u32,
                });
            }
        } else if self.tick == self.start_tick {
            self.events.push(Event::Count { n: 0 });
        }
        let racing = self.tick >= self.start_tick;
        let n = self.karts.len();
        // Controls.
        let mut ctl: Vec<Control> = Vec::with_capacity(n);
        for k in 0..n {
            let c = match self.karts[k].human {
                Some(h) if self.karts[k].finish.is_none() => Control::from(logged[h]),
                _ => self.ai(k),
            };
            ctl.push(c);
        }
        if racing {
            for (k, c) in ctl.iter().enumerate() {
                self.drive(k, c);
            }
            self.collide();
            self.items(&ctl);
            self.baskets();
            self.hazards();
            self.progress();
        }
        for (k, c) in ctl.iter().enumerate() {
            self.karts[k].prev = c.raw;
        }
        self.tick += 1;
        let limit = (self.physics().time_limit * HZ as f64) as u64 + self.start_tick;
        if self.karts.iter().all(|k| k.finish.is_some()) || self.tick >= limit {
            self.over = true;
            self.events.push(Event::Over);
        }
    }

    /// Takes the events since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    // ---------- driving ----------

    fn drive(&mut self, k: usize, c: &Control) {
        let p = self.physics().clone();
        let tr = &self.course.track;
        let catchup = self.catchup(k);
        let kart = &mut self.karts[k];
        let st = kart.stats.clone();
        let f = tr.frame(kart.loc.s);
        kart.off = kart.loc.d.abs() > f.w / 2.0 + 0.4;
        let mut top = st.top * catchup;
        if kart.off && kart.boost <= 0.0 {
            top *= p.offroad;
        }
        if kart.boost > 0.0 {
            top *= kart.boost_power;
        }
        if kart.splat > 0.0 {
            top *= kart.splat_power;
        }
        let (lx, lz) = (kart.hz, -kart.hx);
        let mut fwd = kart.vx * kart.hx + kart.vz * kart.hz;
        let mut lat = kart.vx * lx + kart.vz * lz;
        let mut steer = c.steer;
        let mut gas = c.gas;
        let mut brake = c.brake;
        if kart.spin > 0.0 {
            steer = 0.0;
            gas = false;
            brake = false;
            kart.drift = 0;
            let (hx, hz) = turn(kart.hx, kart.hz, kart.spin_dir * 13.0 * DT);
            kart.hx = hx;
            kart.hz = hz;
            fwd *= 1.0 - 1.8 * DT;
        } else if kart.bump > 0.0 {
            gas = false;
            fwd *= 1.0 - 1.2 * DT;
        }
        // Along the kart.
        if gas {
            if fwd < top {
                let room = (1.0 - fwd / top).max(0.12);
                let a = if kart.boost > 0.0 {
                    st.accel * 2.5
                } else {
                    st.accel
                };
                fwd = (fwd + a * room * DT).min(top.max(fwd));
            }
        } else if brake {
            if fwd > 0.5 {
                fwd -= p.brake * DT;
            } else {
                fwd = (fwd - st.accel * 0.6 * DT).max(-top * 0.35);
            }
        } else {
            fwd -= fwd * p.coast * DT;
        }
        if fwd > top {
            fwd -= (fwd - top) * 1.6 * DT;
        }
        // Steering, and the drift.
        let speed_f = (fwd.abs() / 6.0).min(1.0);
        let high = 1.0 - 0.3 * (fwd.abs() / p.top).min(1.3);
        let mut yaw = steer * st.turn * speed_f * high * sign(fwd);
        if !c.drift {
            kart.drift_ready = true;
        }
        if kart.drift == 0 {
            if c.drift
                && kart.drift_ready
                && steer.abs() > 0.25
                && fwd > 12.0
                && kart.spin <= 0.0
                && kart.bump <= 0.0
            {
                kart.drift = if steer > 0.0 { 1 } else { -1 };
                kart.charge = 0.0;
                kart.drift_ready = false;
            }
        } else if !c.drift || fwd < 8.0 {
            if c.drift {
                // Too slow: the drift dies with nothing.
                kart.charge = 0.0;
            }
            let tier = if kart.charge >= p.drift_charge.1 {
                2
            } else if kart.charge >= p.drift_charge.0 {
                1
            } else {
                0
            };
            if tier > 0 {
                let t = if tier == 2 {
                    p.drift_boost.1
                } else {
                    p.drift_boost.0
                };
                kart.boost = kart.boost.max(t);
                kart.boost_power = p.boost_power;
                self.events.push(Event::Boost { kart: k, tier });
            }
            kart.drift = 0;
            kart.charge = 0.0;
        }
        if kart.drift != 0 {
            let dir = kart.drift as f64;
            let into = clamp(steer * dir, -1.0, 1.0);
            yaw = dir * st.turn * (0.72 + 0.48 * into) * speed_f;
            kart.charge += DT * (0.7 + 0.5 * into.max(0.0));
        }
        let (hx, hz) = turn(kart.hx, kart.hz, yaw * DT);
        kart.hx = hx;
        kart.hz = hz;
        // Sideways: grip pulls the travel back under the nose; a drift lets it slide out.
        let (lx, lz) = (kart.hz, -kart.hx);
        let (slide, mut g) = if kart.drift != 0 {
            (-(kart.drift as f64) * 0.22 * fwd, p.drift_grip)
        } else {
            (0.0, st.grip)
        };
        if kart.off {
            g *= 0.8;
        }
        if kart.spin > 0.0 {
            g *= 0.3;
        }
        lat += (slide - lat) * (g * DT).min(1.0);
        kart.vx = kart.hx * fwd + lx * lat;
        kart.vz = kart.hz * fwd + lz * lat;
        kart.x += kart.vx * DT;
        kart.z += kart.vz * DT;
        // Timers.
        for t in [
            &mut kart.boost,
            &mut kart.spin,
            &mut kart.splat,
            &mut kart.bump,
            &mut kart.immune,
            &mut kart.wall_t,
        ] {
            *t = (*t - DT).max(0.0);
        }
        if kart.unpack > 0.0 {
            kart.unpack = (kart.unpack - DT).max(0.0);
        }
        self.walls(k);
    }

    /// Keeps a kart inside the walls.
    fn walls(&mut self, k: usize) {
        let r = self.physics().radius;
        let tr = &self.course.track;
        let kart = &mut self.karts[k];
        let loc = tr.locate(kart.x, kart.z, Some(kart.loc.i));
        let f = tr.frame(loc.s);
        let (nx, nz) = (f.tz, -f.tx);
        let mut hit = 0.0;
        for (side, wall) in [(1.0, f.left), (-1.0, f.right)] {
            let over = side * loc.d - (wall - r);
            if over > 0.0 {
                kart.x -= nx * side * over;
                kart.z -= nz * side * over;
                let vn = (kart.vx * nx + kart.vz * nz) * side;
                if vn > 0.0 {
                    let sp = (kart.vx * kart.vx + kart.vz * kart.vz).sqrt().max(1e-6);
                    kart.vx -= nx * side * vn * 1.3;
                    kart.vz -= nz * side * vn * 1.3;
                    let keep = 1.0 - 0.45 * (vn / sp).min(1.0);
                    kart.vx *= keep;
                    kart.vz *= keep;
                    hit = vn;
                    // Turn the nose off the wall a little.
                    let into = (kart.hx * nx + kart.hz * nz) * side;
                    if into > 0.0 {
                        let (hx, hz) = norm(
                            kart.hx - nx * side * into * 0.5,
                            kart.hz - nz * side * into * 0.5,
                        );
                        kart.hx = hx;
                        kart.hz = hz;
                    }
                }
            }
        }
        if hit > 0.0 {
            if hit > 8.0 {
                kart.drift = 0;
                kart.charge = 0.0;
            }
            if kart.wall_t <= 0.0 {
                kart.wall_t = 0.4;
                self.events.push(Event::Wall {
                    kart: k,
                    hard: hit > 8.0,
                });
            }
        }
        kart.loc = tr.locate(kart.x, kart.z, Some(loc.i));
    }

    /// Karts bump into each other: the heavier one pushes harder.
    fn collide(&mut self) {
        let r = self.physics().radius;
        let n = self.karts.len();
        for a in 0..n {
            for b in a + 1..n {
                let (dx, dz) = (
                    self.karts[b].x - self.karts[a].x,
                    self.karts[b].z - self.karts[a].z,
                );
                let d2 = dx * dx + dz * dz;
                if d2 >= 4.0 * r * r || d2 < 1e-9 {
                    continue;
                }
                let d = d2.sqrt();
                let (nx, nz) = (dx / d, dz / d);
                let over = 2.0 * r - d;
                let (ma, mb) = (self.karts[a].stats.mass, self.karts[b].stats.mass);
                let (fa, fb) = (mb / (ma + mb), ma / (ma + mb));
                let rel = (self.karts[b].vx - self.karts[a].vx) * nx
                    + (self.karts[b].vz - self.karts[a].vz) * nz;
                {
                    let ka = &mut self.karts[a];
                    ka.x -= nx * over * fa;
                    ka.z -= nz * over * fa;
                }
                {
                    let kb = &mut self.karts[b];
                    kb.x += nx * over * fb;
                    kb.z += nz * over * fb;
                }
                if rel < 0.0 {
                    // Inelastic, with a little bounce.
                    let j = -1.3 * rel;
                    let ka = &mut self.karts[a];
                    ka.vx -= nx * j * fa;
                    ka.vz -= nz * j * fa;
                    let kb = &mut self.karts[b];
                    kb.vx += nx * j * fb;
                    kb.vz += nz * j * fb;
                    if -rel > 3.0 {
                        self.events.push(Event::Bump { a, b });
                    }
                }
            }
        }
        for k in 0..n {
            self.walls(k);
        }
    }

    /// Top speed times this for a computer racer: a little more behind the leading human (or
    /// the leader), a little less ahead of it.
    fn catchup(&self, k: usize) -> f64 {
        let kart = &self.karts[k];
        if kart.human.is_some() && kart.finish.is_none() {
            return 1.0;
        }
        let humans: Vec<f64> = self
            .karts
            .iter()
            .filter(|o| o.human.is_some() && o.finish.is_none())
            .map(|o| o.dist)
            .collect();
        let reference = if humans.is_empty() {
            self.karts.iter().map(|o| o.dist).fold(f64::MIN, f64::max) - 30.0
        } else {
            humans.iter().copied().fold(f64::MIN, f64::max)
        };
        let gap = reference - kart.dist;
        1.0 + self.physics().catchup * clamp(gap / 250.0, -0.6, 1.0)
    }

    // ---------- the computer's driving ----------

    fn ai(&mut self, k: usize) -> Control {
        let racing = self.tick >= self.start_tick;
        let p = self.physics().clone();
        let course = self.course.clone();
        let tr = &course.track;
        let kart = &self.karts[k];
        let st = kart.stats.clone();
        let v = kart.speed();
        let s = kart.loc.s;
        // Where to aim: on the racing line, some way ahead, with this racer's lane.
        let look = 5.0 + v.max(0.0) * 0.42;
        let f = tr.frame(s + look);
        let room = (f.w / 2.0 - 1.2).max(0.0);
        let lane = clamp(f.line + kart.ai.lane, -room, room);
        let (tx, tz) = (f.x + f.tz * lane, f.z - f.tx * lane);
        let (dx, dz) = (tx - kart.x, tz - kart.z);
        let len = (dx * dx + dz * dz).sqrt().max(1e-6);
        let side = (dx * kart.hz - dz * kart.hx) / len;
        let ahead = (dx * kart.hx + dz * kart.hz) / len;
        let mut steer = if ahead < 0.0 {
            sign(side)
        } else {
            clamp(side * 3.2, -1.0, 1.0)
        };
        // How fast: the slowest the line allows ahead, given room to brake.
        let corner = p.corner * (0.82 + 0.25 * st.skill) * (st.grip / p.grip).sqrt();
        let dec = p.brake * 0.8;
        let n = tr.n();
        let mut vt = f64::MAX;
        let reach = v.max(0.0) * v.max(0.0) / (2.0 * dec) + 12.0;
        let mut j = 0.0;
        let i0 = kart.loc.i;
        let mut idx = i0;
        while j < reach {
            let c = course.curv[idx].abs().max(1e-4);
            let vmax = (corner / c).sqrt();
            let allowed = (vmax * vmax + 2.0 * dec * j).sqrt();
            vt = vt.min(allowed);
            idx = (idx + 1) % n;
            j += tr.spacing;
        }
        vt = vt.min(st.top * 1.5);
        let mut gas = v < vt;
        let mut brake = v > vt + 2.0;
        // A drift through long corners.
        let mut drift = false;
        if st.skill > 0.4 && v > 15.0 {
            let mut sum = 0.0;
            let mut cnt = 0.0;
            let mut idx = (i0 + 2) % n;
            for _ in 0..12 {
                sum += course.curv[idx];
                cnt += 1.0;
                idx = (idx + 1) % n;
            }
            let avg = sum / cnt;
            if kart.drift != 0 {
                drift =
                    avg * kart.drift as f64 > 1.0 / 90.0 && kart.charge < p.drift_charge.1 + 0.3;
            } else {
                drift = avg.abs() > 1.0 / 45.0 && steer.abs() > 0.3 && steer * avg > 0.0;
            }
        }
        // Items.
        let mut item = false;
        let hold = kart.ai.hold;
        if let (Some(it), true) = (kart.item, kart.unpack <= 0.0) {
            let def = &self.game.defs.items[it];
            let near = |lo: f64, hi: f64| {
                self.karts.iter().enumerate().any(|(o, other)| {
                    o != k && other.finish.is_none() && {
                        let gap = other.dist - kart.dist;
                        gap > lo && gap < hi
                    }
                })
            };
            item = match def.kind {
                ItemKind::Throw | ItemKind::Aim | ItemKind::Roll => {
                    (hold > 0.6 && near(3.0, 45.0)) || hold > 10.0
                }
                ItemKind::Drop => (hold > 0.6 && near(-30.0, -3.0)) || hold > 9.0,
                ItemKind::Boost => (hold > 1.0 && vt > st.top * 1.2) || hold > 6.0,
            };
        }
        // Stuck against something: back off.
        let mut reverse = kart.ai.reverse;
        let mut stuck = kart.ai.stuck;
        if racing && kart.spin <= 0.0 && kart.bump <= 0.0 {
            if v.abs() < 2.0 && gas {
                stuck += DT;
            } else {
                stuck = (stuck - DT).max(0.0);
            }
        }
        if stuck > 0.8 {
            reverse = 0.9;
            stuck = 0.0;
        }
        if reverse > 0.0 {
            reverse -= DT;
            gas = false;
            brake = true;
            steer = -steer;
            drift = false;
        }
        // Lane changes: wander a little, and keep out of the way of whoever is just ahead.
        let mut ai = kart.ai.clone();
        ai.reverse = reverse;
        ai.stuck = stuck;
        ai.hold = if kart.item.is_some() && kart.unpack <= 0.0 {
            hold + DT
        } else {
            0.0
        };
        ai.lane_t -= DT;
        if ai.lane_t <= 0.0 {
            ai.lane_t = self.rng.range(2.0, 5.0);
            ai.lane_to = self.rng.range(-1.8, 1.8) * (1.2 - st.skill * 0.5);
        }
        for (o, other) in self.karts.iter().enumerate() {
            if o == k {
                continue;
            }
            let gap = other.dist - kart.dist;
            if gap > 0.0 && gap < 9.0 && (other.loc.d - kart.loc.d).abs() < 2.2 {
                ai.lane_to = if other.loc.d > f.line { -2.5 } else { 2.5 };
            }
        }
        ai.lane += clamp(ai.lane_to - ai.lane, -1.5 * DT, 1.5 * DT);
        // No progress for a long while: back on the road.
        if racing && kart.finish.is_none() {
            if kart.dist > ai.best + 4.0 {
                ai.best = kart.dist;
                ai.best_t = 0.0;
            } else {
                ai.best_t += DT;
            }
        }
        let rescue = ai.best_t > 6.0;
        if rescue {
            ai.best_t = 0.0;
        }
        self.karts[k].ai = ai;
        if rescue {
            self.rescue(k);
        }
        let steer_i = (clamp(steer, -1.0, 1.0) * 100.0).round() as i8;
        Control {
            steer: steer_i as f64 / 100.0,
            gas,
            brake,
            drift,
            item,
            raw: Input {
                steer: steer_i,
                gas,
                brake,
                drift,
                item,
            },
        }
    }

    /// Puts a kart back on the racing line where it is, at a standstill.
    fn rescue(&mut self, k: usize) {
        let tr = &self.course.track;
        let kart = &mut self.karts[k];
        let f = tr.frame(kart.loc.s);
        let (x, z) = tr.point(kart.loc.s, f.line);
        kart.x = x;
        kart.z = z;
        kart.hx = f.tx;
        kart.hz = f.tz;
        kart.vx = 0.0;
        kart.vz = 0.0;
        kart.drift = 0;
        kart.loc = tr.locate(x, z, Some(kart.loc.i));
    }

    // ---------- items ----------

    fn items(&mut self, ctl: &[Control]) {
        let game = self.game.clone();
        let course = self.course.clone();
        let tr = &course.track;
        // Using what is held (on the press, not while held).
        for (k, c) in ctl.iter().enumerate() {
            let kart = &self.karts[k];
            if !(c.item && !kart.prev.item) || kart.unpack > 0.0 || kart.spin > 0.0 {
                continue;
            }
            let Some(it) = kart.item else { continue };
            let def = &game.defs.items[it];
            let (x, z, hx, hz) = (kart.x, kart.z, kart.hx, kart.hz);
            let fwd = kart.speed().max(0.0);
            self.events.push(Event::Use {
                kart: k,
                item: def.id.clone(),
            });
            self.karts[k].item = None;
            match def.kind {
                ItemKind::Boost => {
                    let kart = &mut self.karts[k];
                    kart.boost = kart.boost.max(def.time);
                    kart.boost_power = def.power;
                }
                ItemKind::Drop => {
                    self.slicks.push(Slick {
                        item: it,
                        owner: k,
                        x: x - hx * 2.6,
                        z: z - hz * 2.6,
                        age: 0.0,
                        hits: def.hits,
                    });
                }
                ItemKind::Throw | ItemKind::Roll | ItemKind::Aim => {
                    let (sx, sz) = (x + hx * 2.4, z + hz * 2.4);
                    let speed = if def.kind == ItemKind::Throw {
                        def.speed + fwd
                    } else {
                        def.speed
                    };
                    let target = self.ahead(k, 70.0);
                    self.shots.push(Shot {
                        item: it,
                        owner: k,
                        x: sx,
                        z: sz,
                        vx: hx * speed,
                        vz: hz * speed,
                        loc: tr.locate(sx, sz, Some(self.karts[k].loc.i)),
                        age: 0.0,
                        aiming: if def.kind == ItemKind::Aim {
                            def.aim
                        } else {
                            0.0
                        },
                        target,
                        hits: def.hits,
                    });
                }
            }
        }
        // Things in flight.
        let mut keep = Vec::new();
        let shots = std::mem::take(&mut self.shots);
        for mut sh in shots {
            let def = &game.defs.items[sh.item];
            sh.age += DT;
            if sh.age > def.life {
                continue;
            }
            let speed = (sh.vx * sh.vx + sh.vz * sh.vz).sqrt();
            match def.kind {
                ItemKind::Roll => {
                    // Along the road, keeping its distance from the centre.
                    let s = sh.loc.s + def.speed * DT;
                    let f = tr.frame(s);
                    let d = clamp(sh.loc.d, -(f.w / 2.0 - 0.6), f.w / 2.0 - 0.6);
                    let (x, z) = tr.point(s, d);
                    sh.vx = (x - sh.x) / DT;
                    sh.vz = (z - sh.z) / DT;
                    sh.x = x;
                    sh.z = z;
                }
                ItemKind::Aim if sh.aiming > 0.0 => {
                    sh.aiming -= DT;
                    let (mut ax, mut az) = (sh.vx, sh.vz);
                    if let Some(t) = sh.target.or_else(|| self.ahead(sh.owner, 70.0)) {
                        sh.target = Some(t);
                        ax = self.karts[t].x - sh.x;
                        az = self.karts[t].z - sh.z;
                    }
                    let (ux, uz) = norm(ax, az);
                    sh.vx = ux * def.speed;
                    sh.vz = uz * def.speed;
                    if sh.aiming > 0.0 {
                        // Still: it only turns to its target.
                        sh.vx *= 1e-3;
                        sh.vz *= 1e-3;
                    }
                }
                _ => {
                    if def.kind == ItemKind::Throw {
                        if let Some(t) = sh.target {
                            let (tx, tz) = (self.karts[t].x - sh.x, self.karts[t].z - sh.z);
                            let (ux, uz) = norm(tx, tz);
                            let (vx, vz) = norm(sh.vx, sh.vz);
                            // Homes only on what is in front of it.
                            if ux * vx + uz * vz > 0.5 {
                                let a = (3.0 * DT).min(1.0);
                                let (nx, nz) = norm(vx + (ux - vx) * a, vz + (uz - vz) * a);
                                sh.vx = nx * speed;
                                sh.vz = nz * speed;
                            }
                        }
                    }
                    sh.x += sh.vx * DT;
                    sh.z += sh.vz * DT;
                }
            }
            sh.loc = tr.locate(sh.x, sh.z, Some(sh.loc.i));
            let f = tr.frame(sh.loc.s);
            if sh.loc.d > f.left || -sh.loc.d > f.right {
                continue;
            }
            // Hits.
            let mut gone = false;
            for k in 0..self.karts.len() {
                if (k == sh.owner && sh.age < 0.6) || self.karts[k].immune > 0.0 {
                    continue;
                }
                let (dx, dz) = (self.karts[k].x - sh.x, self.karts[k].z - sh.z);
                if dx * dx + dz * dz < def.radius * def.radius {
                    self.hit(k, &def.id, def.effect, def.time, def.power, (sh.vx, sh.vz));
                    sh.hits = sh.hits.saturating_sub(1);
                    if sh.hits == 0 {
                        gone = true;
                        break;
                    }
                }
            }
            if !gone {
                keep.push(sh);
            }
        }
        self.shots = keep;
        let mut keep = Vec::new();
        let slicks = std::mem::take(&mut self.slicks);
        for mut sl in slicks {
            let def = &game.defs.items[sl.item];
            sl.age += DT;
            if sl.age > def.life {
                continue;
            }
            let mut gone = false;
            for k in 0..self.karts.len() {
                if (k == sl.owner && sl.age < 1.0) || self.karts[k].immune > 0.0 {
                    continue;
                }
                let (dx, dz) = (self.karts[k].x - sl.x, self.karts[k].z - sl.z);
                if dx * dx + dz * dz < def.radius * def.radius {
                    let v = (self.karts[k].vx, self.karts[k].vz);
                    self.hit(k, &def.id, def.effect, def.time, def.power, v);
                    sl.hits = sl.hits.saturating_sub(1);
                    if sl.hits == 0 {
                        gone = true;
                        break;
                    }
                }
            }
            if !gone {
                keep.push(sl);
            }
        }
        self.slicks = keep;
    }

    /// The nearest racer ahead of kart `k`, within `range` metres of road.
    fn ahead(&self, k: usize, range: f64) -> Option<usize> {
        let me = self.karts[k].dist;
        let mut best: Option<(f64, usize)> = None;
        for (o, other) in self.karts.iter().enumerate() {
            let gap = other.dist - me;
            if o != k && gap > 0.5 && gap < range && best.is_none_or(|(g, _)| gap < g) {
                best = Some((gap, o));
            }
        }
        best.map(|b| b.1)
    }

    /// Something hits a kart.
    fn hit(&mut self, k: usize, by: &str, effect: Effect, time: f64, power: f64, push: (f64, f64)) {
        let kart = &mut self.karts[k];
        kart.drift = 0;
        kart.charge = 0.0;
        match effect {
            Effect::Splat => {
                kart.splat = time;
                kart.splat_power = power;
            }
            Effect::Spin => {
                kart.spin = time;
                kart.boost = 0.0;
                let (px, pz) = norm(push.0, push.1);
                // Spin the way the push turns it.
                kart.spin_dir = sign(px * kart.hz - pz * kart.hx);
            }
            Effect::Bump => {
                kart.bump = time;
                kart.boost = 0.0;
                let (px, pz) = norm(push.0, push.1);
                kart.vx = kart.vx * 0.3 + px * 5.0;
                kart.vz = kart.vz * 0.3 + pz * 5.0;
            }
            Effect::Boost => {
                kart.boost = kart.boost.max(time);
                kart.boost_power = power;
            }
        }
        kart.immune = time.max(0.5) + 0.4;
        self.events.push(Event::Hit {
            kart: k,
            by: by.to_string(),
            effect,
        });
    }

    // ---------- baskets, pads, hazards ----------

    fn baskets(&mut self) {
        let course = self.course.clone();
        let p = self.physics().clone();
        let r2 = (1.2 + p.radius) * (1.2 + p.radius);
        let n = self.karts.len();
        for (b, spot) in course.baskets.iter().enumerate() {
            if self.baskets[b] > self.tick {
                continue;
            }
            for k in 0..n {
                let (dx, dz) = (self.karts[k].x - spot.x, self.karts[k].z - spot.z);
                if dx * dx + dz * dz > r2 {
                    continue;
                }
                self.baskets[b] = self.tick + (p.basket_respawn * HZ as f64) as u64;
                if self.karts[k].item.is_none() {
                    let item = self.draw_item(k);
                    let kart = &mut self.karts[k];
                    kart.item = Some(item);
                    kart.unpack = p.unpack;
                    self.events.push(Event::Pickup { kart: k });
                }
                break;
            }
        }
        for pad in &course.pads {
            for k in 0..n {
                let kart = &self.karts[k];
                let ds = (kart.loc.s - pad.s).abs();
                if ds < 2.2 && (kart.loc.d - pad.d).abs() < 1.6 + p.radius * 0.5 {
                    let fresh = kart.boost < p.pad_boost - 0.1;
                    let kart = &mut self.karts[k];
                    kart.boost = kart.boost.max(p.pad_boost);
                    kart.boost_power = p.boost_power;
                    if fresh {
                        self.events.push(Event::Pad { kart: k });
                    }
                }
            }
        }
    }

    /// An item for kart `k`, by its place: the odds of the item for that place.
    fn draw_item(&mut self, k: usize) -> usize {
        let place = self.karts[k].place;
        let field = self.karts.len().max(1);
        let weights: Vec<f64> = self
            .game
            .defs
            .items
            .iter()
            .map(|it| {
                let m = it.odds.len();
                let bucket = (place * m / field).min(m - 1);
                it.odds[bucket]
            })
            .collect();
        self.rng.pick(&weights)
    }

    /// Where each hazard is now: (crossing, x, z, hx, hz, moving).
    pub fn hazard_states(&self) -> Vec<(usize, f64, f64, f64, f64, bool)> {
        let t = self.time();
        let tr = &self.course.track;
        let mut out = Vec::new();
        for (i, c) in self.course.crossings.iter().enumerate() {
            let h = &self.game.defs.hazards[c.hazard];
            let span = c.to - c.from;
            let dur = span / h.speed;
            let (lat, dir, moving) = if t < h.offset {
                (c.from, 1.0, false)
            } else {
                let u = t - h.offset;
                let cycle = (u / h.every).floor();
                let ph = u - cycle * h.every;
                let even = (cycle as i64) % 2 == 0;
                let dir = if even { 1.0 } else { -1.0 };
                let along = (ph * h.speed).min(span);
                let lat = if even { c.from + along } else { c.to - along };
                (lat, dir, ph < dur)
            };
            let f = tr.frame(c.s);
            let (x, z) = tr.point(c.s, lat);
            // It drives across: towards the left normal, or away from it.
            out.push((i, x, z, f.tz * dir, -f.tx * dir, moving));
        }
        out
    }

    fn hazards(&mut self) {
        let states = self.hazard_states();
        let r = self.physics().radius;
        for (i, x, z, hx, hz, moving) in states {
            if !moving {
                continue;
            }
            let c = &self.course.crossings[i];
            let h = self.game.defs.hazards[c.hazard].clone();
            for k in 0..self.karts.len() {
                if self.karts[k].immune > 0.0 {
                    continue;
                }
                // Distance from the kart to the hazard's long axis.
                let (dx, dz) = (self.karts[k].x - x, self.karts[k].z - z);
                let along = clamp(dx * hx + dz * hz, -h.length / 2.0, h.length / 2.0);
                let (px, pz) = (dx - hx * along, dz - hz * along);
                if px * px + pz * pz < (h.width / 2.0 + r) * (h.width / 2.0 + r) {
                    self.hit(k, &h.id, h.effect, h.time, 1.0, (hx * 6.0, hz * 6.0));
                }
            }
        }
    }

    // ---------- laps and places ----------

    fn progress(&mut self) {
        let tr = &self.course.track;
        let len = tr.length;
        let laps = self.course.laps;
        let cps = &tr.checkpoints;
        let tick = self.tick;
        let start = self.start_tick;
        for k in 0..self.karts.len() {
            let kart = &mut self.karts[k];
            // `dist` moves with the kart's place along the track.
            let prev_s = tr.wrap(kart.dist);
            let mut ds = kart.loc.s - prev_s;
            if ds > len / 2.0 {
                ds -= len;
            } else if ds < -len / 2.0 {
                ds += len;
            }
            kart.dist += ds;
            if kart.finish.is_some() {
                continue;
            }
            let base = kart.lap as f64 * len;
            while kart.next_cp < cps.len() && kart.dist >= base + cps[kart.next_cp] {
                kart.next_cp += 1;
            }
            if kart.next_cp == cps.len() && kart.dist >= base + len {
                kart.lap += 1;
                kart.next_cp = 0;
                kart.lap_ticks.push(tick + 1 - start);
                if kart.lap >= laps {
                    kart.finish = Some(tick + 1 - start);
                } else {
                    self.events.push(Event::Lap {
                        kart: k,
                        lap: kart.lap + 1,
                    });
                }
            }
        }
        // Places: the finished by time, then by distance.
        let mut order: Vec<usize> = (0..self.karts.len()).collect();
        order.sort_by(|&a, &b| {
            let (ka, kb) = (&self.karts[a], &self.karts[b]);
            match (ka.finish, kb.finish) {
                (Some(x), Some(y)) => x.cmp(&y).then(a.cmp(&b)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => kb.dist.total_cmp(&ka.dist).then(a.cmp(&b)),
            }
        });
        for (place, &k) in order.iter().enumerate() {
            let was = self.karts[k].finish == Some(self.tick + 1 - self.start_tick)
                && self.karts[k].lap_ticks.len() as u32 == laps;
            self.karts[k].place = place;
            if was {
                let time = self.karts[k].finish.unwrap() as f64 * DT;
                self.events.push(Event::Finish {
                    kart: k,
                    place: place + 1,
                    time,
                });
            }
        }
    }

    /// Kart indices, first place first.
    pub fn order(&self) -> Vec<usize> {
        let mut o: Vec<usize> = (0..self.karts.len()).collect();
        o.sort_by_key(|&k| self.karts[k].place);
        o
    }

    pub fn standings(&self) -> Vec<Standing> {
        self.order()
            .into_iter()
            .enumerate()
            .map(|(i, k)| {
                let kart = &self.karts[k];
                let mut best: Option<f64> = None;
                let mut prev = 0;
                for t in &kart.lap_ticks {
                    let l = (t - prev) as f64 * DT;
                    prev = *t;
                    best = Some(best.map_or(l, |b: f64| b.min(l)));
                }
                Standing {
                    place: i + 1,
                    kart: k,
                    racer: self.game.defs.racers[kart.racer].id.clone(),
                    time: kart.finish.map(|f| f as f64 * DT),
                    best_lap: best,
                }
            })
            .collect()
    }

    pub fn view(&self) -> View {
        let defs = &self.game.defs;
        let laps = self.course.laps;
        View {
            tick: self.tick,
            time: self.time(),
            phase: self.phase(),
            karts: self
                .karts
                .iter()
                .map(|k| {
                    let p = &self.game.defs.physics;
                    let mut prev = 0;
                    let lap_times = k
                        .lap_ticks
                        .iter()
                        .map(|t| {
                            let l = (t - prev) as f64 * DT;
                            prev = *t;
                            l
                        })
                        .collect();
                    KartView {
                        racer: defs.racers[k.racer].id.clone(),
                        human: k.human.is_some(),
                        x: k.x,
                        z: k.z,
                        hx: k.hx,
                        hz: k.hz,
                        speed: k.speed(),
                        s: k.loc.s,
                        d: k.loc.d,
                        lap: (k.lap + 1).min(laps),
                        place: k.place + 1,
                        item: k.item.map(|i| defs.items[i].id.clone()),
                        unpacking: k.unpack > 0.0,
                        drift: k.drift,
                        tier: if k.drift == 0 {
                            0
                        } else if k.charge >= p.drift_charge.1 {
                            2
                        } else if k.charge >= p.drift_charge.0 {
                            1
                        } else {
                            0
                        },
                        boost: k.boost > 0.0,
                        spin: k.spin > 0.0,
                        splat: k.splat,
                        bump: k.bump > 0.0,
                        off: k.off,
                        time: k.finish.map(|f| f as f64 * DT),
                        laps: lap_times,
                    }
                })
                .collect(),
            shots: self
                .shots
                .iter()
                .map(|s| ThingView {
                    item: defs.items[s.item].id.clone(),
                    x: s.x,
                    z: s.z,
                    vx: s.vx,
                    vz: s.vz,
                    aiming: s.aiming > 0.0,
                })
                .collect(),
            slicks: self
                .slicks
                .iter()
                .map(|s| ThingView {
                    item: defs.items[s.item].id.clone(),
                    x: s.x,
                    z: s.z,
                    vx: 0.0,
                    vz: 0.0,
                    aiming: false,
                })
                .collect(),
            baskets: self.baskets.iter().map(|b| *b <= self.tick).collect(),
            hazards: self
                .hazard_states()
                .into_iter()
                .map(|(i, x, z, hx, hz, moving)| HazardView {
                    id: self.course.crossings[i].id.clone(),
                    x,
                    z,
                    hx,
                    hz,
                    moving,
                })
                .collect(),
            order: self.order(),
        }
    }
}

/// An input as the physics reads it.
#[derive(Clone, Copy, Debug)]
struct Control {
    /// -1 (right) to 1 (left).
    steer: f64,
    gas: bool,
    brake: bool,
    drift: bool,
    item: bool,
    raw: Input,
}

impl From<Input> for Control {
    fn from(i: Input) -> Control {
        Control {
            steer: clamp(i.steer as f64 / 100.0, -1.0, 1.0),
            gas: i.gas,
            brake: i.brake,
            drift: i.drift,
            item: i.item,
            raw: i,
        }
    }
}
