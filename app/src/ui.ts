// DOM views for the sign and the detail panel. Dynamic data is only ever written with textContent.
import { clampPct, formatClock, formatCost, formatReset, formatTokens, levelOf, shortModel } from './format';
import { t } from './i18n';
import type { TokenTotals, UsageSnapshot, UsageWindow } from './types';

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function totalTokens(tk: TokenTotals): number {
  return tk.input + tk.output + tk.cache_creation + tk.cache_read;
}

/* ------------------------------------------------------------------ sign */

interface Row {
  root: HTMLElement;
  pct: HTMLElement;
  reset: HTMLElement;
  fill: HTMLElement;
}

function makeRow(label: string): Row {
  const root = el('div', 'row');
  const head = el('div', 'row-head');
  const pct = el('span', 'pct', t.noValue);
  const reset = el('span', 'reset');
  head.append(el('span', 'lbl', label), pct, reset);
  const bar = el('div', 'bar');
  const fill = el('div', 'fill');
  bar.append(fill);
  root.append(head, bar);
  return { root, pct, reset, fill };
}

function setRow(row: Row, win: UsageWindow | undefined, now: number): void {
  if (!win) {
    row.pct.textContent = t.noValue;
    row.reset.textContent = '';
    row.fill.style.width = '0%';
    row.root.dataset.level = 'none';
    return;
  }
  const level = levelOf(win.used_pct);
  row.pct.textContent = `${Math.round(win.used_pct)}%`;
  const reset = formatReset(win.resets_at, now);
  row.reset.textContent = reset ? `${t.reset} ${reset}` : '';
  row.fill.style.width = `${clampPct(win.used_pct)}%`;
  row.root.dataset.level = level;
}

/* compact variant: two rows painted on a sprite skin's board (hourglass 5h, calendar week) */
const SVG_NS = 'http://www.w3.org/2000/svg';

function icon(paths: string[]): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('viewBox', '0 0 16 16');
  svg.setAttribute('class', 'c-icon');
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '1.7');
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  for (const d of paths) {
    const p = document.createElementNS(SVG_NS, 'path');
    p.setAttribute('d', d);
    svg.append(p);
  }
  return svg;
}

const HOURGLASS = ['M3.5 1.5h9M3.5 14.5h9', 'M4.5 1.5c0 4 3.5 4.5 3.5 6.5S4.5 10.5 4.5 14.5', 'M11.5 1.5c0 4-3.5 4.5-3.5 6.5s3.5 2.5 3.5 6.5'];
const CALENDAR = ['M2.5 3.5h11a1 1 0 0 1 1 1v8.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1z', 'M1.5 7h13', 'M5 1.5v3M11 1.5v3'];

function makeCompactRow(paths: string[], label: string): { root: HTMLElement; pct: HTMLElement } {
  const root = el('div', 'c-row');
  const pct = el('span', 'c-pct', t.noValue);
  root.append(icon(paths), el('span', 'c-lbl', label), pct);
  return { root, pct };
}

export interface SignView {
  root: HTMLElement;
  /** `live`: the live-usage option is on (decides whether the "turn it on" hint is useful). */
  update(snapshot: UsageSnapshot | null, now: number, live: boolean): void;
}

