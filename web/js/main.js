// OpenKart in the browser: loads the engine and a game, draws the track and runs races at the
// engine's fixed 60 steps a second.
//
// URL parameters: game=<id> (default pachino-gp), track=<id>, racer=<id>, seed=<text>;
// race=1 starts a race at once, auto=1 lets the computer drive your kart too, at=<seconds>
// skips that far into the race (handy for screenshots), cam=far for a higher camera.

import * as THREE from 'three';
import { GLTFLoader } from '../vendor/jsm/loaders/GLTFLoader.js';
import { Engine, fetchGame } from './engine.js';
import { World } from './world.js';
import { makeRacer, itemModel, slickModel, hazardModel, Particles, lookOf } from './karts.js';
import { Hud, Snapshots, clock, ordinal } from './hud.js';
import { Controls } from './input.js';

const $ = (id) => document.getElementById(id);
const params = new URLSearchParams(location.search);
const STEP = 1 / 60;

function fail(e) {
  console.error(e);
  const el = $('error');
  el.textContent = String(e && e.message ? e.message : e);
  el.hidden = false;
}

function progress(f, text) {
  $('loadbar').style.width = `${Math.round(f * 100)}%`;
  if (text) $('loadsub').textContent = text;
}

// ---------- three.js ----------

const canvas = $('scene');
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.shadowMap.enabled = true;
renderer.shadowMap.type = THREE.PCFSoftShadowMap;
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.toneMappingExposure = 1.05;
const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(68, 1, 0.3, 1400);
const hemi = new THREE.HemisphereLight('#fff3dc', '#8a7458', 1.5);
scene.add(hemi);
const sun = new THREE.DirectionalLight('#fff1d6', 2.6);
sun.castShadow = true;
sun.shadow.mapSize.set(2048, 2048);
const sc = sun.shadow.camera;
sc.left = -70; sc.right = 70; sc.top = 70; sc.bottom = -70; sc.near = 1; sc.far = 400;
sun.shadow.bias = -0.0006;
scene.add(sun, sun.target);

function resize() {
  const w = innerWidth, h = innerHeight;
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
}
addEventListener('resize', resize);
resize();

// ---------- state ----------

let engine, cat, course, world, hud, controls, particles;
const models = { racers: {}, karts: {}, items: {}, props: {}, hazards: {} };
let racerObjs = [];
let shotObjs = [], slickObjs = [], hazardObjs = [];
let view = null, prev = null;
let mode = 'loading'; // title, race, paused, results, watch
let me = 0;
let chosen = params.get('racer') || null;
let trackId = params.get('track');
let acc = 0, tick = 0;
let replay = null; // inputs being watched
let lastRace = null;
let finishedAt = null;
let shake = 0;
let splatT = 0;
let attractS = -1;
const camPos = new THREE.Vector3(), camLook = new THREE.Vector3();
let camInit = false;

async function loadModels() {
  const loader = new GLTFLoader();
  const dirs = Object.fromEntries(cat.game.assets);
  const want = [];
  for (const r of cat.racers) {
    want.push(['racers', r.model], ['karts', r.kart]);
  }
  for (const it of cat.items) if (it.model && !it.model.includes(':')) want.push(['items', it.model]);
  for (const h of cat.hazards) want.push(['hazards', h.model]);
  for (const p of course.props) want.push(['props', p.model]);

  const uniq = [...new Map(want.map((w) => [w.join('/'), w])).values()];
  let done = 0;
  await Promise.all(uniq.map(([kind, name]) => new Promise((res) => {
    const dir = dirs[kind] || `models/${kind}`;
    loader.load(`${dir}/${name}`, (g) => {
      // glTF's default metalness is 1: plain-coloured models without it come out black.
      g.scene.traverse((o) => {
        if (!o.isMesh) return;
        for (const m of [].concat(o.material)) if (m.metalness === 1 && !m.metalnessMap) { m.metalness = 0; m.roughness = Math.max(m.roughness, 0.7); }
      });
      models[kind][name] = g.scene;
      progress(0.4 + 0.5 * (++done / uniq.length), `Loading ${name}…`);
      res();
    }, undefined, (e) => { console.warn(`${dir}/${name}`, e); res(); });
  })));
}

