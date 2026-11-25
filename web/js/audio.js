// OpenKart's sound, on Web Audio. Almost everything is synthesised here (engines, tyres,
// whooshes, items, the ambulances' siren, jingles) and the music comes from music.js; a few short
// CC0 samples (web/sounds/, Kenney) add texture. Nothing here changes the race: it listens to
// the engine's events and reads each frame's view, so replays sound the same as races.
//
// The audio starts on the first click or key (browsers keep it silent until then). `M` mutes;
// the volumes of the effects and of the music are saved in localStorage. With ?audiolog=1 every
// sound that fires is logged to window.openkartAudioLog (and the console), for tests.

import { Sequencer } from './music.js';

const KEY = 'openkart.sound';
const SAMPLES = {
  rustle: 'sounds/card-shove-1.ogg', // a basket unpacked (Kenney Casino Audio)
  tick: 'sounds/tick_001.ogg', // rummaging in it (Kenney Interface Sounds)
  select: 'sounds/select_002.ogg', // menu clicks
  drop: 'sounds/drop_002.ogg', // the oil bottle put down
};
const MAX_AI = 3; // computer karts with an engine of their own (the nearest)
const SIREN = [440, 587]; // the bitonale: two tones a fourth apart...
const SIREN_HALF = 0.55; // ...each held about half a second

const clamp = (x, a, b) => Math.max(a, Math.min(b, x));
const mf = (n) => 440 * 2 ** ((n - 69) / 12);

function loadSettings() {
  const d = { sfx: 0.8, music: 0.55, muted: false };
  try { return { ...d, ...JSON.parse(localStorage.getItem(KEY) || '{}') }; } catch { return d; }
}

// ---------- the mixer ----------

// The audio graph: effects and music into a master gain and a gentle limiter.
export class Mixer {
  constructor(ctx) {
    this.ctx = ctx;
    this.master = ctx.createGain();
    const comp = ctx.createDynamicsCompressor();
    comp.threshold.value = -10; comp.knee.value = 8; comp.ratio.value = 8;
    comp.attack.value = 0.004; comp.release.value = 0.25;
    this.master.connect(comp).connect(ctx.destination);
    this.duck = ctx.createGain(); // the pause silences the effects here
    this.duck.connect(this.master);
    this.sfx = ctx.createGain();
    this.sfx.connect(this.duck);
    this.music = ctx.createGain();
    this.music.connect(this.master);
    const n = ctx.sampleRate * 2;
    this.noise = ctx.createBuffer(1, n, ctx.sampleRate);
    const d = this.noise.getChannelData(0);
    for (let i = 0; i < n; i++) d[i] = Math.random() * 2 - 1;
    this.samples = {};
  }

  async load(map, base = '') {
    await Promise.all(Object.entries(map).map(async ([name, url]) => {
      try {
        const res = await fetch(base + url);
        if (!res.ok) return;
        const buf = await res.arrayBuffer();
        this.samples[name] = await new Promise((ok, no) => this.ctx.decodeAudioData(buf, ok, no));
      } catch { /* the synthesised stand-in plays instead */ }
    }));
  }

  // A place to play into: a gain, panned, into the effects.
  out(gain = 1, pan = 0) {
    const g = this.ctx.createGain();
    g.gain.value = gain;
    if (pan && this.ctx.createStereoPanner) {
      const p = this.ctx.createStereoPanner();
      p.pan.value = clamp(pan, -1, 1);
      g.connect(p).connect(this.sfx);
    } else g.connect(this.sfx);
    return g;
  }
}

function gainNode(c, v, to) {
  const g = c.createGain();
  g.gain.value = v;
  if (to) g.connect(to);
  return g;
}
function filter(c, type, f, q = 1) {
  const b = c.createBiquadFilter();
  b.type = type; b.frequency.value = f; b.Q.value = q;
  return b;
}
function osc(c, type, f) {
  const o = c.createOscillator();
  o.type = type; o.frequency.value = f;
  return o;
}
function panner(c) {
  return c.createStereoPanner ? c.createStereoPanner() : c.createGain();
}
function loopNoise(m) {
  const s = m.ctx.createBufferSource();
  s.buffer = m.noise; s.loop = true;
  s.start(0, Math.random() * 1.9);
  return s;
}
function softClip(k) {
  const curve = new Float32Array(1024);
  for (let i = 0; i < 1024; i++) { const x = i / 511.5 - 1; curve[i] = Math.tanh(k * x) / Math.tanh(k); }
  return curve;
}
const glide = (param, v, t, tc = 0.05) => param.setTargetAtTime(v, t, tc);

// ---------- one-shot building blocks; s = { m, t, out } ----------

function tone(s, f, dur, { type = 'square', vol = 0.2, slide = 1, attack = 0.005, delay = 0, vib = 0, vibRate = 6 } = {}) {
  const c = s.m.ctx, t = s.t + delay;
  const o = osc(c, type, f), g = c.createGain();
  o.frequency.setValueAtTime(f, t);
  if (slide !== 1) o.frequency.exponentialRampToValueAtTime(Math.max(20, f * slide), t + dur);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(vol, t + attack);
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  o.connect(g).connect(s.out);
  if (vib) {
    const l = osc(c, 'sine', vibRate), lg = gainNode(c, vib);
    l.connect(lg).connect(o.frequency);
    l.start(t); l.stop(t + dur + 0.05);
  }
  o.start(t); o.stop(t + dur + 0.05);
}

function hiss(s, dur, { vol = 0.2, type = 'bandpass', f = 1000, f2 = 0, q = 1, attack = 0.003, delay = 0, swell = false } = {}) {
  const c = s.m.ctx, t = s.t + delay;
  const src = c.createBufferSource();
  src.buffer = s.m.noise; src.loop = true;
  const fl = filter(c, type, f, q);
  fl.frequency.setValueAtTime(f, t);
  if (f2) fl.frequency.exponentialRampToValueAtTime(f2, t + dur);
  const g = c.createGain();
  g.gain.setValueAtTime(0.0001, t);
  if (swell) {
    g.gain.linearRampToValueAtTime(vol, t + dur * 0.45);
    g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  } else {
    g.gain.exponentialRampToValueAtTime(vol, t + attack);
    g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  }
  src.connect(fl).connect(g).connect(s.out);
  src.start(t, Math.random() * 1.5); src.stop(t + dur + 0.05);
}

