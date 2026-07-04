use chrono::{Local, Timelike};

use super::{Alert, Checker, SysSnapshot};
use crate::config::TimeConfig;

pub struct TimeChecker {
    cfg: TimeConfig,
    last_fired: Option<(u32, u8)>,
}

impl TimeChecker {
    pub fn new(cfg: TimeConfig) -> Self {
        Self {
            cfg,
            last_fired: None,
        }
    }
}

impl Checker for TimeChecker {
    fn key(&self) -> &'static str {
        "time"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&mut self, _sys: &SysSnapshot) -> Result<Option<Alert>, super::CheckerError> {
        let now = Local::now();
        let hour = now.hour();
        let minute = now.minute() as u8;
        if !self.cfg.minutes.contains(&minute) {
            return Ok(None);
        }
        let slot = (hour, minute);
        if self.last_fired == Some(slot) {
            return Ok(None);
        }
        self.last_fired = Some(slot);
        Ok(Some(Alert {
            severity: crate::config::Severity::Warning,
            summary: "⏰ Time reminder".into(),
            body: format!("It's {}", now.format("%H:%M")),
        }))
    }

    fn report(&self, _sys: &SysSnapshot) -> String {
        let now = Local::now();
        let minute = now.minute() as u8;
        let is_time = self.cfg.minutes.contains(&minute);
        let status = if is_time { "will trigger" } else { "—" };
        let minutes_label = if self.cfg.minutes.len() <= 4 {
            self.cfg.minutes.iter().map(|m| format!(":{m:02}")).collect::<Vec<_>>().join(", ")
        } else {
            format!("{} slots", self.cfg.minutes.len())
        };
        format!("  Time    {:>2}:{:02}       {minutes_label}  {status}", now.hour(), minute)
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
        let cfg = TimeConfig {
            enabled: true,
            cooldown_secs: 120,
            ..Default::default()
        };
        let t = TimeChecker::new(cfg);
        assert_eq!(t.cooldown_secs(), 120);
    }

    #[test]
    fn report_contains_time_format() {
        let t = TimeChecker::new(TimeConfig::default());
        let snap = SysSnapshot {
            cpu_usage: 0.0,
            mem_used: 0,
            mem_total: 0,
            mem_available: 0,
            swap_used: 0,
            swap_total: 0,
            load_one: 0.0,
            load_five: 0.0,
            load_fifteen: 0.0,
        };
        let report = t.report(&snap);
        assert!(report.starts_with("  Time"));
        assert!(report.contains(":00") && report.contains(":30"));
    }
}
