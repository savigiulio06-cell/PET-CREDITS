import './style.css';
import { invoke } from '@tauri-apps/api/core';
import { inTauri, setExpanded, startData } from './data';
import { MotionTracker } from './motion';
import { DEFAULT_VARIANT, getPet } from './pets';
import { Rig } from './rig';
import { SIGN } from './layout';
import { createPanel, createSign } from './ui';
import { levelOf, parseTime } from './format';
import type { UsageSnapshot } from './types';

const TICK_MS = 30_000;
const ACTIVE_WINDOW_MS = 2 * 60_000;
const STORAGE_KEY = 'pet-credits.variant';

/* ------------------------------------------------------------- state */
let snapshot: UsageSnapshot | null = null;
let activity = false;
let expanded = false;

function readStored(): string | null {
  try {
    return window.localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

function persistVariant(id: string): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, id);
  } catch {
    // storage may be unavailable (private mode, blocked): the choice just won't persist
  }
}

/* --------------------------------------------------------------- DOM */
const app = document.getElementById('app');
if (!app) throw new Error('#app missing');

const stage = document.createElement('div');
stage.className = 'stage';
const sign = createSign();
sign.root.style.left = `${SIGN.x}px`;
sign.root.style.top = `${SIGN.y}px`;
sign.root.style.width = `${SIGN.w}px`;
sign.root.style.height = `${SIGN.h}px`;
const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
svg.setAttribute('class', 'rig');
svg.setAttribute('aria-hidden', 'true');
const panel = createPanel();
stage.append(sign.root, svg);
app.append(stage, panel.root);

const motion = new MotionTracker();
const rig = new Rig(svg, sign.root, motion);

const reducedQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
rig.setReduced(reducedQuery.matches);
reducedQuery.addEventListener('change', () => rig.setReduced(reducedQuery.matches));

if (inTauri()) {
  // the whole window is a drag handle (the sign, being interactive, is not)
  for (const n of [document.documentElement, document.body, app, stage, svg]) n.setAttribute('data-tauri-drag-region', '');
}

/* ------------------------------------------------------------ render */
function applyVariant(id: string, persist: boolean): void {
  const pet = getPet(id);
  if (!pet) return;
  for (const p of Array.from(app!.classList)) if (p.startsWith('v-')) app!.classList.remove(p);
  app!.classList.add(`v-${pet.id}`);
  app!.style.setProperty('--accent', pet.color);
  rig.setStyle({ color: pet.color, stroke: pet.stroke, head: pet.head ?? 17 });
  if (persist) persistVariant(pet.id);
}

function isWorking(now: number): boolean {
  if (activity) return true;
  const last = parseTime(snapshot?.session?.last_activity);
  return last !== null && now - last >= 0 && now - last < ACTIVE_WINDOW_MS;
}

function petState(now: number): string[] {
  const hasData = snapshot !== null && (snapshot.windows.length > 0 || snapshot.session !== null);
  if (hasData && snapshot?.stale) return ['sleeping'];
  const worst = Math.max(0, ...(snapshot?.windows ?? []).map((w) => w.used_pct));
  const level = hasData ? levelOf(worst) : 'ok';
  const classes: string[] = [];
  if (level === 'crit') classes.push('critical');
  else if (level === 'hi') classes.push('warn');
  if (isWorking(now)) classes.push('working');
  if (classes.length === 0) classes.push('idle');
  return classes;
}

const STATE_CLASSES = ['idle', 'working', 'warn', 'critical', 'sleeping'];

function render(): void {
  const now = Date.now();
  sign.update(snapshot, now);
  panel.update(snapshot);
  const classes = petState(now);
  const root = app!;
  root.classList.remove(...STATE_CLASSES);
  root.classList.add(...classes);
  root.classList.toggle('expanded', expanded);
  rig.setFlags({
    sleeping: classes.includes('sleeping'),
    warn: classes.includes('warn'),
    critical: classes.includes('critical'),
    working: classes.includes('working'),
  });
}

function toggleExpanded(): void {
  expanded = !expanded;
  render();
  void setExpanded(expanded);
}

sign.root.addEventListener('click', toggleExpanded);
sign.root.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault();
    toggleExpanded();
  }
});

// Right click anywhere on the pet: the native menu (same as the tray) instead of
// the WebView's own context menu.
window.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  if (inTauri()) void invoke('show_menu').catch(() => undefined);
});

/* -------------------------------------------------------------- boot */
const query = new URLSearchParams(window.location.search);
applyVariant(query.get('variant') ?? readStored() ?? DEFAULT_VARIANT, false);

void startData({
  onSnapshot(s) {
    snapshot = s;
    render();
  },
  onVariant(id) {
    applyVariant(id, true);
    render();
  },
  onActivity(active) {
    activity = active;
    render();
  },
  onMoved(x, y) {
    motion.push(x, y);
  },
}).then(() => {
  if (!inTauri() && query.get('expanded') === '1') toggleExpanded();
  render();
});

render();
window.setInterval(render, TICK_MS);
