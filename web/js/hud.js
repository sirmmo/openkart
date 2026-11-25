// The heads-up display: lap and clock, place, the item on its plate, the minimap, speed, the
// countdown and banners. Icons and portraits are the game's own models, rendered once.

import * as THREE from 'three';

const $ = (id) => document.getElementById(id);

export function ordinal(n) {
  const s = n % 100 >= 11 && n % 100 <= 13 ? 'th' : ({ 1: 'st', 2: 'nd', 3: 'rd' }[n % 10] || 'th');
  return s;
}

export function clock(t) {
  if (t == null) return '–';
  t = Math.max(0, t);
  const m = Math.floor(t / 60), s = t - m * 60;
  return `${m}:${s.toFixed(1).padStart(4, '0')}`;
}

// Renders objects into small pictures with a renderer of their own.
export class Snapshots {
  constructor(size = 256) {
    this.size = size;
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
    this.renderer.setSize(size, size, false);
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.scene = new THREE.Scene();
    this.scene.add(new THREE.HemisphereLight('#fff6e0', '#6b5a4a', 2.2));
    const sun = new THREE.DirectionalLight('#ffffff', 2.2);
    sun.position.set(3, 5, 4);
    this.scene.add(sun);
    this.camera = new THREE.PerspectiveCamera(30, 1, 0.01, 100);
  }

  // A data URL of `obj`, seen from the front, three quarters.
  shot(obj, { yaw = 0.6, pitch = 0.32, fill = 1.0 } = {}) {
    const holder = new THREE.Group();
    holder.add(obj);
    obj.rotation.y = yaw;
    this.scene.add(holder);
    holder.updateWorldMatrix(true, true);
    // The box of what shows (hidden parts, such as a kart's own driver, do not count).
    const box = new THREE.Box3();
    holder.traverseVisible((o) => { if (o.isMesh) box.expandByObject(o, false); });
    const c = box.getCenter(new THREE.Vector3());
    const r = box.getSize(new THREE.Vector3()).length() / 2 * 0.62;
    const d = (r / Math.sin((this.camera.fov * Math.PI) / 360)) * fill;
    this.camera.position.set(c.x, c.y + Math.sin(pitch) * d, c.z + Math.cos(pitch) * d);
    this.camera.lookAt(c);
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.render(this.scene, this.camera);
    const url = this.renderer.domElement.toDataURL('image/png');
    this.scene.remove(holder);
    return url;
  }
}

export class Hud {
  constructor() {
    this.el = $('hud');
    this.icons = {};
    this.mini = $('minimap');
    this.mg = this.mini.getContext('2d');
    this.lastItem = undefined;
    this.bannerT = 0;
  }

  setCourse(course, racers) {
    const pts = course.track.minimap;
    let x0 = Infinity, x1 = -Infinity, z0 = Infinity, z1 = -Infinity;
    for (const [x, z] of pts) { x0 = Math.min(x0, x); x1 = Math.max(x1, x); z0 = Math.min(z0, z); z1 = Math.max(z1, z); }
    const W = this.mini.width, pad = 18;
    const k = (W - 2 * pad) / Math.max(x1 - x0, z1 - z0);
    const ox = (W - (x1 - x0) * k) / 2, oz = (W - (z1 - z0) * k) / 2;
    this.map = (x, z) => [ox + (x - x0) * k, oz + (z - z0) * k];
    // The track, drawn once.
    const bg = document.createElement('canvas'); bg.width = bg.height = W;
    const g = bg.getContext('2d');
    const line = (w, c) => {
      g.beginPath();
      pts.forEach(([x, z], i) => { const [a, b] = this.map(x, z); i ? g.lineTo(a, b) : g.moveTo(a, b); });
      g.closePath(); g.lineWidth = w; g.strokeStyle = c; g.lineJoin = 'round'; g.stroke();
    };
    line(11, '#2a1d17'); line(6, '#e9dcc0');
    for (const l of course.track.landmarks) {
      const [a, b] = this.map(l.x, l.z);
      g.fillStyle = '#b5452f'; g.fillRect(a - 4, b - 4, 8, 8);
    }
    const [sx, sz] = this.map(course.track.samples[0].x, course.track.samples[0].z);
    g.fillStyle = '#2a1d17'; g.fillRect(sx - 6, sz - 2, 12, 4);
    this.bg = bg;
    this.colours = Object.fromEntries(racers.map((r) => [r.id, r.colour]));
  }

