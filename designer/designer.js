// The OpenKart circuit designer: an OpenStreetMap background (Leaflet), the streets of a box
// fetched from the Overpass API, and a circuit drawn on them by clicking points (routed over
// the street graph) or picking streets. The routing, the checks, the exact `route way` lines and
// the bake are osm2track's own code, compiled to WebAssembly in the engine (see
// crates/openkart-web): `extract`, `snap`, `drive`, `tidy`, `check`, `bake`.
//
// Test-only URL flags (for headless runs): extract=<url of an Overpass JSON file> loads it
// instead of asking Overpass; tiles=none leaves the background blank; view=lat,lon,zoom;
// points=lat,lon;lat,lon... clicks those points; closed=1 ticks "streets closed";
// id=, name=, text=, laps=; start=lat,lon sets the start line; pads=lat,lon;...;
// dress=tree@Street;lamp;... adds scenery from the palette (along a street, or everywhere);
// bake=1 bakes and opens the 3D preview; load=1 clicks "Load streets here" (asks Overpass).

import { Engine } from '../play/js/engine.js';

const $ = (id) => document.getElementById(id);
const params = new URLSearchParams(location.search);
const L = window.L;

const OVERPASS = 'https://overpass-api.de/api/interpreter';
const MAX_SIDE = 3200; // metres: about 3 x 3 km at most
const DRIVABLE = new Set(['motorway', 'trunk', 'primary', 'secondary', 'tertiary', 'motorway_link', 'trunk_link',
  'primary_link', 'secondary_link', 'tertiary_link', 'unclassified', 'residential', 'living_street', 'service',
  'pedestrian', 'road']);
// The scenery a game's `props` folder has, with the settings that look right in Pachino GP.
const PALETTE = [
  { kind: 'tree', label: 'Plane trees', model: 'tree_oak.glb', every: 13, offset: 2, scale: '6.5 9', face: 'random', colour: '#5d8a3a' },
  { kind: 'lamp', label: 'Lamp posts', model: 'lightPostModern.glb', every: 34, offset: 0.6, scale: '8', face: 'road', colour: '#4a4f57' },
  { kind: 'cone', label: 'Cones', model: 'cone.glb', every: 9, offset: 0.4, scale: '1.5', face: 'road', colour: '#e8772e' },
];
const LOOK = ['look sky 9fc8e8', 'look ground c9b79a', 'look palette c8553d d9a441 b5452f e0b36a cf7a52 a9432f e6c88e', 'look start gantry'];
const STORE = 'openkart-designer';

// ---------- state ----------

const S = {
  actions: [], // {t: 'via', node, lat, lon} | {t: 'way', id, lat, lon} | {t: 'span', span}
  legs: [], // the circuit: exact spans {id, forward, from, to}
  wps: [], // the clicked points, for the map
  closing: 0, // how many of the legs close the loop back to the first point
  report: null,
  start: null, // a place: "Via Roma 0.25", or null for the first point
  pads: [],
  dress: [],
  gates: new Map(), // name -> {on, at: [lat, lon] | null}
  tool: 'points',
  armed: null, // 'start' | 'pad' | 'gate'
  kind: PALETTE[0],
};
let engine = null;
const docs = []; // the Overpass documents loaded
const boxes = []; // the boxes asked of Overpass: south, west, north, east
const els = new Map(); // "way/123" -> element, all documents merged
const pos = new Map(); // node -> [lat, lon]
let trackText = null;

// ---------- small helpers ----------

let toastTimer = 0;
function toast(msg, bad = false, ms = 3500) {
  const t = $('toast');
  t.textContent = msg;
  t.className = bad ? 'toast bad' : 'toast';
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, ms);
}
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const km = (m) => (m >= 1000 ? `${(m / 1000).toFixed(2)} km` : `${Math.round(m)} m`);
function metres(a, b) {
  const k = Math.cos(((a[0] + b[0]) / 2) * Math.PI / 180);
  return Math.hypot((a[0] - b[0]) * 111320, (a[1] - b[1]) * 111320 * k);
}
function bearing(a, b) {
  const k = Math.cos(a[0] * Math.PI / 180);
  return Math.atan2((b[1] - a[1]) * k, b[0] - a[0]) * 180 / Math.PI;
}
function call(req) {
  try { return engine.call(req); } catch (e) { return { error: e.message }; }
}
function download(name, text, type = 'text/plain') {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([text], { type }));
  a.download = name;
  document.body.appendChild(a);
  a.click();
  setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 1000);
}
const spanLine = (s) => `way ${s.id}${s.forward === true ? ' forward' : s.forward === false ? ' backward' : ''}${s.from != null ? ` from ${s.from}` : ''}${s.to != null ? ` to ${s.to}` : ''}`;
function wayName(id) {
  const w = els.get(`way/${id}`);
  if (!w) return `way ${id}`;
  return (w.tags && w.tags.name) || `${(w.tags && w.tags.highway) || 'way'} ${id}`;
}
// The points of a span, in the direction of travel.
function spanGeom(s) {
  const w = els.get(`way/${s.id}`);
  if (!w) return [];
  const g = w.geometry.map((p) => [p.lat, p.lon]);
  const n = w.nodes.length;
  const all = (x) => w.nodes.map((v, i) => (v === x ? i : -1)).filter((i) => i >= 0);
  let fs = s.from != null ? all(s.from) : [s.forward === false ? n - 1 : 0];
  let ts = s.to != null ? all(s.to) : [s.forward === false ? 0 : n - 1];
  let best = null;
  for (const f of fs) for (const t of ts) {
    if (f === t || (s.forward != null && s.forward !== t > f)) continue;
    if (!best || Math.abs(f - t) < Math.abs(best[0] - best[1])) best = [f, t];
  }
  if (!best) return [];
  const [f, t] = best;
  return f < t ? g.slice(f, t + 1) : g.slice(t, f + 1).reverse();
}
function lineLength(g) {
  let m = 0;
  for (let i = 1; i < g.length; i++) m += metres(g[i - 1], g[i]);
  return m;
}

// ---------- the map ----------