// Air rushing past: noise through a band that sweeps up and down again.
function whoosh(s, dur, vol, { f0 = 300, f1 = 2600, f2 = 700, delay = 0 } = {}) {
  const c = s.m.ctx, t = s.t + delay;
  const src = c.createBufferSource();
  src.buffer = s.m.noise; src.loop = true;
  const fl = filter(c, 'bandpass', f0, 1.4);
  fl.frequency.setValueAtTime(f0, t);
  fl.frequency.exponentialRampToValueAtTime(f1, t + dur * 0.4);
  fl.frequency.exponentialRampToValueAtTime(f2, t + dur);
  const g = c.createGain();
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(vol, t + dur * 0.3);
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  src.connect(fl).connect(g).connect(s.out);
  src.start(t, Math.random() * 1.5); src.stop(t + dur + 0.05);
}

function sample(s, name, { vol = 0.5, rate = 1, delay = 0 } = {}) {
  const b = s.m.samples[name];
  if (!b) return false;
  const src = s.m.ctx.createBufferSource();
  src.buffer = b; src.playbackRate.value = rate;
  src.connect(gainNode(s.m.ctx, vol, s.out));
  src.start(s.t + delay);
  return true;
}

// A bell: a sine and a quieter inharmonic partial, ringing out.
function bell(s, f, dur, vol, delay = 0) {
  tone(s, f, dur, { type: 'sine', vol, attack: 0.003, delay });
  tone(s, f * 2.76, dur * 0.5, { type: 'sine', vol: vol * 0.3, attack: 0.003, delay });
  tone(s, f * 2, dur * 0.7, { type: 'triangle', vol: vol * 0.25, attack: 0.003, delay });
}

// A brass note: a sawtooth whose filter opens as it is blown.
function brass(s, f, dur, vol, delay = 0) {
  const c = s.m.ctx, t = s.t + delay;
  const o1 = osc(c, 'sawtooth', f), o2 = osc(c, 'sawtooth', f);
  o2.detune.value = 9;
  const fl = filter(c, 'lowpass', 500, 2);
  fl.frequency.setValueAtTime(500, t);
  fl.frequency.exponentialRampToValueAtTime(f * 6, t + 0.06);
  fl.frequency.exponentialRampToValueAtTime(f * 3, t + dur);
  const g = c.createGain();
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(vol, t + 0.03);
  g.gain.setValueAtTime(vol * 0.85, t + Math.max(0.04, dur - 0.05));
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur + 0.08);
  o1.connect(fl); o2.connect(fl); fl.connect(g).connect(s.out);
  for (const o of [o1, o2]) { o.start(t); o.stop(t + dur + 0.1); }
}

// A crowd: a swell of voices (bands of noise), claps and a few whistles.
function cheer(s, dur, vol) {
  hiss(s, dur, { vol: vol * 0.6, f: 800, q: 0.6, swell: true });
  hiss(s, dur * 0.9, { vol: vol * 0.45, f: 1900, q: 0.9, swell: true, delay: 0.1 });
  hiss(s, dur * 0.8, { vol: vol * 0.25, f: 3200, q: 1.5, swell: true, delay: 0.2 });
  for (let i = 0; i < 46; i++) {
    const at = Math.random() * dur * 0.85;
    hiss(s, 0.035, { vol: vol * (0.3 + Math.random() * 0.5), f: 1200 + Math.random() * 1800, q: 2, delay: at });
  }
  for (let i = 0; i < 3; i++) {
    tone(s, 1900 + Math.random() * 700, 0.35, { type: 'sine', vol: vol * 0.12, slide: 1.25, attack: 0.04, delay: 0.3 + i * dur * 0.25, vib: 30, vibRate: 11 });
  }
}

// ---------- the effects ----------

