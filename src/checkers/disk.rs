use sysinfo::Disks;

use super::{Alert, Checker, SysSnapshot};
use crate::config::MetricConfig;

pub struct Disk {
    cfg: MetricConfig,
    disks: Disks,
}

impl Disk {
    pub fn new(cfg: MetricConfig) -> Self {
        Self {
            cfg,
            disks: Disks::new_with_refreshed_list(),
        }
    }

    fn is_real_mount(d: &sysinfo::Disk) -> bool {
        // Filter out file-level bind mounts and stale entries; only real directory mounts count.
        d.mount_point().is_dir()
    }

    fn mount_allowed(&self, mount: &str) -> bool {
        if let Some(exclude) = &self.cfg.exclude_mounts {
            if exclude.iter().any(|p| mount.starts_with(p)) {
                return false;
            }
        }
        if let Some(include) = &self.cfg.include_mounts {
            return include.iter().any(|p| mount.starts_with(p));
        }
        true
    }

    fn max_usage(&self, disks: &Disks) -> Option<(String, u64, u64)> {
        let mut max: Option<(String, u64, u64, f64)> = None;
        for d in disks.iter() {
            if !Self::is_real_mount(d) {
                continue;
            }
            let mount = d.mount_point().to_string_lossy().to_string();
            if !self.mount_allowed(&mount) {
                continue;
            }
            let total = d.total_space();
            if total == 0 {
                continue;
            }
            let avail = d.available_space();
            let used = total.saturating_sub(avail);
            let usage = used as f64 / total as f64;
            match &max {
                Some((_, _, _, existing_usage)) if usage <= *existing_usage => {}
                _ => {
                    max = Some((mount, used, total, usage));
                }
            }
        }
        max.map(|(mount, used, total, _)| (mount, used, total))
    }
}

impl Checker for Disk {
    fn key(&self) -> &'static str {
        "disk"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&mut self, _sys: &SysSnapshot) -> Result<Option<Alert>, super::CheckerError> {
        self.disks.refresh(false);
        let Some((mount, used, total)) = self.max_usage(&self.disks) else {
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

    fn report(&self, _sys: &SysSnapshot) -> String {
        if self.disks.iter().count() == 0 {
            return "  Disk    N/A".into();
        }
        let mut seen: std::collections::HashSet<(u64, u64)> = std::collections::HashSet::new();
        let mut parts: Vec<String> = Vec::new();
        for d in self.disks.iter() {
            if !Self::is_real_mount(d) {
                continue;
            }
            let mount = d.mount_point().to_string_lossy().to_string();
            if !self.mount_allowed(&mount) {
                continue;
            }
            let total = d.total_space();
            if total == 0 {
                continue;
            }
            let avail = d.available_space();
            let used = total.saturating_sub(avail);
            // Deduplicate bind mounts of the same filesystem by (total, available).
            if !seen.insert((total, avail)) {
                continue;
            }
            let usage = used as f64 / total as f64 * 100.0;
            let used_gb = used as f64 / (1024.0 * 1024.0 * 1024.0);
            let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
            parts.push(format!("{mount} {usage:.1}% ({used_gb:.1}/{total_gb}GiB)"));
        }
        if parts.is_empty() {
            return "  Disk    N/A".into();
        }
        let max_usage = self
            .max_usage(&self.disks)
            .map(|(_, u, t)| u as f64 / t as f64 * 100.0)
            .unwrap_or(0.0);
        let sev = self.cfg.severity(max_usage, false);
        let flag = if max_usage > self.cfg.threshold { sev.emoji() } else { "✓" };
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
        let cfg = MetricConfig {
            cooldown_secs: 150,
            ..Default::default()
        };
        let d = Disk::new(cfg);
        assert_eq!(d.cooldown_secs(), 150);
    }
}