const view = (params.get('view') || '').split(',').map(Number);
const map = L.map('map', { preferCanvas: true, zoomControl: true, minZoom: 3, maxZoom: 19 })
  .setView(view.length === 3 && view.every(isFinite) ? [view[0], view[1]] : [44.4958, 11.3545], view.length === 3 ? view[2] : 16);
map.attributionControl.setPrefix('<a href="https://leafletjs.com">Leaflet</a>');
if (params.get('tiles') === 'none') {
  $('map').classList.add('blank');
  map.attributionControl.addAttribution('Streets © <a href="https://www.openstreetmap.org/copyright">OpenStreetMap contributors</a>');
} else {
  // OpenStreetMap's standard tiles, as its tile usage policy asks: a plain map, no prefetching.
  L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
    maxZoom: 19,
    attribution: '© <a href="https://www.openstreetmap.org/copyright">OpenStreetMap contributors</a>',
  }).addTo(map);
}
L.control.scale({ imperial: false }).addTo(map);
const renderer = L.canvas({ padding: 0.3 });
const layers = {
  boxes: L.layerGroup().addTo(map),
  areas: L.layerGroup().addTo(map),
  buildings: L.layerGroup().addTo(map),
  streets: L.layerGroup().addTo(map),
  circuit: L.layerGroup().addTo(map),
  hot: L.layerGroup().addTo(map),
  marks: L.layerGroup().addTo(map),
};

function drawExtract() {
  for (const k of ['areas', 'buildings', 'streets']) layers[k].clearLayers();
  for (const e of els.values()) {
    if (e.type !== 'way' || !e.geometry) continue;
    const g = e.geometry.map((p) => [p.lat, p.lon]);
    const t = e.tags || {};
    if (t.highway && DRIVABLE.has(t.highway) && t.area !== 'yes') {
      const main = ['primary', 'secondary', 'tertiary', 'trunk', 'motorway'].includes(t.highway);
      L.polyline(g, { renderer, color: main ? '#6d5a48' : '#8e7c69', weight: main ? 3 : 2, opacity: 0.75, interactive: false }).addTo(layers.streets);
    } else if (t.amenity === 'hospital') {
      L.polygon(g, { renderer, color: '#5d8a3a', weight: 1, fillColor: '#86a65a', fillOpacity: 0.18, interactive: false }).addTo(layers.areas);
    } else if (t.historic === 'city_gate') {
      L.polygon(g, { renderer, color: '#8a3322', weight: 1, fillColor: '#b5452f', fillOpacity: 0.7, interactive: false }).addTo(layers.buildings);
    } else if (t.building) {
      L.polygon(g, { renderer, color: '#a08a6c', weight: 0.6, fillColor: '#d9c7a6', fillOpacity: 0.35, interactive: false }).addTo(layers.buildings);
    }
  }
}

// ---------- loading streets ----------

function overpassQuery(s, w, n, e) {
  // The same as tools/fetch-osm.sh.
  const b = `${s.toFixed(5)},${w.toFixed(5)},${n.toFixed(5)},${e.toFixed(5)}`;
  return `[out:json][timeout:120];
(
  way["highway"](${b});
  way["building"](${b});
  way["amenity"="hospital"](${b});
  way["historic"](${b});
);
out geom;
(
  relation["historic"](${b});
  relation["building"](${b});
  node["historic"](${b});
);
out tags bb;`;
}

function addDoc(doc, label, box) {
  if (!doc || !Array.isArray(doc.elements)) throw new Error(`${label}: not Overpass JSON (no "elements")`);
  docs.push(doc);
  for (const e of doc.elements) {
    els.set(`${e.type}/${e.id}`, e);
    if (e.type === 'way' && e.geometry && e.nodes) e.nodes.forEach((n, i) => e.geometry[i] && pos.set(n, [e.geometry[i].lat, e.geometry[i].lon]));
  }
  const t0 = performance.now();
  const a = call({ op: 'extract', docs });
  if (a.error) throw new Error(a.error);
  if (box) L.rectangle(box, { renderer, color: '#2f5d45', weight: 1.5, dashArray: '6 6', fill: false, interactive: false }).addTo(layers.boxes);
  drawExtract();
  $('loadInfo').textContent = `${a.drivable} streets and ${a.ways - a.drivable} other ways loaded (${docs.length} ${docs.length === 1 ? 'box' : 'boxes'}, ${Math.round(performance.now() - t0)} ms). ` +
    (S.actions.length ? '' : 'Now click points along the streets, in the order of the race.');
  replay();
}

async function loadHere() {
  const b = map.getBounds();
  const s = b.getSouth(), w = b.getWest(), n = b.getNorth(), e = b.getEast();
  const tall = metres([s, w], [n, w]), wide = metres([s, w], [s, e]);
  if (tall > MAX_SIDE || wide > MAX_SIDE) {
    toast(`This box is ${km(wide)} × ${km(tall)}: zoom in to 3 × 3 km or less (the Overpass servers are shared).`, true, 5000);
    return;
  }
  const btn = $('load');
  btn.disabled = true;
  $('loadInfo').textContent = `Asking Overpass for the streets of ${km(wide)} × ${km(tall)}…`;
  try {
    const res = await fetch(OVERPASS, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: 'data=' + encodeURIComponent(overpassQuery(s, w, n, e)),
    });
    if (res.status === 429 || res.status === 504 || res.status === 502) throw new Error('the Overpass server is busy: try again in a minute');
    if (!res.ok) throw new Error(`Overpass answered ${res.status}`);
    addDoc(await res.json(), 'Overpass', [[s, w], [n, e]]);
    boxes.push([s, w, n, e].map((v) => v.toFixed(4)).join(','));
  } catch (err) {
    $('loadInfo').textContent = `No streets: ${err.message}.`;
    toast(`No streets: ${err.message}`, true);
  } finally {
    btn.disabled = false;
  }
}

async function loadFile(file) {
  try {
    addDoc(JSON.parse(await file.text()), file.name);
    const b = L.latLngBounds([]);
    for (const p of pos.values()) b.extend(p);
    if (!S.actions.length && b.isValid()) map.fitBounds(b);
  } catch (err) {
    toast(err.message, true);
  }
}

// ---------- the circuit ----------

const against = () => ($('closedRoads').checked ? 1 : 4);

