use std::fs;
use std::path::PathBuf;

use super::{Alert, Checker, SysSnapshot};
use crate::config::MetricConfig;

pub struct Battery {
    cfg: MetricConfig,
    paths: Vec<BatteryPath>,
}

struct BatteryPath {
    capacity: PathBuf,
    status: PathBuf,
}

impl Battery {
    pub fn new(cfg: MetricConfig) -> Self {
        Self {
            cfg,
            paths: Self::list_batteries(),
        }
    }

    fn list_batteries() -> Vec<BatteryPath> {
        #[cfg(target_os = "linux")]
        {
            match fs::read_dir("/sys/class/power_supply") {
                Ok(entries) => entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let dir = e.path();
                        let type_path = dir.join("type");
                        let type_content = fs::read_to_string(&type_path).ok()?;
                        if type_content.trim() != "Battery" {
                            return None;
                        }
                        let capacity = dir.join("capacity");
                        if !capacity.exists() {
                            return None;
                        }
                        Some(BatteryPath {
                            capacity,
                            status: dir.join("status"),
                        })
                    })
                    .collect(),
                Err(_) => Vec::new(),
            }
        }
        #[cfg(not(target_os = "linux"))]
        Vec::new()
    }

    fn is_charging_or_full(status_path: &PathBuf) -> Option<bool> {
        let content = fs::read_to_string(status_path).ok()?;
        let s = content.trim();
        Some(s == "Charging" || s == "Full")
    }

    fn read_capacity(&self) -> Option<u16> {
        self.paths.iter().find_map(|bp| {
            if Self::is_charging_or_full(&bp.status).unwrap_or(false) {
                return None;
            }
            let content = fs::read_to_string(&bp.capacity).ok()?;
            content.trim().parse::<u16>().ok()
        })
    }

    fn read_capacity_for_report(&self) -> Option<u16> {
        self.paths.iter().find_map(|bp| {
            let content = fs::read_to_string(&bp.capacity).ok()?;
            content.trim().parse::<u16>().ok()
        })
    }
}

impl Checker for Battery {
    fn key(&self) -> &'static str {
        "battery"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&mut self, _sys: &SysSnapshot) -> Result<Option<Alert>, super::CheckerError> {
        let capacity = match self.read_capacity() {
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

    fn report(&self, _sys: &SysSnapshot) -> String {
        match self.read_capacity_for_report() {
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
        let cfg = MetricConfig {
            cooldown_secs: 200,
            ..Default::default()
        };
        let b = Battery::new(cfg);
        assert_eq!(b.cooldown_secs(), 200);
    }

    #[test]
    fn check_returns_none_when_no_batteries_found() {
        // On systems without batteries, check returns Ok(None)
        let mut b = Battery::new(MetricConfig::default());
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
        let result = b.check(&snap).unwrap();
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
