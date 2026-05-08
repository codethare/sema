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
        let top = sys.processes()
            .iter()
            .max_by(|(_, a), (_, b)| a.cpu_usage().total_cmp(&b.cpu_usage()));
        let body = match top {
            Some((_, p)) => format!("CPU usage: {usage:.1}% (top: {} {:.1}%, threshold: {thr}%)",
                p.name().to_string_lossy(), p.cpu_usage(), thr = self.cfg.threshold),
            None => format!("CPU usage: {usage:.1}% (threshold: {thr}%)",
                thr = self.cfg.threshold),
        };
        Some(Alert { summary: "⚠️ CPU overloaded".into(), body })
    }

    fn report(&self, sys: &System) -> String {
        let usage = sys.global_cpu_usage() as f64;
        let flag = if usage > self.cfg.threshold { "⚠️" } else { "✓" };
        format!("  CPU     {:>6.1}%  阈值: {:>5.1}%  {flag}", usage, self.cfg.threshold)
    }
}