export const SFX = {
  count: (s) => { tone(s, 659, 0.18, { vol: 0.13 }); tone(s, 1318, 0.12, { type: 'sine', vol: 0.05 }); },
  go: (s) => {
    tone(s, 1319, 0.65, { vol: 0.12, vib: 8, vibRate: 7 });
    tone(s, 1976, 0.5, { type: 'sine', vol: 0.06 });
    tone(s, 659, 0.6, { type: 'triangle', vol: 0.12 });
    tone(s, 988, 0.6, { type: 'triangle', vol: 0.08 });
  },
  select: (s) => { if (!sample(s, 'select', { vol: 0.35 })) tone(s, 1200, 0.05, { type: 'triangle', vol: 0.08 }); },
  // A basket: wicker rustling and Capitan Pachino 64's coin.
  pickup: (s) => {
    if (!sample(s, 'rustle', { vol: 0.55, rate: 1.15 })) hiss(s, 0.15, { vol: 0.2, f: 2500, q: 0.8 });
    tone(s, 988, 0.08, { vol: 0.09 });
    tone(s, 1319, 0.3, { vol: 0.09, delay: 0.07 });
  },
  tick: (s) => { if (!sample(s, 'tick', { vol: 0.3, rate: 0.9 + Math.random() * 0.3 })) tone(s, 2400, 0.02, { vol: 0.04 }); },
  reveal: (s) => { tone(s, 784, 0.1, { type: 'triangle', vol: 0.14 }); tone(s, 1175, 0.28, { type: 'triangle', vol: 0.14, delay: 0.08 }); },
  // Pomodorata: a throw, and the splat.
  throw: (s) => {
    whoosh(s, 0.28, 0.3, { f0: 500, f1: 3200, f2: 1200 });
    tone(s, 260, 0.12, { type: 'triangle', vol: 0.18, slide: 2.2 });
  },
  splat: (s) => {
    hiss(s, 0.3, { vol: 0.7, type: 'lowpass', f: 2200, f2: 250, q: 0.8 });
    hiss(s, 0.12, { vol: 0.35, f: 900, q: 0.7 });
    tone(s, 240, 0.2, { type: 'sine', vol: 0.4, slide: 0.3 });
    for (let i = 0; i < 4; i++) tone(s, 900 + Math.random() * 700, 0.05, { type: 'sine', vol: 0.07, slide: 0.5, delay: 0.12 + i * 0.07 + Math.random() * 0.03 });
  },
  // Olive oil: the bottle glugs out; a kart in it slides round.
  glug: (s) => {
    sample(s, 'drop', { vol: 0.4, rate: 0.75 });
    for (let i = 0; i < 5; i++) tone(s, 250 - i * 22, 0.08, { type: 'sine', vol: 0.28, slide: 2.4, attack: 0.01, delay: 0.1 + i * 0.12 });
    hiss(s, 0.6, { vol: 0.08, type: 'lowpass', f: 600, delay: 0.1, swell: true });
  },
  slip: (s) => {
    tone(s, 1100, 0.95, { type: 'triangle', vol: 0.13, slide: 0.35, vib: 60, vibRate: 9 });
    hiss(s, 0.85, { vol: 0.28, f: 2800, q: 5 });
    hiss(s, 0.4, { vol: 0.15, type: 'lowpass', f: 500, delay: 0.05 });
  },
  // Peperoncino: a sizzle, then the boost.
  sizzle: (s) => {
    hiss(s, 0.6, { vol: 0.12, type: 'highpass', f: 5000, q: 0.5, swell: true });
    for (let i = 0; i < 16; i++) hiss(s, 0.025, { vol: 0.08 + Math.random() * 0.14, type: 'highpass', f: 3500 + Math.random() * 3000, delay: i * 0.035 + Math.random() * 0.02 });
  },
  pepper: (s) => {
    whoosh(s, 0.8, 0.45, { f0: 200, f1: 2400, f2: 500, delay: 0.15 });
    hiss(s, 0.9, { vol: 0.35, type: 'lowpass', f: 420, swell: true, delay: 0.1 });
    tone(s, 110, 0.6, { type: 'sawtooth', vol: 0.05, slide: 2, delay: 0.15 });
  },
  // Parmesan and sushi set off (their rolling is a voice), and thump into things.
  rollOut: (s) => { tone(s, 90, 0.25, { type: 'triangle', vol: 0.3, slide: 0.7 }); hiss(s, 0.2, { vol: 0.2, type: 'lowpass', f: 300 }); },
  aim: (s) => { tone(s, 1760, 0.05, { vol: 0.07 }); },
  sushiGo: (s) => { tone(s, 880, 0.12, { type: 'triangle', vol: 0.12, slide: 1.6 }); whoosh(s, 0.25, 0.15, { f0: 800, f1: 3000, f2: 1500 }); },
  thump: (s) => {
    tone(s, 75, 0.35, { type: 'sine', vol: 0.6, slide: 0.45 });
    tone(s, 150, 0.2, { type: 'triangle', vol: 0.2, slide: 0.5 });
    hiss(s, 0.25, { vol: 0.45, type: 'lowpass', f: 320 });
  },
  // Pachino 64's "hurt", when you are hit; and birds round a bumped kart's head.
  ouch: (s) => { tone(s, 500, 0.12, { vol: 0.07, slide: 0.6 }); tone(s, 300, 0.3, { vol: 0.07, slide: 0.5, delay: 0.1 }); },
  dizzy: (s) => { for (let i = 0; i < 3; i++) tone(s, 1900, 0.08, { type: 'sine', vol: 0.05, slide: 1.3, delay: 0.25 + i * 0.16 }); },
  // Boosts.
  pad: (s) => {
    whoosh(s, 0.5, 0.35, { f0: 400, f1: 3000, f2: 900 });
    tone(s, 880, 0.12, { type: 'triangle', vol: 0.1 });
    tone(s, 1320, 0.22, { type: 'triangle', vol: 0.1, delay: 0.07 });
  },
  driftBoost: (s, o) => {
    const big = o.tier === 2;
    whoosh(s, big ? 0.65 : 0.45, big ? 0.42 : 0.3, { f0: 250, f1: big ? 3200 : 2400, f2: 600 });
    tone(s, 140, big ? 0.45 : 0.3, { type: 'sawtooth', vol: 0.05, slide: 2 });
  },
  tier: (s, o) => {
    if (o.tier === 2) { tone(s, 1319, 0.07, { vol: 0.06 }); tone(s, 1568, 0.12, { vol: 0.06, delay: 0.06 }); }
    else tone(s, 1047, 0.08, { vol: 0.06 });
  },
  // Knocks.
  wall: (s) => {
    tone(s, 110, 0.22, { type: 'sine', vol: 0.5, slide: 0.4 });
    hiss(s, 0.15, { vol: 0.5, type: 'lowpass', f: 700 });
    hiss(s, 0.3, { vol: 0.22, f: 1600, q: 1.2, delay: 0.02 });
    tone(s, 420, 0.08, { vol: 0.04 });
  },
  scrape: (s) => { hiss(s, 0.28, { vol: 0.22, f: 1800, f2: 1200, q: 2 }); hiss(s, 0.2, { vol: 0.1, f: 900, q: 1 }); },
  kerb: (s) => {
    for (const d of [0, 0.07]) tone(s, 85, 0.09, { type: 'sine', vol: 0.3, slide: 0.5, delay: d });
    hiss(s, 0.07, { vol: 0.18, type: 'lowpass', f: 450 });
  },
  bonk: (s) => {
    tone(s, 190, 0.16, { type: 'triangle', vol: 0.35, slide: 0.5 });
    tone(s, 95, 0.2, { type: 'sine', vol: 0.35, slide: 0.6 });
    hiss(s, 0.06, { vol: 0.3, type: 'lowpass', f: 1200 });
  },
  // The race.
  place: (s, o) => {
    const [a, b] = o.up ? [660, 990] : [660, 440];
    tone(s, a, 0.07, { vol: 0.06 });
    tone(s, b, 0.13, { vol: 0.06, delay: 0.07 });
  },
  lap: (s) => { [1047, 1319, 1568].forEach((f, i) => bell(s, f, 0.6, 0.1, i * 0.09)); },
  finalLap: (s) => {
    const run = [76, 81, 84, 88, 86, 84, 83, 84, 88];
    run.forEach((n, i) => {
      tone(s, mf(n), 0.12, { type: 'triangle', vol: 0.14, delay: i * 0.075 });
      tone(s, mf(n - 12), 0.1, { vol: 0.03, delay: i * 0.075 });
    });
    const end = run.length * 0.075;
    tone(s, mf(81), 0.7, { type: 'triangle', vol: 0.13, delay: end, vib: 12, vibRate: 13 });
    tone(s, mf(88), 0.7, { type: 'triangle', vol: 0.08, delay: end, vib: 12, vibRate: 13 });
  },
  finish: (s, o) => {
    const notes = o.win
      ? [[67, 0, 0.12], [67, 0.14, 0.12], [67, 0.28, 0.12], [72, 0.42, 0.45], [76, 0.92, 0.12], [79, 1.06, 0.12], [84, 1.2, 1.2]]
      : [[67, 0, 0.15], [72, 0.18, 0.15], [76, 0.36, 0.9]];
    for (const [n, d, len] of notes) brass(s, mf(n), len, 0.09, d);
    const last = notes[notes.length - 1];
    for (const n of o.win ? [64, 67, 72] : [60, 64]) brass(s, mf(n), last[2], 0.05, last[1]);
    cheer(s, o.win ? 4.5 : 3, o.win ? 0.32 : 0.2);
  },
};

