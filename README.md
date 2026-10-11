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

## Usage

- **Drag** the pet to move it around the screen.
- **Click the sign** to open the details panel.
- **Menu**: click the tray icon, or right-click the pet. Entries (the UI is in Italian):
  - Mostra/Nascondi (show/hide)
  - Aspetto (look): choose the skin
  - Aggiorna ora (refresh now)
  - Dati live dal server Anthropic (non ufficiale, usa il login di Claude Code): turn live usage on or off (off by default, see [Terms and risk](#terms-and-risk))
  - Collega a Claude Code / Scollega da Claude Code (connect / disconnect)
  - Esci (quit)

## Connect to Claude Code

Use the tray menu entry, or run the executable from a terminal:

```sh
pet-credits --install     # connect
pet-credits --uninstall   # disconnect, restores your previous status line
```

`--install` adds to `~/.claude/settings.json`:

- a `SessionStart` hook (`pet-credits --ensure-running`) that starts the pet at every Claude Code session;
- a status-line wrapper (`pet-credits --statusline`) that records the usage numbers and then runs your previous status-line command.

A timestamped backup of `settings.json` is saved before every change. The operation is idempotent and reversible, and your other settings are left alone. Details and safety checks are in [SECURITY.md](SECURITY.md).

Other options: `--version`, `--help`. With no option the pet starts.

## Where the numbers come from

1. **Status line (default).** Once connected (`--install`), Claude Code passes its documented usage data to the status-line wrapper, which records it for the pet. The token is not used. Numbers appear after the first answer of a session.
2. **Live usage (optional, off by default).** If you turn it on from the menu, PET-CREDITS also asks `https://api.anthropic.com/api/oauth/usage` for your usage, using the login Claude Code already has, so the numbers stay up to date even when Claude Code is idle. This endpoint is **unofficial and undocumented**; read [Terms and risk](#terms-and-risk) first.

With live usage on, if your Claude Code login expires, the pet keeps showing the last numbers it has, marked as old, and asks you to open `claude` in a terminal and send a message, or to run `/login`. PET-CREDITS never refreshes the token itself.

## Limitations

- The Claude desktop app has a login separate from the CLI's, so using the desktop app does not renew the login the pet relies on.
- Sessions started from the Claude desktop app do not start the pet (tested on Windows): if you mostly use the desktop app, start PET-CREDITS yourself. Whether its status line feeds the pet there is not confirmed either.
- Cloud sessions run nothing locally, but they consume the same plan limits. Those are visible only with live usage on, as long as the CLI login is valid.
- The usage endpoint is undocumented and can change without notice.

## How it works

1. With live usage on, a background thread polls the usage API every 60 s while Claude Code is active and every 5 minutes otherwise (it backs off when the server answers "too many requests").
2. Between polls it watches two local files: the status-line record and the newest Claude Code transcript. When either changes, it refreshes from disk without any network request.
3. The newest data wins: API numbers, unless the status line has something fresher. The last API numbers are also saved to disk, so after a restart the pet shows them (marked as old) until fresh data arrives.
4. The result is sent to the window, which draws the sign and picks the pet's pose. The window never sees the token.

## Security and privacy

What PET-CREDITS reads, writes and sends, in short (the full table is in [SECURITY.md](SECURITY.md)):

| | What | Notes |
|---|---|---|
| Reads | Claude Code's OAuth token (`~/.claude/.credentials.json`, macOS Keychain) | only with live usage on (off by default); in memory only, sent only to `api.anthropic.com` over HTTPS, never written, logged, shown or refreshed |
| Reads | Claude Code transcripts (`~/.claude/projects`) | only token counts, model names and the project folder name; conversation text is not used |
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
  src/claude/          Claude Code provider: usage API, status-line record,
                       transcripts, cached last numbers, settings.json install
  src/codex/           placeholder for other providers
  tests/               integration tests with invented fixtures
app/
  src-tauri/src/       desktop app: window, tray and pet menu, poller thread,
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

## License

[MIT](LICENSE).

PET-CREDITS is an independent project and is not affiliated with, endorsed by or sponsored by Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic.
