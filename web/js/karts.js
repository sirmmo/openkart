// What moves: the racers (a driver from the `racers` models in a kart from the `karts` ones,
// with eyes and a crown where the racer's `look` asks), items in flight and on the road,
// hazards, and the particles of drifts, boosts and splats.

import * as THREE from 'three';

const KART_SCALE = 1.6;
const eyeGeo = new THREE.SphereGeometry(1, 14, 10);
const eyeWhite = new THREE.MeshStandardMaterial({ color: '#ffffff', roughness: 0.3 });
const eyeBlack = new THREE.MeshStandardMaterial({ color: '#111111', roughness: 0.3 });

// `look` lines as an object: { scale: '1.7', eyes: '0.62 angry', ... }.
export function lookOf(def) {
  const o = {};
  for (const [k, v] of def.look || []) o[k] = v;
  return o;
}

// Googly eyes on the front (+z) of a model, found by casting rays at it (as Capitan Pachino 64
// does for its vegetables and bosses).
export function addEyes(obj, { hf = 0.62, spread = 0.22, size = 0.11, angry = false } = {}) {
  obj.updateWorldMatrix(true, true);
  const box = new THREE.Box3().setFromObject(obj);
  const sz = box.getSize(new THREE.Vector3()), ctr = box.getCenter(new THREE.Vector3());
  const rc = new THREE.Raycaster();
  const meshes = [];
  obj.traverse((c) => c.isMesh && meshes.push(c));
  const er = size * Math.min(sz.y, Math.max(sz.x, sz.z) * 1.4);
  const inv = new THREE.Matrix4().copy(obj.matrixWorld).invert();
  for (const side of [-1, 1]) {
    const o = new THREE.Vector3(ctr.x + side * spread * sz.x, box.min.y + hf * sz.y, box.max.z + 2);
    rc.set(o, new THREE.Vector3(0, 0, -1));
    let hit = rc.intersectObjects(meshes, false)[0];
    for (let k = 0.6; !hit && k > 0.1; k -= 0.2) {
      o.x = ctr.x + side * spread * sz.x * k;
      rc.set(o, new THREE.Vector3(0, 0, -1));
      hit = rc.intersectObjects(meshes, false)[0];
    }
    const p = hit ? hit.point.clone() : new THREE.Vector3(o.x, o.y, box.max.z);
    const eye = new THREE.Mesh(eyeGeo, eyeWhite);
    eye.position.copy(p).add(new THREE.Vector3(0, 0, er * 0.35)).applyMatrix4(inv);
    const wscale = new THREE.Vector3(); obj.matrixWorld.decompose(new THREE.Vector3(), new THREE.Quaternion(), wscale);
    eye.scale.setScalar(er / wscale.x);
    const pupil = new THREE.Mesh(eyeGeo, eyeBlack);
    pupil.scale.setScalar(0.5); pupil.position.set(0, 0, 0.62);
    eye.add(pupil);
    obj.add(eye);
    if (angry) {
      const brow = new THREE.Mesh(new THREE.BoxGeometry(2.4, 0.5, 0.5), eyeBlack);
      brow.position.set(0, 1.25, 0.5); brow.rotation.z = side * 0.45;
      eye.add(brow);
    }
  }
}

function crown(gemColour) {
  const g = new THREE.Group();
  const gold = new THREE.MeshStandardMaterial({ color: '#ffc928', metalness: 0.6, roughness: 0.3, side: THREE.DoubleSide });
  g.add(new THREE.Mesh(new THREE.CylinderGeometry(1.1, 1.1, 0.5, 16, 1, true), gold));
  for (let i = 0; i < 6; i++) {
    const sp = new THREE.Mesh(new THREE.ConeGeometry(0.28, 0.8, 6), gold);
    const a = (i / 6) * Math.PI * 2;
    sp.position.set(Math.cos(a), 0.6, Math.sin(a));
    g.add(sp);
  }
  const gem = new THREE.Mesh(new THREE.OctahedronGeometry(0.22), new THREE.MeshStandardMaterial({ color: gemColour, metalness: 0.3, roughness: 0.2 }));
  gem.position.set(0, 0, 1.12);
  g.add(gem);
  return g;
}

function shadowed(o) {
  o.traverse((c) => { if (c.isMesh) { c.castShadow = true; c.receiveShadow = true; } });
  return o;
}