// ---------- continuous voices ----------

// A kart's engine: a buzzy two-stroke (sawtooth and a square an octave down, chugging at half
// its firing rate), through a soft clipper and a filter that opens with the throttle; filtered
// noise for grit; and the hiss of its tyres when it slides.
class EngineVoice {
  constructor(m, vol) {
    const c = m.ctx;
    this.vol = vol;
    this.o1 = osc(c, 'sawtooth', 50);
    this.o2 = osc(c, 'square', 25);
    this.lfo = osc(c, 'sine', 25);
    const mix = c.createGain();
    this.o1.connect(gainNode(c, 0.5, mix));
    this.o2.connect(gainNode(c, 0.28, mix));
    this.bp = filter(c, 'bandpass', 300, 1.2);
    this.grit = gainNode(c, 0.1, mix);
    loopNoise(m).connect(this.bp).connect(this.grit);
    const shaper = c.createWaveShaper();
    shaper.curve = softClip(2.5);
    this.lp = filter(c, 'lowpass', 600, 0.9);
    const am = gainNode(c, 0.75);
    this.lfo.connect(gainNode(c, 0.25, am.gain));
    this.amp = gainNode(c, 0);
    this.pan = panner(c);
    mix.connect(shaper).connect(this.lp).connect(am).connect(this.amp).connect(this.pan).connect(m.sfx);
    this.skid = gainNode(c, 0, this.pan);
    loopNoise(m).connect(filter(c, 'bandpass', 2600, 3)).connect(this.skid);
    for (const o of [this.o1, this.o2, this.lfo]) o.start();
  }

  // p: speed (m/s), throttle (0..1), boost, gain, pan, doppler, skid (0..1).
  set(p, t) {
    const v = Math.abs(p.speed || 0), thr = p.throttle || 0;
    // Gears: the revs climb through each band of speed, and drop at the change.
    const G = [0, 7, 14, 22, 31, 60];
    let gi = 0;
    while (gi < G.length - 2 && v > G[gi + 1]) gi++;
    const frac = clamp((v - G[gi]) / (G[gi + 1] - G[gi]), 0, 1);
    const geared = 0.28 + 0.72 * frac, idle = 0.1 + 0.5 * thr;
    const rpm = v < 3 ? idle + (geared - idle) * (v / 3) : geared;
    const f = (32 + rpm * 105) * (p.boost ? 1.1 : 1) * (p.doppler || 1);
    glide(this.o1.frequency, f, t);
    glide(this.o2.frequency, f / 2, t);
    glide(this.lfo.frequency, f / 2, t);
    glide(this.bp.frequency, f * 5, t);
    glide(this.grit.gain, 0.04 + 0.22 * thr + (p.boost ? 0.18 : 0), t);
    glide(this.lp.frequency, 250 + (900 + 1400 * rpm) * (0.35 + 0.65 * thr) + (p.boost ? 1500 : 0), t);
    glide(this.amp.gain, (p.gain || 0) * this.vol * (0.5 + 0.5 * thr) * (0.8 + 0.4 * rpm), t, 0.06);
    glide(this.skid.gain, (p.gain || 0) * (p.skid || 0) * 0.12, t, 0.06);
    if (this.pan.pan) glide(this.pan.pan, clamp(p.pan || 0, -1, 1), t);
  }
}

// Tyres squealing in a drift: a narrow band of noise and a wavering whistle.
class SquealVoice {
  constructor(m) {
    const c = m.ctx;
    this.amp = gainNode(c, 0, m.sfx);
    this.n = gainNode(c, 0.16, this.amp);
    loopNoise(m).connect(filter(c, 'bandpass', 2400, 5)).connect(this.n);
    this.o = osc(c, 'sine', 1150);
    this.lfo = osc(c, 'sine', 7);
    this.lfo.connect(gainNode(c, 40, this.o.frequency));
    this.o.connect(gainNode(c, 0.035, this.amp));
    this.o.start(); this.lfo.start();
  }

  set(p, t) {
    glide(this.amp.gain, (p.gain || 0) * (p.amount || 0), t, 0.04);
    glide(this.o.frequency, 1050 + 300 * (p.amount || 0), t);
  }
}

// The drift charge: a tone that rises as the boost builds, and warbles when it is full.
class ChargeVoice {
  constructor(m) {
    const c = m.ctx;
    this.amp = gainNode(c, 0, m.sfx);
    const trem = gainNode(c, 1, this.amp);
    this.lfo = osc(c, 'sine', 14);
    this.depth = gainNode(c, 0, trem.gain);
    this.lfo.connect(this.depth);
    const lp = filter(c, 'lowpass', 3000, 0.7);
    lp.connect(trem);
    this.o1 = osc(c, 'triangle', 200);
    this.o2 = osc(c, 'square', 400);
    this.o1.connect(lp);
    this.o2.connect(gainNode(c, 0.15, lp));
    for (const o of [this.o1, this.o2, this.lfo]) o.start();
  }