async function boot() {
  progress(0.05, 'Starting the engine…');
  engine = await Engine.load();
  const gameId = params.get('game') || 'pachino-gp';
  progress(0.15, 'Reading the game files…');
  const files = await fetchGame(gameId);
  cat = engine.call({ op: 'load', files });
  trackId = trackId || cat.game.first;
  course = engine.call({ op: 'course', track: trackId });
  progress(0.3, 'Building Bologna…');
  await loadModels();
  progress(0.92, 'Painting the porticoes…');
  const look = course.look || {};
  const sky = new THREE.Color('#' + (look.sky || '9fc8e8'));
  scene.background = sky;
  scene.fog = new THREE.Fog(sky, 180, 900);
  world = new World(scene, course, models);
  particles = new Particles(scene);
  hud = new Hud();
  hud.setCourse(course, cat.racers);
  controls = new Controls();
  controls.onPause = () => { if (mode === 'race') pause(true); else if (mode === 'paused') pause(false); };
  // Icons and portraits from the models themselves.
  const snaps = new Snapshots(256);
  for (const it of cat.items) {
    const img = new Image();
    img.src = snaps.shot(itemModel(it, models), { yaw: 0.5, pitch: 0.45, fill: 0.85 });
    hud.icons[it.id] = img;
  }
  const portraits = {};
  for (const r of cat.racers) portraits[r.id] = snaps.shot(makeRacer(r, models).root, { yaw: 0.65, pitch: 0.3, fill: 0.78 });
  snaps.renderer.dispose();
  buildTitle(portraits);
  $('loading').hidden = true;
  if (!chosen || !cat.racers.some((r) => r.id === chosen)) chosen = cat.racers[0].id;
  selectRacer(chosen);
  if (params.get('race')) startRace();
  else showTitle();
  requestAnimationFrame(frame);
}

// ---------- title ----------

function buildTitle(portraits) {
  const t = cat.tracks.find((x) => x.id === trackId);
  $('gameName').textContent = cat.game.name;
  $('trackName').textContent = t.name;
  $('trackFacts').textContent = `${(t.length / 1000).toFixed(2)} km · ${t.laps} laps`;
  $('trackText').textContent = t.text;
  const box = $('racers');
  box.innerHTML = '';
  for (const r of cat.racers) {
    const b = document.createElement('button');
    b.className = 'racer';
    b.dataset.id = r.id;
    const bars = (v) => `<i>${[1, 2, 3, 4, 5].map((k) => `<u class="${k <= v ? 'f' : ''}"></u>`).join('')}</i>`;
    b.innerHTML = `<img src="${portraits[r.id]}" alt="" style="width:100%;height:84px;object-fit:contain;display:block;margin:-6px 0 0">
      <span class="nm"><span class="dot" style="background:${r.colour}"></span>${r.name}</span>
      <span class="who">${r.text}</span>
      <span class="stat">Speed ${bars(r.speed)}</span><span class="stat">Accel ${bars(r.accel)}</span>
      <span class="stat">Grip ${bars(r.grip)}</span><span class="stat">Weight ${bars(r.weight)}</span>`;
    b.onclick = () => selectRacer(r.id);
    box.appendChild(b);
  }
  $('go').onclick = () => startRace();
  $('again').onclick = () => startRace();
  $('menu').onclick = () => showTitle();
  $('watch').onclick = () => watchReplay();
  $('resume').onclick = () => pause(false);
  $('quit').onclick = () => { $('pause').hidden = true; showTitle(); };
}

function selectRacer(id) {
  chosen = id;
  for (const b of document.querySelectorAll('.racer')) b.classList.toggle('on', b.dataset.id === id);
}

