// The track in 3D, from the engine's `course` answer: the road ribbon with kerbs and
// pavements, barriers along the walls, the buildings from OpenStreetMap (Bologna colours,
// porticoes where they face the track), the hospital's grounds and pavilions, the city gates,
// the scenery from `dress` lines, boost pads, the start gantry and the picnic baskets.
// Coordinates are the engine's: metres, x east, z south, y up.

import * as THREE from 'three';

const UP = new THREE.Vector3(0, 1, 0);

// A string to a number in [0, 1), the same every time.
export function hash(s) {
  let h = 2166136261;
  for (const c of String(s)) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return ((h >>> 0) % 100000) / 100000;
}

function hex(v, d) {
  return v ? new THREE.Color('#' + String(v).replace('#', '')) : new THREE.Color(d);
}

function canvasTexture(w, h, draw, repeat = true) {
  const c = document.createElement('canvas');
  c.width = w; c.height = h;
  draw(c.getContext('2d'), w, h);
  const t = new THREE.CanvasTexture(c);
  t.colorSpace = THREE.SRGBColorSpace;
  if (repeat) t.wrapS = t.wrapT = THREE.RepeatWrapping;
  t.anisotropy = 4;
  return t;
}

function noise(g, w, h, n, alpha, light = false) {
  for (let i = 0; i < n; i++) {
    const v = Math.random();
    g.fillStyle = light ? `rgba(255,255,255,${alpha * v})` : `rgba(0,0,0,${alpha * v})`;
    g.fillRect(Math.random() * w, Math.random() * h, 1 + Math.random() * 2, 1 + Math.random() * 2);
  }
}

// ---------- textures ----------

function asphalt() {
  // u across the road (0 left .. 1 right), v along it (one repeat every 10 m).
  return canvasTexture(256, 256, (g, w, h) => {
    g.fillStyle = '#55555b'; g.fillRect(0, 0, w, h);
    noise(g, w, h, 5000, 0.25); noise(g, w, h, 2500, 0.1, true);
    g.fillStyle = '#f2efe6';
    g.fillRect(w * 0.025, 0, w * 0.018, h); g.fillRect(w * 0.957, 0, w * 0.018, h);
    g.fillRect(w * 0.494, 0, w * 0.012, h * 0.45);
  });
}

function kerbTex() {
  return canvasTexture(64, 64, (g, w, h) => {
    g.fillStyle = '#c9302c'; g.fillRect(0, 0, w, h / 2);
    g.fillStyle = '#f4f1ea'; g.fillRect(0, h / 2, w, h / 2);
  });
}

function paving() {
  return canvasTexture(128, 128, (g, w, h) => {
    g.fillStyle = '#cdbd9f'; g.fillRect(0, 0, w, h);
    noise(g, w, h, 1500, 0.12);
    g.strokeStyle = 'rgba(90,70,50,.35)'; g.lineWidth = 2;
    for (let i = 0; i <= 4; i++) {
      g.beginPath(); g.moveTo(0, i * h / 4); g.lineTo(w, i * h / 4); g.stroke();
      for (let j = 0; j < 2; j++) {
        const x = (j * w / 2) + (i % 2) * w / 4;
        g.beginPath(); g.moveTo(x, i * h / 4); g.lineTo(x, (i + 1) * h / 4); g.stroke();
      }
    }
  });
}

// One bay of a façade: a window with green shutters on a plastered wall, one storey high.
function facade(wall, shutter = '#2f5d45', frame = '#efe6d0') {
  return canvasTexture(128, 128, (g, w, h) => {
    g.fillStyle = wall; g.fillRect(0, 0, w, h);
    noise(g, w, h, 1400, 0.08); noise(g, w, h, 600, 0.06, true);
    g.fillStyle = 'rgba(0,0,0,.12)'; g.fillRect(0, h - 8, w, 3);
    const ww = w * 0.3, wh = h * 0.5, x = (w - ww) / 2, y = h * 0.2;
    g.fillStyle = frame; g.fillRect(x - 5, y - 5, ww + 10, wh + 12);
    g.fillStyle = '#2b3138'; g.fillRect(x, y, ww, wh);
    g.fillStyle = 'rgba(160,200,230,.35)'; g.fillRect(x + 3, y + 3, ww / 2 - 4, wh - 6);
    g.fillStyle = shutter;
    for (const sx of [x - 5 - ww * 0.48, x + ww + 5]) {
      g.fillRect(sx, y - 2, ww * 0.48, wh + 6);
      g.fillStyle = 'rgba(0,0,0,.25)';
      for (let k = 0; k < 9; k++) g.fillRect(sx + 2, y + 2 + k * (wh / 9), ww * 0.48 - 4, 1.5);
      g.fillStyle = shutter;
    }
  });
}

