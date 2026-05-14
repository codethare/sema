use chrono::{Local, Timelike};
use sysinfo::System;

use super::{Alert, Checker};
use crate::config::TimeConfig;

pub struct TimeChecker {
    cfg: TimeConfig,
}

impl TimeChecker {
    pub fn new(cfg: TimeConfig) -> Self {
        Self { cfg }
    }
}

impl Checker for TimeChecker {
    fn key(&self) -> &'static str {
        "time"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&self, _sys: &System) -> Result<Option<Alert>, super::CheckerError> {
        let now = Local::now();
        let minute = now.minute();
        if minute != 0 && minute != 30 {
            return Ok(None);
        }
        Ok(Some(Alert {
            severity: crate::config::Severity::Warning,
            summary: "⏰ Time reminder".into(),
            body: format!("It's {}", now.format("%H:%M")),
        }))
    }

    fn report(&self, _sys: &System) -> String {
        let now = Local::now();
        let minute = now.minute();
        let is_time = minute == 0 || minute == 30;
        let status = if is_time { "will trigger" } else { "—" };
        format!("  Time    {:>2}:{:02}       :00/:30  {status}", now.hour(), minute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_returns_time() {
        let t = TimeChecker::new(TimeConfig::default());
        assert_eq!(t.key(), "time");
    }

    #[test]
    fn cooldown_returns_from_config() {
        let cfg = TimeConfig { enabled: true, cooldown_secs: 120 };
        let t = TimeChecker::new(cfg);
        assert_eq!(t.cooldown_secs(), 120);
    }

    #[test]
    fn report_contains_time_format() {
        let t = TimeChecker::new(TimeConfig::default());
        let sys = System::new();
        let report = t.report(&sys);
        assert!(report.starts_with("  Time"));
        assert!(report.contains(":00/:30"));
    }
}