function showTitle() {
  mode = 'title';
  $('results').hidden = true;
  $('title').hidden = false;
  $('touch').hidden = true;
  hud.show(false);
  // The racers on the grid, and the camera flying round the course.
  newRace(params.get('seed') || 'title', true);
}

// ---------- races ----------

function entries(allComputer) {
  // The player starts at the back of the grid.
  const others = cat.racers.filter((r) => r.id !== chosen).slice(0, cat.game.field - 1);
  const list = others.map((r) => ({ racer: r.id, human: false }));
  list.push({ racer: chosen, human: !allComputer });
  return list;
}

function clearRacers() {
  for (const o of [...racerObjs, ...shotObjs, ...slickObjs, ...hazardObjs]) scene.remove(o.root || o);
  racerObjs = []; shotObjs = []; slickObjs = []; hazardObjs = [];
}

function newRace(seed, allComputer, inputs = null) {
  clearRacers();
  const es = entries(allComputer);
  const answer = engine.call({ op: 'race', seed, track: trackId, entries: es });
  view = answer.view; prev = view;
  me = view.karts.findIndex((k) => k.racer === chosen);
  racerObjs = view.karts.map((k) => {
    const o = makeRacer(cat.racers.find((r) => r.id === k.racer), models);
    scene.add(o.root);
    return o;
  });
  hazardObjs = view.hazards.map((h) => {
    const o = hazardModel(cat.hazards.find((x) => x.id === h.id), models);
    scene.add(o);
    return o;
  });
  tick = 0; acc = 0; camInit = false; finishedAt = null; splatT = 0;
  lastSetup = { seed, entries: es };
  replay = inputs;
}
let lastSetup = null;

function startRace() {
  $('title').hidden = true;
  $('results').hidden = true;
  $('pause').hidden = true;
  const seed = params.get('seed') || String(Date.now() % 1e9);
  const auto = !!params.get('auto');
  newRace(seed, auto);
  mode = 'race';
  hud.show(true);
  hud.lastItem = undefined;
  $('touch').hidden = !(matchMedia('(pointer: coarse)').matches || controls.usedTouch);
  const at = +(params.get('at') || 0);
  if (at > 0) {
    const n = Math.round((at + cat.physics.countdown) * 60);
    const a = engine.call({ op: 'step', n, inputs: auto ? [] : [{ gas: true }] });
    view = a.view; prev = view; tick += n;
  }
}

function watchReplay() {
  if (!lastRace) return;
  $('results').hidden = true;
  newRace(lastRace.setup.seed, false, lastRace.inputs);
  // In a replay the player's kart is driven by the saved inputs.
  mode = 'watch';
  hud.show(true);
  hud.lastItem = undefined;
}

function pause(on) {
  if (on && mode === 'race') { mode = 'paused'; $('pause').hidden = false; }
  else if (!on && mode === 'paused') { mode = 'race'; $('pause').hidden = true; acc = 0; }
}

function showResults() {
  mode = 'results';
  $('touch').hidden = true;
  const st = engine.call({ op: 'standings' }).standings;
  if (!replay) lastRace = engine.call({ op: 'save' });
  const mine = st.find((s) => s.kart === me);
  $('resultTitle').textContent = mine ? (mine.place === 1 ? 'You win!' : `${mine.place}${ordinal(mine.place)} place`) : 'Results';
  $('standings').innerHTML = st.map((s) => {
    const r = cat.racers.find((x) => x.id === s.racer);
    return `<tr class="${s.kart === me ? 'me' : ''}"><td class="n">${s.place}</td><td><span class="dot" style="display:inline-block;width:12px;height:12px;border-radius:50%;background:${r.colour};border:2px solid #2a1d17"></span> ${r.name}</td>
      <td class="t">${s.time != null ? clock(s.time) : 'DNF'}<br><small>best lap ${clock(s.best_lap)}</small></td></tr>`;
  }).join('');
  $('results').hidden = false;
  $('watch').hidden = !lastRace;
}