function bricks(base = '#a9472f') {
  return canvasTexture(128, 128, (g, w, h) => {
    g.fillStyle = '#d6c3a5'; g.fillRect(0, 0, w, h);
    const bh = h / 8, bw = w / 4;
    for (let r = 0; r < 8; r++) {
      for (let c = -1; c < 5; c++) {
        const x = c * bw + (r % 2) * bw / 2;
        const k = 0.85 + Math.random() * 0.3;
        const col = new THREE.Color(base).multiplyScalar(k);
        g.fillStyle = '#' + col.getHexString();
        g.fillRect(x + 1.5, r * bh + 1.5, bw - 3, bh - 3);
      }
    }
    noise(g, w, h, 900, 0.1);
  });
}

function tiles() {
  return canvasTexture(128, 128, (g, w, h) => {
    g.fillStyle = '#9b4a32'; g.fillRect(0, 0, w, h);
    for (let r = 0; r < 8; r++) {
      g.fillStyle = r % 2 ? 'rgba(0,0,0,.18)' : 'rgba(255,220,180,.08)';
      g.fillRect(0, r * h / 8, w, h / 16);
    }
    noise(g, w, h, 1200, 0.15);
  });
}

function checker(n = 8, a = '#111', b = '#f4f1ea') {
  return canvasTexture(128, 128, (g, w, h) => {
    const s = w / n;
    for (let i = 0; i < n; i++) for (let j = 0; j < n; j++) {
      g.fillStyle = (i + j) % 2 ? a : b; g.fillRect(i * s, j * s, s, s);
    }
  });
}

function chevrons() {
  return canvasTexture(128, 128, (g, w, h) => {
    g.fillStyle = '#c0392b'; g.fillRect(0, 0, w, h);
    g.fillStyle = '#ffcc33';
    for (let k = 0; k < 2; k++) {
      const y = k * h / 2;
      g.beginPath(); g.moveTo(w * 0.1, y + h * 0.42); g.lineTo(w * 0.5, y + h * 0.08); g.lineTo(w * 0.9, y + h * 0.42);
      g.lineTo(w * 0.9, y + h * 0.5); g.lineTo(w * 0.5, y + h * 0.2); g.lineTo(w * 0.1, y + h * 0.5); g.fill();
    }
  });
}

function wicker() {
  return canvasTexture(64, 64, (g, w, h) => {
    g.fillStyle = '#b07a3e'; g.fillRect(0, 0, w, h);
    g.strokeStyle = '#7a4f22'; g.lineWidth = 2;
    for (let i = 0; i < 8; i++) { g.beginPath(); g.moveTo(0, i * 8 + 4); g.lineTo(w, i * 8 + 4); g.stroke(); }
    g.strokeStyle = 'rgba(255,230,180,.35)';
    for (let i = 0; i < 8; i++) { g.beginPath(); g.moveTo(i * 8 + (i % 2) * 4, 0); g.lineTo(i * 8 + (i % 2) * 4, h); g.stroke(); }
  });
}

export function textSprite(text, { size = 34, bg = 'rgba(42,29,23,.85)', fg = '#f6ecd6', scale = 0.02 } = {}) {
  const c = document.createElement('canvas');
  const g = c.getContext('2d');
  g.font = `700 ${size}px 'Barlow Condensed', sans-serif`;
  const w = Math.ceil(g.measureText(text).width) + size;
  c.width = w; c.height = Math.ceil(size * 1.5);
  g.font = `700 ${size}px 'Barlow Condensed', sans-serif`;
  g.fillStyle = bg;
  g.beginPath(); g.roundRect(0, 0, c.width, c.height, size * 0.35); g.fill();
  g.fillStyle = fg; g.textBaseline = 'middle'; g.textAlign = 'center';
  g.fillText(text, w / 2, c.height / 2 + 1);
  const t = new THREE.CanvasTexture(c);
  t.colorSpace = THREE.SRGBColorSpace;
  const sp = new THREE.Sprite(new THREE.SpriteMaterial({ map: t, depthWrite: false }));
  sp.scale.set(c.width * scale * (size / 34), c.height * scale * (size / 34), 1);
  return sp;
}

