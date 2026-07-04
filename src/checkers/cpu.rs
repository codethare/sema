use super::{Alert, Checker, SysSnapshot};
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

    fn check(&mut self, sys: &SysSnapshot) -> Result<Option<Alert>, super::CheckerError> {
        let usage = sys.cpu_usage;
        if usage.is_nan() || usage <= self.cfg.threshold {
            return Ok(None);
        }
        let sev = self.cfg.severity(usage, false);
        let idle = 100.0 - usage;
        Ok(Some(Alert {
            severity: sev,
            summary: format!("{} CPU overloaded", sev.emoji()),
            body: format!(
                "CPU: {usage:.1}%\nIdle: {idle:.1}%\nLoad: {:.2} {:.2} {:.2}",
                sys.load_one, sys.load_five, sys.load_fifteen
            ),
        }))
    }

    fn report(&self, sys: &SysSnapshot) -> String {
        let usage = sys.cpu_usage;
        if usage.is_nan() {
            return "  CPU     N/A".into();
        }
        let sev = self.cfg.severity(usage, false);
        let flag = if usage > self.cfg.threshold { sev.emoji() } else { "✓" };
        format!(
            "  CPU     {:>6.1}%  load: {:.2} {:.2} {:.2}  {flag}",
            usage, sys.load_one, sys.load_five, sys.load_fifteen
        )
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