// ---------- events ----------

function onEvents(events) {
  for (const e of events) {
    const mine = e.kart === me;
    switch (e.kind) {
      case 'count': hud.count(e.n); world.setLights(e.n ? [0, 5, 4, 2][e.n] || 1 : 0); beep(e.n ? 440 : 880, e.n ? 0.12 : 0.35); break;
      case 'lap':
        if (mine) hud.banner(e.lap === course.laps ? 'Last lap!' : `Lap ${e.lap}`);
        break;
      case 'finish':
        if (mine) { hud.banner(e.place === 1 ? 'Arrivo! First!' : `Arrivo! ${e.place}${ordinal(e.place)}`, 3); finishedAt = performance.now(); }
        break;
      case 'hit': {
        const k = view.karts[e.kart];
        particles.emit(k.x, 1.2, k.z, e.effect === 'splat' ? '#d12a1d' : e.effect === 'spin' ? '#c9b43a' : '#ffe08a', { n: 24, speed: 6, up: 4, life: 0.7 });
        if (mine) { shake = 0.5; if (e.effect === 'splat') splatT = 2.6; }
        break;
      }
      case 'boost':
        if (mine) shake = 0.15;
        break;
      case 'wall':
        if (mine && e.hard) shake = 0.35;
        break;
      case 'pad': case 'pickup': case 'use': break;
      default: break;
    }
  }
}

let audio = null;
function beep(freq, dur) {
  try {
    audio ||= new AudioContext();
    const o = audio.createOscillator(), g = audio.createGain();
    o.type = 'square'; o.frequency.value = freq;
    g.gain.setValueAtTime(0.06, audio.currentTime);
    g.gain.exponentialRampToValueAtTime(0.001, audio.currentTime + dur);
    o.connect(g).connect(audio.destination);
    o.start(); o.stop(audio.currentTime + dur);
  } catch { /* no sound */ }
}

// ---------- drawing ----------

const lerp = (a, b, t) => a + (b - a) * t;

function drawRacers(dt, alpha) {
  view.karts.forEach((k, i) => {
    const p = prev.karts[i] || k;
    const o = racerObjs[i];
    const x = lerp(p.x, k.x, alpha), z = lerp(p.z, k.z, alpha);
    o.root.position.set(x, 0.05, z);
    const yaw = Math.atan2(k.hx, k.hz);
    o.root.rotation.y = yaw;
    // Drift: the body swings out; spin: it twirls.
    const want = k.drift ? -k.drift * 0.38 : 0;
    o.yaw = lerp(o.yaw, want, Math.min(1, dt * 8));
    if (k.drift && !o.prevDrift) o.hop = 1;
    o.prevDrift = k.drift;
    o.hop = Math.max(0, o.hop - dt * 5);
    o.body.rotation.y = o.yaw;
    o.body.position.y = Math.sin(o.hop * Math.PI) * 0.35 + (k.bump ? Math.abs(Math.sin(performance.now() / 60)) * 0.3 : 0);
    o.body.rotation.z = k.off ? Math.sin(performance.now() / 40) * 0.03 : 0;
    for (const w of o.wheels) w.rotation.x += k.speed * dt / 0.3;
    o.splat.visible = k.splat > 0;
    o.flames.visible = k.boost;
    if (k.boost) for (const f of o.flames.children) f.scale.set(1, 0.7 + Math.random() * 0.6, 1);
    // Sparks from the back wheels while drifting, green then red as the charge builds.
    if (k.drift && k.tier > 0) {
      const col = k.tier === 2 ? '#ff4a1c' : '#7ccf3a';
      for (const side of [-0.7, 0.7]) {
        const bx = x - k.hx * 1.1 + k.hz * side, bz = z - k.hz * 1.1 - k.hx * side;
        particles.emit(bx, 0.25, bz, col, { n: 1, speed: 2.5, up: 2, life: 0.35 });
      }
    } else if (k.drift) {
      particles.emit(x - k.hx * 1.1, 0.2, z - k.hz * 1.1, '#e8e2d6', { n: 1, speed: 1.5, up: 1, life: 0.3, grav: 1 });
    }
    if (k.off && Math.abs(k.speed) > 6) particles.emit(x - k.hx, 0.2, z - k.hz, '#bfae8c', { n: 1, speed: 2, up: 1.5, life: 0.5, grav: 2 });
  });
}