// A racer in its kart, facing +z, in metres.
export function makeRacer(def, models) {
  const look = lookOf(def);
  const root = new THREE.Group();
  const body = new THREE.Group(); // tilts and hops
  root.add(body);
  const kart = shadowed(models.karts[def.kart].clone(true));
  kart.scale.setScalar(KART_SCALE);
  body.add(kart);
  let seat = new THREE.Vector3(0, 0.45, -0.1);
  const wheels = [];
  kart.traverse((o) => {
    if (o.name === 'character') {
      seat = o.position.clone();
      o.visible = false;
    }
    if (/^wheel/.test(o.name)) wheels.push(o);
  });
  // The driver: on the seat, scaled and lifted as the racer's look says.
  const driver = new THREE.Group();
  const model = shadowed(models.racers[def.model].clone(true));
  const holder = new THREE.Group();
  holder.add(model);
  if (look.stand) model.rotation.x = Math.PI / 2;
  if (look.turn) model.rotation.y = (+look.turn * Math.PI) / 180;
  holder.scale.setScalar(+(look.scale || 1));
  driver.add(holder);
  driver.position.copy(seat).multiplyScalar(KART_SCALE);
  driver.position.y = +(look.lift || 0.5);
  body.add(driver);
  // Stand the model on the seat, centred.
  holder.updateWorldMatrix(true, true);
  const box = new THREE.Box3().setFromObject(holder);
  const c = box.getCenter(new THREE.Vector3());
  model.position.sub(new THREE.Vector3(c.x, box.min.y, c.z).divideScalar(+(look.scale || 1)));
  if (look.eyes) {
    const [hf, mood] = look.eyes.split(/\s+/);
    addEyes(holder, { hf: +hf || 0.62, angry: mood === 'angry', size: 0.1, spread: 0.2 });
  }
  if (look.crown) {
    holder.updateWorldMatrix(true, true);
    const b2 = new THREE.Box3().setFromObject(holder);
    const cr = crown('#' + look.crown);
    const k = 0.32;
    cr.scale.setScalar(k);
    cr.position.set(0, (b2.max.y - driver.position.y) + 0.05, 0);
    driver.add(shadowed(cr));
  }
  // A splat of sauce, shown when hit by a tomato.
  const splat = new THREE.Mesh(new THREE.SphereGeometry(0.5, 10, 8), new THREE.MeshStandardMaterial({ color: '#c8261d', roughness: 0.4 }));
  splat.scale.set(1.4, 0.35, 1.1); splat.position.set(0, 1.1, 0.7); splat.visible = false;
  body.add(splat);
  // Boost flames.
  const flames = new THREE.Group();
  const flameMat = new THREE.MeshBasicMaterial({ color: '#ff9a2a', transparent: true, opacity: 0.85 });
  for (const x of [-0.35, 0.35]) {
    const f = new THREE.Mesh(new THREE.ConeGeometry(0.22, 1.1, 8), flameMat);
    f.rotation.x = -Math.PI / 2; f.position.set(x, 0.45, -1.35);
    flames.add(f);
  }
  flames.visible = false;
  body.add(flames);
  return { root, body, wheels, splat, flames, def, spin: 0, hop: 0, prevDrift: 0, yaw: 0 };
}

// ---------- items and hazards ----------

function wheelOf(colour) {
  const g = new THREE.Group();
  const rind = new THREE.MeshStandardMaterial({ color: colour, roughness: 0.6 });
  const face = new THREE.MeshStandardMaterial({ color: new THREE.Color(colour).multiplyScalar(1.08), roughness: 0.8 });
  const cyl = new THREE.Mesh(new THREE.CylinderGeometry(0.85, 0.85, 0.5, 24), [rind, face, face]);
  cyl.rotation.z = Math.PI / 2;
  g.add(shadowed(cyl));
  const band = new THREE.Mesh(new THREE.CylinderGeometry(0.86, 0.86, 0.12, 24, 1, true), new THREE.MeshStandardMaterial({ color: '#8a5a1a' }));
  band.rotation.z = Math.PI / 2;
  g.add(band);
  return g;
}

// A model for an item by its `model` name: a glTF from the items folder, or `wheel:<hex>`.
export function itemModel(def, models) {
  const name = def.model || '';
  if (name.startsWith('wheel:')) return wheelOf('#' + name.slice(6));
  const m = models.items[name];
  if (!m) return new THREE.Mesh(new THREE.SphereGeometry(0.4), new THREE.MeshStandardMaterial({ color: '#ccc' }));
  const g = new THREE.Group();
  const c = shadowed(m.clone(true));
  g.add(c);
  // About a metre across.
  const box = new THREE.Box3().setFromObject(c);
  const size = box.getSize(new THREE.Vector3());
  const k = 0.95 / Math.max(size.x, size.y, size.z);
  c.scale.setScalar(k);
  const ctr = box.getCenter(new THREE.Vector3()).multiplyScalar(k);
  c.position.set(-ctr.x, -box.min.y * k, -ctr.z);
  return g;
}