function drive(a, b) {
  if (a === b) return [];
  const d = call({ op: 'drive', from: a, to: b, against: against() });
  return d.error ? null : d.drive.spans;
}

// A picked street: from where the circuit is (or its nearer end) to its far end, towards the
// click.
function wayLegs(a, end) {
  const w = els.get(`way/${a.id}`);
  if (!w) return { spans: [], end };
  const nodes = w.nodes, n = nodes.length;
  const out = [];
  let i0 = 0;
  if (end != null && nodes.includes(end)) i0 = nodes.indexOf(end);
  else if (end != null) {
    const p = pos.get(end);
    const near = metres(p, pos.get(nodes[0])) <= metres(p, pos.get(nodes[n - 1])) ? 0 : n - 1;
    const d = drive(end, nodes[near]);
    if (d) out.push(...d);
    i0 = near;
  }
  let ci = 0, cd = Infinity;
  nodes.forEach((v, i) => { const m = metres([a.lat, a.lon], pos.get(v)); if (m < cd) { cd = m; ci = i; } });
  let goal;
  if (i0 === 0) goal = n - 1;
  else if (i0 === n - 1) goal = 0;
  else if (ci !== i0) goal = ci > i0 ? n - 1 : 0;
  else goal = i0 < n - 1 - i0 ? n - 1 : 0;
  out.push({ id: a.id, forward: goal > i0, from: nodes[i0], to: nodes[goal] });
  return { spans: out, end: nodes[goal], first: nodes[i0] };
}

// The circuit from the actions: each point routed from the one before, each street picked
// from where the circuit is, closed back to the first point, spurs tidied away.
function replay() {
  const spans = [];
  let end = null, first = null;
  S.wps = [];
  const failed = [];
  for (const a of S.actions) {
    if (a.t === 'via') {
      if (!pos.has(a.node)) continue;
      S.wps.push(a);
      if (end != null) {
        const d = drive(end, a.node);
        if (!d) { failed.push(a); continue; }
        spans.push(...d);
      }
      first ??= a.node;
      end = a.node;
    } else if (a.t === 'way') {
      if (!els.has(`way/${a.id}`)) continue;
      const r = wayLegs(a, end);
      spans.push(...r.spans);
      first ??= r.first;
      end = r.end;
    } else if (a.t === 'span') {
      spans.push(a.span);
      first ??= a.span.from;
      end = a.span.to;
    }
  }
  if (failed.length) {
    S.actions = S.actions.filter((a) => !failed.includes(a));
    toast('No street leads there from the last point.', true);
    replay();
    return;
  }
  S.closing = 0;
  if (first != null && end != null && first !== end && spans.length) {
    const d = drive(end, first);
    if (d) { spans.push(...d); S.closing = d.length; }
  }
  const closed = first != null && spans.length > 1;
  const t = spans.length ? call({ op: 'tidy', spans, closed }) : { spans: [] };
  S.legs = t.error ? spans : t.spans;
  update();
}

function addAction(a) {
  S.actions.push(a);
  replay();
}

function undo() {
  if (!S.actions.length) return;
  S.actions.pop();
  replay();
}

function reverse() {
  const flip = (s) => ({ id: s.id, forward: s.forward == null ? null : !s.forward, from: s.to, to: s.from });
  const acts = [...S.actions].reverse().map((a) => {
    if (a.t === 'span') return { t: 'span', span: flip(a.span) };
    return a;
  });
  // A street picked runs the other way: its click is now at its far end.
  S.actions = acts;
  replay();
  toast('The circuit runs the other way round now.');
}

function removeLeg(i) {
  S.actions = S.legs.filter((_, j) => j !== i).map((span) => ({ t: 'span', span }));
  replay();
}

function bridgeGaps() {
  const out = [];
  S.legs.forEach((s, i) => {
    if (i && out.length && out[out.length - 1].to !== s.from) {
      const d = drive(out[out.length - 1].to, s.from);
      if (d) out.push(...d);
    }
    out.push(s);
  });
  S.actions = out.map((span) => ({ t: 'span', span }));
  replay();
}

// ---------- places on the circuit ----------

function placeToS(place) {
  const r = S.report;
  if (!r || !r.closed) return null;
  if (place == null) return 0;
  if (/^-?\d+(\.\d+)?$/.test(place)) return ((+place % r.length) + r.length) % r.length;
  const words = place.trim().split(/\s+/);
  const f = +words.pop();
  const name = words.join(' ');
  const parts = r.sections.filter((s) => s.name === name);
  if (!parts.length || !isFinite(f)) return null;
  let left = parts.reduce((a, s) => a + s.to - s.from, 0) * f;
  for (const s of parts) {
    if (left <= s.to - s.from) return s.from + left;
    left -= s.to - s.from;
  }
  return parts[parts.length - 1].to;
}

function sToPlace(s) {
  const r = S.report;
  const sec = r.sections.find((x) => s >= x.from && s < x.to) || r.sections[r.sections.length - 1];
  const parts = r.sections.filter((x) => x.name === sec.name);
  const total = parts.reduce((a, x) => a + x.to - x.from, 0);
  let off = 0;
  for (const x of parts) {
    if (x === sec) { off += s - x.from; break; }
    off += x.to - x.from;
  }
  const f = Math.min(1, Math.max(0, off / total));
  return `${sec.name} ${f.toFixed(2)}`;
}

function nearestOnLine(lat, lon) {
  let best = null;
  for (const p of S.report.line) {
    const m = metres([lat, lon], p);
    if (!best || m < best.m) best = { m, s: p[2], p };
  }
  return best;
}

function pointAt(s) {
  const line = S.report.line;
  let lo = 0, hi = line.length - 1;
  while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (line[mid][2] <= s) lo = mid; else hi = mid - 1; }
  return { p: line[lo], q: line[(lo + 1) % line.length] };
}

// ---------- the block ----------

function settings() {
  return {
    id: ($('tid').value.trim().toLowerCase().replace(/[^a-z0-9-]+/g, '-').replace(/^-+|-+$/g, '')) || 'mytrack',
    name: $('tname').value.trim() || 'My circuit',
    text: $('ttext').value.trim(),
    laps: Math.max(1, Math.min(99, Math.round(+$('laps').value || 3))),
    widen: +$('widen').value || 1.25,
    minw: +$('minw').value || 8.5,
    verge: +$('verge').value || 0,
    scenery: +$('scenery').value || 0,
    baskets: Math.round(+$('baskets').value || 0),
    game: $('game').value,
  };
}

