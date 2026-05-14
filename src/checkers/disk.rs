use std::cell::RefCell;

use sysinfo::{Disks, System};

use super::{Alert, Checker};
use crate::config::MetricConfig;

pub struct Disk {
    cfg: MetricConfig,
    disks: RefCell<Disks>,
}

impl Disk {
    pub fn new(cfg: MetricConfig) -> Self {
        Self {
            cfg,
            disks: RefCell::new(Disks::new_with_refreshed_list()),
        }
    }

    fn max_usage(disks: &Disks) -> Option<(String, u64, u64)> {
        let mut max: Option<(String, u64, u64)> = None;
        for d in disks.iter() {
            let total = d.total_space();
            if total == 0 {
                continue;
            }
            let avail = d.available_space();
            let used = total.saturating_sub(avail);
            match &max {
                Some((_, _, existing_used)) if used <= *existing_used => {}
                _ => {
                    let mount = d.mount_point().to_string_lossy().to_string();
                    max = Some((mount, used, total));
                }
            }
        }
        max
    }
}

impl Checker for Disk {
    fn key(&self) -> &'static str {
        "disk"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&self, _sys: &System) -> Result<Option<Alert>, super::CheckerError> {
        let mut disks = self.disks.borrow_mut();
        disks.refresh(false);
        let Some((mount, used, total)) = Self::max_usage(&disks) else {
            return Ok(None);
        };
        let usage = used as f64 / total as f64 * 100.0;
        if usage <= self.cfg.threshold {
            return Ok(None);
        }
        let sev = self.cfg.severity(usage, false);
        let used_gb = used as f64 / (1024.0 * 1024.0 * 1024.0);
        let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
        Ok(Some(Alert {
            severity: sev,
            summary: format!("{} Disk usage high", sev.emoji()),
            body: format!(
                "{mount}: {usage:.1}% ({used_gb:.1}GiB / {total_gb:.1}GiB, threshold: {thr}%)",
                thr = self.cfg.threshold
            ),
        }))
    }

    fn report(&self, _sys: &System) -> String {
        let disks = self.disks.borrow();
        if disks.iter().count() == 0 {
            return "  Disk    N/A".into();
        }
        let mut parts: Vec<String> = Vec::new();
        for d in disks.iter() {
            let total = d.total_space();
            if total == 0 {
                continue;
            }
            let avail = d.available_space();
            let used = total.saturating_sub(avail);
            let usage = used as f64 / total as f64 * 100.0;
            let mount = d.mount_point().to_string_lossy().to_string();
            let used_gb = used as f64 / (1024.0 * 1024.0 * 1024.0);
            let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
            parts.push(format!(
                "{mount} {usage:.1}% ({used_gb:.1}/{total_gb}GiB)"
            ));
        }
        if parts.is_empty() {
            return "  Disk    N/A".into();
        }
        let sev = self.cfg.severity(
            Self::max_usage(&disks)
                .map(|(_, u, t)| u as f64 / t as f64 * 100.0)
                .unwrap_or(0.0),
            false,
        );
        let flag = sev.emoji();
        format!("  Disk    {flag}  [{}]", parts.join("  "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_returns_disk() {
        let d = Disk::new(MetricConfig::default());
        assert_eq!(d.key(), "disk");
    }

    #[test]
    fn cooldown_returns_from_config() {
        let cfg = MetricConfig { cooldown_secs: 150, ..Default::default() };
        let d = Disk::new(cfg);
        assert_eq!(d.cooldown_secs(), 150);
    }
}
