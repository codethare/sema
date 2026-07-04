use super::{Alert, Checker, SysSnapshot};
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

    fn check(&mut self, sys: &SysSnapshot) -> Result<Option<Alert>, super::CheckerError> {
        let total = sys.mem_total;
        if total == 0 {
            return Ok(None);
        }
        let usage = if self.cfg.use_available {
            (total.saturating_sub(sys.mem_available)) as f64 / total as f64 * 100.0
        } else {
            sys.mem_used as f64 / total as f64 * 100.0
        };
        if usage <= self.cfg.threshold {
            return Ok(None);
        }
        let sev = self.cfg.severity(usage, false);
        let total_mb = total / (1024 * 1024);
        let avail_mb = sys.mem_available / (1024 * 1024);
        let body = format!("Memory: {usage:.1}%\nAvailable: {avail_mb}MB / {total_mb}MB");
        Ok(Some(Alert {
            severity: sev,
            summary: format!("{} Memory usage high", sev.emoji()),
            body,
        }))
    }

    fn report(&self, sys: &SysSnapshot) -> String {
        let total = sys.mem_total;
        if total == 0 {
            return "  Memory    N/A  threshold: N/A".into();
        }
        let used = if self.cfg.use_available {
            total.saturating_sub(sys.mem_available)
        } else {
            sys.mem_used
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_returns_memory() {
        let m = Memory::new(MetricConfig::default());
        assert_eq!(m.key(), "memory");
    }

    #[test]
    fn cooldown_returns_from_config() {
        let cfg = MetricConfig {
            cooldown_secs: 120,
            ..Default::default()
        };
        let m = Memory::new(cfg);
        assert_eq!(m.cooldown_secs(), 120);
    }
}
