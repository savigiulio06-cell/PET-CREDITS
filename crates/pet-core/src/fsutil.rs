//! Safe file writes shared by every module that persists something.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Atomically replace `path` with `contents`.
///
/// - A symlinked `path` (e.g. dotfiles managed settings.json) is resolved and
///   the *target* is replaced, so the link survives.
/// - The temp file is created with `create_new` and a unique name in the same
///   directory, so concurrent writers never clobber each other's temp file.
/// - On Unix the new file keeps the permissions of the file it replaces, or
///   gets `0600` if there was none: files that may hold secrets never become
///   group/world readable because of us.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let target = resolve_symlink(path);
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let tmp = dir.join(format!(
        ".{name}.pet-credits-tmp-{}-{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| -> std::io::Result<()> {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let mode = fs::metadata(&target)
                .map(|m| m.permissions().mode() & 0o777)
                .unwrap_or(0o600);
            opts.mode(mode);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
        drop(f);
        #[cfg(windows)]
        {
            // Keep a read-only attribute etc. of the original file.
            if let Ok(m) = fs::metadata(&target) {
                let _ = fs::set_permissions(&tmp, m.permissions());
            }
        }
        fs::rename(&tmp, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn resolve_symlink(path: &Path) -> PathBuf {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => fs::canonicalize(path).unwrap_or(path.into()),
        _ => path.to_path_buf(),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn new_file_is_private() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.json");
        write_atomic(&p, b"{}").unwrap();
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn keeps_existing_permissions() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.json");
        fs::write(&p, "old").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o640)).unwrap();
        write_atomic(&p, b"new").unwrap();
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(fs::read_to_string(&p).unwrap(), "new");
    }

    #[test]
    fn writes_through_symlink() {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real.json");
        let link = d.path().join("link.json");
        fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_atomic(&link, b"new").unwrap();
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
    }
}
