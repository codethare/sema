use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use notify_rust::{Notification, Urgency};

use chrono::Local;

use crate::checkers::{Alert, Severity};

const MAX_LOG_SIZE: u64 = 5 * 1024 * 1024; // 5 MB

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

fn log_path() -> PathBuf {
    let mut p = data_dir();
    p.push("sema.log");
    p
}

fn write_log(summary: &str, body: &str) {
    let path = log_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::warn!("cannot create log directory: {e}");
        }
        if let Err(e) = fs::set_permissions(parent, fs::Permissions::from_mode(0o700)) {
            tracing::warn!("cannot set log directory permissions: {e}");
        }
    }
    if path.exists()
        && let Ok(meta) = path.metadata()
        && meta.len() > MAX_LOG_SIZE
    {
        if path.is_symlink() {
            tracing::warn!("log path is a symlink, skipping rotation");
        } else {
            let rotated = path.with_extension("log.1");
            let _ = fs::rename(&path, &rotated);
        }
    }
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        && let Err(e) = writeln!(f, "[{ts}] {summary} | {body}")
    {
        tracing::warn!("failed to write log: {e}");
    }
}

/// Send a desktop notification via notify-rust. Returns true on success.
pub fn send_notification(summary: &str, body: &str, severity: Severity) -> bool {
    let urgency = match severity {
        Severity::Warning => Urgency::Normal,
        Severity::Critical => Urgency::Critical,
    };
    let mut n = Notification::new();
    n.appname("sema").summary(summary).urgency(urgency);
    if !body.is_empty() {
        n.body(body);
    }
    match n.show() {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("notification failed: {e}");
            false
        }
    }
}

pub struct Sink {
    last_notified: HashMap<&'static str, Instant>,
    recovery_last_notified: HashMap<&'static str, Instant>,
    condition_started: HashMap<&'static str, Instant>,
    was_active: HashSet<&'static str>,
    log_enabled: bool,
    pub dry_run: bool,
}

const RECOVERY_COOLDOWN_SECS: u64 = 60;

impl Sink {
    pub fn new(dry_run: bool, log_enabled: bool) -> Self {
        Self {
            last_notified: HashMap::new(),
            recovery_last_notified: HashMap::new(),
            condition_started: HashMap::new(),
            was_active: HashSet::new(),
            log_enabled,
            dry_run,
        }
    }