const dressLine = (d) => `dress ${d.kind} ${d.model} every ${d.every}${d.side !== 'both' ? ` side ${d.side}` : ''} offset ${d.offset} scale ${d.scale} face ${d.face}${d.along ? ` along ${d.along}` : ''}`;

// The track block. `exact`: the check's exact lines (one per stretch of a way), when the loop
// closes; otherwise the legs as drawn.
function block(exact = true) {
  const c = settings();
  const r = S.report;
  const route = exact && r && r.closed ? r.exact : S.legs.map(spanLine);
  const out = [];
  if (exact) {
    const streets = [];
    if (r && r.closed) for (const s of r.sections) if (streets[streets.length - 1] !== s.name) streets.push(s.name);
    out.push(`# ${c.name}: drawn with the OpenKart circuit designer on OpenStreetMap streets.`);
    if (streets.length) out.push(`# ${km(r.length)}: ${streets.join(', ')}.`);
    for (const b of boxes) out.push(`# Its streets: sh tools/fetch-osm.sh ${b} osm/${c.id}.json (or the designer's extract)`);
    out.push(`# Build it: sh tools/new-track.sh games/${c.game} ${c.id}.kart ${c.id}-extract.json`, '');
  }
  out.push(`track ${c.id}`, `    name ${c.name}`);
  const words = c.text.split(/\s+/).filter(Boolean);
  for (let i = 0, line = ''; i <= words.length; i++) {
    if (i === words.length || (line + ' ' + words[i]).length > 86) {
      if (line) out.push(`    text ${line}`);
      line = words[i] || '';
    } else line = line ? `${line} ${words[i]}` : words[i];
  }
  out.push(`    osm ${c.id}.track`, `    laps ${c.laps}`);
  for (const l of route) out.push(`    route ${l}`);
  out.push(`    start ${S.start || 0}`);
  out.push(`    widen ${c.widen}`, `    min-width ${c.minw}`, `    verge ${c.verge}`, `    scenery ${c.scenery}`);
  for (const [name, g] of S.gates) {
    if (!g.on) continue;
    out.push(g.at ? `    landmark gate ${name} at ${g.at[0].toFixed(6)} ${g.at[1].toFixed(6)}` : `    landmark gate ${name}`);
  }
  for (const d of S.dress) out.push(`    ${dressLine(d)}`);
  if (c.baskets > 0) out.push(`    baskets every ${Math.max(20, c.baskets)} first 200 across 4`);
  for (const p of S.pads) out.push(`    pad ${p}`);
  for (const l of LOOK) out.push(`    ${l}`);
  out.push('end', '');
  return out.join('\n');
}

// ---------- after every change: check, draw, list ----------

let checkTimer = 0;
function update() {
  clearTimeout(checkTimer);
  checkTimer = setTimeout(runCheck, 30);
  drawLegsList();
}

function runCheck() {
  if (S.legs.length < 2) {
    S.report = null;
  } else {
    const a = call({ op: 'check', block: block(false) });
    S.report = a.error ? { closed: false, error: a.error, gaps: [], oneway: [], corners: [], problems: [], exact: [], line: [], sections: [], landmarks: [] } : a.report;
    for (const [kind, name] of S.report.landmarks || []) {
      if (kind === 'gate' && !S.gates.has(name)) S.gates.set(name, { on: true, at: null });
    }
  }
  drawCircuit();
  drawChecks();
  drawSettings();
  $('block').textContent = S.legs.length ? block() : '# Draw a circuit first.';
  const c = settings();
  $('cmd').textContent = `sh tools/new-track.sh games/${c.game} ${c.id}.kart ${c.id}-extract.json\n\n# which adds the block to the game and runs\ncargo run --release -p osm2track -- games/${c.game} ${c.id}-extract.json --track ${c.id}`;
  save();
}

function drawLegsList() {
  const ol = $('legs');
  ol.innerHTML = '';
  S.legs.forEach((s, i) => {
    const li = document.createElement('li');
    const g = spanGeom(s);
    li.innerHTML = `<span class="nm" title="${esc(spanLine(s))}">${esc(wayName(s.id))}</span><span class="m">${Math.round(lineLength(g))} m</span><button class="x" title="Remove this leg" aria-label="Remove leg ${i + 1}">✕</button>`;
    li.querySelector('button').onclick = () => removeLeg(i);
    li.onmouseenter = () => { layers.hot.clearLayers(); L.polyline(g, { renderer, color: '#ffd23f', weight: 9, opacity: 0.9, interactive: false }).addTo(layers.hot); };
    li.onmouseleave = () => layers.hot.clearLayers();
    ol.appendChild(li);
  });
  $('undo').disabled = !S.actions.length;
  $('reverse').disabled = S.legs.length < 2;
}

function icon(cls, html = '', size = [18, 18]) {
  return L.divIcon({ className: '', html: `<div class="${cls}" style="width:${size[0]}px;height:${size[1]}px">${html}</div>`, iconSize: size });
}

