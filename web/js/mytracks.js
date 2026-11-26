// Tracks drawn in the circuit designer and kept in this browser, for the game to race on. The
// designer saves each track it bakes (its `.kart` block and the baked `.track`); the game adds
// them to the game's own files. Both pages share one origin (designer/ and play/ on the site),
// so they share localStorage.

const KEY = 'openkart-tracks';

function readAll() {
  try {
    const all = JSON.parse(localStorage.getItem(KEY) || '{}');
    return all && typeof all === 'object' ? all : {};
  } catch {
    return {};
  }
}

/** The tracks saved for a game, most recent first: [{id, name, kart, track, saved}]. */
export function myTracks(game) {
  return Object.values(readAll()[game] || {}).sort((a, b) => b.saved - a.saved);
}

/** Saves (or replaces) a track. Throws a readable error when the browser's storage is full. */
export function saveTrack(game, t) {
  const all = readAll();
  all[game] = all[game] || {};
  all[game][t.id] = { id: t.id, name: t.name, kart: t.kart, track: t.track, saved: Date.now() };
  try {
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    throw new Error('The browser has no room for another track: remove one from the game\'s track list.');
  }
}

export function removeTrack(game, id) {
  const all = readAll();
  if (!all[game] || !all[game][id]) return;
  delete all[game][id];
  try { localStorage.setItem(KEY, JSON.stringify(all)); } catch { /* nothing to free */ }
}

/**
 * The game's files with this browser's tracks added. A saved track whose id is taken by one of
 * the game's own tracks races as `<id>-yours`, so it never hides the original. Returns
 * {files, mine: [{id, name, savedId}]}, `mine` most recent first.
 */
export function withMyTracks(game, files) {
  const names = new Set(files.map(([n]) => n));
  const out = files.slice();
  const mine = [];
  for (const t of myTracks(game)) {
    let id = t.id;
    for (let n = 1; names.has(`${id}.kart`) || names.has(`${id}.track`); n++) id = n === 1 ? `${t.id}-yours` : `${t.id}-yours-${n}`;
    const esc = t.id.replace(/[-]/g, '\\-');
    const kart = id === t.id ? t.kart : t.kart
      .replace(new RegExp(`^track ${esc}\\s*$`, 'm'), `track ${id}`)
      .replace(new RegExp(`^(\\s*osm )${esc}\\.track\\s*$`, 'm'), `$1${id}.track`);
    out.push([`${id}.kart`, kart], [`${id}.track`, t.track]);
    names.add(`${id}.kart`).add(`${id}.track`);
    mine.push({ id, name: t.name, savedId: t.id });
  }
  return { files: out, mine };
}
