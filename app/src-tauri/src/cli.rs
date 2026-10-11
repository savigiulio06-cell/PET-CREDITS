//! Command-line modes. They run before Tauri starts, never open a window and
//! must stay fast: `--statusline` is executed by Claude Code on every update and
//! `--ensure-running` blocks Claude Code's SessionStart hook until it returns.

use std::io::{Read, Write};
use std::path::PathBuf;

use pet_core::claude::{install, statusline};
use pet_core::{paths, Error};

const HELP: &str = "\
PET-CREDITS - pet da desktop che mostra i limiti di utilizzo di Claude Code

Uso: pet-credits [OPZIONE]

  (nessuna opzione)   avvia il pet
  --install           collega PET-CREDITS a Claude Code (modifica settings.json, con backup)
  --uninstall         scollega PET-CREDITS e ripristina la status line precedente
  --statusline        usato da Claude Code: legge il JSON da stdin e stampa la status line
  --ensure-running    usato da Claude Code (hook SessionStart): avvia il pet se non c'e'
  --version           mostra la versione
  --help              mostra questo aiuto
";

/// Handle a CLI mode if `args` (without the program name) asks for one.
/// Returns the process exit code, or `None` to start the app normally.
pub fn dispatch(args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str)? {
        "--statusline" => Some(statusline_mode()),
        "--ensure-running" => Some(ensure_running_mode()),
        "--install" => Some(with_console(|| install_mode(true))),
        "--uninstall" => Some(with_console(|| install_mode(false))),
        "--help" | "-h" => Some(with_console(|| {
            print!("{HELP}");
            0
        })),
        "--version" | "-V" => Some(with_console(|| {
            println!("pet-credits {}", env!("CARGO_PKG_VERSION"));
            0
        })),
        // Anything else (e.g. arguments added by the OS) just starts the app.
        _ => None,
    }
}

/* ------------------------------------------------------------ --statusline */

fn statusline_mode() -> i32 {
    // Claude Code shows whatever we print: a panic message must never leak into it.
    std::panic::set_hook(Box::new(|_| {}));
    let line = std::panic::catch_unwind(|| {
        let mut raw = Vec::new();
        if std::io::stdin().read_to_end(&mut raw).is_err() && raw.is_empty() {
            return String::new();
        }
        let input = String::from_utf8_lossy(&raw);
        statusline::run(&input, &paths::bridge_file(), &paths::app_home())
    })
    .unwrap_or_default();
    // One trailing newline, whatever the chained command printed. Write errors
    // (closed pipe) are ignored on purpose.
    let _ = writeln!(std::io::stdout(), "{}", line.trim_end_matches(['\r', '\n']));
    0
}

/* -------------------------------------------------------- --ensure-running */

/// Relaunch ourselves with no arguments as a detached process and return at
/// once. Prints nothing: a SessionStart hook's stdout goes into Claude's context.
/// If an instance is already running, the new process hands over to it (the
/// single-instance plugin) and exits.
fn ensure_running_mode() -> i32 {
    let _ = spawn_detached();
    0
}

fn spawn_detached() -> std::io::Result<()> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        // Windows passes every inheritable handle to the child, not only the
        // three redirected to NUL: the pet would keep Claude Code's hook pipes
        // open and the hook would hang until the pet exits. Our own standard
        // handles are not needed by the child, so stop them being inherited.
        // SAFETY: plain Win32 calls on this process's own standard handles;
        // failures (e.g. no handle) are harmless and ignored.
        unsafe {
            for std_id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                let h = GetStdHandle(std_id);
                if !h.is_null() {
                    SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
                }
            }
        }
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    // The child is intentionally not waited for.
    cmd.spawn().map(drop)
}

/* ------------------------------------------------- --install / --uninstall */

fn install_mode(do_install: bool) -> i32 {
    let result = if do_install {
        install_here()
    } else {
        uninstall_here()
    };
    match result {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(msg) => {
            eprintln!("Errore: {msg}");
            1
        }
    }
}

/// Our own executable, canonical, without the Windows `\\?\` prefix (the
/// command line written into settings.json must work in cmd.exe/PowerShell).
fn current_exe_clean() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|_| "impossibile determinare il percorso dell'eseguibile".to_string())?;
    Ok(strip_verbatim(exe))
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

/// Short, user-safe description of an error: never file contents.
fn describe(e: &Error) -> String {
    match e {
        Error::Io(io) => format!("errore di lettura/scrittura ({:?})", io.kind()),
        Error::Json(_) => {
            "un file JSON (settings.json o stato di PET-CREDITS) non e' valido".into()
        }
        Error::Format(m) => format!("formato inatteso: {m}"),
        Error::Unavailable(m) => format!("installazione rifiutata per sicurezza: {m}"),
        _ => "operazione non riuscita".into(),
    }
}

/// Shared by the CLI and the tray menu. Returns a one-line Italian summary.
pub fn install_here() -> Result<String, String> {
    let exe = install::install_target(&current_exe_clean()?);
    let report = install::install(&paths::claude_home(), &paths::app_home(), &exe)
        .map_err(|e| describe(&e))?;
    Ok(summary("Collegato a Claude Code", &report))
}

pub fn uninstall_here() -> Result<String, String> {
    let report =
        install::uninstall(&paths::claude_home(), &paths::app_home()).map_err(|e| describe(&e))?;
    Ok(summary("Scollegato da Claude Code", &report))
}

fn summary(done: &str, r: &install::InstallReport) -> String {
    if !r.changed {
        return format!(
            "{done}: nessuna modifica necessaria ({})",
            r.settings_path.display()
        );
    }
    let mut s = format!("{done}: aggiornato {}", r.settings_path.display());
    if let Some(b) = &r.backup_path {
        s.push_str(&format!("\nBackup: {}", b.display()));
    }
    s
}

/* ----------------------------------------------------------------- console */

/// On Windows the release build is a GUI-subsystem program, so it has no
/// console: attach to the parent's so `--install`/`--help` can print.
#[cfg(windows)]
fn with_console(f: impl FnOnce() -> i32) -> i32 {
    attach_parent_console();
    f()
}

#[cfg(not(windows))]
fn with_console(f: impl FnOnce() -> i32) -> i32 {
    f()
}

#[cfg(windows)]
fn attach_parent_console() {
    use std::fs::OpenOptions;
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    // SAFETY: plain Win32 calls; handles opened here are intentionally leaked
    // (they live as long as the process).
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return; // no parent console (e.g. launched from Explorer)
        }
        // A GUI-subsystem process started from a console has no standard
        // handles unless the shell redirected them; point the missing ones at
        // the console. Redirected output (`> file`, `| more`) is left alone.
        for std_id in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let h = GetStdHandle(std_id);
            if h.is_null() || h == INVALID_HANDLE_VALUE {
                if let Ok(con) = OpenOptions::new().write(true).open("CONOUT$") {
                    SetStdHandle(std_id, con.into_raw_handle());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_prefix_is_removed_but_unc_is_kept() {
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\C:\Apps\pet-credits.exe")),
            PathBuf::from(r"C:\Apps\pet-credits.exe")
        );
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\UNC\srv\share\pet-credits.exe")),
            PathBuf::from(r"\\?\UNC\srv\share\pet-credits.exe")
        );
        assert_eq!(
            strip_verbatim(PathBuf::from("/usr/bin/pet-credits")),
            PathBuf::from("/usr/bin/pet-credits")
        );
    }

    #[test]
    fn unknown_args_start_the_app() {
        assert_eq!(dispatch(&[]), None);
        assert_eq!(dispatch(&["-psn_0_12345".to_string()]), None);
    }
}
