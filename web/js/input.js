// Controls: keyboard, gamepad and touch, read into one input per step:
// { steer: -100 (right) .. 100 (left), gas, brake, drift, item }.

const KEYS = {
  left: ['ArrowLeft', 'KeyA'],
  right: ['ArrowRight', 'KeyD'],
  gas: ['ArrowUp', 'KeyW'],
  brake: ['ArrowDown', 'KeyS'],
  drift: ['Space', 'ShiftLeft', 'ShiftRight'],
  item: ['KeyX', 'KeyE', 'Enter', 'ControlLeft', 'ControlRight'],
};

export class Controls {
  constructor() {
    this.down = new Set();
    this.touch = new Set();
    this.steer = 0;
    this.usedTouch = false;
    this.onPause = null;
    addEventListener('keydown', (e) => {
      if (e.code === 'Escape' || e.code === 'KeyP') { this.onPause?.(); return; }
      if (Object.values(KEYS).some((k) => k.includes(e.code))) e.preventDefault();
      this.down.add(e.code);
    });
    addEventListener('keyup', (e) => this.down.delete(e.code));
    addEventListener('blur', () => this.down.clear());
    for (const b of document.querySelectorAll('#touch button')) {
      const k = b.dataset.k;
      const on = (e) => { e.preventDefault(); this.touch.add(k); b.classList.add('on'); };
      const off = (e) => { e.preventDefault(); this.touch.delete(k); b.classList.remove('on'); };
      b.addEventListener('pointerdown', on);
      b.addEventListener('pointerup', off);
      b.addEventListener('pointercancel', off);
      b.addEventListener('pointerleave', off);
    }
    addEventListener('touchstart', () => { this.usedTouch = true; }, { passive: true });
    this.padStart = false;
  }

  key(name) {
    return KEYS[name].some((c) => this.down.has(c)) || this.touch.has(name);
  }

  // One step's input; `dt` eases keyboard steering in and out.
  read(dt) {
    const pads = navigator.getGamepads ? [...navigator.getGamepads()].filter(Boolean) : [];
    let padSteer = 0, padGas = false, padBrake = false, padDrift = false, padItem = false, start = false;
    for (const p of pads) {
      const x = p.axes[0] || 0;
      if (Math.abs(x) > 0.15) padSteer = -x;
      const b = (i) => p.buttons[i] && (p.buttons[i].pressed || p.buttons[i].value > 0.3);
      padGas ||= b(0) || b(7);
      padBrake ||= b(1) || b(6);
      padDrift ||= b(5);
      padItem ||= b(2) || b(4) || b(3);
      start ||= b(9);
    }
    if (start && !this.padStart) this.onPause?.();
    this.padStart = start;
    const want = (this.key('left') ? 1 : 0) - (this.key('right') ? 1 : 0);
    if (want === 0) this.steer = Math.abs(this.steer) < dt * 8 ? 0 : this.steer - Math.sign(this.steer) * dt * 8;
    else this.steer = Math.max(-1, Math.min(1, (Math.sign(want) !== Math.sign(this.steer) ? 0 : this.steer) + want * dt * 6));
    const steer = padSteer || this.steer;
    // On a touch screen the kart accelerates by itself.
    const brake = this.key('brake') || padBrake;
    return {
      steer: Math.round(Math.max(-1, Math.min(1, steer)) * 100),
      gas: this.key('gas') || padGas || (this.usedTouch && !brake),
      brake,
      drift: this.key('drift') || padDrift,
      item: this.key('item') || padItem,
    };
  }
}