  show(on) { this.el.hidden = !on; }

  banner(text, secs = 2) {
    const b = $('banner');
    b.textContent = text;
    b.classList.add('show');
    this.bannerT = secs;
  }

  count(n) {
    const c = $('count');
    c.textContent = n > 0 ? String(n) : 'VIA!';
    c.className = 'count pop' + (n > 0 ? '' : ' go');
    void c.offsetWidth;
    c.style.display = 'block';
    clearTimeout(this.countT);
    this.countT = setTimeout(() => { c.style.display = 'none'; }, n > 0 ? 900 : 1000);
  }

  update(dt, view, me, laps, items) {
    if (this.bannerT > 0 && (this.bannerT -= dt) <= 0) $('banner').classList.remove('show');
    const k = view.karts[me];
    $('lap').textContent = `${k.lap}/${laps}`;
    $('clock').textContent = clock(k.time ?? view.time);
    $('place').textContent = k.place;
    $('ord').textContent = ordinal(k.place);
    $('of').textContent = `of ${view.karts.length}`;
    $('speed').textContent = Math.round(Math.abs(k.speed) * 3.6);
    const key = k.item ? (k.unpacking ? '?' + k.item : k.item) : '';
    if (key !== this.lastItem) {
      this.lastItem = key;
      const plate = $('plate');
      const icon = $('itemIcon');
      const g = icon.getContext('2d');
      g.clearRect(0, 0, icon.width, icon.height);
      plate.classList.toggle('unpack', !!k.unpacking);
      if (k.unpacking) {
        this.drawBasket(g, icon.width);
        $('itemName').textContent = 'Unpacking…';
      } else if (k.item && this.icons[k.item]) {
        g.drawImage(this.icons[k.item], 0, 0, icon.width, icon.height);
        $('itemName').textContent = items.find((i) => i.id === k.item)?.name || k.item;
      } else {
        $('itemName').textContent = '';
      }
    }
    // Minimap.
    const g = this.mg, W = this.mini.width;
    g.clearRect(0, 0, W, W);
    g.drawImage(this.bg, 0, 0);
    view.karts.forEach((o, i) => {
      if (i === me) return;
      const [a, b] = this.map(o.x, o.z);
      g.beginPath(); g.arc(a, b, 6, 0, 7); g.fillStyle = this.colours[o.racer] || '#fff'; g.fill();
      g.lineWidth = 2; g.strokeStyle = '#2a1d17'; g.stroke();
    });
    const [a, b] = this.map(k.x, k.z);
    g.save(); g.translate(a, b); g.rotate(Math.atan2(k.hx, -k.hz));
    g.beginPath(); g.moveTo(0, -11); g.lineTo(8, 8); g.lineTo(0, 4); g.lineTo(-8, 8); g.closePath();
    g.fillStyle = this.colours[k.racer] || '#fff'; g.fill(); g.lineWidth = 3; g.strokeStyle = '#2a1d17'; g.stroke();
    g.restore();
  }

  drawBasket(g, W) {
    g.save(); g.translate(W / 2, W / 2 + 8);
    g.fillStyle = '#b07a3e'; g.strokeStyle = '#2a1d17'; g.lineWidth = 4;
    g.beginPath(); g.moveTo(-40, -10); g.lineTo(40, -10); g.lineTo(32, 32); g.lineTo(-32, 32); g.closePath(); g.fill(); g.stroke();
    g.beginPath(); g.arc(0, -10, 30, Math.PI, 0); g.stroke();
    g.fillStyle = '#c62f2b';
    for (let i = 0; i < 4; i++) g.fillRect(-40 + i * 20, -18, 10, 10);
    g.restore();
  }
}