// ---------- geometry helpers ----------

// Triangles in plain arrays, merged into one mesh per material.
class Batch {
  constructor() { this.p = []; this.n = []; this.uv = []; }
  tri(a, b, c, na, uva, uvb, uvc) {
    this.p.push(...a, ...b, ...c);
    for (let i = 0; i < 3; i++) this.n.push(...na);
    this.uv.push(...uva, ...uvb, ...uvc);
  }
  // A quad a b c d (counter-clockwise seen from where its normal points).
  quad(a, b, c, d, nrm, uvs) {
    this.tri(a, b, c, nrm, uvs[0], uvs[1], uvs[2]);
    this.tri(a, c, d, nrm, uvs[0], uvs[2], uvs[3]);
  }
  mesh(material) {
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.Float32BufferAttribute(this.p, 3));
    g.setAttribute('normal', new THREE.Float32BufferAttribute(this.n, 3));
    g.setAttribute('uv', new THREE.Float32BufferAttribute(this.uv, 2));
    const m = new THREE.Mesh(g, material);
    m.castShadow = true; m.receiveShadow = true;
    return m;
  }
}

// A wall from a to b (x, z), between heights y0 and y1, facing `out` (unit x, z); uv in metres
// over `bay` (the façade texture is one bay wide and one storey high).
function wall(batch, a, b, y0, y1, out, bay = 3.4, u0 = 0) {
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
  const n = [out[0], 0, out[1]];
  const A0 = [a[0], y0, a[1]], B0 = [b[0], y0, b[1]], B1 = [b[0], y1, b[1]], A1 = [a[0], y1, a[1]];
  const uv = [[u0, y0 / bay], [u0 + len / bay, y0 / bay], [u0 + len / bay, y1 / bay], [u0, y1 / bay]];
  // (b - a) x up is (-ez, 0, ex): the face A0 B0 B1 looks that way. Wind it to face `out`.
  const ex = b[0] - a[0], ez = b[1] - a[1];
  if (-ez * n[0] + ex * n[2] > 0) batch.quad(A0, B0, B1, A1, n, uv);
  else batch.quad(B0, A0, A1, B1, n, [uv[1], uv[0], uv[3], uv[2]]);
}

// Up-facing triangles over a polygon at height y.
function cap(batch, poly, y, scale = 0.25) {
  const shape = poly.map(([x, z]) => new THREE.Vector2(x, z));
  const tris = THREE.ShapeUtils.triangulateShape(shape, []);
  for (const [i, j, k] of tris) {
    const a = poly[i], b = poly[j], c = poly[k];
    // Cross of (b - a) x (c - a) in y for x-z: must point up.
    const cy = (b[1] - a[1]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[1] - a[1]);
    const P = (p) => [p[0], y, p[1]], U = (p) => [p[0] * scale, p[1] * scale];
    if (cy > 0) batch.tri(P(a), P(b), P(c), [0, 1, 0], U(a), U(b), U(c));
    else batch.tri(P(a), P(c), P(b), [0, 1, 0], U(a), U(c), U(b));
  }
}

// Instanced copies of a loaded model (each of its meshes one InstancedMesh).
export function instanced(model, mats, { tint = null, shadow = true } = {}) {
  const group = new THREE.Group();
  if (!mats.length) return group;
  model.updateMatrixWorld(true);
  model.traverse((o) => {
    if (!o.isMesh) return;
    let material = o.material;
    if (tint && tint[material.name]) {
      material = material.clone();
      material.color = new THREE.Color(tint[material.name]);
      if (material.map) material.map = null;
    }
    const im = new THREE.InstancedMesh(o.geometry, material, mats.length);
    const m = new THREE.Matrix4();
    mats.forEach((t, i) => im.setMatrixAt(i, m.multiplyMatrices(t, o.matrixWorld)));
    im.castShadow = shadow; im.receiveShadow = true;
    im.instanceMatrix.needsUpdate = true;
    im.computeBoundingSphere();
    group.add(im);
  });
  return group;
}

function placeMatrix(x, y, z, angle, s) {
  return new THREE.Matrix4().compose(
    new THREE.Vector3(x, y, z),
    new THREE.Quaternion().setFromAxisAngle(UP, angle),
    new THREE.Vector3(s, s, s),
  );
}

// ---------- the world ----------