function drawCircuit() {
  layers.circuit.clearLayers();
  layers.marks.clearLayers();
  const r = S.report;
  if (r && r.closed && r.line.length) {
    const g = r.line.map((p) => [p[0], p[1]]);
    L.polygon(g, { renderer, color: '#2a1d17', weight: 10, fill: false, opacity: 0.85, interactive: false }).addTo(layers.circuit);
    L.polygon(g, { renderer, color: '#e8352b', weight: 6, fill: false, interactive: false }).addTo(layers.circuit);
    // Direction arrows.
    const every = Math.max(70, r.length / 22);
    for (let s = every / 2; s < r.length; s += every) {
      const { p, q } = pointAt(s);
      const b = bearing(p, q);
      L.marker([p[0], p[1]], { icon: icon('', `<div class="arrow" style="transform:rotate(${b}deg)"></div>`, [14, 14]), interactive: false, keyboard: false }).addTo(layers.marks);
    }
    // The start line: a chequered bar across the road.
    const s0 = placeToS(S.start);
    if (s0 != null) {
      const { p, q } = pointAt(s0);
      const b = bearing(p, q) + 90;
      L.marker([p[0], p[1]], {
        icon: icon('', `<div style="width:34px;height:10px;transform:rotate(${b}deg);border:2px solid #2a1d17;background:repeating-conic-gradient(#2a1d17 0 25%, #fff 0 50%) 0 0/8px 8px"></div>`, [34, 10]),
        title: `Start: ${S.start || 'the first point'}`, keyboard: false,
      }).addTo(layers.marks);
    }
    for (const pad of S.pads) {
      const s = placeToS(pad);
      if (s == null) continue;
      const { p } = pointAt(s);
      L.circleMarker([p[0], p[1]], { renderer, radius: 6, color: '#2a1d17', weight: 2, fillColor: '#e0a92e', fillOpacity: 1, interactive: false }).addTo(layers.marks);
    }
  } else {
    // Not a loop yet: the legs as drawn.
    for (const s of S.legs) {
      const g = spanGeom(s);
      L.polyline(g, { renderer, color: '#2a1d17', weight: 8, opacity: 0.7, interactive: false }).addTo(layers.circuit);
      L.polyline(g, { renderer, color: '#e8352b', weight: 4, dashArray: '8 6', interactive: false }).addTo(layers.circuit);
    }
  }
  S.wps.forEach((w, i) => {
    L.marker([w.lat, w.lon], { icon: icon(i ? 'wp' : 'wp first', String(i + 1), [22, 22]), title: `Point ${i + 1}`, keyboard: false, interactive: false }).addTo(layers.marks);
  });
  for (const [name, g] of S.gates) {
    if (!g.on) continue;
    const at = g.at || gatePos(name);
    if (at) L.marker(at, { icon: icon('', '<div style="width:14px;height:14px;background:#b5452f;border:2px solid #2a1d17"></div>', [14, 14]), title: name, keyboard: false }).addTo(layers.marks);
  }
  if (r) {
    for (const x of r.gaps) L.marker(x.at, { icon: icon('issue', '', [26, 26]), title: x.what, keyboard: false }).addTo(layers.marks);
    for (const x of [...r.oneway, ...r.corners]) L.marker(x.at, { icon: icon('issue warn', '', [24, 24]), keyboard: false, interactive: false }).addTo(layers.marks);
  }
}

function gatePos(name) {
  for (const e of els.values()) {
    if (!e.tags || e.tags.name !== name) continue;
    if (e.geometry) {
      const n = e.geometry.length;
      return [e.geometry.reduce((a, p) => a + p.lat, 0) / n, e.geometry.reduce((a, p) => a + p.lon, 0) / n];
    }
    if (e.bounds) return [(e.bounds.minlat + e.bounds.maxlat) / 2, (e.bounds.minlon + e.bounds.maxlon) / 2];
    if (e.lat != null) return [e.lat, e.lon];
  }
  return null;
}

function drawChecks() {
  const ul = $('checks');
  ul.innerHTML = '';
  const add = (cls, html, at) => {
    const li = document.createElement('li');
    li.className = cls + (at ? ' go' : '');
    li.innerHTML = html;
    if (at) li.onclick = () => map.setView(at, Math.max(map.getZoom(), 17));
    ul.appendChild(li);
  };
  const r = S.report;
  if (!r) {
    add('info', S.legs.length ? 'One more point: a circuit needs at least two legs.' : (docs.length ? 'Click points along the streets, in the order of the race.' : 'Load the streets first.'));
    return;
  }
  if (r.closed) {
    const laps = settings().laps;
    const cls = r.length < 800 || r.length > 5000 ? 'warn' : 'ok';
    add(cls, `<span class="big">${km(r.length)}</span> a lap, ${km(r.length * laps)} for ${laps} laps${cls === 'warn' ? ' (1.5 to 3 km races best)' : ''}`);
    add('ok', `Closes into one loop: ${r.exact.length} stretches of ${new Set(r.exact.map((l) => l.split(' ')[1])).size} ways`);
  } else {
    add('bad', `Does not close: ${esc(r.error || 'the legs do not make a loop')}`);
  }
  for (const g of r.gaps) add('bad', `Gap: ${esc(g.what)}, ${Math.round(g.metres)} m apart <button class="ghost" style="padding:0 6px">Bridge</button>`, g.at);
  if (r.gaps.length) ul.querySelectorAll('li.bad button').forEach((b) => { b.onclick = (e) => { e.stopPropagation(); bridgeGaps(); }; });
  const closedRoads = $('closedRoads').checked;
  for (const o of r.oneway) add(closedRoads ? 'info' : 'warn', `${esc(o.name || `way ${o.way}`)}: ${Math.round(o.metres)} m against the one-way street`, o.at);
  if (r.closed && !r.oneway.length) add('ok', 'No one-way street driven the wrong way');
  for (const c of r.corners) add('warn', `Very sharp corner on ${esc(c.name)}: it turns ${c.turn}° within 30 m (radius ${Math.round(c.radius)} m)`, c.at);
  if (r.closed && !r.corners.length) add('ok', 'No hairpins: every corner turns less than 110° within 30 m');
  for (const p of r.problems) add('bad', esc(p));
}

