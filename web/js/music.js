// OpenKart's music: an original tarantella-style tune in 6/8, played by a small step sequencer
// on Web Audio (mandolin, accordion, accordion bass, tamburello and frame drum). No files.
//
// A tune is a list of bars, one chord and one bar of melody each. A melody bar is six eighth
// notes written as note names: "A4 C5 E5 A5- E5" (a dash holds the note one more eighth, "r" is a
// rest). The sequencer schedules a little ahead of the audio clock; `scheduleUntil` lets an
// OfflineAudioContext render the same notes.

const NAMES = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };
const freq = (n) => 440 * 2 ** ((n - 69) / 12);

// "G#4" -> 68.
function noteNumber(s) {
  const m = /^([A-G])(#|b)?(\d)$/.exec(s);
  if (!m) throw new Error(`music: bad note ${s}`);
  return NAMES[m[1]] + (m[2] === '#' ? 1 : m[2] === 'b' ? -1 : 0) + 12 * (+m[3] + 1);
}

// "A4 C5 E5 A5- E5" -> [[69, 1], [72, 1], [76, 1], [81, 2], [76, 1]].
function parseBar(text) {
  const out = text.trim().split(/\s+/).map((tok) => {
    const name = tok.replace(/-+$/, '');
    const len = 1 + tok.length - name.length;
    return [name === 'r' ? null : noteNumber(name), len];
  });
  const sum = out.reduce((a, [, l]) => a + l, 0);
  if (sum !== 6) throw new Error(`music: bar "${text}" is ${sum} eighths, not 6`);
  return out;
}

// Chords: the accordion's right hand, and its bass buttons (root on 1, fifth on 4).
const CHORDS = {
  Am: { notes: [57, 60, 64], bass: [45, 40] },
  E7: { notes: [56, 59, 62, 64], bass: [40, 47] },
  Dm: { notes: [57, 62, 65], bass: [38, 45] },
  C: { notes: [55, 60, 64], bass: [48, 43] },
  G7: { notes: [55, 59, 62, 65], bass: [43, 38] },
  F: { notes: [57, 60, 65], bass: [41, 48] },
};

// "Tarantella di Sant'Orsola", written for OpenKart: A minor, then C major, back by E7.
const MELODY = [
  ['Am', 'A4 C5 E5 A5- E5'], ['Am', 'F5 E5 D5 C5 B4 C5'], ['E7', 'B4- G#4 B4 D5 E5'], ['E7', 'G#5- F5 E5 D5 B4'],
  ['E7', 'D5 E5 F5 E5 D5 C5'], ['E7', 'B4-- E5- r'], ['Am', 'A4 C5 E5 A5 B5 C6'], ['Am', 'B5 A5 G#5 A5--'],
  ['Dm', 'F5- A5 D6- A5'], ['Dm', 'F5 E5 D5 A4- D5'], ['Am', 'C5- E5 A5- C6'], ['Am', 'B5 A5 E5 C5- E5'],
  ['E7', 'D5- B4 G#4- B4'], ['E7', 'E5 F5 E5 D5 C5 B4'], ['Am', 'C5 E5 A4 B4 G#4 B4'], ['Am', 'A4--- r-'],
  ['C', 'G5- E5 C5- E5'], ['C', 'G5 A5 G5 F5 E5 D5'], ['G7', 'F5- D5 B4- D5'], ['G7', 'F5 G5 F5 D5 B4 D5'],
  ['C', 'E5- G5 C6- B5'], ['F', 'A5- F5 C5- A5'], ['G7', 'G5 F5 D5 B4 D5 F5'], ['C', 'E5- D5 C5--'],
  ['F', 'A5 G5 F5 E5 F5 A5'], ['C', 'G5- E5 C5 D5 E5'], ['Dm', 'F5 E5 D5 A5- F5'], ['Am', 'E5- C5 A4 B4 C5'],
  ['Dm', 'D5 F5 A5 D6- C6'], ['Am', 'C6 A5 E5 C5- A4'], ['E7', 'B4 E5 G#5 B5 A5 G#5'], ['E7', 'B5-- G#5 E5 B4'],
];

// Each bar as six slots: the melody note starting there (and its length), the chord.
function compile(bars) {
  return bars.map(([chord, text]) => {
    const slots = Array(6).fill(null);
    let p = 0;
    for (const [n, len] of parseBar(text)) {
      if (n != null) slots[p] = { n, len };
      p += len;
    }
    return { chord: CHORDS[chord], slots };
  });
}

// The two arrangements: `bpm` counts dotted quarters (two to a bar of 6/8).
export const TUNES = {
  race: { bpm: 132, bars: compile(MELODY), style: 'race' },
  menu: { bpm: 84, bars: compile(MELODY.slice(0, 16)), style: 'menu' },
};

// ---------- instruments ----------

function vca(ctx, out) {
  const g = ctx.createGain();
  g.gain.value = 0;
  g.connect(out);
  return g;
}

// A mandolin: two strings of a course, slightly apart, plucked once or in tremolo.
function mandolin(ctx, out, t, n, dur, vel, tremolo) {
  const f = freq(n);
  const lp = ctx.createBiquadFilter();
  lp.type = 'lowpass';
  lp.Q.value = 1.5;
  const g = vca(ctx, out);
  lp.connect(g);
  const oscs = [-5, 5].map((cents) => {
    const o = ctx.createOscillator();
    o.type = 'sawtooth';
    o.frequency.value = f;
    o.detune.value = cents;
    o.connect(lp);
    return o;
  });
  const picks = tremolo ? Math.max(1, Math.round(dur * 13)) : 1;
  const period = dur / picks;
  for (let i = 0; i < picks; i++) {
    const ti = t + i * period;
    const v = vel * (i === 0 ? 1 : 0.62 + 0.1 * Math.sin(i * 1.7));
    g.gain.setValueAtTime(0.0001, ti);
    g.gain.exponentialRampToValueAtTime(v, ti + 0.004);
    g.gain.exponentialRampToValueAtTime(v * 0.28, ti + Math.min(period, 0.09));
    lp.frequency.setValueAtTime(Math.min(9000, f * 9), ti);
    lp.frequency.exponentialRampToValueAtTime(Math.max(700, f * 2.2), ti + 0.12);
  }
  const end = t + dur + (tremolo ? 0.05 : 0.25);
  g.gain.exponentialRampToValueAtTime(0.0001, end);
  for (const o of oscs) { o.start(t); o.stop(end + 0.02); }
}

// An accordion chord: two reeds a note, one a little sharp and one a little flat (musette).
function accordion(ctx, out, t, notes, dur, vel) {
  const lp = ctx.createBiquadFilter();
  lp.type = 'lowpass';
  lp.frequency.value = 2100;
  lp.Q.value = 0.6;
  const g = vca(ctx, out);
  lp.connect(g);
  const oscs = [];
  for (const n of notes) {
    for (const [type, cents] of [['sawtooth', 7], ['square', -7]]) {
      const o = ctx.createOscillator();
      o.type = type;
      o.frequency.value = freq(n);
      o.detune.value = cents;
      const og = ctx.createGain();
      og.gain.value = type === 'square' ? 0.5 : 1;
      o.connect(og).connect(lp);
      oscs.push(o);
    }
  }
  const a = Math.min(0.02, dur / 4);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(vel, t + a);
  g.gain.setValueAtTime(vel, t + Math.max(a, dur - 0.04));
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur + 0.05);
  for (const o of oscs) { o.start(t); o.stop(t + dur + 0.08); }
}

