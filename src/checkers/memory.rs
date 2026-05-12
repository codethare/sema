use sysinfo::System;

use super::{Alert, Checker};
use crate::config::MetricConfig;

pub struct Memory {
    cfg: MetricConfig,
}

impl Memory {
    pub fn new(cfg: MetricConfig) -> Self {
        Self { cfg }
    }
}

impl Checker for Memory {
    fn key(&self) -> &'static str {
        "memory"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&mut self, sys: &System) -> Result<Option<Alert>, super::CheckerError> {
        let total = sys.total_memory();
        if total == 0 {
            return Ok(None);
        }
        let used = sys.used_memory();
        let usage = used as f64 / total as f64 * 100.0;
        if usage <= self.cfg.threshold {
            return Ok(None);
        }
        let sev = self.cfg.severity(usage, false);
        let total_mb = total / (1024 * 1024);
        let avail_mb = sys.available_memory() / (1024 * 1024);
        let body = format!("Memory: {usage:.1}%\nAvailable: {avail_mb}MB / {total_mb}MB");
        Ok(Some(Alert {
            severity: sev,
            summary: format!("{} Memory usage high", sev.emoji()),
            body,
        }))
    }

    fn report(&self, sys: &System) -> String {
        let total = sys.total_memory();
        let used = sys.used_memory();
        let usage = used as f64 / total as f64 * 100.0;
        let used_mb = used / (1024 * 1024);
        let total_mb = total / (1024 * 1024);
        let sev = self.cfg.severity(usage, false);
        let flag = if usage > self.cfg.threshold { sev.emoji() } else { "✓" };
        format!(
            "  Memory  {:>6.1}%  threshold: {:>5.1}%  {flag}  ({used_mb}MB / {total_mb}MB)",
            usage, self.cfg.threshold
        )
    }
}