  // p: level (0..1 within the tier), tier (0, 1, 2).
  set(p, t) {
    const lv = clamp(p.level || 0, 0, 1);
    const f = [180 + 260 * lv, 440 + 120 * lv, 660 + 100 * lv][p.tier || 0];
    glide(this.o1.frequency, f, t, 0.03);
    glide(this.o2.frequency, f * 2, t, 0.03);
    glide(this.depth.gain, p.tier === 2 ? 0.5 : 0, t);
    glide(this.amp.gain, (p.gain || 0) * (0.045 + 0.015 * (p.tier || 0)), t, 0.04);
  }
}

// Off the road: a low rumble, juddering faster with speed.
class RumbleVoice {
  constructor(m) {
    const c = m.ctx;
    this.amp = gainNode(c, 0, m.sfx);
    const am = gainNode(c, 0.6, this.amp);
    this.lfo = osc(c, 'square', 8);
    this.lfo.connect(gainNode(c, 0.4, am.gain));
    loopNoise(m).connect(filter(c, 'lowpass', 170, 1.5)).connect(am);
    this.o = osc(c, 'triangle', 42);
    this.o.connect(gainNode(c, 0.25, am));
    this.lfo.start(); this.o.start();
  }

  set(p, t) {
    const v = Math.abs(p.speed || 0);
    glide(this.lfo.frequency, 4 + v * 0.6, t);
    glide(this.amp.gain, (p.gain || 0) * 0.55 * clamp(v / 18, 0.2, 1), t, 0.05);
  }
}

// An ambulance: the bitonale, two tones alternating, through a horn-shaped filter. The tones are
// scheduled ahead on the audio clock; Doppler bends them through `detune`.
export class SirenVoice {
  constructor(m) {
    const c = m.ctx;
    this.o1 = osc(c, 'square', SIREN[0]);
    this.o2 = osc(c, 'sawtooth', SIREN[0]);
    this.o2.detune.value = 6;
    const bp = filter(c, 'bandpass', 1100, 0.6);
    const lp = filter(c, 'lowpass', 3600, 0.7);
    this.amp = gainNode(c, 0);
    this.pan = panner(c);
    this.o1.connect(gainNode(c, 0.6, bp));
    this.o2.connect(gainNode(c, 0.4, bp));
    bp.connect(lp).connect(this.amp).connect(this.pan).connect(m.sfx);
    for (const o of [this.o1, this.o2]) o.start();
    this.next = -1;
    this.hi = Math.random() < 0.5;
  }

  set(p, t) {
    if (p.on) {
      if (this.next < t) this.next = t;
      while (this.next < t + 0.4) {
        const f = SIREN[this.hi ? 1 : 0];
        this.o1.frequency.setValueAtTime(f, this.next);
        this.o2.frequency.setValueAtTime(f, this.next);
        this.hi = !this.hi;
        this.next += SIREN_HALF;
      }
    }
    const cents = 1200 * Math.log2(p.doppler || 1);
    glide(this.o1.detune, cents, t, 0.05);
    glide(this.o2.detune, cents + 6, t, 0.05);
    glide(this.amp.gain, p.on ? (p.gain || 0) * 0.16 : 0, t, 0.08);
    if (this.pan.pan) glide(this.pan.pan, clamp(p.pan || 0, -1, 1), t);
  }
}

// A rolling item: a parmesan wheel (a deep rumble) or a sushi roll (lighter), its judder at
// the rate it turns.
class RollVoice {
  constructor(m) {
    const c = m.ctx;
    this.amp = gainNode(c, 0);
    this.pan = panner(c);
    this.amp.connect(this.pan).connect(m.sfx);
    const am = gainNode(c, 0.6, this.amp);
    this.lfo = osc(c, 'sine', 8);
    this.lfo.connect(gainNode(c, 0.4, am.gain));
    this.lp = filter(c, 'lowpass', 220, 1.2);
    loopNoise(m).connect(this.lp).connect(am);
    this.o = osc(c, 'triangle', 48);
    this.o.connect(gainNode(c, 0.35, am));
    this.lfo.start(); this.o.start();
  }

  // p: kind ('roll' or 'aim'), speed, gain, pan, doppler.
  set(p, t) {
    const heavy = p.kind !== 'aim';
    const r = heavy ? 0.85 : 0.4;
    glide(this.lfo.frequency, clamp(Math.abs(p.speed || 0) / (2 * Math.PI * r), 2, 30), t);
    glide(this.lp.frequency, heavy ? 220 : 900, t);
    glide(this.o.frequency, (heavy ? 48 : 110) * (p.doppler || 1), t);
    glide(this.amp.gain, (p.gain || 0) * (heavy ? 0.7 : 0.35), t, 0.06);
    if (this.pan.pan) glide(this.pan.pan, clamp(p.pan || 0, -1, 1), t);
  }
}

export const VOICES = { EngineVoice, SquealVoice, ChargeVoice, RumbleVoice, SirenVoice, RollVoice };

// ---------- the game's sound ----------

export class Sound {
  constructor({ log = false } = {}) {
    this.settings = loadSettings();
    this.m = null;
    this.seq = null;
    this.log = log ? [] : null;
    if (log) window.openkartAudioLog = this.log;
    this.music = 'off';
    this.mode = null;
    this.voices = {};
    this.on = {};
    this.cam = { x: 0, z: 0, rx: 1, rz: 0, vx: 0, vz: 0 };
    this.kinds = {}; // item id -> kind
    this.charge = [0.7, 1.5];
    this.unpack = 1;
    this.laps = 3;
    this.uis = [];
    this.reset();
  }

  // The game's items and numbers.
  setGame(cat) {
    for (const it of cat.items) this.kinds[it.id] = it.kind;
    const p = cat.physics || {};
    if (Array.isArray(p.drift_charge)) this.charge = p.drift_charge;
    if (p.unpack) this.unpack = p.unpack;
  }

  // Creates the audio on the first click or key, or wakes it.
  start() {
    if (!this.m) {
      let ctx;
      try { ctx = new (window.AudioContext || window.webkitAudioContext)(); } catch { return; }
      this.m = new Mixer(ctx);
      this.m.load(SAMPLES);
      this.seq = new Sequencer(ctx, this.m.music, this.m.noise);
      this.apply();
      this.timer = setInterval(() => this.seq.pump(), 50);
      const want = this.music;
      this.music = 'off';
      this.setMusic(want);
    }
    if (this.m.ctx.state === 'suspended') this.m.ctx.resume().catch(() => {});
  }