// The accordion's bass buttons: a round low note with a little reed in it.
function bass(ctx, out, t, n, dur, vel) {
  const g = vca(ctx, out);
  const lp = ctx.createBiquadFilter();
  lp.type = 'lowpass';
  lp.frequency.value = 520;
  lp.connect(g);
  const o1 = ctx.createOscillator(), o2 = ctx.createOscillator();
  o1.type = 'triangle'; o1.frequency.value = freq(n);
  o2.type = 'sawtooth'; o2.frequency.value = freq(n + 12);
  const g2 = ctx.createGain(); g2.gain.value = 0.25;
  o1.connect(lp); o2.connect(g2).connect(lp);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(vel, t + 0.012);
  g.gain.exponentialRampToValueAtTime(vel * 0.55, t + 0.12);
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  for (const o of [o1, o2]) { o.start(t); o.stop(t + dur + 0.02); }
}

// The tamburello: a jingle of filtered noise; `roll` shakes it.
function tamburello(ctx, out, noise, t, vel, roll = false) {
  const s = ctx.createBufferSource();
  s.buffer = noise;
  const hp = ctx.createBiquadFilter();
  hp.type = 'highpass'; hp.frequency.value = 5200;
  const bp = ctx.createBiquadFilter();
  bp.type = 'peaking'; bp.frequency.value = 8800; bp.gain.value = 9; bp.Q.value = 2;
  const g = vca(ctx, out);
  s.connect(hp).connect(bp).connect(g);
  const len = roll ? 0.2 : 0.07;
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(vel, t + 0.002);
  if (roll) {
    for (let i = 1; i < 5; i++) {
      g.gain.exponentialRampToValueAtTime(vel * 0.3, t + i * 0.04 - 0.01);
      g.gain.exponentialRampToValueAtTime(vel * (0.8 - i * 0.12), t + i * 0.04);
    }
  }
  g.gain.exponentialRampToValueAtTime(0.0001, t + len);
  s.start(t, Math.random() * 1.5);
  s.stop(t + len + 0.02);
}

