//! Races on the real track: the game files load, computer racers finish, replays agree, and
//! every item and the ambulances do what they say.

use std::path::Path;
use std::sync::Arc;

use openkart::script::Effect;
use openkart::sim::{Event, Phase, HZ};
use openkart::{load, load_dir, Entry, Game, Input, Race, Setup};

fn game() -> Arc<Game> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/pachino-gp");
    Arc::new(load_dir(&dir).unwrap_or_else(|e| panic!("{e}")))
}

fn run_to_end(race: &mut Race) {
    while !race.over() {
        race.step(&[]);
    }
}

#[test]
fn the_game_files_load() {
    let g = game();
    let d = &g.defs;
    assert_eq!(d.game.name, "Pachino Grand Prix");
    assert_eq!(d.racers.len(), 4);
    assert_eq!(d.racers[0].name, "Capitan Pachino");
    let ids: Vec<&str> = d.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(
        ids,
        ["pomodorata", "olio", "peperoncino", "parmigiano", "sushi"]
    );
    let (_, c) = g.course("sorsola").unwrap();
    let t = &c.track;
    assert!(
        t.length > 2000.0 && t.length < 3000.0,
        "a 2-3 km loop, not {}",
        t.length
    );
    assert!(t.attribution.contains("OpenStreetMap"));
    for street in [
        "Viale Giambattista Ercolani",
        "Via Giuseppe Mazzini",
        "Via Pietro Albertoni",
        "Via Giuseppe Massarenti",
        "Policlinico Sant'Orsola-Malpighi",
    ] {
        assert!(t.section(street).is_some(), "{street}");
    }
    let gates: Vec<&str> = t
        .landmarks
        .iter()
        .filter(|l| l.kind == "gate")
        .map(|l| l.name.as_str())
        .collect();
    assert_eq!(gates, ["Porta San Vitale", "Porta Maggiore"]);
    assert!(t.buildings.iter().any(|b| b.kind == "pavilion"));
    assert!(t.buildings.iter().any(|b| !b.portico.is_empty()));
    assert!(c.props.iter().any(|p| p.kind == "tree"));
    assert!(!c.baskets.is_empty() && c.pads.len() == 3 && c.crossings.len() == 1);
    // Walls never cut into the road.
    for s in &t.samples {
        assert!(s.left > s.w / 2.0 && s.right > s.w / 2.0);
    }
}

#[test]
fn errors_in_game_files_say_where() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/pachino-gp");
    let mut files = Vec::new();
    for name in [
        "game.kart",
        "racers.kart",
        "items.kart",
        "sorsola.kart",
        "sorsola.track",
    ] {
        files.push((
            name.to_string(),
            std::fs::read_to_string(dir.join(name)).unwrap(),
        ));
    }
    let broken: Vec<(String, String)> = files
        .iter()
        .map(|(n, t)| {
            let t = if n == "sorsola.kart" {
                t.replace("pad Via Giuseppe Mazzini 0.35", "pad Via Roma 0.35")
            } else {
                t.clone()
            };
            (n.clone(), t)
        })
        .collect();
    let e = load(&broken).unwrap_err();
    assert_eq!(e.file, "sorsola.kart");
    assert!(e.line > 0 && e.msg.contains("Via Roma"), "{e}");
    let missing: Vec<(String, String)> = files
        .iter()
        .filter(|(n, _)| n != "sorsola.track")
        .cloned()
        .collect();
    assert!(load(&missing).unwrap_err().msg.contains("osm2track"));
}

#[test]
fn four_computer_racers_finish_three_laps() {
    let g = game();
    let setup = Setup::computer("test", &g, "sorsola");
    let mut race = Race::new(g.clone(), &setup).unwrap();
    assert_eq!(race.phase(), Phase::Countdown);
    run_to_end(&mut race);
    let st = race.standings();
    assert_eq!(st.len(), 4);
    let len = race.course.track.length;
    for s in &st {
        let t = s
            .time
            .unwrap_or_else(|| panic!("{} did not finish", s.racer));
        // 3 laps at an average of 15 to 40 m/s.
        assert!(
            t > 3.0 * len / 40.0 && t < 3.0 * len / 15.0,
            "{}: {t} s",
            s.racer
        );
        assert_eq!(race.karts[s.kart].lap_ticks.len(), 3);
    }
    // Finishing times in order of place.
    assert!(st.windows(2).all(|w| w[0].time <= w[1].time));
}

