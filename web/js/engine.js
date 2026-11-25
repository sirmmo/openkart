// The engine as WebAssembly: JSON requests in, JSON answers out (see crates/openkart-web).

export class Engine {
  static async load(url = 'openkart.wasm') {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`${url}: ${res.status}`);
    const { instance } = await WebAssembly.instantiate(await res.arrayBuffer(), {});
    return new Engine(instance.exports);
  }

  constructor(wasm) {
    this.wasm = wasm;
  }

  call(request) {
    const w = this.wasm;
    const bytes = new TextEncoder().encode(JSON.stringify(request));
    const ptr = w.kart_alloc(bytes.length);
    new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes);
    const out = w.kart_call(ptr, bytes.length);
    const answer = JSON.parse(new TextDecoder().decode(new Uint8Array(w.memory.buffer, out, w.kart_out_len())));
    if (answer && answer.error) throw new Error(answer.error);
    return answer;
  }
}

// A game's files, listed in games/<id>/files.json by tools/site.sh.
export async function fetchGame(id) {
  const base = `games/${id}/`;
  const res = await fetch(base + 'files.json');
  if (!res.ok) throw new Error(`there is no game ${id}`);
  const names = await res.json();
  return Promise.all(names.map(async (n) => [n, await (await fetch(base + n)).text()]));
}
