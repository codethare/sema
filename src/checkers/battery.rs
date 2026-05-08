use std::fs;

use sysinfo::System;

use super::{Alert, Checker};
use crate::config::MetricConfig;

pub struct Battery {
    cfg: MetricConfig,
}

impl Battery {
    pub fn new(cfg: MetricConfig) -> Self {
        Self { cfg }
    }

    fn read_capacity() -> Option<u8> {
        (0..4).find_map(|i| {
            let path = format!("/sys/class/power_supply/BAT{i}/capacity");
            let content = fs::read_to_string(&path).ok()?;
            content.trim().parse::<u8>().ok()
        })
    }
}

impl Checker for Battery {
    fn key(&self) -> &'static str {
        "battery"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&mut self, _sys: &System) -> Option<Alert> {
        let capacity = Self::read_capacity()?;
        if (capacity as f64) >= self.cfg.threshold {
            return None;
        }
        Some(Alert {
            summary: "🔋 Battery low".into(),
            body: format!("Battery: {capacity}% (threshold: {thr}%)", thr = self.cfg.threshold),
        })
    }

    fn report(&self, _sys: &System) -> String {
        match Self::read_capacity() {
            Some(cap) => {
                let flag = if (cap as f64) < self.cfg.threshold { "⚠️" } else { "✓" };
                format!("  Battery {:>6}%  threshold: {:>5.1}%  {flag}", cap, self.cfg.threshold)
            }
            None => format!("  Battery {:>6}    threshold: {:>5.1}%  -  (not detected)", "N/A", self.cfg.threshold),
        }
    }
}
