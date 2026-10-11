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

export interface SignView {
  root: HTMLElement;
  update(snapshot: UsageSnapshot | null, now: number): void;
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
  empty.append(el('div', 'empty-title', t.waiting), el('div', 'empty-hint', t.waitingHint));

  root.append(badge, issue, rows, empty);

  return {
    root,
    update(snapshot, now) {
      const hasData = snapshot !== null && (snapshot.windows.length > 0 || snapshot.session !== null);
      root.classList.toggle('empty-state', !hasData);
      const loginIssue = snapshot?.issue === 'auth_expired' || snapshot?.issue === 'login_missing' ? snapshot.issue : null;
      issue.textContent = loginIssue ? t.issues[loginIssue] : '';
      root.classList.toggle('has-issue', loginIssue !== null);
      root.classList.toggle('stale', hasData && snapshot !== null && (snapshot.stale || loginIssue !== null));
      if (!hasData || snapshot === null) return;

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
  update(snapshot: UsageSnapshot | null): void;
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
    update(snapshot) {
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
      const detail = [snapshot.issue ? t.issues[snapshot.issue] : '', snapshot.note ?? ''].filter(Boolean).join(' · ');
      note.textContent = detail;
      note.hidden = !detail;
    },
  };
}