function syncThings(list, objs, make, alpha, prevList) {
  while (objs.length > list.length) scene.remove(objs.pop());
  list.forEach((t, i) => {
    if (!objs[i] || objs[i].userData.item !== t.item) {
      if (objs[i]) scene.remove(objs[i]);
      objs[i] = make(t);
      objs[i].userData.item = t.item;
      scene.add(objs[i]);
    }
    const p = prevList && prevList[i] && prevList[i].item === t.item ? prevList[i] : t;
    objs[i].position.set(lerp(p.x, t.x, alpha), 0, lerp(p.z, t.z, alpha));
  });
}

function drawThings(dt, alpha) {
  const now = performance.now() / 1000;
  syncThings(view.shots, shotObjs, (t) => {
    const g = new THREE.Group();
    const m = itemModel(cat.items.find((x) => x.id === t.item), models);
    g.add(m);
    g.userData.inner = m;
    return g;
  }, alpha, prev.shots);
  view.shots.forEach((t, i) => {
    const o = shotObjs[i];
    const def = cat.items.find((x) => x.id === t.item);
    const sp = Math.hypot(t.vx, t.vz);
    if (sp > 0.5) o.rotation.y = Math.atan2(t.vx, t.vz);
    const inner = o.userData.inner;
    if (def.kind === 'throw') { inner.scale.setScalar(1.5); inner.position.y = 0.9 + Math.sin(now * 9) * 0.15; inner.rotation.x += dt * 12; }
    else if (def.kind === 'roll') { inner.position.y = 0.85; inner.rotation.x += sp * dt / 0.85; }
    else if (def.kind === 'aim') {
      inner.position.y = 0.2;
      inner.scale.setScalar(1.3);
      if (t.aiming) inner.position.x = Math.sin(now * 50) * 0.06;
      else inner.rotation.x += sp * dt / 0.4;
    }
  });
  syncThings(view.slicks, slickObjs, () => {
    const g = slickModel();
    return g;
  }, 1, null);
  view.hazards.forEach((h, i) => {
    const o = hazardObjs[i];
    if (!o) return;
    const p = prev.hazards[i] || h;
    o.position.set(lerp(p.x, h.x, alpha), 0.02, lerp(p.z, h.z, alpha));
    o.rotation.y = Math.atan2(h.hx, h.hz);
    const l = o.userData.light;
    if (l) l.material.emissiveIntensity = h.moving && Math.floor(now * 6) % 2 ? 3 : 0.3;
  });
}

function chase(dt, k, far) {
  const back = far ? 16 : 7.2, up = far ? 9 : 3.1;
  const hx = k.hx, hz = k.hz;
  const want = new THREE.Vector3(k.x - hx * back, up, k.z - hz * back);
  const look = new THREE.Vector3(k.x + hx * 5, 1.2, k.z + hz * 5);
  if (!camInit) { camPos.copy(want); camLook.copy(look); camInit = true; }
  const f = 1 - Math.exp(-dt * 6);
  camPos.lerp(want, f);
  camLook.lerp(look, 1 - Math.exp(-dt * 10));
  camera.position.copy(camPos);
  if (shake > 0) {
    camera.position.x += (Math.random() - 0.5) * shake;
    camera.position.y += (Math.random() - 0.5) * shake;
    shake = Math.max(0, shake - dt);
  }
  camera.lookAt(camLook);
  const fov = 68 + (k.boost ? 8 : 0) + Math.min(6, Math.abs(k.speed) / 6);
  camera.fov = lerp(camera.fov, fov, Math.min(1, dt * 4));
  camera.updateProjectionMatrix();
}