export function createSign(): SignView {
  const root = el('div', 'sign');
  root.setAttribute('role', 'button');
  root.tabIndex = 0;
  root.title = t.signHint;

  const badge = el('div', 'badge', t.stale);
  const issue = el('div', 'issue');
  const rows = el('div', 'rows');
  const session = makeRow(t.fiveHour);
  const weekly = makeRow(t.weekly);
  const scoped = el('div', 'scoped');
  const chat = el('div', 'chat');
  const chatTokens = el('span', 'chat-tokens');
  const chatCost = el('span', 'chat-cost');
  chat.append(el('span', 'lbl', t.chat), chatTokens, chatCost);
  rows.append(session.root, weekly.root, scoped, chat);

  const empty = el('div', 'empty');
  const emptyTitle = el('div', 'empty-title', t.waiting);
  const emptyHint = el('div', 'empty-hint', t.waitingHint);
  empty.append(emptyTitle, emptyHint);

  const compact = el('div', 'compact-rows');
  const cSession = makeCompactRow(HOURGLASS, t.fiveHour);
  const cWeekly = makeCompactRow(CALENDAR, t.weekly);
  compact.append(cSession.root, cWeekly.root);

  root.append(badge, issue, rows, empty, compact);

  return {
    root,
    update(snapshot, now, live) {
      const hasData = snapshot !== null && (snapshot.windows.length > 0 || snapshot.session !== null);
      // No limit windows and no live usage: the status line never ran (desktop app, IDE),
      // so say how to get numbers instead of "waiting".
      const needsLive = !live && (snapshot === null || snapshot.windows.length === 0);
      emptyTitle.textContent = needsLive ? t.liveHint : t.waiting;
      emptyHint.textContent = needsLive ? '' : t.waitingHint;
      emptyHint.hidden = needsLive;
      root.classList.toggle('empty-state', !hasData || needsLive);
      const loginIssue = snapshot?.issue === 'auth_expired' || snapshot?.issue === 'login_missing' ? snapshot.issue : null;
      // compact rows: "—" when there is no data or it is stale
      const fresh = hasData && snapshot !== null && !snapshot.stale && loginIssue === null;
      const pctOf = (kind: string): string => {
        const w = fresh ? snapshot?.windows.find((x) => x.kind === kind) : undefined;
        return w ? `${Math.round(w.used_pct)}%` : t.noValue;
      };
      cSession.pct.textContent = pctOf('session');
      cWeekly.pct.textContent = pctOf('weekly');
      issue.textContent = loginIssue ? t.issues[loginIssue] : '';
      root.classList.toggle('has-issue', loginIssue !== null);
      root.classList.toggle('stale', hasData && snapshot !== null && (snapshot.stale || loginIssue !== null));
      if (!hasData || snapshot === null || needsLive) return;

      setRow(session, snapshot.windows.find((w) => w.kind === 'session'), now);
      setRow(weekly, snapshot.windows.find((w) => w.kind === 'weekly'), now);

      const extras = snapshot.windows.filter((w) => w.kind === 'weekly_scoped' || w.kind === 'other').slice(0, 2);
      scoped.replaceChildren(
        ...extras.map((w) => {
          const item = el('div', 'scoped-item');
          item.dataset.level = levelOf(w.used_pct);
          const name = w.label.replace(/^weekly\s*[·:\-–]\s*/i, '');
          const reset = formatReset(w.resets_at, now);
          item.append(el('span', 'lbl', name), el('span', 'pct', `${Math.round(w.used_pct)}%`), el('span', 'reset', reset));
          return item;
        }),
      );
      scoped.hidden = extras.length === 0;

      const s = snapshot.session;
      chatTokens.textContent = s ? `${formatTokens(totalTokens(s.tokens))} ${t.tokensUnit}` : t.noValue;
      chatCost.textContent = s ? formatCost(s.cost_usd_est) : '';
    },
  };
}

/* ----------------------------------------------------------------- panel */

export interface PanelView {
  root: HTMLElement;
  update(snapshot: UsageSnapshot | null, live: boolean, loginError: boolean): void;
}

