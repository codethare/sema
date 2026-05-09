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

    fn is_cpu_sensor(label: &str) -> bool {
        let l = label.to_lowercase();
        // On non-Linux, include all components (no sysfs label filtering)
        if cfg!(not(target_os = "linux")) {
            return true;
        }
        // Exclude GPU/NVMe/disk sensors
        if l.contains("gpu") || l.contains("nvidia") || l.contains("amdgpu")
            || l.contains("nvme") || l.contains("ssd") || l.contains("hdd")
        {
            return false;
        }
        // CPU sensors typically have these in their labels
        l.contains("cpu") || l.contains("core") || l.contains("package")
            || l.contains("tctl") || l.contains("tdie") || l.contains("ccd")
            || l.contains("soc") || l.contains("edge") || l.contains("junction")
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
            if !Self::is_cpu_sensor(comp.label()) {
                continue;
            }
            let Some(temp) = comp.temperature() else { continue };
            if temp > self.cfg.threshold as f32 {
                match &hottest {
                    Some((_, max)) if temp <= *max => {}
                    _ => hottest = Some((comp.label().to_string(), temp)),
                }
            }
        }
        hottest.map(|(label, temp)| Alert {
            summary: format!("{} CPU temperature high", self.cfg.severity_label(temp as f64, false)),
            body: format!("{label}: {temp:.0}°C (threshold: {thr}°C)", thr = self.cfg.threshold),
        })
    }

    fn report(&self, _sys: &System) -> String {
        let components = Components::new_with_refreshed_list();
        let temps: Vec<(String, f32)> = components
            .iter()
            .filter(|c| Self::is_cpu_sensor(c.label()))
            .filter_map(|c| {
                let t = c.temperature()?;
                Some((c.label().to_string(), t))
            })
            .collect();
        let Some((_, max_temp)) = temps.iter().max_by(|(_, a), (_, b)| a.total_cmp(b)) else {
            return "  Temp    N/A".into();
        };
        let parts: String = temps.iter()
            .map(|(l, t)| format!("{l}: {t:.0}°C"))
            .collect::<Vec<_>>()
            .join(", ");
        let flag = if *max_temp > self.cfg.threshold as f32 { "⚠️" } else { "✓" };
        format!("  Temp    {:>5.0}°C  threshold: {:>5.1}°C  {flag}  [{parts}]",
            max_temp, self.cfg.threshold)
    }
}
