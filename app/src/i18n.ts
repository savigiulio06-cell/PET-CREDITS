// All user-visible strings live here. Add a language by adding an object
// with the same shape and registering it in `languages`.

const it = {
  locale: 'it-IT',
  fiveHour: '5h',
  weekly: 'Sett.',
  chat: 'Chat',
  tokensUnit: 'tok',
  reset: 'reset',
  resetNow: 'in corso',
  noValue: '—',
  stale: 'dati non aggiornati',
  waiting: 'in attesa di Claude Code…',
  waitingHint: 'avvia una sessione',
  details: 'Dettagli',
  source: 'Fonte',
  updatedAt: 'aggiornato alle',
  sessionLabel: 'Sessione',
  agents: 'Agenti',
  noAgents: 'nessun agente attivo',
  requests: 'rich.',
  estimateHint: 'costi stimati a prezzi API',
  signHint: 'Clic per i dettagli',
  issues: {
    auth_expired: 'Accesso scaduto: apri claude nel terminale e scrivi un messaggio',
    login_missing: 'Nessun accesso: apri claude nel terminale ed esegui il login',
    rate_limited: 'Troppe richieste: riprovo tra poco',
    api_unavailable: 'Servizio non raggiungibile',
  },
  sources: {
    api: 'API',
    statusline: 'statusline',
    local_logs: 'log locali',
    none: 'nessuna',
  },
};

export type Strings = typeof it;

const en: Strings = {
  locale: 'en-US',
  fiveHour: '5h',
  weekly: 'Week',
  chat: 'Chat',
  tokensUnit: 'tok',
  reset: 'reset',
  resetNow: 'now',
  noValue: '—',
  stale: 'data out of date',
  waiting: 'waiting for Claude Code…',
  waitingHint: 'start a session',
  details: 'Details',
  source: 'Source',
  updatedAt: 'updated at',
  sessionLabel: 'Session',
  agents: 'Agents',
  noAgents: 'no active agents',
  requests: 'req.',
  estimateHint: 'costs estimated at API prices',
  signHint: 'Click for details',
  issues: {
    auth_expired: 'Login expired: open claude in the terminal and send a message',
    login_missing: 'Not logged in: open claude in the terminal and log in',
    rate_limited: 'Too many requests: retrying shortly',
    api_unavailable: 'Service unreachable',
  },
  sources: {
    api: 'API',
    statusline: 'statusline',
    local_logs: 'local logs',
    none: 'none',
  },
};

const languages: Record<string, Strings> = { it, en };

function pickLanguage(): string {
  try {
    const q = new URLSearchParams(window.location.search).get('lang');
    if (q && q in languages) return q;
    const stored = window.localStorage.getItem('pet-credits.lang');
    if (stored && stored in languages) return stored;
  } catch {
    // storage unavailable: fall through to the default
  }
  return 'it';
}

export const t: Strings = languages[pickLanguage()] ?? it;