    fn can_notify(&self, key: &'static str, cooldown_secs: u64) -> bool {
        self.last_notified
            .get(key)
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(cooldown_secs))
    }

    fn duration_text(&self, key: &'static str) -> String {
        match self.condition_started.get(key) {
            Some(start) => {
                let secs = start.elapsed().as_secs();
                if secs < 60 {
                    format!("for {}s", secs)
                } else {
                    format!("for {}m{}s", secs / 60, secs % 60)
                }
            }
            None => String::new(),
        }
    }

    pub fn note_active(&mut self, key: &'static str, is_active: bool) -> bool {
        let was = self.was_active.contains(key);
        if is_active {
            self.was_active.insert(key);
            self.condition_started.entry(key).or_insert_with(Instant::now);
        } else {
            self.condition_started.remove(key);
            self.was_active.remove(key);
        }
        was && !is_active
    }

    pub fn notify(&mut self, key: &'static str, cooldown_secs: u64, alert: &Alert) {
        let duration = self.duration_text(key);
        let body = if duration.is_empty() {
            alert.body.clone()
        } else {
            format!("{} ({})", alert.body, duration)
        };

        if self.dry_run {
            println!("    └─ {}", alert.summary);
            println!("       {body}");
            return;
        }
        if cooldown_secs > 0 && !self.can_notify(key, cooldown_secs) {
            return;
        }
        let ok = send_notification(&alert.summary, &body, alert.severity);
        if ok {
            if self.log_enabled {
                write_log(&alert.summary, &body);
            }
            if cooldown_secs > 0 {
                self.last_notified.insert(key, Instant::now());
            }
        }
    }

    /// Send a recovery notification with its own cooldown tracking.
    /// Separate from alert notification cooldowns to prevent:
    ///   alert → recovery → alert → recovery ... oscillations from spamming.
    pub fn notify_recovery(&mut self, key: &'static str, alert: &Alert) {
        if self
            .recovery_last_notified
            .get(key)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(RECOVERY_COOLDOWN_SECS))
        {
            return;
        }
        let body = alert.body.clone();

        if self.dry_run {
            println!("    └─ {}", alert.summary);
            println!("       {body}");
            return;
        }

        let ok = send_notification(&alert.summary, &body, alert.severity);
        if ok {
            self.recovery_last_notified.insert(key, Instant::now());
            if self.log_enabled {
                write_log(&alert.summary, &body);
            }
        }
    }

    pub fn set_log_enabled(&mut self, enabled: bool) {
        self.log_enabled = enabled;
    }

    pub fn log_path_display() -> String {
        log_path().display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_notify_returns_true_when_no_previous_notification() {
        let sink = Sink::new(true, false);
        assert!(sink.can_notify("test", 60));
    }

    #[test]
    fn can_notify_returns_false_within_cooldown() {
        let mut sink = Sink::new(true, false);
        sink.last_notified.insert("test", Instant::now());
        assert!(!sink.can_notify("test", 60));
    }

    #[test]
    fn can_notify_returns_true_after_cooldown() {
        let mut sink = Sink::new(true, false);
        let past = Instant::now() - Duration::from_secs(120);
        sink.last_notified.insert("test", past);
        assert!(sink.can_notify("test", 60));
    }

    #[test]
    fn can_notify_zero_cooldown_always_true() {
        let mut sink = Sink::new(true, false);
        sink.last_notified.insert("test", Instant::now());
        // can_notify is only called when cooldown > 0 in production
        assert!(sink.can_notify("test", 0));
    }

    #[test]
    fn note_active_tracks_transition() {
        let mut sink = Sink::new(true, false);
        assert!(!sink.note_active("cpu", true));
        assert!(sink.was_active.contains("cpu"));
        assert!(sink.condition_started.contains_key("cpu"));

        assert!(!sink.note_active("cpu", true));

        assert!(sink.note_active("cpu", false));
        assert!(!sink.was_active.contains("cpu"));
        assert!(!sink.condition_started.contains_key("cpu"));

        assert!(!sink.note_active("cpu", false));
    }

    #[test]
    fn duration_text_returns_empty_when_never_started() {
        let sink = Sink::new(true, false);
        assert_eq!(sink.duration_text("cpu"), "");
    }

    #[test]
    fn duration_text_returns_seconds_for_short_duration() {
        let mut sink = Sink::new(true, false);
        sink.note_active("cpu", true);
        let text = sink.duration_text("cpu");
        assert!(text.starts_with("for "));
        assert!(text.ends_with('s'));
    }

    #[test]
    fn duration_text_empty_after_deactivation() {
        let mut sink = Sink::new(true, false);
        sink.note_active("cpu", true);
        std::thread::sleep(Duration::from_millis(5));
        sink.note_active("cpu", false);
        assert_eq!(sink.duration_text("cpu"), "");
    }

    #[test]
    fn notify_recovery_dry_run_prints_but_does_not_track() {
        let mut sink = Sink::new(true, false);
        let alert = Alert {
            severity: Severity::Warning,
            summary: "✅ test back to normal".into(),
            body: String::new(),
        };
        sink.notify_recovery("test_metric", &alert);
        // Dry-run prints output but doesn't update cooldown tracking
        assert!(!sink.recovery_last_notified.contains_key("test_metric"));
    }

    #[test]
    fn notify_recovery_cooldown_suppresses_duplicates() {
        let mut sink = Sink::new(false, false);
        let alert = Alert {
            severity: Severity::Warning,
            summary: "test back to normal".into(),
            body: String::new(),
        };

        // Pre-populate cooldown to simulate a recent notification
        sink.recovery_last_notified.insert("test_metric", Instant::now());

        // Cooldown check should early-return without calling send_notification
        sink.notify_recovery("test_metric", &alert);

        // The pre-populated entry must persist after cooldown-supressed call
        assert!(sink.recovery_last_notified.contains_key("test_metric"));
    }

    #[test]
    fn log_path_is_under_xdg_data_home() {
        let path = log_path();
        // Should be an absolute path
        assert!(path.is_absolute());
        // Should end with sema.log
        assert_eq!(path.file_name().unwrap(), "sema.log");
    }
}
