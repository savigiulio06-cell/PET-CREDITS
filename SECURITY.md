# Security

PET-CREDITS runs on your machine and reads data that Claude Code already stores locally.
Security is the project's first rule: if anything below is not true of the code, that is a bug.

## What it touches
| What | Why | How |
|---|---|---|
| `~/.claude/.credentials.json` (macOS: Keychain item `Claude Code-credentials`, read with `/usr/bin/security`) | read the OAuth access token to ask Anthropic for your usage % (same data as Claude's settings page) | read-only, kept in memory only, sent **only** to `https://api.anthropic.com` (no redirects followed), never written, logged or refreshed. On macOS the Keychain asks for permission the first time; "Always Allow" lets PET-CREDITS read that item without asking again. |
| `~/.claude/projects/**/*.jsonl` | per-session / per-agent token counts | read-only |
| `~/.claude/settings.json` | add a `SessionStart` hook and a status-line wrapper | only when you run `pet-credits --install` (or the tray item); see below |
| `<config dir>/pet-credits/` | `config.json` (pet settings), `install-state.json` (your previous status-line command), `statusline-bridge.json` (usage %, session id, model, session cost and the path of the current transcript), `last-usage.json` (last usage percentages and reset times, so they survive a restart; no credentials) | files created with owner-only permissions (`0600` on Unix) |

`<config dir>` is `%APPDATA%` on Windows, `~/.config` on Linux, `~/Library/Application Support` on macOS.

### `--install` / `--uninstall`
- `--install` writes into `settings.json` a `statusLine` and a `SessionStart` hook pointing to the
  PET-CREDITS executable. It refuses to do so if the executable is in a temporary, removable or
  other-user-writable location, or if its path contains characters a shell would interpret.
- Before every change a backup `settings.json.pet-credits-backup-<timestamp>` is saved next to it.
  Backups are never deleted automatically: remove them yourself when you no longer need them.
- Writes are atomic, keep the file's permissions and key order, and follow symlinks.
- `--uninstall` removes only PET-CREDITS's entries and restores your previous status line.
- After install, on every status-line refresh `pet-credits --statusline` records the usage numbers
  and then **runs your previous status-line command through the shell**, exactly as Claude Code did
  before, passing it the same input.

## Network
Only `https://api.anthropic.com/api/oauth/usage`, every 30 s while Claude Code is active and every
5 minutes otherwise. Standard `HTTPS_PROXY` settings are honoured (the proxy cannot read the
token: TLS is end-to-end). No telemetry, no analytics, no auto-update, no other hosts.

The usage endpoint is **not officially documented** by Anthropic and may change or stop working.
It is on by default; you can turn it off from the tray menu and rely on the status-line data only.

## Downloads
Release binaries are built by GitHub Actions from the tagged source, without caches, and published
with a `SHA256SUMS` file and GitHub build-provenance attestations. Verify a download with:

```sh
gh attestation verify <file> --repo savigiulio06-cell/PET-CREDITS
```

Binaries are **not code-signed** yet: Windows SmartScreen and macOS Gatekeeper will warn you.

## Reporting a vulnerability
Please open a [private security advisory](https://github.com/savigiulio06-cell/PET-CREDITS/security/advisories/new)
instead of a public issue.