function drawSettings() {
  const r = S.report;
  // The start.
  $('startInfo').textContent = r && r.closed
    ? `Start line: ${S.start ? S.start : 'at the first point'}${S.start && placeToS(S.start) == null ? ' (not on the circuit any more)' : ''}.`
    : '';
  $('setStart').classList.toggle('armed', S.armed === 'start');
  $('addPad').classList.toggle('armed', S.armed === 'pad');
  $('placeGate').classList.toggle('armed', S.armed === 'gate');
  const pads = $('pads');
  pads.innerHTML = '';
  S.pads.forEach((p, i) => {
    const li = document.createElement('li');
    li.innerHTML = `<span>Boost pad: ${esc(p)}</span><button class="x" aria-label="Remove">✕</button>`;
    li.querySelector('button').onclick = () => { S.pads.splice(i, 1); update(); };
    pads.appendChild(li);
  });
  // Streets for `along`.
  const sel = $('dAlong');
  const keep = sel.value;
  const names = r && r.closed ? [...new Set(r.sections.map((s) => s.name))] : [];
  sel.innerHTML = '<option value="">the whole circuit</option>' + names.map((n) => `<option>${esc(n)}</option>`).join('');
  if (names.includes(keep)) sel.value = keep;
  const dl = $('dress');
  dl.innerHTML = '';
  S.dress.forEach((d, i) => {
    const li = document.createElement('li');
    const p = PALETTE.find((x) => x.kind === d.kind);
    li.innerHTML = `<span>${esc(p ? p.label : d.kind)} every ${d.every} m${d.side !== 'both' ? `, ${d.side}` : ''}, ${esc(d.along || 'everywhere')}</span><button class="x" aria-label="Remove">✕</button>`;
    li.querySelector('button').onclick = () => { S.dress.splice(i, 1); update(); };
    dl.appendChild(li);
  });
  const gates = $('gates');
  if (!S.gates.size) gates.textContent = 'Gates near the circuit show here.';
  else {
    gates.innerHTML = '';
    for (const [name, g] of S.gates) {
      const l = document.createElement('label');
      l.innerHTML = `<input type="checkbox" ${g.on ? 'checked' : ''}> ${esc(name)}${g.at ? ' (placed)' : ''}`;
      l.querySelector('input').onchange = (e) => { g.on = e.target.checked; update(); };
      gates.appendChild(l);
    }
  }
}

// ---------- clicks ----------

function hint() {
  const h = {
    start: 'Click the circuit where the start line goes',
    pad: 'Click the circuit where the boost pad goes',
    gate: 'Click the map where the gate stands',
  }[S.armed];
  $('hint').textContent = h || (docs.length ? (S.tool === 'points' ? 'Click near a street to add a point' : 'Click a street to add it') : '');
  $('map').classList.toggle('pick', !!docs.length);
  $('toolHelp').textContent = S.tool === 'points'
    ? 'Each point is joined to the one before along the streets (the shortest drive), and the last back to the first.'
    : 'Each street is added from where the circuit is to its far end, towards where you click.';
}

function arm(what) {
  S.armed = S.armed === what ? null : what;
  hint();
  drawSettings();
}

function onClick(lat, lon) {
  if (S.armed === 'gate') {
    const name = $('gateName').value.trim();
    if (!name) { toast('Give the gate a name first.', true); return; }
    S.gates.set(name, { on: true, at: [lat, lon] });
    $('gateName').value = '';
    S.armed = null; hint(); update();
    return;
  }
  if (S.armed === 'start' || S.armed === 'pad') {
    if (!S.report || !S.report.closed) { toast('Close the loop first.', true); return; }
    const near = nearestOnLine(lat, lon);
    if (near.m > 60) { toast('Click on the circuit.', true); return; }
    const place = sToPlace(near.s);
    if (S.armed === 'start') S.start = place; else S.pads.push(place);
    S.armed = null; hint(); update();
    return;
  }
  if (!docs.length) { toast('Load the streets first: "Load streets here".', true); return; }
  const mpp = 40075016.686 * Math.cos(lat * Math.PI / 180) / 2 ** (map.getZoom() + 8);
  const a = call({ op: 'snap', lat, lon, within: Math.max(8, 16 * mpp) });
  if (a.error || !a.snap) { toast('No street there.', true, 1500); return; }
  const s = a.snap;
  if (S.tool === 'points') addAction({ t: 'via', node: s.node, lat: s.lat, lon: s.lon });
  else addAction({ t: 'way', id: s.way, lat, lon });
}

map.on('click', (e) => onClick(e.latlng.lat, e.latlng.lng));

// ---------- import, save ----------