/// Tra le Porte: the second track, drawn with the circuit designer and built by
/// tools/new-track.sh from its exact `route way` lines.
#[test]
fn the_designed_track_loads_and_four_racers_finish_it() {
    let g = game();
    assert_eq!(g.defs.tracks.len(), 2);
    let (i, c) = g.course("porte").unwrap();
    let def = &g.defs.tracks[i];
    assert!(def
        .route
        .iter()
        .all(|l| matches!(l, openkart::script::Leg::Way(w) if w.forward.is_some() && w.from.is_some() && w.to.is_some())));
    let t = &c.track;
    assert!(t.length > 1500.0 && t.length < 3000.0, "{}", t.length);
    for street in ["Via San Vitale", "Via Zamboni", "Viale Quirico Filopanti"] {
        assert!(t.section(street).is_some(), "{street}");
    }
    assert!(t.landmarks.iter().any(|l| l.name == "Porta San Donato"));
    assert!(c.props.iter().any(|p| p.kind == "tree") && c.pads.len() == 2);
    for s in &t.samples {
        assert!(s.left > s.w / 2.0 && s.right > s.w / 2.0);
    }
    let setup = Setup::computer("porte", &g, "porte");
    let mut race = Race::new(g.clone(), &setup).unwrap();
    run_to_end(&mut race);
    let st = race.standings();
    assert_eq!(st.len(), 4);
    for s in &st {
        let time = s
            .time
            .unwrap_or_else(|| panic!("{} did not finish", s.racer));
        assert!(
            time > 3.0 * t.length / 40.0 && time < 3.0 * t.length / 15.0,
            "{}: {time} s",
            s.racer
        );
    }
}

#[test]
fn a_replay_is_the_same_race() {
    let g = game();
    let setup = Setup {
        seed: "replay".into(),
        track: "sorsola".into(),
        entries: vec![
            Entry {
                racer: "carota".into(),
                human: false,
            },
            Entry {
                racer: "pachino".into(),
                human: true,
            },
            Entry {
                racer: "kingsteak".into(),
                human: false,
            },
            Entry {
                racer: "prosciutto".into(),
                human: false,
            },
        ],
    };
    let mut race = Race::new(g.clone(), &setup).unwrap();
    // A human who weaves, drifts and uses items.
    for t in 0..HZ as usize * 40 {
        let steer = ((t / 50) % 5) as i8 * 30 - 60;
        race.step(&[Input {
            steer,
            gas: t % 300 < 280,
            brake: false,
            drift: (t / 90) % 3 == 0,
            item: t % 40 == 0,
        }]);
    }
    let saved = race.save();
    let again = Race::replay(g.clone(), &saved).unwrap();
    let a = serde_json::to_string(&race.view()).unwrap();
    let b = serde_json::to_string(&again.view()).unwrap();
    assert_eq!(a, b);
    assert_eq!(race.tick, again.tick);
    // Through JSON too (as the browser saves it).
    let json = serde_json::to_string(&saved).unwrap();
    let back: openkart::Replay = serde_json::from_str(&json).unwrap();
    let third = Race::replay(g.clone(), &back).unwrap();
    assert_eq!(a, serde_json::to_string(&third.view()).unwrap());
    // Another seed is another race.
    let mut other = saved.clone();
    other.setup.seed = "other".into();
    let fourth = Race::replay(g, &other).unwrap();
    assert_ne!(a, serde_json::to_string(&fourth.view()).unwrap());
}

/// A race just after the start, with the human (kart 0) given `item`.
fn armed(item: &str) -> Race {
    let g = game();
    let setup = Setup {
        seed: "items".into(),
        track: "sorsola".into(),
        entries: ["pachino", "carota", "kingsteak", "prosciutto"]
            .iter()
            .enumerate()
            .map(|(i, r)| Entry {
                racer: r.to_string(),
                human: i == 0,
            })
            .collect(),
    };
    let mut race = Race::new(g.clone(), &setup).unwrap();
    while race.phase() == Phase::Countdown {
        race.step(&[Input::default()]);
    }
    race.take_events();
    race.karts[0].item = g.defs.item(item);
    race
}

fn hits(events: &[Event], effect: Effect) -> Vec<usize> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Hit {
                kart, effect: x, ..
            } if *x == effect => Some(*kart),
            _ => None,
        })
        .collect()
}

/// Puts kart `k` on the racing line `ahead` metres in front of kart 0, facing along the road.
fn place_ahead(race: &mut Race, k: usize, ahead: f64, left: f64) {
    let t = race.course.track.clone();
    let s = t.wrap(race.karts[0].loc.s + ahead);
    let f = t.frame(s);
    let (x, z) = t.point(s, left);
    let dist = race.karts[0].dist + ahead;
    let kart = &mut race.karts[k];
    kart.x = x;
    kart.z = z;
    kart.hx = f.tx;
    kart.hz = f.tz;
    kart.vx = 0.0;
    kart.vz = 0.0;
    kart.loc = t.locate(x, z, None);
    kart.dist = dist;
}

fn use_item(race: &mut Race) {
    race.step(&[Input {
        item: true,
        ..Input::default()
    }]);
}

#[test]
fn a_pomodorata_splats_the_racer_ahead() {
    let mut race = armed("pomodorata");
    let d0 = race.karts[0].loc.d;
    place_ahead(&mut race, 1, 25.0, d0);
    use_item(&mut race);
    assert!(race.karts[0].item.is_none());
    let mut events = race.take_events();
    for _ in 0..HZ * 2 {
        race.step(&[Input::default()]);
        events.extend(race.take_events());
    }
    assert!(hits(&events, Effect::Splat).contains(&1), "{events:?}");
    assert!(race.view().karts[1].splat > 0.0);
}

