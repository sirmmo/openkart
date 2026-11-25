//! Races the computer's racers round a track and reports the result.
//!
//! ```sh
//! cargo run --release --example race -- games/pachino-gp [seed] [track]
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use openkart::{load_dir, sim::Event, Race, Setup};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args
        .first()
        .map(String::as_str)
        .unwrap_or("games/pachino-gp");
    let seed = args.get(1).map(String::as_str).unwrap_or("1");
    let game = match load_dir(std::path::Path::new(dir)) {
        Ok(g) => Arc::new(g),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let track = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| game.defs.game.first.clone());
    let setup = Setup::computer(seed, &game, &track);
    let mut race = Race::new(game.clone(), &setup).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut offroad = vec![0usize; race.karts.len()];
    let mut ticks = 0usize;
    while !race.over() {
        race.step(&[]);
        ticks += 1;
        for (i, k) in race.karts.iter().enumerate() {
            if k.off {
                offroad[i] += 1;
            }
        }
        for e in race.take_events() {
            let key = match &e {
                Event::Hit { by, effect, .. } => format!("hit {by} {effect:?}"),
                Event::Use { item, .. } => format!("use {item}"),
                Event::Wall { hard, .. } => format!("wall{}", if *hard { " hard" } else { "" }),
                Event::Boost { tier, .. } => format!("drift boost {tier}"),
                other => format!("{:?}", other)
                    .split([' ', '{'])
                    .next()
                    .unwrap()
                    .to_string(),
            };
            *counts.entry(key).or_default() += 1;
        }
    }
    println!(
        "{} on {} ({:.0} m, {} laps), seed {seed}: {:.1} s",
        game.defs.game.name,
        track,
        race.course.track.length,
        race.course.laps,
        race.time()
    );
    for s in race.standings() {
        let k = &race.karts[s.kart];
        println!(
            "  {}. {:<12} {:>7}  best lap {:>6}  laps {:?}  off-road {:.0}%",
            s.place,
            s.racer,
            s.time.map_or("DNF".into(), |t| format!("{t:.1}")),
            s.best_lap.map_or("-".into(), |t| format!("{t:.1}")),
            k.lap_ticks,
            100.0 * offroad[s.kart] as f64 / ticks as f64
        );
    }
    for (k, v) in counts {
        println!("  {v:>5}  {k}");
    }
}
