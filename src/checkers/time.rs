use chrono::{Local, Timelike};
use sysinfo::System;

use super::{Alert, Checker};
use crate::config::TimeConfig;

pub struct TimeChecker {
    cfg: TimeConfig,
}

impl TimeChecker {
    pub fn new(cfg: TimeConfig) -> Self {
        Self { cfg }
    }
}

impl Checker for TimeChecker {
    fn key(&self) -> &'static str {
        "time"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&mut self, _sys: &System) -> Option<Alert> {
        let now = Local::now();
        let minute = now.minute();
        if minute != 0 && minute != 30 {
            return None;
        }
        Some(Alert {
            summary: "⏰ Time reminder".into(),
            body: format!("It's {}", now.format("%H:%M")),
        })
    }

    fn report(&self, _sys: &System) -> String {
        let now = Local::now();
        let minute = now.minute();
        let is_time = minute == 0 || minute == 30;
        let status = if is_time { "当前时间，将触发提醒" } else { "—" };
        format!("  时间   {:>2}:{:02}       {:>8}    {status}", now.hour(), minute, "整点/半点")
    }
}