export function slickModel() {
  const g = new THREE.Group();
  const m = new THREE.Mesh(new THREE.CircleGeometry(1.9, 24), new THREE.MeshStandardMaterial({ color: '#a89a24', roughness: 0.05, metalness: 0.2, transparent: true, opacity: 0.78 }));
  m.rotation.x = -Math.PI / 2; m.position.y = 0.06;
  m.scale.set(1, 0.78, 1);
  g.add(m);
  const m2 = new THREE.Mesh(new THREE.CircleGeometry(0.9, 16), new THREE.MeshStandardMaterial({ color: '#d7c64a', roughness: 0.02, transparent: true, opacity: 0.6 }));
  m2.rotation.x = -Math.PI / 2; m2.position.set(0.4, 0.07, -0.3);
  g.add(m2);
  return g;
}

export function hazardModel(def, models) {
  const m = models.hazards[def.model];
  const g = new THREE.Group();
  if (!m) return g;
  const c = shadowed(m.clone(true));
  const box = new THREE.Box3().setFromObject(c);
  const size = box.getSize(new THREE.Vector3());
  c.scale.setScalar(def.length / Math.max(size.z, 0.01));
  g.add(c);
  // A flashing light on the roof.
  const light = new THREE.Mesh(new THREE.BoxGeometry(0.9, 0.25, 0.3), new THREE.MeshStandardMaterial({ color: '#2a6bff', emissive: '#2a6bff', emissiveIntensity: 1 }));
  light.position.set(0, size.y * def.length / Math.max(size.z, 0.01) + 0.15, 0.6);
  g.add(light);
  g.userData.light = light;
  return g;
}

// ---------- particles ----------

export class Particles {
  constructor(scene, max = 600) {
    this.max = max;
    this.p = [];
    const g = new THREE.BufferGeometry();
    this.pos = new Float32Array(max * 3);
    this.col = new Float32Array(max * 3);
    g.setAttribute('position', new THREE.BufferAttribute(this.pos, 3));
    g.setAttribute('color', new THREE.BufferAttribute(this.col, 3));
    const c = document.createElement('canvas'); c.width = c.height = 32;
    const x = c.getContext('2d');
    const gr = x.createRadialGradient(16, 16, 0, 16, 16, 16);
    gr.addColorStop(0, 'rgba(255,255,255,1)'); gr.addColorStop(0.5, 'rgba(255,255,255,.8)'); gr.addColorStop(1, 'rgba(255,255,255,0)');
    x.fillStyle = gr; x.fillRect(0, 0, 32, 32);
    this.points = new THREE.Points(g, new THREE.PointsMaterial({ size: 0.45, vertexColors: true, map: new THREE.CanvasTexture(c), transparent: true, depthWrite: false }));
    this.points.frustumCulled = false;
    scene.add(this.points);
  }
  emit(x, y, z, colour, { n = 1, speed = 2, up = 1, life = 0.5, grav = 4 } = {}) {
    const c = new THREE.Color(colour);
    for (let i = 0; i < n; i++) {
      if (this.p.length >= this.max) this.p.shift();
      this.p.push({ x, y, z, vx: (Math.random() - 0.5) * speed, vy: Math.random() * up, vz: (Math.random() - 0.5) * speed, t: life, life, c, grav });
    }
  }
  update(dt) {
    this.p = this.p.filter((q) => (q.t -= dt) > 0);
    for (let i = 0; i < this.max; i++) {
      const q = this.p[i];
      if (!q) { this.pos[i * 3 + 1] = -1000; continue; }
      q.vy -= q.grav * dt;
      q.x += q.vx * dt; q.y += q.vy * dt; q.z += q.vz * dt;
      this.pos.set([q.x, q.y, q.z], i * 3);
      const f = q.t / q.life;
      this.col.set([q.c.r * f + (1 - f), q.c.g * f + (1 - f), q.c.b * f + (1 - f)], i * 3);
    }
    this.points.geometry.attributes.position.needsUpdate = true;
    this.points.geometry.attributes.color.needsUpdate = true;
  }
}
