use sysinfo::System;

use super::{Alert, Checker};
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

    fn check(&mut self, sys: &System) -> Option<Alert> {
        let total = sys.total_memory();
        let used = sys.used_memory();
        let usage = used as f64 / total as f64 * 100.0;
        if usage <= self.cfg.threshold {
            return None;
        }
        let used_mb = used / (1024 * 1024);
        let total_mb = total / (1024 * 1024);
        let top = sys.processes()
            .iter()
            .max_by_key(|(_, p)| p.memory());
        let body = match top {
            Some((_, p)) => format!("Memory: {used_mb}MB / {total_mb}MB ({usage:.1}%, top: {} {}MB, threshold: {thr}%)",
                p.name().to_string_lossy(), p.memory() / (1024 * 1024), thr = self.cfg.threshold),
            None => format!("Memory: {used_mb}MB / {total_mb}MB ({usage:.1}%, threshold: {thr}%)",
                thr = self.cfg.threshold),
        };
        Some(Alert { summary: format!("{} Memory usage high", self.cfg.severity_label(usage, false)), body })
    }

    fn report(&self, sys: &System) -> String {
        let total = sys.total_memory();
        let used = sys.used_memory();
        let usage = used as f64 / total as f64 * 100.0;
        let used_mb = used / (1024 * 1024);
        let total_mb = total / (1024 * 1024);
        let flag = if usage > self.cfg.threshold { "⚠️" } else { "✓" };
        format!("  Memory  {:>6.1}%  threshold: {:>5.1}%  {flag}  ({used_mb}MB / {total_mb}MB)",
            usage, self.cfg.threshold)
    }
}
