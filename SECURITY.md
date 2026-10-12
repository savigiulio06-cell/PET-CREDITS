# Security

PET-CREDITS runs on your machine and reads data that Claude Code already stores locally.
Security is the project's first rule: if anything below is not true of the code, that is a bug.

## What it touches
| What | Why | How |
|---|---|---|
| `~/.claude/.credentials.json` (macOS: Keychain item `Claude Code-credentials`, read with `/usr/bin/security`) | read the OAuth access token to ask Anthropic for your usage % (same data as Claude's settings page) | read-only, kept in memory only, sent **only** to `https://api.anthropic.com` (no redirects followed), never written, logged or refreshed. On macOS the Keychain asks for permission the first time; the request comes from the system tool `/usr/bin/security`, so "Always Allow" lets *that tool*, and therefore any program you run that calls it, read the item without asking again: choose "Allow" if unsure. Only read when you turn on live usage (off by default). |
| `~/.claude/projects/**/*.jsonl` | per-session / per-agent token counts | read-only |
| `~/.claude/settings.json` | add a `SessionStart` hook and a status-line wrapper | only when you run `pet-credits --install` (or the tray item); see below |
| names of the running processes | notice the Claude desktop app / CLI (a process named exactly `claude`) so the pet can appear and hide by itself | read locally every few seconds, only while "Mostra quando si apre Claude" is on; only the name is looked at (no command lines, paths or users); compared in memory and discarded; never stored, logged or sent. Done with the `sysinfo` crate: no `tasklist` or `ps` is run. |
| login entry of your OS | start the pet at login in standby (`--standby`, window hidden) | written by the `auto-launch` crate only when you tick "Mostra quando si apre Claude" (first-run card or tray menu) and removed when you untick it. Windows: the value `PET-CREDITS` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (current user only, never the machine-wide key), with the executable path in double quotes; macOS: a plist in `~/Library/LaunchAgents`; Linux: a `.desktop` file in `~/.config/autostart`. The entry only runs the PET-CREDITS executable with `--standby`. Development builds never write it. To remove it: untick "Mostra quando si apre Claude" in the menu, or delete that registry value / plist / `.desktop` file by hand. (Removing it on Windows also tries to delete a machine-wide value of the same name, which only works if one exists and you have the rights.) |
| `<config dir>/pet-credits/` | `config.json` (pet settings, whether the first-run card was answered, the two choices above), `install-state.json` (your previous status-line command), `statusline-bridge.json` (usage %, session id, model, session cost and the path of the current transcript), `last-usage.json` (last usage percentages and reset times, so they survive a restart; no credentials) | files created with owner-only permissions (`0600` on Unix) |

`<config dir>` is `%APPDATA%` on Windows, `~/.config` on Linux, `~/Library/Application Support` on macOS.

### `--install` / `--uninstall`
- `--install` writes into `settings.json` a `statusLine` and a `SessionStart` hook pointing to the
  PET-CREDITS executable. It refuses to do so if the executable is in a temporary location
  (system temp folder, `/tmp`, `/var/tmp`, `/dev/shm`, `/Volumes`, AppImage mounts, macOS app
  translocation), if its path contains characters a shell would interpret, or, on macOS/Linux, if
  the file or a folder above it is writable by group or others. On Windows folder permissions
  are **not** checked: keep PET-CREDITS in its default per-user install folder
  (`%LOCALAPPDATA%\PET-CREDITS`), not in a folder other users can write to.
- Before every change a backup `settings.json.pet-credits-backup-<timestamp>` is saved next to it.
  Backups are never deleted automatically: remove them yourself when you no longer need them.
- Writes are atomic, keep the file's permissions and key order, and follow symlinks.
- `--uninstall` removes only PET-CREDITS's entries and restores your previous status line.
- After install, on every status-line refresh `pet-credits --statusline` records the usage numbers
  and then **runs your previous status-line command through the shell** (`sh` on macOS/Linux; on
  Windows Git Bash if `CLAUDE_CODE_GIT_BASH_PATH` is set, otherwise `cmd`), passing it the same input,
  with a 5-second limit. The command is the one you had configured yourself; it is marked with
  `PET_CREDITS_CHAINED=1` so that, if it calls PET-CREDITS again, nothing is chained twice.
- Run `pet-credits --uninstall` (or "Scollega da Claude Code") **before** uninstalling the app,
  otherwise `settings.json` keeps pointing to a missing executable.

## Dependencies
Besides Tauri, the app uses `tauri-plugin-single-instance`, `auto-launch` (the login entry
above; called from Rust only, no JavaScript permission involved) and `sysinfo`
(default features off, only process names are read). Lockfiles are committed and `cargo audit` /
`npm audit` run in CI. The page's Tauri permissions are still only: receive events and drag the window.
Its own commands (`get_prefs`, `complete_onboarding`, ...) take plain booleans and only change the
choices above.

## Network
No new network access: process names and the login entry stay on your machine.
Only `https://api.anthropic.com/api/oauth/usage`, and only if you turn on live usage: every 60 s
while Claude Code is active and the pet is visible, every 5 minutes otherwise, and not at all while the
pet is hidden and Claude is not open. Standard `HTTPS_PROXY` settings are honoured (the proxy cannot read the
token: TLS is end-to-end). No telemetry, no analytics, no auto-update, no other hosts.

The usage endpoint is **not officially documented** by Anthropic and may change or stop working.
It is **off by default**: turn it on from the menu ("Dati live dal server Anthropic") if you accept
the risk described in the README, section "Terms and risk".

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