function importBlock(text) {
  const acts = [];
  let skipped = 0;
  const dress = [], pads = [], gates = new Map();
  let start = null;
  let textLines = [];
  for (const raw of text.split('\n')) {
    const line = raw.replace(/(^|\s)#.*$/, '').trim();
    const w = line.split(/\s+/);
    if (!line) continue;
    const rest = w.slice(1).join(' ');
    if (w[0] === 'track') $('tid').value = w[1] || 'mytrack';
    else if (w[0] === 'name') $('tname').value = rest;
    else if (w[0] === 'text') textLines.push(rest);
    else if (w[0] === 'laps') $('laps').value = w[1];
    else if (w[0] === 'widen') $('widen').value = w[1];
    else if (w[0] === 'min-width') $('minw').value = w[1];
    else if (w[0] === 'verge') $('verge').value = w[1];
    else if (w[0] === 'scenery') $('scenery').value = w[1];
    else if (w[0] === 'start') start = rest === '0' ? null : rest;
    else if (w[0] === 'pad') pads.push(rest);
    else if (w[0] === 'baskets') $('baskets').value = w[w.indexOf('every') + 1] || 0;
    else if (w[0] === 'route') {
      if (w[1] !== 'way') { skipped++; continue; }
      const span = { id: +w[2], forward: null, from: null, to: null };
      for (let i = 3; i < w.length; i++) {
        if (w[i] === 'forward') span.forward = true;
        else if (w[i] === 'backward') span.forward = false;
        else if (w[i] === 'from') span.from = +w[++i];
        else if (w[i] === 'to') span.to = +w[++i];
      }
      acts.push({ t: 'span', span });
    } else if (w[0] === 'dress') {
      const d = { kind: w[1], model: w[2], every: +w[4], side: 'both', offset: 1.5, scale: '1', face: 'road', along: '' };
      for (let i = 5; i < w.length; i++) {
        if (w[i] === 'side') d.side = w[++i];
        else if (w[i] === 'offset') d.offset = +w[++i];
        else if (w[i] === 'face') d.face = w[++i];
        else if (w[i] === 'scale') { d.scale = w[++i]; if (isFinite(+w[i + 1]) && w[i + 1] !== undefined) d.scale += ' ' + w[++i]; }
        else if (w[i] === 'along') { d.along = w.slice(i + 1).join(' '); break; }
      }
      dress.push(d);
    } else if (w[0] === 'landmark' && w[1] === 'gate') {
      const at = w.indexOf('at');
      if (at > 0) gates.set(w.slice(2, at).join(' '), { on: true, at: [+w[at + 1], +w[at + 2]] });
      else gates.set(w.slice(2).join(' '), { on: true, at: null });
    }
  }
  if (textLines.length) $('ttext').value = textLines.join(' ');
  S.actions = acts; S.start = start; S.pads = pads; S.dress = dress; S.gates = gates;
  if (skipped) toast(`${skipped} route lines were not exact \`route way\` lines and were left out.`, true, 6000);
  replay();
}

function save() {
  try {
    localStorage.setItem(STORE, JSON.stringify({
      actions: S.actions, start: S.start, pads: S.pads, dress: S.dress, gates: [...S.gates], tool: S.tool,
      closed: $('closedRoads').checked, centre: map.getCenter(), zoom: map.getZoom(),
      fields: Object.fromEntries(['tid', 'tname', 'ttext', 'laps', 'widen', 'minw', 'verge', 'scenery', 'baskets'].map((k) => [k, $(k).value])),
    }));
  } catch { /* no storage: nothing to remember */ }
}

function restore() {
  if (params.toString()) return; // a test run starts clean
  let d = null;
  try { d = JSON.parse(localStorage.getItem(STORE) || 'null'); } catch { d = null; }
  if (!d) return;
  S.actions = d.actions || []; S.start = d.start || null; S.pads = d.pads || []; S.dress = d.dress || [];
  S.gates = new Map(d.gates || []);
  $('closedRoads').checked = !!d.closed;
  for (const [k, v] of Object.entries(d.fields || {})) if ($(k)) $(k).value = v;
  if (d.centre) map.setView(d.centre, d.zoom || 16);
  if (S.actions.length) $('loadInfo').textContent = 'Your circuit from last time comes back when its streets are loaded: "Load streets here".';
}

// ---------- the 3D preview: the game's own renderer ----------

let preview = null;
async function bakeAndPreview() {
  if (!S.report || !S.report.closed) { toast('Close the loop first.', true); return; }
  const c = settings();
  $('bakeInfo').textContent = 'Baking: walls, racing line, buildings, scenery…';
  await new Promise((r) => setTimeout(r, 20));
  try {
    const base = `../play/games/${c.game}/`;
    const names = await (await fetch(base + 'files.json')).json();
    // The game's files, with this track's block in place of any older one.
    const files = await Promise.all(names.filter((n) => n !== `${c.id}.kart` && n !== `${c.id}.track`).map(async (n) => [n, await (await fetch(base + n)).text()]));
    files.push([`${c.id}.kart`, block()]);
    const t0 = performance.now();
    const b = call({ op: 'bake', files, track: c.id });
    if (b.error) throw new Error(b.error);
    trackText = b.text;
    $('dlTrack').disabled = false;
    $('bakeInfo').textContent = `Baked in ${Math.round(performance.now() - t0)} ms: ${km(b.length)}, ${b.sections.length} sections, ${b.buildings} buildings, ${b.props} pieces of scenery, ${b.landmarks} gates.`;
    const course = call({ op: 'course', track: c.id });
    if (course.error) throw new Error(course.error);
    await openPreview(course, b.game);
  } catch (err) {
    $('bakeInfo').textContent = `The bake failed: ${err.message}`;
    toast(`The bake failed: ${err.message}`, true, 6000);
  }
}

async function openPreview(course, game) {
  const THREE = await import('three');
  const { World } = await import('../play/js/world.js');
  const { GLTFLoader } = await import('../play/vendor/jsm/loaders/GLTFLoader.js');
  if (preview) { preview.stop(); preview = null; }
  $('preview').hidden = false;
  $('pName').textContent = course.name;
  $('pFacts').textContent = `${km(course.track.length)} · ${course.laps} laps · a preview: the game draws it like this`;
  const canvas = $('scene');
  const renderer3 = new THREE.WebGLRenderer({ canvas, antialias: true });
  renderer3.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer3.outputColorSpace = THREE.SRGBColorSpace;
  renderer3.shadowMap.enabled = true;
  renderer3.toneMapping = THREE.ACESFilmicToneMapping;
  const scene = new THREE.Scene();
  const look = course.look || {};
  const sky = new THREE.Color('#' + (look.sky || '9fc8e8'));
  scene.background = sky;
  scene.fog = new THREE.Fog(sky, 180, 900);
  scene.add(new THREE.HemisphereLight('#fff3dc', '#8a7458', 1.5));
  const sun = new THREE.DirectionalLight('#fff1d6', 2.6);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  Object.assign(sun.shadow.camera, { left: -90, right: 90, top: 90, bottom: -90, near: 1, far: 400 });
  scene.add(sun, sun.target);
  const camera = new THREE.PerspectiveCamera(62, 1, 0.3, 1500);
  // The props the course uses, from the game's folder.
  const dirs = Object.fromEntries(game.assets || []);
  const models = { props: {} };
  const loader = new GLTFLoader();
  await Promise.all([...new Set(course.props.map((p) => p.model))].map((name) => new Promise((res) => {
    loader.load(`../play/${dirs.props || 'models/props'}/${name}`, (g) => {
      g.scene.traverse((o) => {
        if (!o.isMesh) return;
        for (const m of [].concat(o.material)) if (m.metalness === 1 && !m.metalnessMap) { m.metalness = 0; m.roughness = Math.max(m.roughness, 0.7); }
      });
      models.props[name] = g.scene;
      res();
    }, undefined, () => res());
  })));
  const world = new World(scene, course, models);
  const S2 = course.track.samples, n = S2.length;
  let s = 0, running = true, last = performance.now();
  const size = () => {
    renderer3.setSize(innerWidth, innerHeight, false);
    camera.aspect = innerWidth / innerHeight;
    camera.updateProjectionMatrix();
  };
  size();
  addEventListener('resize', size);
  // The camera s metres round the circuit, a little above and behind the road.
  const pose = (at) => {
    const i = Math.floor((at / course.track.length) * n) % n;
    const a = S2[i], b = S2[(i + 14) % n];
    camera.position.set(a.x - a.tx * 9 + a.tz * 3, 8, a.z - a.tz * 9 - a.tx * 3);
    camera.lookAt(b.x, 1.5, b.z);
    sun.position.set(camera.position.x + 60, 140, camera.position.z + 40);
    sun.target.position.set(camera.position.x, 0, camera.position.z);
  };
  pose(0);
  const frame = (now) => {
    if (!running) return;
    requestAnimationFrame(frame);
    const dt = Math.max(0, Math.min(0.1, (now - last) / 1000)); // frame times can come out of order
    last = now;
    // A flight round the circuit.
    s = (s + dt * 16) % course.track.length;
    pose(s);
    world.update(now / 1000, null);
    renderer3.render(scene, camera);
  };
  requestAnimationFrame(frame);
  preview = {
    renderer: renderer3, scene, camera, pose,
    stop() {
      running = false;
      removeEventListener('resize', size);
      world.dispose();
      renderer3.dispose();
    },
  };
}

function closePreview() {
  if (preview) { preview.stop(); preview = null; }
  $('preview').hidden = true;
}

// ---------- the panel ----------

function wire() {
  $('load').onclick = loadHere;
  $('file').onchange = (e) => e.target.files[0] && loadFile(e.target.files[0]);
  for (const b of document.querySelectorAll('[data-tool]')) {
    b.onclick = () => {
      S.tool = b.dataset.tool;
      for (const x of document.querySelectorAll('[data-tool]')) { x.classList.toggle('on', x === b); x.setAttribute('aria-checked', String(x === b)); }
      hint();
      save();
    };
  }
  $('closedRoads').onchange = () => replay();
  $('undo').onclick = undo;
  $('reverse').onclick = reverse;
  $('clear').onclick = () => {
    if (S.actions.length && !confirm('Clear the circuit?')) return;
    S.actions = []; S.start = null; S.pads = [];
    replay();
  };
  $('importBtn').onclick = () => $('importDlg').showModal();
  $('importDlg').addEventListener('close', () => {
    if ($('importDlg').returnValue === 'ok') importBlock($('importText').value);
  });
  for (const id of ['tid', 'tname', 'ttext', 'laps', 'widen', 'minw', 'verge', 'scenery', 'baskets', 'game']) $(id).addEventListener('input', update);
  $('setStart').onclick = () => arm('start');
  $('addPad').onclick = () => arm('pad');
  $('placeGate').onclick = () => arm('gate');
  const pal = $('palette');
  for (const p of PALETTE) {
    const b = document.createElement('button');
    b.innerHTML = `<i style="color:${p.colour}">●</i>${p.label}`;
    b.className = p === S.kind ? 'on' : '';
    b.onclick = () => {
      S.kind = p;
      for (const x of pal.children) x.classList.toggle('on', x === b);
      $('dEvery').value = p.every;
      $('dSide').value = p.kind === 'lamp' ? 'right' : 'both';
    };
    pal.appendChild(b);
  }
  $('addDress').onclick = () => {
    const p = S.kind;
    S.dress.push({ kind: p.kind, model: p.model, every: Math.max(1, +$('dEvery').value || p.every), side: $('dSide').value, offset: p.offset, scale: p.scale, face: p.face, along: $('dAlong').value });
    update();
  };
  $('dlBlock').onclick = () => {
    if (!S.legs.length) { toast('Draw a circuit first.', true); return; }
    download(`${settings().id}.kart`, block());
  };
  $('dlExtract').onclick = () => {
    if (!docs.length) { toast('Load the streets first.', true); return; }
    const merged = {
      version: 0.6,
      generator: 'OpenKart circuit designer, from the Overpass API',
      osm3s: { copyright: 'The data included in this document is from www.openstreetmap.org. The data is made available under ODbL.' },
      elements: [...els.values()],
    };
    download(`${settings().id}-extract.json`, JSON.stringify(merged), 'application/json');
  };
  $('bake').onclick = bakeAndPreview;
  $('dlTrack').onclick = () => trackText && download(`${settings().id}.track`, trackText);
  $('pClose').onclick = closePreview;
  addEventListener('keydown', (e) => {
    if (e.target.matches('input, textarea, select')) return;
    if ((e.ctrlKey || e.metaKey) && e.key === 'z') { e.preventDefault(); undo(); }
    if (e.key === 'Escape') { if (!$('preview').hidden) closePreview(); else if (S.armed) arm(S.armed); }
  });
}

// ---------- start ----------

async function boot() {
  wire();
  restore();
  hint();
  drawChecks();
  $('block').textContent = '# Draw a circuit first.';
  engine = await Engine.load('../play/openkart.wasm');
  // Test-only flags.
  const p = (k) => params.get(k);
  for (const [k, id] of [['id', 'tid'], ['name', 'tname'], ['text', 'ttext'], ['laps', 'laps']]) if (p(k)) $(id).value = p(k);
  if (p('closed')) $('closedRoads').checked = true;
  if (p('extract')) {
    const res = await fetch(p('extract'));
    addDoc(await res.json(), p('extract'));
    if (!p('view')) {
      const b = L.latLngBounds([]);
      for (const q of pos.values()) b.extend(q);
      if (b.isValid()) map.fitBounds(b);
    }
  }
  if (p('load')) await loadHere();
  const pts = (k) => (p(k) || '').split(';').filter(Boolean).map((x) => x.split(',').map(Number));
  for (const [lat, lon] of pts('points')) onClick(lat, lon);
  runCheck();
  for (const spec of (p('dress') || '').split(';').filter(Boolean)) {
    const [kind, along] = spec.split('@');
    const q = PALETTE.find((x) => x.kind === kind);
    if (q) S.dress.push({ kind: q.kind, model: q.model, every: q.every, side: q.kind === 'lamp' ? 'right' : 'both', offset: q.offset, scale: q.scale, face: q.face, along: along || '' });
  }
  if (p('start')) { const [lat, lon] = pts('start')[0]; S.armed = 'start'; onClick(lat, lon); }
  for (const [lat, lon] of pts('pads')) { S.armed = 'pad'; onClick(lat, lon); }
  runCheck();
  if (S.legs.length && !p('view')) map.fitBounds(L.latLngBounds(S.legs.flatMap(spanGeom)), { padding: [30, 30] });
  if (p('bake')) await bakeAndPreview();
}

// For tests and screenshots.
window.designer = { get report() { return S.report; }, get legs() { return S.legs; }, get preview() { return preview; }, block };

boot().catch((e) => { console.error(e); toast(`The designer could not start: ${e.message}`, true, 10000); });
