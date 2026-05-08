use sysinfo::System;

use super::{Alert, Checker};
use crate::config::MetricConfig;

pub struct Swap {
    cfg: MetricConfig,
}

impl Swap {
    pub fn new(cfg: MetricConfig) -> Self {
        Self { cfg }
    }
}

impl Checker for Swap {
    fn key(&self) -> &'static str {
        "swap"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn enabled(&self) -> bool {
        self.cfg.enabled
    }

    fn check(&mut self, sys: &System) -> Option<Alert> {
        let total = sys.total_swap();
        if total == 0 {
            return None;
        }
        let used = sys.used_swap();
        let usage = used as f64 / total as f64 * 100.0;
        if usage <= self.cfg.threshold {
            return None;
        }
        let used_mb = used / (1024 * 1024);
        let total_mb = total / (1024 * 1024);
        Some(Alert {
            summary: "⚠️ Swap usage high".into(),
            body: format!("Swap: {used_mb}MB / {total_mb}MB ({usage:.1}%, threshold: {thr}%)",
                thr = self.cfg.threshold),
        })
    }

    fn report(&self, sys: &System) -> String {
        let total = sys.total_swap();
        if total == 0 {
            return format!("  Swap    {:>6}   阈值: {:>5.1}%  -  (未启用)", "N/A", self.cfg.threshold);
        }
        let used = sys.used_swap();
        let usage = used as f64 / total as f64 * 100.0;
        let used_mb = used / (1024 * 1024);
        let total_mb = total / (1024 * 1024);
        let flag = if usage > self.cfg.threshold { "⚠️" } else { "✓" };
        format!("  Swap    {:>6.1}%  阈值: {:>5.1}%  {flag}  ({used_mb}MB / {total_mb}MB)",
            usage, self.cfg.threshold)
    }
}
