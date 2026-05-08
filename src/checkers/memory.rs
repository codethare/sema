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
        let top3: Vec<String> = {
            let mut procs: Vec<(u64, String)> = sys.processes()
                .values()
                .map(|p| (p.memory(), p.name().to_string_lossy().into_owned()))
                .collect();
            procs.sort_by_key(|b| std::cmp::Reverse(b.0));
            procs.truncate(3);
            procs.into_iter().map(|(mem, name)| {
                let mb = mem / (1024 * 1024);
                format!("{name} {mb}MB")
            }).collect()
        };
        let body = format!("Memory: {used_mb}MB / {total_mb}MB ({usage:.1}%)  top: {}", top3.join(", "));
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
