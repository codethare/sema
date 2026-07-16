use std::fs;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Local;

const MAX_LOG_SIZE: u64 = 5 * 1024 * 1024; // 5 MB

// ponytail: single-threaded callers (only main loop calls log::write).
// Mutex kept for static storage compat (RefCell is !Sync); never
// contended, overhead negligible.
static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);
// ponytail: same rationale as LOG_FILE.
static TRACE_FILE: Mutex<Option<File>> = Mutex::new(None);

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        let mut p = PathBuf::from(dir);
        if p.exists() {
            p = p.canonicalize().unwrap_or(p);
        }
        p.push("sema");
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| {
        tracing::warn!("HOME not set, using current directory for logs");
        ".".to_string()
    });
    PathBuf::from(home).join(".local").join("share").join("sema")
}

pub fn log_path() -> PathBuf {
    let mut p = data_dir();
    p.push("sema.log");
    p
}

pub fn trace_log_path() -> PathBuf {
    data_dir().join("sema-trace.log")
}

pub fn log_path_display() -> String {
    log_path().display().to_string()
}

pub fn write_trace(line: &str) {
    let path = trace_log_path();
    let mut guard = TRACE_FILE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(f) = guard.as_mut() {
        if f.write_all(line.as_bytes()).is_ok() && f.flush().is_ok() {
            return;
        }
        *guard = None;
    }
    if let Some(mut f) = open_log_file(&path) {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
        *guard = Some(f);
    }
}

fn ensure_log_dir(path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::warn!("cannot create log directory: {e}");
        }
        if let Err(e) = fs::set_permissions(parent, fs::Permissions::from_mode(0o700)) {
            tracing::warn!("cannot set log directory permissions: {e}");
        }
    }
}

fn rotate_if_needed(path: &std::path::Path) -> bool {
    if path.exists()
        && let Ok(meta) = path.metadata()
        && meta.len() > MAX_LOG_SIZE
    {
        if path.is_symlink() {
            tracing::warn!("log path is a symlink, skipping rotation");
            return false;
        }
        // Keep two backups: sema.log -> sema.log.1 -> sema.log.2
        let old1 = path.with_extension("log.1");
        let old2 = path.with_extension("log.2");
        let _ = fs::rename(&old1, &old2);
        let _ = fs::rename(path, &old1);
        return true;
    }
    false
}

fn open_log_file(path: &std::path::Path) -> Option<File> {
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .ok()
}

static LOG_DIR_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();

pub fn write(summary: &str, body: &str) {
    let path = log_path();
    LOG_DIR_INIT.get_or_init(|| {
        ensure_log_dir(&path);
    });
    let rotated = rotate_if_needed(&path);

    let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
    let line = format!("[{ts}] {summary} | {body}\n");

    let mut guard = LOG_FILE.lock().unwrap_or_else(|e| e.into_inner());
    if rotated {
        // Rotation renamed the file; the cached handle points to the old inode.
        // Drop it so the next open picks up the new sema.log.
        *guard = None;
    }
    let mut needs_reopen = true;
    if let Some(f) = guard.as_mut() {
        if f.write_all(line.as_bytes()).is_ok() && f.flush().is_ok() {
            needs_reopen = false;
        } else {
            *guard = None;
        }
    }
    if needs_reopen {
        if let Some(mut f) = open_log_file(&path) {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
            *guard = Some(f);
        }
    }
    // Emit as tracing event for correlation in trace log
    write_trace(&line);
}
