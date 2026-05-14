use sysinfo::System;

use super::{Alert, Checker};
use crate::config::MetricConfig;

pub struct Swap {
    cfg: MetricConfig,
}

impl Swap {
    pub fn new(cfg: MetricConfig) -> Self {
        Self { cfg }
    }
}

impl Checker for Swap {
    fn key(&self) -> &'static str {
        "swap"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&self, sys: &System) -> Result<Option<Alert>, super::CheckerError> {
        let total = sys.total_swap();
        if total == 0 {
            return Ok(None);
        }
        let used = sys.used_swap();
        let usage = used as f64 / total as f64 * 100.0;
        if usage <= self.cfg.threshold {
            return Ok(None);
        }
        let sev = self.cfg.severity(usage, false);
        let used_mb = used / (1024 * 1024);
        let total_mb = total / (1024 * 1024);
        Ok(Some(Alert {
            severity: sev,
            summary: format!("{} Swap usage high", sev.emoji()),
            body: format!(
                "Swap: {used_mb}MB / {total_mb}MB ({usage:.1}%, threshold: {thr}%)",
                thr = self.cfg.threshold
            ),
        }))
    }

    fn report(&self, sys: &System) -> String {
        let total = sys.total_swap();
        if total == 0 {
            return format!("  Swap    {:>6}    threshold: {:>5.1}%  -  (disabled)", "N/A", self.cfg.threshold);
        }
        let used = sys.used_swap();
        let usage = used as f64 / total as f64 * 100.0;
        let used_mb = used / (1024 * 1024);
        let total_mb = total / (1024 * 1024);
        let sev = self.cfg.severity(usage, false);
        let flag = if usage > self.cfg.threshold { sev.emoji() } else { "✓" };
        format!(
            "  Swap    {:>6.1}%  threshold: {:>5.1}%  {flag}  ({used_mb}MB / {total_mb}MB)",
            usage, self.cfg.threshold
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_returns_swap() {
        let s = Swap::new(MetricConfig::default());
        assert_eq!(s.key(), "swap");
    }

    #[test]
    fn cooldown_returns_from_config() {
        let cfg = MetricConfig { cooldown_secs: 90, ..Default::default() };
        let s = Swap::new(cfg);
        assert_eq!(s.cooldown_secs(), 90);
    }
}
