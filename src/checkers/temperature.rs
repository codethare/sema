use sysinfo::Components;

use super::{Alert, Checker, SysSnapshot};
use crate::config::MetricConfig;

pub struct Temperature {
    cfg: MetricConfig,
    components: Components,
}

impl Temperature {
    pub fn new(cfg: MetricConfig) -> Self {
        Self {
            cfg,
            components: Components::new_with_refreshed_list(),
        }
    }

    fn is_cpu_sensor(label: &str) -> bool {
        // On non-Linux, include all components (no sysfs label filtering)
        if cfg!(not(target_os = "linux")) {
            return true;
        }
        // sysfs thermal labels are ASCII; avoid allocating a lowercase String.
        fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
            if needle.len() > haystack.len() {
                return false;
            }
            haystack
                .as_bytes()
                .windows(needle.len())
                .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
        }
        // Exclude GPU/NVMe/disk sensors
        if contains_ignore_case(label, "gpu")
            || contains_ignore_case(label, "nvidia")
            || contains_ignore_case(label, "amdgpu")
            || contains_ignore_case(label, "nvme")
            || contains_ignore_case(label, "ssd")
            || contains_ignore_case(label, "hdd")
        {
            return false;
        }
        // CPU sensors typically have these in their labels
        contains_ignore_case(label, "cpu")
            || contains_ignore_case(label, "core")
            || contains_ignore_case(label, "package")
            || contains_ignore_case(label, "tctl")
            || contains_ignore_case(label, "tdie")
            || contains_ignore_case(label, "ccd")
            || contains_ignore_case(label, "soc")
    }
}

impl Checker for Temperature {
    fn key(&self) -> &'static str {
        "temperature"
    }

    fn cooldown_secs(&self) -> u64 {
        self.cfg.cooldown_secs
    }

    fn check(&mut self, _sys: &SysSnapshot) -> Result<Option<Alert>, super::CheckerError> {
        self.components.refresh(false);
        let mut hottest: Option<(String, f32)> = None;
        for comp in self.components.iter() {
            if !Self::is_cpu_sensor(comp.label()) {
                continue;
            }
            let Some(temp) = comp.temperature() else { continue };
            if temp.is_nan() {
                continue;
            }
            if temp > self.cfg.threshold as f32 {
                match &hottest {
                    Some((_, max)) if temp <= *max => {}
                    _ => hottest = Some((comp.label().to_string(), temp)),
                }
            }
        }
        Ok(hottest.map(|(label, temp)| {
            let sev = self.cfg.severity(temp as f64, false);
            Alert {
                severity: sev,
                summary: format!("{} CPU temperature high", sev.emoji()),
                body: format!("{label}: {temp:.0}°C (threshold: {thr}°C)", thr = self.cfg.threshold),
            }
        }))
    }

    fn report(&self, _sys: &SysSnapshot) -> String {
        let temps: Vec<(String, f32)> = self.components
            .iter()
            .filter(|c| Self::is_cpu_sensor(c.label()))
            .filter_map(|c| {
                let t = c.temperature()?;
                if t.is_nan() {
                    return None;
                }
                Some((c.label().to_string(), t))
            })
            .collect();
        let Some((_, max_temp)) = temps.iter().max_by(|(_, a), (_, b)| a.total_cmp(b)) else {
            return "  Temp    N/A".into();
        };
        let parts: String = temps
            .iter()
            .map(|(l, t)| format!("{l}: {t:.0}°C"))
            .collect::<Vec<_>>()
            .join(", ");
        let sev = self.cfg.severity(*max_temp as f64, false);
        let flag = if *max_temp > self.cfg.threshold as f32 {
            sev.emoji()
        } else {
            "✓"
        };
        format!("  Temp    {:>5.0}°C  threshold: {:>5.1}°C  {flag}  [{parts}]", max_temp, self.cfg.threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_returns_temperature() {
        let t = Temperature::new(MetricConfig::default());
        assert_eq!(t.key(), "temperature");
    }

    #[test]
    fn is_cpu_sensor_filters_gpu_and_nvme() {
        assert!(!Temperature::is_cpu_sensor("nvme0"));
        assert!(!Temperature::is_cpu_sensor("amdgpu"));
        assert!(!Temperature::is_cpu_sensor("GPU"));

        assert!(Temperature::is_cpu_sensor("cpu0"));
        assert!(Temperature::is_cpu_sensor("core0"));
        assert!(Temperature::is_cpu_sensor("Package id 0"));
        assert!(Temperature::is_cpu_sensor("Tctl"));
    }
}
