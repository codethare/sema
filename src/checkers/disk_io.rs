use std::cell::{Cell, RefCell};
use std::fs;
use std::time::Instant;

use sysinfo::System;

use super::{Alert, Checker, CheckerError};
use crate::config::MetricConfig;

pub struct DiskIo {
    cfg: MetricConfig,
    prev_iowait: Cell<u64>,
    prev_total: Cell<u64>,
    prev_time: RefCell<Option<Instant>>,
}

impl DiskIo {
    pub fn new(cfg: MetricConfig) -> Self {
        Self {
            cfg,
            prev_iowait: Cell::new(0),
            prev_total: Cell::new(0),
            prev_time: RefCell::new(None),
        }
    }

    fn read_iowait() -> Result<(u64, u64), CheckerError> {
        let content = fs::read_to_string("/proc/stat")?;
        let cpu_line = content.lines().next().ok_or_else(|| {
            CheckerError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, "/proc/stat is empty"))
        })?;
        let fields: Vec<&str> = cpu_line.split_whitespace().collect();
        // cpu  user  nice  system  idle  iowait  irq  softirq  steal  guest  guest_nice
        //  0    1     2      3       4      5      6     7        8      9       10
        if fields.len() < 6 || fields[0] != "cpu" {
            return Err(CheckerError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unexpected /proc/stat format: '{}'", cpu_line),
            )));
        }
        let parse = |i: usize| -> Result<u64, CheckerError> {
            fields[i].parse().map_err(|e| {
                CheckerError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("parse field {i}: {e}")))
            })
        };
        let iowait = parse(5)?;
        let total: u64 = fields[1..].iter().filter_map(|s| s.parse::<u64>().ok()).sum();
        Ok((iowait, total))
    }
}

impl Checker for DiskIo {
    fn key(&self) -> &'static str {
        "disk_io"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&self, _sys: &System) -> Result<Option<Alert>, CheckerError> {
        let (iowait, total) = match Self::read_iowait() {
            Ok(v) => v,
            // Non-Linux or no /proc/stat — skip silently
            Err(CheckerError::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };

        let now = Instant::now();
        let prev_time = *self.prev_time.borrow();
        match prev_time {
            Some(_t) => {}
            None => {
                // First reading — establish baseline
                self.prev_iowait.set(iowait);
                self.prev_total.set(total);
                self.prev_time.replace(Some(now));
                return Ok(None);
            }
        }

        let iowait_delta = iowait.saturating_sub(self.prev_iowait.get());
        let total_delta = total.saturating_sub(self.prev_total.get());
        self.prev_iowait.set(iowait);
        self.prev_total.set(total);
        self.prev_time.replace(Some(now));

        // Need at least some CPU activity for a meaningful percentage
        if total_delta == 0 {
            return Ok(None);
        }
        let pct = iowait_delta as f64 / total_delta as f64 * 100.0;
        if pct.is_nan() || pct <= self.cfg.threshold {
            return Ok(None);
        }
        let sev = self.cfg.severity(pct, false);
        Ok(Some(Alert {
            severity: sev,
            summary: format!("{} Disk I/O wait high", sev.emoji()),
            body: format!("I/O wait: {pct:.1}% (threshold: {thr}%)", thr = self.cfg.threshold),
        }))
    }

    fn report(&self, _sys: &System) -> String {
        match Self::read_iowait() {
            Ok((_iowait, _total)) => {
                let sev = self.cfg.severity(self.cfg.threshold - 1.0, false);
                let flag = sev.emoji();
                format!("  I/O Wait {:>5}    threshold: {:>5.1}%  {flag}", "—", self.cfg.threshold)
            }
            Err(_) => "  I/O Wait N/A".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_returns_disk_io() {
        let d = DiskIo::new(MetricConfig::default());
        assert_eq!(d.key(), "disk_io");
    }

    #[test]
    fn cooldown_returns_from_config() {
        let cfg = MetricConfig {
            cooldown_secs: 90,
            ..Default::default()
        };
        let d = DiskIo::new(cfg);
        assert_eq!(d.cooldown_secs(), 90);
    }
}
