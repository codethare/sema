use sysinfo::System;

use super::{Alert, Checker};
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

    fn check(&mut self, sys: &System) -> Option<Alert> {
        let usage = sys.global_cpu_usage() as f64;
        if usage <= self.cfg.threshold {
            return None;
        }
        let top3: Vec<String> = {
            let mut procs: Vec<(f32, String)> = sys.processes()
                .values()
                .map(|p| (p.cpu_usage(), p.name().to_string_lossy().into_owned()))
                .collect();
            procs.sort_by(|a, b| b.0.total_cmp(&a.0));
            procs.truncate(3);
            procs.into_iter().map(|(cpu, name)| format!("{name} {cpu:.1}%")).collect()
        };
        let body = format!("CPU usage: {usage:.1}%  top: {}", top3.join(", "));
        Some(Alert { summary: format!("{} CPU overloaded", self.cfg.severity_label(usage, false)), body })
    }

    fn report(&self, sys: &System) -> String {
        let usage = sys.global_cpu_usage() as f64;
        let flag = if usage > self.cfg.threshold { "⚠️" } else { "✓" };
        format!("  CPU     {:>6.1}%  threshold: {:>5.1}%  {flag}", usage, self.cfg.threshold)
    }
}