export function createPanel(): PanelView {
  const root = el('div', 'panel');
  const head = el('div', 'panel-head');
  const source = el('span', 'panel-source');
  const updated = el('span', 'panel-updated');
  head.append(source, updated);
  const summary = el('div', 'panel-summary');
  const title = el('div', 'panel-title', t.agents);
  const list = el('div', 'agents');
  const note = el('div', 'panel-note');
  const hint = el('div', 'panel-hint', t.estimateHint);
  root.append(head, summary, title, list, note, hint);

  return {
    root,
    update(snapshot, live, loginError) {
      if (!snapshot) {
        source.textContent = `${t.source}: ${t.sources.none}`;
        updated.textContent = '';
        list.replaceChildren(el('div', 'agent-empty', t.noAgents));
        note.textContent = '';
        note.hidden = true;
        summary.textContent = '';
        return;
      }
      source.textContent = `${t.source}: ${t.sources[snapshot.source] ?? t.sources.none}`;
      updated.textContent = `${t.updatedAt} ${formatClock(snapshot.fetched_at)}`;

      const s = snapshot.session;
      summary.textContent = s
        ? [t.sessionLabel + (s.project ? ` ${s.project}` : ''), `${formatTokens(totalTokens(s.tokens))} ${t.tokensUnit}`, formatCost(s.cost_usd_est)].join(' · ')
        : '';
      summary.hidden = !s;

      const agents = s?.agents ?? [];
      if (agents.length === 0) {
        list.replaceChildren(el('div', 'agent-empty', t.noAgents));
      } else {
        list.replaceChildren(
          ...agents.map((a) => {
            const row = el('div', 'agent');
            row.append(
              el('span', 'a-name', a.agent),
              el('span', 'a-model', shortModel(a.model)),
              el('span', 'a-tok', formatTokens(totalTokens(a.tokens))),
              el('span', 'a-cost', formatCost(a.cost_usd_est)),
            );
            return row;
          }),
        );
      }
      const needsLive = !live && snapshot.windows.length === 0;
      const detail = [snapshot.issue ? t.issues[snapshot.issue] : '', needsLive ? t.liveHint : '', loginError ? t.loginError : '', snapshot.note ?? '']
        .filter(Boolean)
        .join(' · ');
      note.textContent = detail;
      note.hidden = !detail;
    },
  };
}

/* ------------------------------------------------------------ first-run card */

export interface OnboardingView {
  root: HTMLElement;
  /** Show the card (Show me: on; Live usage: pre-ticked with `live`, the current setting) and focus the first box. */
  open(live: boolean): void;
  close(): void;
}

function checkRow(id: string, text: string, checked: boolean): { row: HTMLLabelElement; box: HTMLInputElement } {
  const row = el('label', 'onb-row');
  const box = el('input');
  box.type = 'checkbox';
  box.id = id;
  box.checked = checked;
  row.append(box, el('span', 'onb-text', text));
  return { row, box };
}

/** `onDone` gets the two answers when the user presses OK. Text is only ever set with textContent. */
export function createOnboarding(onDone: (showWithClaude: boolean, liveUsage: boolean) => void): OnboardingView {
  const root = el('div', 'onboarding');
  root.hidden = true;
  root.setAttribute('role', 'dialog');
  root.setAttribute('aria-modal', 'true');
  root.setAttribute('aria-labelledby', 'onb-title');

  const title = el('div', 'onb-title', t.onboarding.title);
  title.id = 'onb-title';
  const show = checkRow('onb-show', t.onboarding.show, true);
  const live = checkRow('onb-live', t.onboarding.live, false);
  const note = el('div', 'onb-note', t.onboarding.liveNote);
  const ok = el('button', 'onb-ok', t.onboarding.ok);
  ok.type = 'button';
  ok.addEventListener('click', () => onDone(show.box.checked, live.box.checked));
  root.append(title, show.row, live.row, note, ok);

  // Keep Tab inside the card (the pet behind it is inert).
  root.addEventListener('keydown', (e) => {
    if (e.key !== 'Tab') return;
    const items = [show.box, live.box, ok];
    const i = items.indexOf(document.activeElement as HTMLInputElement | HTMLButtonElement);
    const next = e.shiftKey ? (i <= 0 ? items.length - 1 : i - 1) : i === items.length - 1 ? 0 : i + 1;
    e.preventDefault();
    items[next]?.focus();
  });

  return {
    root,
    open(liveNow) {
      if (!root.hidden) return; // already open: keep what the user has ticked
      show.box.checked = true;
      live.box.checked = liveNow;
      root.hidden = false;
      show.box.focus();
    },
    close() {
      root.hidden = true;
    },
  };
}