export class World {
  constructor(scene, course, models) {
    this.scene = scene;
    this.course = course;
    this.models = models;
    this.root = new THREE.Group();
    scene.add(this.root);
    this.look = course.look || {};
    this.samples = course.track.samples;
    this.length = course.track.length;
    this.build();
  }

  // The point d metres left of the centre at sample i.
  at(i, d) {
    const s = this.samples[(i + this.samples.length) % this.samples.length];
    return [s.x + s.tz * d, s.z - s.tx * d];
  }

  // Nearest sample to (x, z): brute force over a stride, then refined.
  nearest(x, z) {
    let best = 0, bd = Infinity;
    const S = this.samples;
    for (let i = 0; i < S.length; i += 4) {
      const d = (S[i].x - x) ** 2 + (S[i].z - z) ** 2;
      if (d < bd) { bd = d; best = i; }
    }
    for (let i = best - 4; i <= best + 4; i++) {
      const s = S[(i + S.length) % S.length];
      const d = (s.x - x) ** 2 + (s.z - z) ** 2;
      if (d < bd) { bd = d; best = (i + S.length) % S.length; }
    }
    return best;
  }

  build() {
    this.ground();
    this.road();
    this.barriers();
    this.buildings();
    this.gates();
    this.dress();
    this.start();
    this.pads();
    this.baskets();
  }

  ground() {
    const g = new THREE.Mesh(new THREE.PlaneGeometry(6000, 6000), new THREE.MeshLambertMaterial({ color: hex(this.look.ground, '#c9b79a') }));
    g.rotation.x = -Math.PI / 2; g.position.y = -0.05; g.receiveShadow = true;
    this.root.add(g);
    const grass = new THREE.MeshLambertMaterial({ color: '#86a65a' });
    for (const a of this.course.track.areas) {
      const shape = new THREE.Shape(a.foot.map(([x, z]) => new THREE.Vector2(x, -z)));
      const m = new THREE.Mesh(new THREE.ShapeGeometry(shape), grass);
      m.rotation.x = -Math.PI / 2; m.position.y = 0.0; m.receiveShadow = true;
      this.root.add(m);
    }
  }

  // Strips along the track between two lateral offsets (functions of the sample).
  strip(material, from, to, y, vScale, uAcross = true) {
    const S = this.samples, n = S.length;
    const b = new Batch();
    for (let i = 0; i < n; i++) {
      const j = (i + 1) % n;
      const a0 = this.at(i, from(S[i])), a1 = this.at(i, to(S[i]));
      const b0 = this.at(j, from(S[j])), b1 = this.at(j, to(S[j]));
      const v0 = S[i].s / vScale, v1 = (j === 0 ? this.length : S[j].s) / vScale;
      const P = (p) => [p[0], y, p[1]];
      const uvA0 = uAcross ? [0, v0] : [v0, 0], uvA1 = uAcross ? [1, v0] : [v0, 1];
      const uvB0 = uAcross ? [0, v1] : [v1, 0], uvB1 = uAcross ? [1, v1] : [v1, 1];
      // from is the left edge (larger d) when from > to.
      b.tri(P(a0), P(a1), P(b1), [0, 1, 0], uvA0, uvA1, uvB1);
      b.tri(P(a0), P(b1), P(b0), [0, 1, 0], uvA0, uvB1, uvB0);
    }
    const m = b.mesh(material);
    m.castShadow = false;
    material.side = THREE.DoubleSide;
    this.root.add(m);
    return m;
  }

  road() {
    const road = new THREE.MeshStandardMaterial({ map: asphalt(), roughness: 0.92 });
    this.strip(road, (s) => s.w / 2, (s) => -s.w / 2, 0.03, 10);
    const kt = kerbTex();
    const kerb = new THREE.MeshStandardMaterial({ map: kt, roughness: 0.8 });
    this.strip(kerb, (s) => s.w / 2 + 0.7, (s) => s.w / 2, 0.07, 3, false);
    this.strip(kerb, (s) => -s.w / 2, (s) => -s.w / 2 - 0.7, 0.07, 3, false);
    const pave = new THREE.MeshStandardMaterial({ map: paving(), roughness: 1 });
    pave.map.repeat.set(1, 1);
    this.strip(pave, (s) => s.left, (s) => s.w / 2 + 0.7, 0.1, 2.5, false);
    this.strip(pave, (s) => -s.w / 2 - 0.7, (s) => -s.right, 0.1, 2.5, false);
  }

