# PET-CREDITS

A desktop pet that shows your Claude Code usage limits in real time, on a sign held by a small stick figure.

PET-CREDITS is a small always-on-top overlay (Tauri 2) with a system-tray icon. It runs on Windows, macOS and Linux.

## What it shows

- **5-hour window**: how much of the current 5-hour limit you have used.
- **Week**: how much of the weekly limit you have used.
- **Per session / per agent consumption**: token counts read from your local Claude Code transcripts.
- **Pet states**: the pet idles, works while Claude Code is active, gets worried as a limit gets high (warn) and then critical, and falls asleep when the numbers it has are stale.

## Install

Download the installer for your OS from the [Releases](https://github.com/savigiulio06-cell/PET-CREDITS/releases) page. Each release is built by GitHub Actions and contains:

- Windows: `.msi` and `.exe` (NSIS)
- macOS: `.dmg` (universal)
- Linux: `.deb`, `.rpm` and `.AppImage`
- `SHA256SUMS`

Verify your download:

```sh
sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify <file> --repo savigiulio06-cell/PET-CREDITS
```

The binaries are **not code-signed** yet. Windows SmartScreen and macOS Gatekeeper will warn you the first time you run them.

## First run

The first time the pet opens, a small card asks two questions (you can change both later from the menu):

- **Show me when you open Claude** (on by default): the pet starts with your computer, waits in the tray, appears when Claude is open and hides again when it is not. See [Start with Claude](#start-with-claude).
- **Live usage from Anthropic's server** (off by default; pre-ticked if you already turned it on): numbers that also work with the Claude desktop app and IDE extensions. It uses an unofficial endpoint; read [Terms and risk](#terms-and-risk) first.

The card is shown once, also to people who already had live usage on: in that case its box is already ticked, and nothing changes until you press OK.

## Usage

- **Drag** the pet to move it around the screen.
- **Click the sign** to open the details panel.
- **Menu**: click the tray icon, or right-click the pet. Entries (the UI is in Italian):
  - Mostra/Nascondi (show/hide)
  - Aspetto (look): choose the skin
  - Aggiorna ora (refresh now)
  - Dati live dal server Anthropic (non ufficiale, usa il login di Claude Code): turn live usage on or off (off by default, see [Terms and risk](#terms-and-risk))
  - Mostra quando si apre Claude (show when Claude opens): turn "start with Claude" on or off (see below)
  - Collega a Claude Code / Scollega da Claude Code (connect / disconnect)
  - Esci (quit)

## Start with Claude

With "Show me when you open Claude" on, PET-CREDITS registers itself to start when you log in (a per-user login entry, started with `--standby` so the window stays hidden; the tray icon is still there; it is never written by development builds). A light background check, every few seconds, decides that Claude is open when:

- a process named exactly `claude` is running (the Claude desktop app, and the native Claude Code CLI), or
- a Claude Code transcript under `~/.claude/projects` was written in the last 2 minutes (this also covers the CLI installed with npm, which runs as `node`).

When Claude appears the pet shows up (sprite skins replay their entrance). When Claude has been gone the pet hides again after about 1 to 3 minutes (it waits a full minute without any sign of Claude, so a restart or a short gap does not make it flicker; with the npm CLI, which is only recognised through its transcript, the last write still counts as "open" for 2 more minutes), but only if it was shown automatically: if you opened it yourself (tray icon, menu, or by launching it normally) it stays. Only process *names* are looked at, locally; nothing is stored or sent. Turning the option off (tray menu) removes the login entry; if the system refuses to change it, the choice is put back to what it really is and the details panel says so.

## Connect to Claude Code

Use the tray menu entry, or run the executable from a terminal:

```sh
pet-credits --install     # connect
pet-credits --uninstall   # disconnect, restores your previous status line
```

`--install` adds to `~/.claude/settings.json`:

- a `SessionStart` hook (`pet-credits --ensure-running`) that starts the pet at every Claude Code session (or shows it, if it is waiting in the tray);
- a status-line wrapper (`pet-credits --statusline`) that records the usage numbers and then runs your previous status-line command.

A timestamped backup of `settings.json` is saved before every change. The operation is idempotent and reversible, and your other settings are left alone. Details and safety checks are in [SECURITY.md](SECURITY.md).

Other options: `--version`, `--help`. With no option the pet starts. `--standby` (used by the login entry) starts it hidden, and exits at once, removing the entry, if you had not chosen "show when Claude opens".

Connecting is only needed for the status-line numbers below; "Start with Claude" and live usage work without it.

## Where the numbers come from

1. **Status line (default).** Once connected (`--install`), Claude Code passes its documented usage data to the status-line wrapper, which records it for the pet. The token is not used. Numbers appear after the first answer of a session. Claude Code runs the status line only in the **terminal CLI**: never in the desktop app or in IDE extensions.
2. **Live usage (optional, off by default).** If you turn it on from the menu, PET-CREDITS also asks `https://api.anthropic.com/api/oauth/usage` for your usage, using the login Claude Code already has, so the numbers stay up to date even when Claude Code is idle. This endpoint is **unofficial and undocumented**; read [Terms and risk](#terms-and-risk) first.

Which source works where:

| Where you use Claude | Pet appears by itself | Limit numbers (5h, week) |
|---|---|---|
| Terminal CLI | yes (session hook, or "Start with Claude") | status line, or live usage |
| Claude desktop app | with "Start with Claude" | live usage only |
| IDE extension | with "Start with Claude" | live usage only |

Per-session and per-agent token counts come from the local transcripts in every case. Without limit numbers the sign suggests turning on live usage from the menu.

With live usage on, if your Claude Code login expires, the pet keeps showing the last numbers it has, marked as old, and asks you to open `claude` in a terminal and send a message, or to run `/login`. PET-CREDITS never refreshes the token itself.

## Limitations

- The Claude desktop app has a login separate from the CLI's, so using the desktop app does not renew the login the pet relies on.
- The desktop app and IDE extensions never run the status line, so their limit numbers need live usage. The pet itself appears for them through "Start with Claude" (it does not need the session hook).
- "Start with Claude" recognises the desktop app and the native CLI by the process name `claude`; if another program uses that name, the pet will think Claude is open.
- Cloud sessions run nothing locally, but they consume the same plan limits. Those are visible only with live usage on, as long as the CLI login is valid.
- The usage endpoint is undocumented and can change without notice.

## How it works

1. With live usage on, a background thread polls the usage API every 60 s while Claude Code is active and the pet is visible, and every 5 minutes otherwise (it backs off when the server answers "too many requests"). While the pet is hidden and Claude is not open, it makes no request at all.
2. Between polls it watches two local files: the status-line record and the newest Claude Code transcript. When either changes, it refreshes from disk without any network request.
3. The newest data wins: API numbers, unless the status line has something fresher. The last API numbers are also saved to disk, so after a restart the pet shows them (marked as old) until fresh data arrives.
4. The result is sent to the window, which draws the sign and picks the pet's pose. The window never sees the token.

## Security and privacy

What PET-CREDITS reads, writes and sends, in short (the full table is in [SECURITY.md](SECURITY.md)):

| | What | Notes |
|---|---|---|
| Reads | Claude Code's OAuth token (`~/.claude/.credentials.json`, macOS Keychain) | only with live usage on (off by default); in memory only, sent only to `api.anthropic.com` over HTTPS, never written, logged, shown or refreshed |
| Reads | Claude Code transcripts (`~/.claude/projects`) | only token counts, model names and the project folder name; conversation text is not used |
| Reads | names of the running processes | only with "show when Claude opens" on; to notice a process called `claude`; never stored or sent |
| Changes | the per-user login entry of your OS (Windows registry `Run` key, macOS LaunchAgent, Linux `~/.config/autostart`) | only with "show when Claude opens" on; removed when you turn it off |
| Changes | `~/.claude/settings.json` | only on `--install` / `--uninstall`, with a backup each time |
| Writes | its own folder (`%APPDATA%`, `~/.config` or `~/Library/Application Support`, then `pet-credits/`) | settings, usage percentages, session id, model, the current transcript path; no secrets |
| Sends | `https://api.anthropic.com/api/oauth/usage` | only with live usage on; the only network request; no telemetry, analytics or auto-update |

Things to be aware of:

- **Trust the binary you run.** Any program that can read your Claude Code token could misuse it. Download PET-CREDITS only from this repository's Releases, and check `SHA256SUMS` and the build attestation. The source is public and the release binaries are built by GitHub Actions from the tagged commit.
- **Live usage is opt-in.** See [Terms and risk](#terms-and-risk).
- **Your previous status line keeps running.** After `--install`, the status-line wrapper runs your previous status-line command through the shell (see [SECURITY.md](SECURITY.md) for which shell), with a time limit and a guard against calling itself in a loop.
- **Disconnect before uninstalling.** Run `pet-credits --uninstall` before removing the app, or `settings.json` keeps pointing to a missing executable.
- **Keep the default install folder** on Windows: PET-CREDITS does not check folder permissions there.

## Terms and risk

PET-CREDITS is an unofficial, community project, not affiliated with, endorsed by or sponsored by Anthropic. By default it only uses Claude Code's documented status line.

Optionally, *live usage* reads the OAuth token that Claude Code already stored on your computer, in memory only, and sends it solely to `api.anthropic.com` to call an undocumented, read-only usage endpoint (`/api/oauth/usage`). It makes no model requests, never refreshes or stores the token and never sends it elsewhere.

Anthropic's terms restrict the use of subscription credentials outside its own apps ([Claude Code: Legal and compliance](https://code.claude.com/docs/en/legal-and-compliance), [Consumer Terms](https://www.anthropic.com/legal/consumer-terms)) and may change or be enforced without notice. A read-only usage check is neither explicitly allowed nor explicitly forbidden there. **Turning live usage on is at your own risk**; you can turn it off at any time. This is not legal advice.

Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## Build from source

Prerequisites: Rust (stable), Node 22, pnpm 10, and WebView2 on Windows (on Linux, the WebKitGTK and appindicator development packages used in the release workflow).

```sh
cargo test -p pet-core
cargo clippy -p pet-core -- -D warnings

cd app
pnpm install
pnpm tauri dev      # run the app (needs a WebView)
pnpm tauri build    # build installers
```

## Project structure

```
crates/pet-core/     Pure Rust library, testable without a GUI
  src/model.rs         data model sent to the UI (limits, sessions, agents)
  src/presence.rs      "is Claude open?" decision logic (name match, debounce)
  src/claude/          Claude Code provider: usage API, status-line record,
                       transcripts, cached last numbers, settings.json install
  src/codex/           placeholder for other providers
  tests/               integration tests with invented fixtures
app/
  src-tauri/src/       desktop app: window, tray and pet menu, poller thread,
                       Claude watcher (process names), login entry,
                       command-line modes (--install, --statusline, ...)
  src/                 UI (TypeScript + Vite): sign, details panel, stick figure
pets/<id>/           skins (manifest.json)
.github/workflows/   CI (tests, lint, audits) and release builds
```

## Skins

Each skin is a folder `pets/<id>/manifest.json`. Included: `stick-blue`, `stick-green`, `stick-orange`. Manifest fields:

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | skin id (same as the folder name) |
| `name` | yes | display name |
| `author` | yes | author |
| `license` | yes | license of the skin |
| `description` | yes | short description |
| `color` | yes | line colour, `#rgb` or `#rrggbb` |
| `stroke` | yes | limb thickness in px (3 to 16) |
| `head` | no | head radius in px |
| `default` | no | `true` to make it the default skin |

### Sprite skins

A skin is a stick figure by default. With `"kind": "sprite"` it is drawn from PNG poses instead, placed next to `manifest.json`. Extra fields:

| Field | Meaning |
|---|---|
| `kind` | `"stick"` (default) or `"sprite"` |
| `anchorX` | stage x (px) where standing poses are anchored |
| `poses` | `idle` and `fly` are required; `low` and `land` are optional (`low` falls back to `idle`) |
| `entrance` | `"crash-landing"` plays the landing sequence at start |
| `locomotion` | `"fly"` shows the `fly` pose while the window is dragged |
| `lowThresholds` | `{ "session": 90, "weekly": 70 }`: used percentage that switches to the `low` pose |
| `quotes` | optional `greeting` and `low` speech bubbles: `{ "text": "...", "lang": "en" }` |

Each pose has `src` (PNG file name), `w`, `h` (PNG size), `scale` (CSS px per PNG px) and `anchor` (the point on the ground; the body centre for `fly`). `idle` and `low` also have `sign`, the rectangle of the painted board (`x`, `y`, `w`, `h`, `bg`) where the live numbers are drawn; `land` has `impact` (where the fist hits the ground); `fly` can set `faces` (`"left"` or `"right"`).

Skins in `pets-local/` are picked up the same way as the ones in `pets/`, but that folder is gitignored and is bundled only into builds made on the machine that has it. Do not distribute builds that include `pets-local`.

## License

[MIT](LICENSE).

PET-CREDITS is an independent project and is not affiliated with, endorsed by or sponsored by Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic.
