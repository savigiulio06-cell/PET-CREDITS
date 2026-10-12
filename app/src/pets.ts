// Pet skins: pets/<id>/manifest.json, plus optional local-only skins in pets-local/<id>/ (gitignored).
// Stick skins are drawn procedurally (rig.ts); sprite skins use bundled PNG poses (sprite.ts).
import type { PetManifest, SpriteQuote, SpritePose, SpriteSign, SpriteSpec } from './types';

const publicManifests = import.meta.glob('../../pets/*/manifest.json', { eager: true, import: 'default' });
// pets-local may not exist (CI, other users): the glob is then simply empty.
const localManifests = import.meta.glob('../../pets-local/*/manifest.json', { eager: true, import: 'default' });

// Sprite images come ONLY from these bundled maps, keyed by "<folder>/<file>.png".
const publicImages = import.meta.glob('../../pets/*/*.png', { eager: true, query: '?url', import: 'default' });
const localImages = import.meta.glob('../../pets-local/*/*.png', { eager: true, query: '?url', import: 'default' });

const COLOR_RE = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;
const FILE_RE = /^[A-Za-z0-9_-]+\.png$/;
const LANG_RE = /^[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})*$/;

function folderOf(path: string): string {
  return path.split('/').slice(-2, -1)[0] ?? '';
}

function fileOf(path: string): string {
  return path.split('/').pop() ?? '';
}

const imageUrls = new Map<string, string>();
// Keys are per origin ("pets:<folder>/<file>" / "local:<folder>/<file>"): a skin only sees its own images.
for (const [origin, group] of [['pets', publicImages], ['local', localImages]] as const) {
  for (const [path, url] of Object.entries(group)) {
    if (typeof url === 'string') imageUrls.set(`${origin}:${folderOf(path)}/${fileOf(path)}`, url);
  }
}

function str(v: unknown, fallback: string): string {
  return typeof v === 'string' ? v : fallback;
}

function num(v: unknown, fallback: number, min: number, max: number): number {
  return typeof v === 'number' && Number.isFinite(v) ? Math.min(max, Math.max(min, v)) : fallback;
}

function finite(v: unknown): v is number {
  return typeof v === 'number' && Number.isFinite(v);
}

function obj(v: unknown): Record<string, unknown> | null {
  return typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
}

function pair(v: unknown): [number, number] | null {
  return Array.isArray(v) && v.length === 2 && finite(v[0]) && finite(v[1]) ? [v[0], v[1]] : null;
}

function parseSign(raw: unknown): SpriteSign | null {
  const r = obj(raw);
  if (!r || !finite(r.x) || !finite(r.y) || !finite(r.w) || !finite(r.h)) return null;
  if (r.w <= 0 || r.h <= 0 || typeof r.bg !== 'string' || !COLOR_RE.test(r.bg)) return null;
  return { x: r.x, y: r.y, w: r.w, h: r.h, bg: r.bg };
}

function parsePose(key: string, raw: unknown, needSign: boolean): SpritePose | null {
  const r = obj(raw);
  if (!r || typeof r.src !== 'string' || !FILE_RE.test(r.src)) return null;
  const url = imageUrls.get(`${key}/${r.src}`);
  if (!url) return null;
  if (!finite(r.w) || !finite(r.h) || !finite(r.scale) || r.w < 1 || r.h < 1 || r.w > 4096 || r.h > 4096) return null;
  if (r.scale < 0.05 || r.scale > 4) return null;
  const anchor = pair(r.anchor);
  if (!anchor) return null;
  const pose: SpritePose = {
    src: r.src,
    url,
    w: r.w,
    h: r.h,
    scale: r.scale,
    anchor,
    faces: r.faces === 'left' ? 'left' : 'right',
  };
  if (needSign) {
    const sign = parseSign(r.sign);
    if (!sign) return null;
    pose.sign = sign;
  }
  const impact = pair(r.impact);
  if (impact) pose.impact = impact;
  return pose;
}

function parseQuote(raw: unknown): SpriteQuote | undefined {
  const r = obj(raw);
  if (!r || typeof r.text !== 'string') return undefined;
  const text = r.text.trim();
  if (text.length === 0 || text.length > 140) return undefined;
  const quote: SpriteQuote = { text };
  if (typeof r.lang === 'string' && LANG_RE.test(r.lang)) quote.lang = r.lang;
  return quote;
}

/** A sprite manifest with missing/invalid required poses (idle, fly) is rejected (null). */
function parseSprite(key: string, m: Record<string, unknown>): SpriteSpec | null {
  const poses = obj(m.poses);
  if (!poses) return null;
  const idle = parsePose(key, poses.idle, true);
  const fly = parsePose(key, poses.fly, false);
  if (!idle || !fly) return null;
  const low = parsePose(key, poses.low, true) ?? undefined;
  const land = parsePose(key, poses.land, false) ?? undefined;
  const th = obj(m.lowThresholds);
  const quotes = obj(m.quotes);
  const spec: SpriteSpec = {
    anchorX: num(m.anchorX, 200, 0, 320),
    poses: { idle, fly },
    entrance: m.entrance === 'crash-landing' ? 'crash-landing' : 'none',
    locomotion: m.locomotion === 'fly' ? 'fly' : 'none',
    lowThresholds: { session: num(th?.session, 90, 0, 1000), weekly: num(th?.weekly, 70, 0, 1000) },
    quotes: {},
  };
  if (low) spec.poses.low = low;
  if (land) spec.poses.land = land;
  const greeting = parseQuote(quotes?.greeting);
  const lowQuote = parseQuote(quotes?.low);
  if (greeting) spec.quotes.greeting = greeting;
  if (lowQuote) spec.quotes.low = lowQuote;
  return spec;
}

function loadPets(): PetManifest[] {
  const out: PetManifest[] = [];
  const seen = new Set<string>();
  const all = [
    ...Object.entries(publicManifests).map(([p, r]) => ['pets', p, r] as const),
    ...Object.entries(localManifests).map(([p, r]) => ['local', p, r] as const),
  ];
  for (const [origin, path, raw] of all) {
    const folder = folderOf(path);
    // The id ends up in a CSS class: restrict it to a safe alphabet.
    if (!/^[a-z0-9_-]+$/.test(folder) || typeof raw !== 'object' || raw === null) continue;
    if (seen.has(folder)) continue;
    const m = raw as Record<string, unknown>;
    const kind = m.kind === 'sprite' ? 'sprite' : 'stick';
    const pet: PetManifest = {
      id: folder,
      name: str(m.name, folder),
      author: str(m.author, ''),
      license: str(m.license, 'MIT'),
      description: str(m.description, ''),
      default: m.default === true,
      color: typeof m.color === 'string' && COLOR_RE.test(m.color) ? m.color : '#f28c28',
      stroke: num(m.stroke, 9, 3, 16),
      head: num(m.head, 17, 10, 26),
      kind,
    };
    if (kind === 'sprite') {
      const sprite = parseSprite(`${origin}:${folder}`, m);
      if (!sprite) continue; // invalid sprite skin: skipped
      pet.sprite = sprite;
    }
    seen.add(folder);
    out.push(pet);
  }
  // Same order as the backend: the default skin first, then alphabetical.
  return out.sort((a, b) =>
    a.default === b.default ? a.id.localeCompare(b.id) : a.default ? -1 : 1,
  );
}

export const pets: PetManifest[] = loadPets();

export const DEFAULT_VARIANT: string = pets[0]?.id ?? 'stick-orange';

export function getPet(id: string | null | undefined): PetManifest | undefined {
  return pets.find((p) => p.id === id) ?? pets[0];
}