  // Low barriers along the walls, red and white, open where a hazard crosses.
  barriers() {
    const S = this.samples, n = S.length;
    const gaps = this.course.crossings.map((c) => c.s);
    const box = new THREE.BoxGeometry(1.9, 0.85, 0.45);
    box.translate(0, 0.42, 0);
    const mat = new THREE.MeshStandardMaterial({ roughness: 0.6 });
    const im = new THREE.InstancedMesh(box, mat, n * 2);
    const red = new THREE.Color('#c9302c'), white = new THREE.Color('#f0ece2');
    let k = 0;
    const m = new THREE.Matrix4();
    for (let i = 0; i < n; i++) {
      const s = S[i];
      if (gaps.some((g) => Math.abs(g - s.s) < 7 || Math.abs(g - s.s) > this.length - 7)) continue;
      const angle = Math.atan2(s.tx, s.tz) + Math.PI / 2;
      for (const side of [1, -1]) {
        const d = side > 0 ? s.left + 0.2 : -s.right - 0.2;
        const [x, z] = this.at(i, d);
        m.compose(new THREE.Vector3(x, 0.1, z), new THREE.Quaternion().setFromAxisAngle(UP, angle), new THREE.Vector3(1, 1, 1));
        im.setMatrixAt(k, m);
        im.setColorAt(k, (Math.floor(s.s / 2) % 2) ? red : white);
        k++;
      }
    }
    im.count = k;
    im.castShadow = true; im.receiveShadow = true;
    im.instanceMatrix.needsUpdate = true;
    if (im.instanceColor) im.instanceColor.needsUpdate = true;
    this.root.add(im);
  }

