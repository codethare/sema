use sysinfo::{Components, System};

use super::{Alert, Checker};
use crate::config::MetricConfig;

pub struct Temperature {
    cfg: MetricConfig,
}

impl Temperature {
    pub fn new(cfg: MetricConfig) -> Self {
        Self { cfg }
    }
}

impl Checker for Temperature {
    fn key(&self) -> &'static str {
        "temperature"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&mut self, _sys: &System) -> Option<Alert> {
        let components = Components::new_with_refreshed_list();
        let mut hottest: Option<(String, f32)> = None;
        for comp in &components {
            let Some(temp) = comp.temperature() else { continue };
            if temp > self.cfg.threshold as f32 {
                match &hottest {
                    Some((_, max)) if temp <= *max => {}
                    _ => hottest = Some((comp.label().to_string(), temp)),
                }
            }
        }
        hottest.map(|(label, temp)| Alert {
            summary: "🌡️ Temperature high".into(),
            body: format!("{label}: {temp:.0}°C (threshold: {thr}°C)", thr = self.cfg.threshold),
        })
    }

    fn report(&self, _sys: &System) -> String {
        let components = Components::new_with_refreshed_list();
        let temps: Vec<(String, f32)> = components
            .iter()
            .filter_map(|c| Some((c.label().to_string(), c.temperature()?)))
            .collect();
        if temps.is_empty() {
            return "  温度   N/A".into();
        }
        let (_, max_temp) = temps
            .iter()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .unwrap();
        let parts: String = temps.iter()
            .map(|(l, t)| format!("{l}: {t:.0}°C"))
            .collect::<Vec<_>>()
            .join(", ");
        let flag = if *max_temp > self.cfg.threshold as f32 { "⚠️" } else { "✓" };
        format!("  温度   {:>5.0}°C 阈值: {:>5.1}°C  {flag}  [{parts}]",
            max_temp, self.cfg.threshold)
    }
}
