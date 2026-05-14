use std::fs;

use sysinfo::System;

use super::{Alert, Checker};
use crate::config::MetricConfig;

pub struct Battery {
    cfg: MetricConfig,
}

impl Battery {
    pub fn new(cfg: MetricConfig) -> Self {
        Self { cfg }
    }

    fn list_batteries() -> Vec<String> {
        match fs::read_dir("/sys/class/power_supply") {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().contains("BAT"))
                .map(|e| e.path().join("capacity"))
                .filter(|p| p.exists())
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn read_capacity() -> Option<u16> {
        #[cfg(target_os = "linux")]
        {
            Self::list_batteries().iter().find_map(|path| {
                let content = fs::read_to_string(path).ok()?;
                content.trim().parse::<u16>().ok()
            })
        }
        #[cfg(not(target_os = "linux"))]
        None
    }
}

impl Checker for Battery {
    fn key(&self) -> &'static str {
        "battery"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&self, _sys: &System) -> Result<Option<Alert>, super::CheckerError> {
        let capacity = match Self::read_capacity() {
            Some(c) => c,
            None => return Ok(None),
        };
        if (capacity as f64) >= self.cfg.threshold {
            return Ok(None);
        }
        let sev = self.cfg.severity(capacity as f64, true);
        Ok(Some(Alert {
            severity: sev,
            summary: format!("{} Battery low", sev.emoji()),
            body: format!("Battery: {capacity}% (threshold: {thr}%)", thr = self.cfg.threshold),
        }))
    }

    fn report(&self, _sys: &System) -> String {
        match Self::read_capacity() {
            Some(cap) => {
                let sev = self.cfg.severity(cap as f64, true);
                let flag = if (cap as f64) < self.cfg.threshold {
                    sev.emoji()
                } else {
                    "✓"
                };
                format!("  Battery {:>6}%  threshold: {:>5.1}%  {flag}", cap, self.cfg.threshold)
            }
            None => format!("  Battery {:>6}    threshold: {:>5.1}%  -  (not detected)", "N/A", self.cfg.threshold),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_returns_battery() {
        let b = Battery::new(MetricConfig::default());
        assert_eq!(b.key(), "battery");
    }

    #[test]
    fn cooldown_returns_from_config() {
        let cfg = MetricConfig { cooldown_secs: 200, ..Default::default() };
        let b = Battery::new(cfg);
        assert_eq!(b.cooldown_secs(), 200);
    }

    #[test]
    fn check_returns_none_when_no_batteries_found() {
        // On systems without batteries, check returns Ok(None)
        let b = Battery::new(MetricConfig::default());
        let sys = System::new();
        let result = b.check(&sys).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn list_batteries_on_non_linux() {
        #[cfg(not(target_os = "linux"))]
        {
            let batteries = Battery::list_batteries();
            assert!(batteries.is_empty());
        }
    }
}
