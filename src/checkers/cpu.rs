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
        let idle = 100.0 - usage;
        let load = System::load_average();
        Some(Alert {
            summary: format!("{} CPU overloaded", self.cfg.severity_label(usage, false)),
            body: format!("CPU: {usage:.1}%\nIdle: {idle:.1}%\nLoad: {:.2} {:.2} {:.2}",
                load.one, load.five, load.fifteen),
        })
    }

    fn report(&self, sys: &System) -> String {
        let usage = sys.global_cpu_usage() as f64;
        let load = System::load_average();
        let flag = if usage > self.cfg.threshold { "⚠️" } else { "✓" };
        format!("  CPU     {:>6.1}%  load: {:.2} {:.2} {:.2}  {flag}",
            usage, load.one, load.five, load.fifteen)
    }
}