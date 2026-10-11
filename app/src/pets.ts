// Pet skins: pets/<id>/manifest.json only. The stick figure is drawn procedurally (rig.ts).
import type { PetManifest } from './types';

const manifests = import.meta.glob('../../pets/*/manifest.json', { eager: true, import: 'default' });

const COLOR_RE = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;

function folderOf(path: string): string {
  return path.split('/').slice(-2, -1)[0] ?? '';
}

function str(v: unknown, fallback: string): string {
  return typeof v === 'string' ? v : fallback;
}

function num(v: unknown, fallback: number, min: number, max: number): number {
  return typeof v === 'number' && Number.isFinite(v) ? Math.min(max, Math.max(min, v)) : fallback;
}

function loadPets(): PetManifest[] {
  const out: PetManifest[] = [];
  for (const [path, raw] of Object.entries(manifests)) {
    const folder = folderOf(path);
    // The id ends up in a CSS class: restrict it to a safe alphabet.
    if (!/^[a-z0-9_-]+$/.test(folder) || typeof raw !== 'object' || raw === null) continue;
    const m = raw as Record<string, unknown>;
    out.push({
      id: folder,
      name: str(m.name, folder),
      author: str(m.author, ''),
      license: str(m.license, 'MIT'),
      description: str(m.description, ''),
      default: m.default === true,
      color: typeof m.color === 'string' && COLOR_RE.test(m.color) ? m.color : '#f28c28',
      stroke: num(m.stroke, 9, 3, 16),
      head: num(m.head, 17, 10, 26),
    });
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