  buildings() {
    const palette = (this.look.palette || 'c8553d d9a441 b5452f e0b36a cf7a52').split(/\s+/).map((c) => '#' + c);
    const kinds = {
      pavilion: ['#e9d6a6', '#ead9b8', '#dcc08a'],
      church: ['#b9573a', '#c76a45'],
      tower: ['#a9472f'],
      roof: ['#8b8b88'],
      shed: ['#b8aa92'],
    };
    const mats = new Map();
    const matFor = (key, colour, shutter) => {
      if (!mats.has(key)) {
        mats.set(key, { batch: new Batch(), material: new THREE.MeshStandardMaterial({ map: facade(colour, shutter), roughness: 0.95 }) });
      }
      return mats.get(key).batch;
    };
    const roofB = new Batch();
    const portB = new Batch();
    const ceilB = new Batch();
    const columns = [];
    const labels = [];
    const DEPTH = 3, POST = 3.9;
    for (const bd of this.course.track.buildings) {
      const poly = bd.foot;
      const nPoly = poly.length;
      let colour, shutter = '#2f5d45', key;
      if (kinds[bd.kind]) {
        const list = kinds[bd.kind];
        colour = list[Math.floor(hash(bd.id) * list.length)];
        if (bd.kind === 'pavilion') shutter = '#7a8a90';
        key = bd.kind + colour;
      } else {
        colour = palette[Math.floor(hash(bd.id) * palette.length)];
        key = 'h' + colour;
        if (hash(bd.id + 'x') < 0.3) shutter = '#6b4a2b';
        key += shutter;
      }
      const batch = matFor(key, colour, shutter);
      const h = bd.height;
      const portico = new Set(bd.portico || []);
      let u = 0;
      for (let i = 0; i < nPoly; i++) {
        const a = poly[i], b = poly[(i + 1) % nPoly];
        const ex = b[0] - a[0], ez = b[1] - a[1];
        const len = Math.hypot(ex, ez);
        if (len < 0.01) continue;
        const out = [ez / len, -ex / len];
        if (portico.has(i) && h > POST + 2) {
          wall(batch, a, b, POST, h, out, 3.4, u);
          // Recessed ground floor, its ceiling, and the columns with their arches.
          const ia = [a[0] - out[0] * DEPTH, a[1] - out[1] * DEPTH], ib = [b[0] - out[0] * DEPTH, b[1] - out[1] * DEPTH];
          wall(portB, ia, ib, 0, POST, out, 3.4, u);
          wall(portB, a, ia, 0, POST, [ex / len, ez / len], 3.4);
          wall(portB, ib, b, 0, POST, [-ex / len, -ez / len], 3.4);
          const P = (p) => [p[0], POST - 0.02, p[1]];
          ceilB.quad(P(a), P(ia), P(ib), P(b), [0, -1, 0], [[0, 0], [0, 1], [1, 1], [1, 0]]);
          const bays = Math.max(1, Math.round(len / 3.6));
          for (let k = 0; k <= bays; k++) {
            const f = k / bays;
            columns.push({ x: a[0] + ex * f - out[0] * 0.3, z: a[1] + ez * f - out[1] * 0.3, angle: Math.atan2(ex, ez), w: len / bays, arch: k < bays, f });
          }
        } else {
          wall(batch, a, b, 0, h, out, 3.4, u);
        }
        u += len / 3.4;
      }
      cap(roofB, poly, h);
      if (bd.kind === 'pavilion' && bd.name) {
        const short = bd.name.split(' - ')[0];
        labels.push({ text: short, poly, h });
      }
      if (bd.kind === 'church' || bd.kind === 'tower') {
        // A pitched roof is too much: a cornice will do.
        cap(roofB, poly, h + 0.02);
      }
    }
    for (const { batch, material } of mats.values()) this.root.add(batch.mesh(material));
    const roofMat = new THREE.MeshStandardMaterial({ map: tiles(), roughness: 0.9, side: THREE.DoubleSide });
    this.root.add(roofB.mesh(roofMat));
    const portMat = new THREE.MeshStandardMaterial({ map: facade('#c9a77a', '#5c3b22', '#b89a70'), roughness: 1, color: '#d8c4a4' });
    this.root.add(portB.mesh(portMat));
    const ceil = ceilB.mesh(new THREE.MeshStandardMaterial({ color: '#b39a78', side: THREE.DoubleSide }));
    this.root.add(ceil);
    // Columns and arches.
    const colGeo = new THREE.BoxGeometry(0.55, POST, 0.55); colGeo.translate(0, POST / 2, 0);
    const colMat = new THREE.MeshStandardMaterial({ color: '#e4d2ae', roughness: 0.9 });
    const cols = new THREE.InstancedMesh(colGeo, colMat, columns.length);
    const archShape = new THREE.Shape();
    archShape.moveTo(-0.5, 0); archShape.lineTo(0.5, 0); archShape.lineTo(0.5, 1); archShape.lineTo(-0.5, 1); archShape.lineTo(-0.5, 0);
    const hole = new THREE.Path(); hole.moveTo(-0.42, 0); hole.lineTo(-0.42, 0.25); hole.absarc(0, 0.25, 0.42, Math.PI, 0, true); hole.lineTo(0.42, 0); hole.lineTo(-0.42, 0);
    archShape.holes.push(hole);
    const archGeo = new THREE.ExtrudeGeometry(archShape, { depth: 0.5, bevelEnabled: false, curveSegments: 10 });
    archGeo.translate(0, 0, -0.25);
    const arches = new THREE.InstancedMesh(archGeo, colMat, columns.length);
    const m = new THREE.Matrix4();
    let na = 0;
    columns.forEach((c, i) => {
      cols.setMatrixAt(i, placeMatrix(c.x, 0, c.z, c.angle, 1));
      if (c.arch) {
        // Arch from this column to the next, spanning w, its top at the ceiling.
        const ax = Math.sin(c.angle), az = Math.cos(c.angle);
        const cx = c.x + ax * c.w / 2, cz = c.z + az * c.w / 2;
        const hgt = Math.min(c.w * 0.62, POST * 0.6);
        m.compose(new THREE.Vector3(cx, POST - hgt, cz), new THREE.Quaternion().setFromAxisAngle(UP, c.angle - Math.PI / 2), new THREE.Vector3(c.w, hgt, 1));
        arches.setMatrixAt(na++, m);
      }
    });
    arches.count = na;
    for (const im of [cols, arches]) { im.castShadow = true; im.receiveShadow = true; im.instanceMatrix.needsUpdate = true; this.root.add(im); }
    // Pavilion names on their roofs.
    for (const l of labels) {
      let cx = 0, cz = 0;
      for (const [x, z] of l.poly) { cx += x; cz += z; }
      cx /= l.poly.length; cz /= l.poly.length;
      const i = this.nearest(cx, cz);
      if (Math.hypot(this.samples[i].x - cx, this.samples[i].z - cz) > 90) continue;
      const sp = textSprite(l.text, { bg: 'rgba(47,93,69,.92)', size: 40, scale: 0.05 });
      sp.position.set(cx, l.h + 3, cz);
      this.root.add(sp);
    }
  }