  now() { return this.m ? this.m.ctx.currentTime + 0.01 : 0; }

  // ----- settings -----

  apply() {
    try { localStorage.setItem(KEY, JSON.stringify(this.settings)); } catch { /* private mode */ }
    if (this.m) {
      const t = this.m.ctx.currentTime, s = this.settings;
      glide(this.m.master.gain, s.muted ? 0 : 1, t, 0.03);
      glide(this.m.sfx.gain, s.sfx, t, 0.03);
      glide(this.m.music.gain, s.music * 0.6, t, 0.03);
    }
    for (const f of this.uis) f();
  }

  toggleMute() {
    this.settings.muted = !this.settings.muted;
    this.apply();
    this.note('mute', { muted: this.settings.muted });
  }

  setVolume(kind, v) {
    this.settings[kind] = clamp(v, 0, 1);
    if (this.settings.muted && v > 0) this.settings.muted = false;
    this.apply();
  }

  // Wires the page's controls: [data-sound=mute] buttons, [data-sound=sfx|music] sliders.
  bindUI(root = document) {
    for (const el of root.querySelectorAll('[data-sound]')) {
      const kind = el.dataset.sound;
      if (kind === 'mute') {
        el.addEventListener('click', (e) => { e.stopPropagation(); this.start(); this.toggleMute(); el.blur(); });
        this.uis.push(() => {
          el.classList.toggle('off', this.settings.muted);
          el.setAttribute('aria-pressed', String(this.settings.muted));
          el.title = this.settings.muted ? 'Sound off (M)' : 'Sound on (M)';
        });
      } else {
        el.addEventListener('input', () => { this.start(); this.setVolume(kind, el.value / 100); });
        el.addEventListener('change', () => { this.sfx('select'); el.blur(); });
        this.uis.push(() => { el.value = Math.round(this.settings[kind] * 100); });
      }
    }
    for (const f of this.uis) f();
  }

  // ----- music -----

  setMusic(name, at) {
    if (name === this.music) return;
    this.music = name;
    this.note('music', { tune: name });
    if (this.seq) this.seq.play(name, at ?? this.now() + 0.05);
  }

  // ----- the log, for tests -----

  note(name, extra) {
    if (!this.log) return;
    const e = { t: +(performance.now() / 1000).toFixed(2), rt: this.rt, name, ...extra };
    if (this.log.length < 20000) this.log.push(e);
    console.debug('[audio]', name, extra ? JSON.stringify(extra) : '');
  }

  // ----- playing -----

  // Where something is, heard from the camera: its loudness (ref metres is full volume), pan
  // and Doppler factor.
  place(x, z, vx = 0, vz = 0, ref = 8) {
    const c = this.cam;
    const dx = x - c.x, dz = z - c.z;
    const d = Math.max(0.5, Math.hypot(dx, dz));
    const gain = Math.min(1, (ref / Math.max(ref, d)) ** 1.3);
    const pan = clamp((dx * c.rx + dz * c.rz) / Math.max(d, 4), -1, 1) * 0.8;
    const vr = ((vx - c.vx) * dx + (vz - c.vz) * dz) / d; // > 0: going away
    return { d, gain, pan, doppler: clamp(343 / (343 + vr), 0.8, 1.25) };
  }

  // Plays an effect: in the middle, or where (x, z) is.
  sfx(name, o = {}) {
    let gain = o.gain ?? 1, pan = 0;
    if (o.x != null) {
      const p = this.place(o.x, o.z, 0, 0, o.ref ?? 10);
      gain *= p.gain;
      pan = p.pan;
      if (gain < 0.03) return;
    }
    this.note(name, o.log);
    if (!this.m || !SFX[name]) return;
    SFX[name]({ m: this.m, t: this.now(), out: this.m.out(gain, pan) }, o);
  }

  // Turns a continuous voice on or off and sets it.
  voice(key, on, make, p, label = key) {
    if (on !== !!this.on[key]) {
      this.on[key] = on;
      this.note(`${label}:${on ? 'on' : 'off'}`, label !== key ? { voice: key } : undefined);
    }
    if (!this.m) return;
    let v = this.voices[key];
    if (!v) {
      if (!on) return;
      v = this.voices[key] = make(this.m);
    }
    v.set(on ? p : { ...p, gain: 0, on: false }, this.now());
  }

  // A new race (or the title's grid).
  reset() {
    this.r = {
      drift: 0, driftT: 0, tier: 0, off: false, kerbT: 0, unpacking: false, unpackT: 0, tickT: 0,
      place: null, pending: null, pendingT: 0, finished: false, raceMusic: false,
      shots: [], nextShot: 1, hits: [], aiSlots: Array(MAX_AI).fill(-1), last: [], lastHaz: [], lastTick: null, throttle: 0, aiThr: [],
    };
  }

  // The engine's events of a step.
  events(list, view, me) {
    const ks = view.karts;
    for (const e of list) {
      const k = ks[e.kart];
      const mine = e.kart === me;
      const at = (kk, extra = {}) => (mine ? { ...extra } : { x: kk.x, z: kk.z, ...extra });
      switch (e.kind) {
        case 'count':
          this.sfx(e.n ? 'count' : 'go', { log: { n: e.n } });
          break;
        case 'pickup':
          this.sfx('pickup', at(k, { log: { kart: e.kart } }));
          break;
        case 'use': {
          const kind = this.kinds[e.item];
          const o = at(k, { log: { kart: e.kart, item: e.item } });
          if (kind === 'throw') this.sfx('throw', o);
          else if (kind === 'drop') this.sfx('glug', o);
          else if (kind === 'boost') { this.sfx('sizzle', o); this.sfx('pepper', o); }
          else if (kind === 'roll') this.sfx('rollOut', o);
          else if (kind === 'aim') this.sfx('aim', o);
          break;
        }
        case 'hit': {
          const kind = this.kinds[e.by];
          const o = at(k, { log: { kart: e.kart, by: e.by } });
          this.r.hits.push({ by: e.by, x: k.x, z: k.z });
          if (kind === 'throw') this.sfx('splat', o);
          else if (kind === 'drop' || e.effect === 'spin') this.sfx('slip', o);
          else this.sfx('thump', o);
          if (e.effect === 'bump') this.sfx('dizzy', o);
          if (mine) this.sfx('ouch');
          break;
        }
        case 'boost':
          this.sfx('driftBoost', at(k, { tier: e.tier, gain: mine ? 1 : 0.6, log: { kart: e.kart, tier: e.tier } }));
          break;
        case 'pad':
          this.sfx('pad', at(k, { gain: mine ? 1 : 0.6, log: { kart: e.kart } }));
          break;
        case 'wall':
          this.sfx(e.hard ? 'wall' : 'scrape', at(k, { log: { kart: e.kart } }));
          break;
        case 'bump': {
          const a = ks[e.a], b = ks[e.b];
          if (e.a === me || e.b === me) this.sfx('bonk', { log: { a: e.a, b: e.b } });
          else this.sfx('bonk', { x: (a.x + b.x) / 2, z: (a.z + b.z) / 2, log: { a: e.a, b: e.b } });
          break;
        }
        case 'lap':
          if (mine) {
            if (e.lap === this.laps) {
              this.sfx('finalLap', { log: { lap: e.lap } });
              if (this.seq && this.seq.name === 'race') this.seq.rate = 1.08;
            } else this.sfx('lap', { log: { lap: e.lap } });
          }
          break;
        case 'finish':
          if (mine) {
            this.r.finished = true;
            this.setMusic('off');
            this.sfx('finish', { win: e.place === 1, log: { place: e.place } });
          }
          break;
        default: break;
      }
    }
  }

