use sysinfo::System;

use super::{Alert, Checker};
use crate::config::MetricConfig;

pub struct Cpu {
    cfg: MetricConfig,
}

impl Cpu {
    pub fn new(cfg: MetricConfig) -> Self {
        Self { cfg }
    }
}

impl Checker for Cpu {
    fn key(&self) -> &'static str {
        "cpu"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&self, sys: &System) -> Result<Option<Alert>, super::CheckerError> {
        let usage = sys.global_cpu_usage() as f64;
        if usage.is_nan() || usage <= self.cfg.threshold {
            return Ok(None);
        }
        let sev = self.cfg.severity(usage, false);
        let idle = 100.0 - usage;
        let load = System::load_average();
        Ok(Some(Alert {
            severity: sev,
            summary: format!("{} CPU overloaded", sev.emoji()),
            body: format!(
                "CPU: {usage:.1}%\nIdle: {idle:.1}%\nLoad: {:.2} {:.2} {:.2}",
                load.one, load.five, load.fifteen
            ),
        }))
    }

    fn report(&self, sys: &System) -> String {
        let usage = sys.global_cpu_usage() as f64;
        if usage.is_nan() {
            return "  CPU     N/A".into();
        }
        let load = System::load_average();
        let sev = self.cfg.severity(usage, false);
        let flag = if usage > self.cfg.threshold { sev.emoji() } else { "✓" };
        format!("  CPU     {:>6.1}%  load: {:.2} {:.2} {:.2}  {flag}", usage, load.one, load.five, load.fifteen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_returns_cpu() {
        let c = Cpu::new(MetricConfig::default());
        assert_eq!(c.key(), "cpu");
    }

    #[test]
    fn cooldown_returns_from_config() {
        let cfg = MetricConfig {
            cooldown_secs: 300,
            ..Default::default()
        };
        let c = Cpu::new(cfg);
        assert_eq!(c.cooldown_secs(), 300);
    }
}