  // City gates: a brick block with an arched passage along the road, battlements, and a tiled
  // roof on the bigger ones.
  gates() {
    const brick = bricks();
    for (const l of this.course.track.landmarks) {
      if (l.kind !== 'gate') continue;
      const [w0, d0, h0] = l.size;
      const w = Math.max(w0, 9), d = Math.max(d0, 7), h = Math.max(h0, 12);
      const g = new THREE.Group();
      const shape = new THREE.Shape();
      shape.moveTo(-w / 2, 0); shape.lineTo(w / 2, 0); shape.lineTo(w / 2, h); shape.lineTo(-w / 2, h); shape.lineTo(-w / 2, 0);
      const aw = Math.min(w * 0.42, 6), ah = h * 0.42;
      const hole = new THREE.Path();
      hole.moveTo(-aw / 2, 0); hole.lineTo(-aw / 2, ah); hole.absarc(0, ah, aw / 2, Math.PI, 0, true); hole.lineTo(aw / 2, 0); hole.lineTo(-aw / 2, 0);
      shape.holes.push(hole);
      const geo = new THREE.ExtrudeGeometry(shape, { depth: d, bevelEnabled: false, curveSegments: 16 });
      geo.translate(0, 0, -d / 2);
      const t = brick.clone(); t.needsUpdate = true; t.repeat.set(0.33, 0.33);
      const mat = new THREE.MeshStandardMaterial({ map: t, roughness: 0.9 });
      const body = new THREE.Mesh(geo, mat);
      body.castShadow = true; body.receiveShadow = true;
      g.add(body);
      // Stone trim round the arch and at the top.
      const trim = new THREE.MeshStandardMaterial({ color: '#e2d3b4', roughness: 0.8 });
      const band = new THREE.Mesh(new THREE.BoxGeometry(w + 0.6, 0.6, d + 0.6), trim);
      band.position.y = h; g.add(band);
      const merlon = new THREE.BoxGeometry(0.9, 1.3, 0.6);
      for (const side of [-1, 1]) {
        for (let x = -w / 2 + 0.6; x <= w / 2 - 0.4; x += 1.7) {
          const mm = new THREE.Mesh(merlon, mat); mm.position.set(x, h + 0.95, side * (d / 2 + 0.0)); mm.castShadow = true; g.add(mm);
        }
      }
      if (w0 > 12) {
        const roof = new THREE.Mesh(new THREE.ConeGeometry(Math.hypot(w, d) / 2 * 0.95, h * 0.35, 4), new THREE.MeshStandardMaterial({ map: tiles(), roughness: 0.9 }));
        roof.rotation.y = Math.PI / 4; roof.scale.set(w / Math.hypot(w, d) * 1.41, 1, d / Math.hypot(w, d) * 1.41);
        roof.position.y = h + 0.3 + h * 0.175; roof.castShadow = true; g.add(roof);
      }
      const sign = textSprite(l.name, { size: 40, scale: 0.032, bg: 'rgba(181,69,47,.92)' });
      sign.position.set(0, h + (w0 > 12 ? h * 0.4 : 2.6) + 2, 0);
      g.add(sign);
      g.position.set(l.x, 0, l.z);
      g.rotation.y = Math.atan2(l.dx, l.dz);
      this.root.add(g);
    }
  }

  // Scenery from the `dress` lines: models by name, set beside the road.
  dress() {
    const byModel = new Map();
    for (const p of this.course.props) {
      if (!byModel.has(p.model)) byModel.set(p.model, []);
      byModel.get(p.model).push(p);
    }
    for (const [name, list] of byModel) {
      const model = this.models.props[name];
      if (!model) continue;
      const kind = list[0].kind;
      // Baked by osm2track: where, which way, how big.
      const mats = list.map((p) => placeMatrix(p.x, kind === 'tree' ? 0 : 0.1, p.z, p.angle, p.scale));
      const tint = kind === 'tree' ? { leafsGreen: '#7d9a45', woodBark: '#c9b994' } : null;
      this.root.add(instanced(model, mats, { tint }));
    }
  }