  // Each frame: the listener, the engines, tyres, items on the road, the sirens, the music.
  // st: { view, me, mode, dt, cam: { x, z, rx, rz }, throttle (the player's, or null), laps }
  update(dt, st) {
    const { view, me, mode } = st;
    if (!view) return;
    if (st.laps) this.laps = st.laps;
    this.rt = +view.time.toFixed(2);
    if (mode !== this.mode) this.enter(mode, this.mode);
    const r = this.r;
    // The listener: the camera, and how fast it moves (for Doppler).
    const c = this.cam;
    if (dt > 0) {
      const vx = (st.cam.x - c.x) / dt, vz = (st.cam.z - c.z) / dt;
      const ok = Math.hypot(vx, vz) < 80;
      c.vx = ok ? c.vx + (vx - c.vx) * Math.min(1, dt * 8) : 0;
      c.vz = ok ? c.vz + (vz - c.vz) * Math.min(1, dt * 8) : 0;
    }
    Object.assign(c, { x: st.cam.x, z: st.cam.z, rx: st.cam.rx, rz: st.cam.rz });
    const racing = mode === 'race' || mode === 'watch';
    const ks = view.karts;
    // What changed since the engine last stepped: each kart's velocity, from its positions, and
    // whether it is speeding up (the throttle of karts whose input we do not see).
    const steps = r.lastTick == null ? 0 : view.tick - r.lastTick;
    if (steps !== 0) {
      const sdt = steps / 60;
      r.vel = ks.map((k, i) => {
        const l = r.last[i];
        return l && steps > 0 && steps < 30 ? { vx: (k.x - l.x) / sdt, vz: (k.z - l.z) / sdt } : { vx: 0, vz: 0 };
      });
      r.thrWant = ks.map((k, i) => {
        const l = r.last[i];
        if (!l || steps < 0) return 0.5;
        const dv = Math.abs(k.speed) - Math.abs(l.speed);
        return dv > 0.005 * steps || Math.abs(k.speed) > 24 ? 1 : 0.15;
      });
      r.hazVel = (view.hazards || []).map((h, i) => {
        const l = r.lastHaz[i];
        return l && steps > 0 && steps < 30 ? { vx: (h.x - l.x) / sdt, vz: (h.z - l.z) / sdt } : { vx: 0, vz: 0 };
      });
    }
    r.lastTick = view.tick;
    r.last = ks.map((k) => ({ x: k.x, z: k.z, speed: k.speed }));
    r.lastHaz = (view.hazards || []).map((h) => ({ x: h.x, z: h.z }));
    const vel = r.vel || [];
    // The music: the race tune from the start to the player's finish.
    if (racing && view.phase === 'race' && !r.finished && !r.raceMusic) { r.raceMusic = true; this.setMusic('race'); }
    if (racing && view.phase === 'countdown') this.setMusic('off');

    // The player's kart.
    const k = ks[me];
    const thrWant = st.throttle != null ? st.throttle : (r.thrWant?.[me] ?? 0.5);
    r.throttle += (thrWant - r.throttle) * Math.min(1, dt * 12);
    const speed = Math.abs(k.speed);
    this.voice('engine', racing, (m) => new EngineVoice(m, 0.3),
      { speed: k.speed, throttle: r.throttle, boost: k.boost, gain: 1, pan: 0, doppler: 1, skid: 0 });
    const sliding = racing && ((k.drift && speed > 6) || (k.spin && speed > 3));
    this.voice('squeal', sliding, (m) => new SquealVoice(m), { gain: 1, amount: k.spin ? 1 : clamp(speed / 22, 0.3, 1) });
    // The drift charge, and a blip at each tier.
    if (racing && k.drift) {
      if (!r.drift) { r.driftT = 0; r.tier = 0; this.note('drift'); }
      r.driftT += dt;
      if (k.tier > r.tier) this.sfx('tier', { tier: k.tier, log: { tier: k.tier } });
      r.tier = k.tier;
    }
    r.drift = racing ? k.drift : 0;
    const [c1, c2] = this.charge;
    const level = k.tier === 2 ? (r.driftT - c2) / 1 : k.tier === 1 ? (r.driftT - c1) / (c2 - c1) : r.driftT / c1;
    this.voice('charge', racing && !!k.drift && speed > 5, (m) => new ChargeVoice(m), { gain: 1, level, tier: k.tier });
    // Off the road: the kerb as it goes over, and the rumble.
    r.kerbT -= dt;
    if (racing && k.off !== r.off && speed > 4 && r.kerbT <= 0) { this.sfx('kerb'); r.kerbT = 0.3; }
    r.off = k.off;
    this.voice('rumble', racing && k.off && speed > 2, (m) => new RumbleVoice(m), { gain: 1, speed });
    // Unpacking a basket: rummaging, slower and slower, then the item.
    if (racing && k.unpacking) {
      if (!r.unpacking) { r.unpackT = 0; r.tickT = 0; }
      r.unpackT += dt;
      r.tickT -= dt;
      if (r.tickT <= 0) { this.sfx('tick'); r.tickT = 0.06 + 0.14 * clamp(r.unpackT / this.unpack, 0, 1); }
    } else if (r.unpacking && racing && k.item) this.sfx('reveal', { log: { item: k.item } });
    r.unpacking = racing && k.unpacking;
    // Places: a blip when one changes and holds for a moment.
    if (racing && view.phase === 'race' && k.time == null) {
      if (r.place == null) r.place = k.place;
      if (k.place !== r.place) {
        if (k.place !== r.pending) { r.pending = k.place; r.pendingT = 0; }
        r.pendingT += dt;
        if (r.pendingT > 0.35) {
          this.sfx('place', { up: k.place < r.place, log: { from: r.place, to: k.place } });
          r.place = k.place;
          r.pending = null;
        }
      } else r.pending = null;
    }

    // The computer's karts: the nearest have an engine each.
    const near = racing
      ? ks.map((o, i) => ({ i, d: Math.hypot(o.x - c.x, o.z - c.z) })).filter((o) => o.i !== me).sort((a, b) => a.d - b.d).slice(0, MAX_AI).map((o) => o.i)
      : [];
    const slots = r.aiSlots;
    for (let s = 0; s < MAX_AI; s++) if (!near.includes(slots[s])) slots[s] = -1;
    for (const i of near) if (!slots.includes(i)) slots[slots.indexOf(-1)] = i;
    for (let s = 0; s < MAX_AI; s++) {
      const i = slots[s];
      if (i < 0) { this.voice(`ai${s}`, false, null, {}, 'engine-ai'); continue; }
      const o = ks[i];
      const v = vel[i] || { vx: 0, vz: 0 };
      const p = this.place(o.x, o.z, v.vx, v.vz, 7);
      const prevThr = r.aiThr[i] ?? 0.5;
      r.aiThr[i] = prevThr + ((r.thrWant?.[i] ?? 0.5) - prevThr) * Math.min(1, dt * 6);
      const os = Math.abs(o.speed);
      this.voice(`ai${s}`, true, (m) => new EngineVoice(m, 0.17), {
        speed: o.speed, throttle: r.aiThr[i], boost: o.boost, gain: p.gain, pan: p.pan, doppler: p.doppler,
        skid: (o.drift && os > 6) || (o.spin && os > 3) ? 0.7 : 0,
      }, 'engine-ai');
    }

    // Items on the road: rolling wheels and sushi, the sushi's aiming beeps, and thumps when they
    // end against something.
    this.items(dt, view, racing);

    // The ambulances.
    (view.hazards || []).forEach((h, i) => {
      const hv = r.hazVel?.[i] || { vx: 0, vz: 0 };
      const p = this.place(h.x, h.z, hv.vx, hv.vz, 14);
      this.voice(`siren${i}`, racing && h.moving && p.gain > 0.02, (m) => new SirenVoice(m),
        { on: true, gain: p.gain, pan: p.pan, doppler: p.doppler }, 'siren');
    });
    r.hits = [];
  }

