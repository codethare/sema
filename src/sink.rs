use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::{LazyLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use notify_rust::{Notification, Urgency};

use crate::checkers::{Alert, Severity};
use crate::log;

const NOTIFICATION_TIMEOUT_SECS: u64 = 5;

struct NotifReq {
    summary: String,
    body: String,
    urgency: Urgency,
    reply: mpsc::Sender<bool>,
}

static NOTIF_TX: LazyLock<mpsc::Sender<NotifReq>> = LazyLock::new(|| {
    let (tx, rx) = mpsc::channel::<NotifReq>();
    thread::spawn(move || {
        for req in rx {
            let mut n = Notification::new();
            n.appname("sema").summary(&req.summary).urgency(req.urgency);
            if !req.body.is_empty() {
                n.body(&req.body);
            }
            let ok = match n.show() {
                Ok(_) => true,
                Err(e) => {
                    tracing::warn!("notification failed: {e}");
                    false
                }
            };
            let _ = req.reply.send(ok);
        }
    });
    tx
});

pub struct PendingAlert {
    pub key: &'static str,
    pub cooldown: u64,
    pub recovery: bool,
    pub alert: Alert,
}

pub fn send_notification(summary: &str, body: &str, severity: Severity) -> bool {
    let urgency = match severity {
        Severity::Warning => Urgency::Normal,
        Severity::Critical => Urgency::Critical,
    };
    let (tx, rx) = mpsc::channel();
    let req = NotifReq {
        summary: summary.to_string(),
        body: body.to_string(),
        urgency,
        reply: tx,
    };
    if NOTIF_TX.send(req).is_err() {
        tracing::warn!("notification worker thread stopped");
        return false;
    }
    match rx.recv_timeout(Duration::from_secs(NOTIFICATION_TIMEOUT_SECS)) {
        Ok(ok) => ok,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            tracing::warn!("notification timed out after {NOTIFICATION_TIMEOUT_SECS}s");
            false
        }
        Err(_) => {
            tracing::warn!("notification worker disconnected");
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
    recovery_cooldown_secs: u64,
}

impl Sink {
    pub fn new(dry_run: bool, log_enabled: bool, recovery_cooldown_secs: u64) -> Self {
        Self {
            last_notified: HashMap::new(),
            recovery_last_notified: HashMap::new(),
            condition_started: HashMap::new(),
            was_active: HashSet::new(),
            log_enabled,
            dry_run,
            recovery_cooldown_secs,
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

    /// Send or print a notification. Returns true when notification was actually sent.
    fn emit(&mut self, key: &'static str, body: String, alert: &Alert, recovery: bool) {
        if self.dry_run {
            println!("    └─ {}", alert.summary);
            println!("       {body}");
            return;
        }
        let ok = send_notification(&alert.summary, &body, alert.severity);
        if self.log_enabled {
            log::write(&alert.summary, &body);
        }
        if ok {
            if recovery {
                self.recovery_last_notified.insert(key, Instant::now());
            } else {
                self.last_notified.insert(key, Instant::now());
            }
        }
    }

    pub fn notify(&mut self, key: &'static str, cooldown_secs: u64, alert: &Alert) {
        let duration = self.duration_text(key);
        let body = if duration.is_empty() {
            alert.body.clone()
        } else {
            format!("{} ({})", alert.body, duration)
        };
        if cooldown_secs > 0 && !self.can_notify(key, cooldown_secs) {
            return;
        }
        self.emit(key, body, alert, false);
    }

    /// Send a recovery notification with its own cooldown tracking.
    /// Separate from alert notification cooldowns to prevent:
    ///   alert → recovery → alert → recovery ... oscillations from spamming.
    pub fn notify_recovery(&mut self, key: &'static str, alert: &Alert) {
        if self
            .recovery_last_notified
            .get(key)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(self.recovery_cooldown_secs))
        {
            return;
        }
        self.emit(key, alert.body.clone(), alert, true);
    }

    /// Dispatch a batch of pending alerts, grouping multiple into one composite notification.
    pub fn notify_pending(&mut self, pending: Vec<PendingAlert>) {
        if pending.is_empty() {
            return;
        }
        if pending.len() == 1 {
            let p = &pending[0];
            if p.recovery {
                self.notify_recovery(p.key, &p.alert);
            } else {
                self.notify(p.key, p.cooldown, &p.alert);
            }
        } else {
            let max_cooldown = pending.iter().map(|p| p.cooldown).max().unwrap();
            let severity = pending
                .iter()
                .map(|p| p.alert.severity)
                .max_by_key(|s| match s {
                    Severity::Critical => 1,
                    Severity::Warning => 0,
                })
                .unwrap_or(Severity::Warning);
            let mut body = String::new();
            for (i, p) in pending.iter().enumerate() {
                if i > 0 {
                    body.push_str(" | ");
                }
                let _ = write!(body, "{}: {}", p.alert.summary, p.alert.body);
            }
            let composite = Alert {
                severity,
                summary: format!("{} {} alerts", severity.emoji(), pending.len()),
                body,
            };
            self.notify("group", max_cooldown, &composite);
        }
    }

    pub fn set_log_enabled(&mut self, enabled: bool) {
        self.log_enabled = enabled;
    }

    pub fn set_recovery_cooldown_secs(&mut self, secs: u64) {
        self.recovery_cooldown_secs = secs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_notify_returns_true_when_no_previous_notification() {
        let sink = Sink::new(true, false, 60);
        assert!(sink.can_notify("test", 60));
    }

    #[test]
    fn can_notify_returns_false_within_cooldown() {
        let mut sink = Sink::new(true, false, 60);
        sink.last_notified.insert("test", Instant::now());
        assert!(!sink.can_notify("test", 60));
    }

    #[test]
    fn can_notify_returns_true_after_cooldown() {
        let mut sink = Sink::new(true, false, 60);
        let past = Instant::now() - Duration::from_secs(120);
        sink.last_notified.insert("test", past);
        assert!(sink.can_notify("test", 60));
    }

    #[test]
    fn can_notify_zero_cooldown_always_true() {
        let mut sink = Sink::new(true, false, 60);
        sink.last_notified.insert("test", Instant::now());
        // can_notify is only called when cooldown > 0 in production
        assert!(sink.can_notify("test", 0));
    }

    #[test]
    fn note_active_tracks_transition() {
        let mut sink = Sink::new(true, false, 60);
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
        let sink = Sink::new(true, false, 60);
        assert_eq!(sink.duration_text("cpu"), "");
    }

    #[test]
    fn duration_text_returns_seconds_for_short_duration() {
        let mut sink = Sink::new(true, false, 60);
        sink.note_active("cpu", true);
        let text = sink.duration_text("cpu");
        assert!(text.starts_with("for "));
        assert!(text.ends_with('s'));
    }

    #[test]
    fn duration_text_empty_after_deactivation() {
        let mut sink = Sink::new(true, false, 60);
        sink.note_active("cpu", true);
        std::thread::sleep(Duration::from_millis(5));
        sink.note_active("cpu", false);
        assert_eq!(sink.duration_text("cpu"), "");
    }

    #[test]
    fn notify_recovery_dry_run_prints_but_does_not_track() {
        let mut sink = Sink::new(true, false, 60);
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
        let mut sink = Sink::new(false, false, 60);
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
        let path = log::log_path();
        // Should be an absolute path
        assert!(path.is_absolute());
        // Should end with sema.log
        assert_eq!(path.file_name().unwrap(), "sema.log");
    }
}