  start() {
    const s = this.samples[0];
    const w = s.w;
    // A chequered line across the road.
    const line = new THREE.Mesh(new THREE.PlaneGeometry(w, 2.2), new THREE.MeshStandardMaterial({ map: checker(8), roughness: 0.8 }));
    line.material.map.repeat.set(w / 2.2, 1);
    line.rotation.x = -Math.PI / 2;
    line.rotation.z = -Math.atan2(s.tz, s.tx) - Math.PI / 2;
    line.position.set(s.x, 0.045, s.z);
    this.root.add(line);
    // A gantry over it: two posts beyond the barriers, a chequered beam and the start lights.
    const t = new THREE.Vector3(s.tx, 0, s.tz);
    const g = new THREE.Group();
    const post = new THREE.MeshStandardMaterial({ color: '#e9e2d2', roughness: 0.6 });
    const span = s.left + s.right + 1.2;
    const h = 7.5;
    for (const d of [s.left + 0.6, -s.right - 0.6]) {
      const p = new THREE.Mesh(new THREE.BoxGeometry(0.5, h, 0.5), post);
      p.position.set(0, h / 2, -d);
      p.castShadow = true;
      g.add(p);
    }
    const flag = new THREE.MeshStandardMaterial({ map: checker(4), roughness: 0.7 });
    flag.map.repeat.set(span / 1.4, 1);
    const beam = new THREE.Mesh(new THREE.BoxGeometry(0.6, 1.4, span), [flag, flag, post, post, post, post]);
    beam.position.set(0, h, -(s.left - s.right) / 2);
    beam.castShadow = true;
    g.add(beam);
    this.lights = [];
    for (let k = 0; k < 5; k++) {
      const l = new THREE.Mesh(new THREE.SphereGeometry(0.32, 12, 8), new THREE.MeshStandardMaterial({ color: '#330a08', emissive: '#ff2a1a', emissiveIntensity: 0 }));
      l.position.set(-0.35, h - 1.1, -(s.left - s.right) / 2 + (k - 2) * 0.9);
      g.add(l);
      this.lights.push(l);
    }
    // The group's local z runs across the road (to the left), x along it.
    g.position.set(s.x, 0, s.z);
    g.rotation.y = Math.atan2(t.x, t.z) - Math.PI / 2;
    this.root.add(g);
  }

  // The start lights: n of 5 lit (the countdown), or all off.
  setLights(n) {
    (this.lights || []).forEach((l, i) => { l.material.emissiveIntensity = i < n ? 2.5 : 0; });
  }

  pads() {
    const tex = chevrons();
    this.padTex = tex;
    for (const p of this.course.pads) {
      const m = new THREE.Mesh(new THREE.PlaneGeometry(3.2, 4.4), new THREE.MeshStandardMaterial({ map: tex, emissive: '#ff7a1a', emissiveIntensity: 0.6, emissiveMap: tex, roughness: 0.5 }));
      m.rotation.order = 'YXZ';
      m.rotation.y = Math.atan2(p.tx, p.tz) + Math.PI;
      m.rotation.x = -Math.PI / 2;
      m.position.set(p.x, 0.05, p.z);
      this.root.add(m);
    }
  }

  // Picnic baskets: a wicker box with a gingham cloth and a handle.
  baskets() {
    const wick = new THREE.MeshStandardMaterial({ map: wicker(), roughness: 1 });
    wick.map.repeat.set(2, 1);
    const cloth = new THREE.MeshStandardMaterial({ map: checker(6, '#c62f2b', '#f6f1e6'), roughness: 0.9 });
    const body = new THREE.BoxGeometry(1.1, 0.62, 0.78);
    const lid = new THREE.BoxGeometry(1.18, 0.1, 0.86);
    const handle = new THREE.TorusGeometry(0.42, 0.05, 6, 16, Math.PI);
    this.basketMeshes = this.course.baskets.map((b, i) => {
      const g = new THREE.Group();
      const m1 = new THREE.Mesh(body, wick); m1.position.y = 0.31; m1.castShadow = true;
      const m2 = new THREE.Mesh(lid, cloth); m2.position.y = 0.66; m2.castShadow = true;
      const m3 = new THREE.Mesh(handle, wick); m3.position.y = 0.7;
      g.add(m1, m2, m3);
      g.position.set(b.x, 0.25, b.z);
      g.userData.phase = i * 0.7;
      this.root.add(g);
      return g;
    });
  }

  update(t, view) {
    if (this.padTex) this.padTex.offset.y = (t * 1.6) % 1;
    if (this.basketMeshes && view) {
      this.basketMeshes.forEach((g, i) => {
        const on = view.baskets[i];
        g.visible = on;
        g.rotation.y = t * 1.3 + g.userData.phase;
        g.position.y = 0.3 + Math.sin(t * 2.5 + g.userData.phase) * 0.12;
      });
    }
  }
}
