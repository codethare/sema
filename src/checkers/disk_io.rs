use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Instant;

use super::{Alert, Checker, CheckerError, SysSnapshot};
use crate::config::MetricConfig;

pub struct DiskIo {
    cfg: MetricConfig,
    prev_iowait: u64,
    prev_total: u64,
    prev_time: Option<Instant>,
}

impl DiskIo {
    pub fn new(cfg: MetricConfig) -> Self {
        Self {
            cfg,
            prev_iowait: 0,
            prev_total: 0,
            prev_time: None,
        }
    }

    fn read_iowait() -> Result<(u64, u64), CheckerError> {
        let file = File::open("/proc/stat")?;
        let mut reader = BufReader::new(file);
        let mut cpu_line = String::with_capacity(256);
        reader.read_line(&mut cpu_line)?;
        if cpu_line.is_empty() {
            return Err(CheckerError::InvalidData("/proc/stat is empty".into()));
        }
        let fields: Vec<&str> = cpu_line.split_whitespace().collect();
        // cpu  user  nice  system  idle  iowait  irq  softirq  steal  guest  guest_nice
        //  0    1     2      3       4      5      6     7        8      9       10
        if fields.len() < 6 || fields[0] != "cpu" {
            return Err(CheckerError::InvalidData(format!(
                "unexpected /proc/stat format: '{}'",
                cpu_line.trim_end()
            )));
        }
        let parse = |i: usize| -> Result<u64, CheckerError> {
            fields[i].parse().map_err(|e| {
                CheckerError::InvalidData(format!("parse field {i}: {e}"))
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

    fn check(&mut self, _sys: &SysSnapshot) -> Result<Option<Alert>, CheckerError> {
        let (iowait, total) = match Self::read_iowait() {
            Ok(v) => v,
            // Non-Linux or no /proc/stat — skip silently
            Err(CheckerError::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };

        let now = Instant::now();
        let prev_time = self.prev_time;
        match prev_time {
            Some(_t) => {}
            None => {
                // First reading — establish baseline
                self.prev_iowait = iowait;
                self.prev_total = total;
                self.prev_time = Some(now);
                return Ok(None);
            }
        }

        let iowait_delta = iowait.saturating_sub(self.prev_iowait);
        let total_delta = total.saturating_sub(self.prev_total);
        self.prev_iowait = iowait;
        self.prev_total = total;
        self.prev_time = Some(now);

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

    fn report(&self, _sys: &SysSnapshot) -> String {
        let (iowait, total) = match Self::read_iowait() {
            Ok(v) => v,
            Err(_) => return "  I/O Wait N/A".into(),
        };

        let prev_time = self.prev_time;
        let prev_iowait = self.prev_iowait;
        let prev_total = self.prev_total;

        match prev_time {
            None => format!("  I/O Wait {:>5}    threshold: {:>5.1}%  ✓  (baseline)", "—", self.cfg.threshold),
            Some(_) => {
                let iowait_delta = iowait.saturating_sub(prev_iowait);
                let total_delta = total.saturating_sub(prev_total);
                if total_delta == 0 {
                    return format!("  I/O Wait {:>5}    threshold: {:>5.1}%  ✓", "—", self.cfg.threshold);
                }
                let pct = iowait_delta as f64 / total_delta as f64 * 100.0;
                let sev = self.cfg.severity(pct, false);
                let flag = if pct > self.cfg.threshold { sev.emoji() } else { "✓" };
                format!("  I/O Wait {:>5.1}%  threshold: {:>5.1}%  {flag}", pct, self.cfg.threshold)
            }
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