  items(dt, view, racing) {
    const r = this.r;
    const shots = racing ? view.shots : [];
    // Who is who from frame to frame: the nearest of the same item.
    const prev = r.shots;
    const used = new Set();
    const now = shots.map((s) => {
      let best = null, bd = 12;
      for (const p of prev) {
        if (used.has(p) || p.item !== s.item) continue;
        const d = Math.hypot(p.x - s.x, p.z - s.z);
        if (d < bd) { bd = d; best = p; }
      }
      if (best) used.add(best);
      return { id: best ? best.id : r.nextShot++, item: s.item, x: s.x, z: s.z, vx: s.vx, vz: s.vz, aiming: s.aiming, beep: best ? best.beep : 0, wasAiming: best ? best.aiming : s.aiming };
    });
    for (const p of prev) {
      if (used.has(p) || !racing) continue;
      // Gone without hitting a kart: into a wall, or its time ran out.
      const kind = this.kinds[p.item];
      const hit = r.hits.some((h) => h.by === p.item && Math.hypot(h.x - p.x, h.z - p.z) < 10);
      if (!hit && (kind === 'roll' || kind === 'aim')) this.sfx('thump', { x: p.x, z: p.z, gain: kind === 'aim' ? 0.5 : 1, log: { item: p.item } });
    }
    r.shots = now;
    // Rolling voices for the two nearest rolling things; beeps while sushi aims.
    const rolling = [];
    for (const s of now) {
      const kind = this.kinds[s.item];
      if (kind === 'aim' && s.aiming) {
        s.beep -= dt;
        if (s.beep <= 0) { this.sfx('aim', { x: s.x, z: s.z, ref: 12 }); s.beep = 0.14; }
      } else if (kind === 'aim' && s.wasAiming) this.sfx('sushiGo', { x: s.x, z: s.z, ref: 12, log: { item: s.item } });
      if ((kind === 'roll' || kind === 'aim') && !s.aiming) rolling.push({ s, kind, p: this.place(s.x, s.z, s.vx, s.vz, 8) });
    }
    rolling.sort((a, b) => a.p.d - b.p.d);
    for (let i = 0; i < 2; i++) {
      const o = rolling[i];
      this.voice(`roll${i}`, !!o && o.p.gain > 0.02, (m) => new RollVoice(m),
        o ? { kind: o.kind, speed: Math.hypot(o.s.vx, o.s.vz), gain: o.p.gain, pan: o.p.pan, doppler: o.p.doppler } : {}, 'roll');
    }
  }

  // The game moves between its screens.
  enter(mode, from) {
    this.mode = mode;
    const m = this.m;
    if (m) {
      const t = m.ctx.currentTime;
      glide(m.duck.gain, mode === 'paused' ? 0 : 1, t, 0.05);
      glide(m.music.gain, this.settings.music * 0.6 * (mode === 'paused' ? 0.4 : 1), t, 0.1);
    }
    if (mode === 'title') this.setMusic('menu');
    else if (mode === 'results') this.setMusic('menu', this.now() + 2.5);
    else if ((mode === 'race' || mode === 'watch') && from !== 'paused') this.setMusic('off');
  }
}
