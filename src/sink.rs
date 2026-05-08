use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::Local;
use notify_rust::Notification;

use crate::checkers::Alert;

const MAX_LOG_SIZE: u64 = 5 * 1024 * 1024; // 5 MB

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        let mut p = PathBuf::from(dir);
        p.push("sema");
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
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
        fs::create_dir_all(parent).ok();
    }
    if path.exists() && let Ok(meta) = path.metadata() && meta.len() > MAX_LOG_SIZE {
        let rotated = path.with_extension("log.1");
        let _ = fs::rename(&path, &rotated);
    }
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        writeln!(f, "[{ts}] {summary} | {body}").ok();
    }
}

pub struct Sink {
    last_notified: HashMap<String, Instant>,
    condition_started: HashMap<String, Instant>,
    was_active: HashSet<String>,
    log_enabled: bool,
    pub dry_run: bool,
}

impl Sink {
    pub fn new(dry_run: bool, log_enabled: bool) -> Self {
        Self {
            last_notified: HashMap::new(),
            condition_started: HashMap::new(),
            was_active: HashSet::new(),
            log_enabled,
            dry_run,
        }
    }

    fn can_notify(&self, key: &str, cooldown_secs: u64) -> bool {
        self.last_notified
            .get(key)
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(cooldown_secs))
    }

    fn duration_text(&self, key: &str) -> String {
        match self.condition_started.get(key) {
            Some(start) => {
                let secs = start.elapsed().as_secs();
                if secs < 60 {
                    format!("持续 {}s", secs)
                } else {
                    format!("持续 {}m{}s", secs / 60, secs % 60)
                }
            }
            None => String::new(),
        }
    }

    pub fn note_active(&mut self, key: &str, is_active: bool) -> bool {
        let was = self.was_active.contains(key);
        if is_active {
            self.was_active.insert(key.to_string());
            self.condition_started
                .entry(key.to_string())
                .or_insert_with(Instant::now);
        } else {
            self.condition_started.remove(key);
            self.was_active.remove(key);
        }
        was && !is_active
    }

    pub fn notify(&mut self, key: &str, cooldown_secs: u64, alert: &Alert) {
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
        let ok = Notification::new()
            .summary(&alert.summary)
            .body(&body)
            .appname("sema")
            .show()
            .is_ok();
        if ok {
            if self.log_enabled {
                write_log(&alert.summary, &body);
            }
            if cooldown_secs > 0 {
                self.last_notified.insert(key.to_string(), Instant::now());
            }
        }
    }

    pub fn log_path_display() -> String {
        log_path().display().to_string()
    }
}