// A frame drum: a low thump that drops in pitch.
function drum(ctx, out, t, vel) {
  const o = ctx.createOscillator();
  o.type = 'sine';
  o.frequency.setValueAtTime(130, t);
  o.frequency.exponentialRampToValueAtTime(52, t + 0.14);
  const g = vca(ctx, out);
  o.connect(g);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(vel, t + 0.004);
  g.gain.exponentialRampToValueAtTime(0.0001, t + 0.28);
  o.start(t);
  o.stop(t + 0.3);
}

// ---------- the sequencer ----------

const LOOKAHEAD = 0.3;

export class Sequencer {
  constructor(ctx, out, noise) {
    this.ctx = ctx;
    this.noise = noise;
    // Each tune plays into its own gain, so one can fade while the next starts.
    this.out = out;
    this.tune = null;
    this.name = 'off';
    this.rate = 1;
    this.step = 0;
    this.next = 0;
    this.bus = null;
  }

  // Starts a tune at `at` (the audio clock), fading out the one playing.
  play(name, at = this.ctx.currentTime + 0.05) {
    if (name === this.name) return;
    this.stop(at);
    this.name = name;
    if (!TUNES[name]) return;
    this.tune = TUNES[name];
    this.bus = this.ctx.createGain();
    this.bus.gain.setValueAtTime(0.0001, at);
    this.bus.gain.exponentialRampToValueAtTime(1, at + 0.15);
    this.bus.connect(this.out);
    this.step = 0;
    this.next = at;
    this.rate = 1;
  }

  stop(at = this.ctx.currentTime, fade = 0.5) {
    if (this.bus) {
      const b = this.bus;
      if (b.gain.cancelAndHoldAtTime) b.gain.cancelAndHoldAtTime(at);
      else { b.gain.cancelScheduledValues(at); b.gain.setValueAtTime(1, at); }
      b.gain.exponentialRampToValueAtTime(0.0001, at + fade);
      setTimeout(() => b.disconnect(), (at - this.ctx.currentTime + fade + 1) * 1000);
    }
    this.bus = null;
    this.tune = null;
    this.name = 'off';
  }

  // Seconds an eighth note lasts.
  get eighth() {
    return 60 / (this.tune.bpm * this.rate) / 3;
  }

  // Called often (a timer): schedules what falls in the next moments.
  pump() {
    if (this.tune) this.scheduleUntil(this.ctx.currentTime + LOOKAHEAD);
  }

  scheduleUntil(end) {
    while (this.tune && this.next < end) {
      this.playStep(this.step, Math.max(this.next, this.ctx.currentTime));
      this.next += this.eighth;
      this.step++;
    }
  }

  playStep(step, t) {
    const { ctx, bus, tune, noise } = this;
    const e = this.eighth;
    const bars = tune.bars;
    const bi = Math.floor(step / 6) % bars.length, pos = step % 6;
    const bar = bars[bi];
    const swing = pos % 3 === 1 ? e * 0.04 : 0; // a lilt on the second eighth of each beat
    const ts = t + swing;
    const menu = tune.style === 'menu';
    const slot = bar.slots[pos];
    if (slot) {
      const tremolo = slot.len >= (menu ? 2 : 3);
      mandolin(ctx, bus, ts, slot.n, slot.len * e * 0.96, menu ? 0.13 : 0.15, tremolo);
    }
    if (menu) {
      if (pos === 0) accordion(ctx, bus, t, bar.chord.notes, 6 * e * 0.98, 0.05);
      if (pos === 0 || pos === 3) bass(ctx, bus, t, bar.chord.bass[pos ? 1 : 0], 2.6 * e, 0.22);
      if (pos === 0) tamburello(ctx, bus, noise, t, 0.05, true);
      return;
    }
    // Race: bass on 1 and 4, short chords between ("oom-pa-pa"), tamburello on every eighth.
    if (pos === 0 || pos === 3) {
      bass(ctx, bus, t, bar.chord.bass[pos ? 1 : 0], 1.6 * e, 0.3);
      drum(ctx, bus, t, pos ? 0.28 : 0.4);
    } else {
      accordion(ctx, bus, ts, bar.chord.notes, e * 0.55, pos % 3 === 1 ? 0.075 : 0.055);
    }
    const fill = bi % 8 === 7 && pos >= 4;
    tamburello(ctx, bus, noise, ts, [0.16, 0.06, 0.09, 0.13, 0.06, 0.09][pos], pos === 2 || pos === 5);
    if (fill) tamburello(ctx, bus, noise, ts + e / 2, 0.1);
  }
}