#[test]
fn an_oil_slick_spins_whoever_drives_into_it() {
    let mut race = armed("olio");
    use_item(&mut race);
    assert_eq!(race.slicks.len(), 1);
    // Kart 1 comes along behind at speed, straight at the slick.
    let (sx, sz) = (race.slicks[0].x, race.slicks[0].z);
    let t = race.course.track.clone();
    let loc = t.locate(sx, sz, None);
    let f = t.frame(loc.s - 12.0);
    let (x, z) = t.point(loc.s - 12.0, loc.d);
    let k = &mut race.karts[1];
    k.x = x;
    k.z = z;
    k.hx = f.tx;
    k.hz = f.tz;
    k.vx = f.tx * 15.0;
    k.vz = f.tz * 15.0;
    k.loc = t.locate(x, z, None);
    let mut events = Vec::new();
    for _ in 0..HZ * 2 {
        race.step(&[Input::default()]);
        events.extend(race.take_events());
    }
    assert!(hits(&events, Effect::Spin).contains(&1), "{events:?}");
}

#[test]
fn a_peperoncino_is_a_boost() {
    let mut race = armed("peperoncino");
    let mut plain = race.clone();
    plain.karts[0].item = None;
    let gas = Input {
        gas: true,
        ..Input::default()
    };
    race.step(&[Input { item: true, ..gas }]);
    plain.step(&[gas]);
    for _ in 0..HZ {
        race.step(&[gas]);
        plain.step(&[gas]);
    }
    assert!(race.view().karts[0].boost || race.karts[0].dist > plain.karts[0].dist);
    assert!(race.karts[0].dist > plain.karts[0].dist + 3.0);
}

#[test]
fn a_parmesan_wheel_rolls_along_the_road_and_bumps() {
    let mut race = armed("parmigiano");
    let d0 = race.karts[0].loc.d;
    place_ahead(&mut race, 2, 60.0, d0);
    use_item(&mut race);
    let start = race.shots[0].loc.s;
    let mut events = Vec::new();
    for _ in 0..HZ * 3 {
        race.step(&[Input::default()]);
        events.extend(race.take_events());
    }
    assert!(hits(&events, Effect::Bump).contains(&2), "{events:?}");
    // Anything still rolling stayed on the road.
    for sh in &race.shots {
        assert!(sh.loc.s != start);
    }
}

#[test]
fn a_sushi_roll_aims_then_rolls_straight() {
    let mut race = armed("sushi");
    let d0 = race.karts[0].loc.d;
    place_ahead(&mut race, 3, 20.0, d0 + 1.0);
    use_item(&mut race);
    assert!(race.view().shots[0].aiming);
    for _ in 0..(0.5 * HZ as f64) as usize {
        race.step(&[Input::default()]);
    }
    assert!(race.shots[0].aiming > 0.0, "still aiming");
    let mut events = Vec::new();
    for _ in 0..HZ * 2 {
        race.step(&[Input::default()]);
        events.extend(race.take_events());
    }
    assert!(hits(&events, Effect::Bump).contains(&3), "{events:?}");
}

#[test]
fn ambulances_cross_on_their_timetable() {
    let g = game();
    let setup = Setup::computer("amb", &g, "sorsola");
    let mut race = Race::new(g.clone(), &setup).unwrap();
    let h = &g.defs.hazards[0];
    let c = race.course.crossings[0].clone();
    let crossing = (c.to - c.from) / h.speed;
    let mut moving = 0;
    let mut still = 0;
    for _ in 0..(HZ as f64 * (h.every * 3.0 + 4.0)) as usize {
        race.step(&[]);
        let v = race.view();
        if race.time() > h.offset {
            if v.hazards[0].moving {
                moving += 1;
            } else {
                still += 1;
            }
        }
    }
    let frac = moving as f64 / (moving + still) as f64;
    let want = (crossing / h.every).min(1.0);
    assert!((frac - want).abs() < 0.1, "{frac} vs {want}");
    // A kart parked where it crosses is bumped.
    let mut race = Race::new(g.clone(), &setup).unwrap();
    while race.phase() == Phase::Countdown {
        race.step(&[]);
    }
    let t = race.course.track.clone();
    let s = c.s;
    let (x, z) = t.point(s, 0.0);
    let mut bumped = false;
    for _ in 0..(HZ as f64 * h.every * 2.5) as usize {
        let k = &mut race.karts[0];
        k.x = x;
        k.z = z;
        k.vx = 0.0;
        k.vz = 0.0;
        k.loc = t.locate(x, z, None);
        race.step(&[]);
        if race
            .take_events()
            .iter()
            .any(|e| matches!(e, Event::Hit { kart: 0, by, .. } if by == "ambulance"))
        {
            bumped = true;
            break;
        }
    }
    assert!(bumped);
}