// The title camera: a slow flight along the course, ahead of the grid.
function attract(dt) {
  const S = course.track.samples;
  const n = S.length;
  if (attractS < 0) attractS = course.track.length - 45;
  attractS = (attractS + dt * 9) % course.track.length;
  const i = Math.floor((attractS / course.track.length) * n) % n;
  const a = S[i], b = S[(i + 12) % n];
  const side = 7;
  camera.position.set(a.x + a.tz * side * 0.6 - a.tx * 4, 7.5, a.z - a.tx * side * 0.6 - a.tz * 4);
  camera.lookAt(b.x, 2, b.z);
  camera.fov = 60; camera.updateProjectionMatrix();
}

function placeSun(x, z) {
  sun.position.set(x + 60, 140, z + 40);
  sun.target.position.set(x, 0, z);
}

// ---------- the loop ----------

let last = performance.now();
function frame(now) {
  requestAnimationFrame(frame);
  const dt = Math.min(0.1, (now - last) / 1000);
  last = now;
  if (!view) return;
  if (mode === 'race' || mode === 'watch') {
    acc += dt;
    const input = controls.read(dt);
    let steps = 0;
    while (acc >= STEP && steps < 8) {
      acc -= STEP; steps++;
      let inputs;
      if (mode === 'watch') inputs = replay && replay[tick] ? replay[tick] : [];
      else inputs = params.get('auto') ? [] : [input];
      prev = view;
      const a = engine.call({ op: 'step', inputs });
      view = a.view;
      tick++;
      onEvents(a.events);
    }
    if (steps === 8) acc = 0;
    // After the player's finish, the others are raced to the line quickly.
    const k = view.karts[me];
    if (k.time != null && finishedAt && performance.now() - finishedAt > 3500 && view.phase !== 'over') {
      const a = engine.call({ op: 'step', n: 60 * 120, inputs: [] });
      prev = view = a.view;
    }
    if (view.phase === 'over' && (!finishedAt || performance.now() - finishedAt > 2500)) showResults();
  }
  const alpha = mode === 'race' || mode === 'watch' ? Math.min(1, acc / STEP) : 1;
  drawRacers(dt, alpha);
  drawThings(dt, alpha);
  world.update(now / 1000, view);
  particles.update(dt);
  const k = view.karts[me];
  if (mode === 'title') attract(dt);
  else chase(dt, { ...k, x: lerp(prev.karts[me].x, k.x, alpha), z: lerp(prev.karts[me].z, k.z, alpha) }, params.get('cam') === 'far');
  placeSun(camera.position.x, camera.position.z);
  if (mode === 'race' || mode === 'watch' || mode === 'paused' || mode === 'results') hud.update(dt, view, me, course.laps, cat.items);
  // A tomato on the windscreen.
  splatT = Math.max(0, splatT - dt);
  const sp = $('splat');
  if (splatT > 0) {
    if (!sp.dataset.on) {
      sp.dataset.on = '1';
      const blobs = [];
      for (let i = 0; i < 9; i++) blobs.push(`radial-gradient(circle at ${10 + Math.random() * 80}% ${15 + Math.random() * 70}%, rgba(205,38,28,.92) 0 ${4 + Math.random() * 7}%, rgba(205,38,28,0) ${8 + Math.random() * 8}%)`);
      sp.style.background = blobs.join(',');
    }
    sp.style.opacity = Math.min(1, splatT / 0.8);
  } else if (sp.dataset.on) { delete sp.dataset.on; sp.style.opacity = 0; }
  renderer.render(scene, camera);
}

boot().catch(fail);
window.openkart = { get view() { return view; }, get engine() { return engine; } };
