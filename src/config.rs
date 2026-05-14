use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use serde::Deserialize;

const MIN_INTERVAL: u64 = 1;
const MAX_INTERVAL: u64 = 3600;
const MIN_COOLDOWN: u64 = 1;
const MAX_CONFIG_SIZE: u64 = 1024 * 1024; // 1 MB

/// sema top-level configuration
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,

    #[serde(default)]
    pub disk: MetricConfig,

    #[serde(default)]
    pub cpu: MetricConfig,

    #[serde(default)]
    pub memory: MetricConfig,

    #[serde(default)]
    pub swap: MetricConfig,

    #[serde(default)]
    pub battery: MetricConfig,

    #[serde(default)]
    pub time: TimeConfig,

    #[serde(default)]
    pub temperature: MetricConfig,

    #[serde(default)]
    pub network: NetworkConfig,

    #[serde(default)]
    pub disk_io: MetricConfig,

    #[serde(default)]
    pub log: LogConfig,
}

/// Alert severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Critical,
}

impl Severity {
    /// Emoji for display
    pub fn emoji(self) -> &'static str {
        match self {
            Severity::Warning => "⚠️",
            Severity::Critical => "🔴",
        }
    }
}

/// Generic metric configuration (CPU / memory / swap / battery / temperature / network / disk)
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct MetricConfig {
    pub enabled: bool,
    pub threshold: f64,
    /// Critical alert threshold (optional). Values above this are CRITICAL.
    pub critical: Option<f64>,
    pub cooldown_secs: u64,
}

impl Default for MetricConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 50.0,
            critical: None,
            cooldown_secs: 60,
        }
    }
}

impl MetricConfig {
    /// Determine severity from value. `inverted`=true means lower is more severe (e.g., battery).
    pub fn severity(&self, val: f64, inverted: bool) -> Severity {
        if let Some(crit) = self.critical {
            let severe = if inverted { val < crit } else { val > crit };
            if severe {
                return Severity::Critical;
            }
        }
        Severity::Warning
    }
}

/// Time reminder configuration
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TimeConfig {
    pub enabled: bool,
    pub cooldown_secs: u64,
}

impl Default for TimeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cooldown_secs: 60,
        }
    }
}

/// Log configuration
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LogConfig {
    pub enabled: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

const fn default_check_interval() -> u64 {
    10
}

macro_rules! metric_defaults {
    ($($name:ident: $threshold:expr),* $(,)?) => {
        $(
            pub fn $name() -> MetricConfig {
                MetricConfig {
                    enabled: true,
                    threshold: $threshold,
                    critical: None,
                    cooldown_secs: 60,
                }
            }
        )*
    };
}

metric_defaults! {
    default_cpu: 50.0,
    default_memory: 50.0,
    default_swap: 80.0,
    default_battery: 84.0,
}

pub fn default_disk() -> MetricConfig {
    MetricConfig {
        enabled: true,
        threshold: 90.0,
        critical: Some(95.0),
        cooldown_secs: 60,
    }
}

pub fn default_temperature() -> MetricConfig {
    MetricConfig {
        enabled: true,
        threshold: 80.0,
        critical: Some(95.0),
        cooldown_secs: 60,
    }
}

pub fn default_disk_io() -> MetricConfig {
    MetricConfig {
        enabled: true,
        threshold: 30.0,
        critical: Some(80.0),
        cooldown_secs: 60,
    }
}

/// Network-specific configuration with interface filtering
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct NetworkConfig {
    pub enabled: bool,
    pub threshold: f64,
    pub critical: Option<f64>,
    pub cooldown_secs: u64,
    /// Exclude loopback interface (lo) from traffic totals
    pub exclude_loopback: bool,
    /// Only include these interfaces (empty = all allowed)
    pub include_interfaces: Option<Vec<String>>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 100.0,
            critical: Some(500.0),
            cooldown_secs: 60,
            exclude_loopback: true,
            include_interfaces: None,
        }
    }
}

impl NetworkConfig {
    pub fn severity(&self, val: f64) -> crate::config::Severity {
        if let Some(crit) = self.critical {
            if val > crit {
                return crate::config::Severity::Critical;
            }
        }
        crate::config::Severity::Warning
    }
}

pub fn default_time() -> TimeConfig {
    TimeConfig::default()
}

pub fn default_log() -> LogConfig {
    LogConfig::default()
}

impl Config {
    pub fn into_checkers(self) -> Vec<Box<dyn crate::checkers::Checker>> {
        crate::checkers::all_checkers(self)
    }

    pub fn load() -> Self {
        Self::load_from(None)
    }

    pub fn load_from(path: Option<&std::path::Path>) -> Self {
        let path = path.map(|p| p.to_path_buf()).unwrap_or_else(config_path);
        // Open the file once and operate on the fd to avoid TOCTOU races
        // between exists()/metadata()/read_to_string() on the path.
        let content = match std::fs::File::open(&path) {
            Ok(mut file) => {
                // Reject config files larger than 1 MB to prevent OOM
                if let Ok(meta) = file.metadata() && meta.len() > MAX_CONFIG_SIZE {
                    tracing::warn!("config file too large ({} bytes, max {MAX_CONFIG_SIZE}), using defaults", meta.len());
                    return Config::default();
                }
                let mut content = String::new();
                if let Err(e) = file.read_to_string(&mut content) {
                    tracing::warn!("cannot read config {path:?}: {e}, using defaults");
                    return Config::default();
                }
                content
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Config::default();
            }
            Err(e) => {
                tracing::warn!("cannot open config {path:?}: {e}, using defaults");
                return Config::default();
            }
        };
        match toml::from_str::<ConfigFile>(&content) {
            Ok(raw) => raw.into(),
            Err(e) => {
                tracing::warn!("config parse failed: {e}, using defaults");
                Config::default()
            }
        }
    }

    fn validate(&mut self) {
        if self.check_interval_secs < MIN_INTERVAL || self.check_interval_secs > MAX_INTERVAL {
            tracing::warn!(
                "check_interval_secs {} out of range [{MIN_INTERVAL},{MAX_INTERVAL}], clamped",
                self.check_interval_secs
            );
            self.check_interval_secs = self.check_interval_secs.clamp(MIN_INTERVAL, MAX_INTERVAL);
        }
        Self::validate_metric("disk", &mut self.disk, 0.0, 100.0);
        Self::validate_metric("cpu", &mut self.cpu, 0.0, 100.0);
        Self::validate_metric("memory", &mut self.memory, 0.0, 100.0);
        Self::validate_metric("swap", &mut self.swap, 0.0, 100.0);
        Self::validate_metric("battery", &mut self.battery, 0.0, 100.0);
        Self::validate_metric("temperature", &mut self.temperature, 0.0, 150.0);
        Self::validate_metric("disk_io", &mut self.disk_io, 0.0, 100.0);
        {
            let m = &mut self.network;
            if m.threshold < 0.0 || m.threshold > 1_000_000.0 {
                tracing::warn!("network.threshold {} out of range [0,1000000], clamped", m.threshold);
                m.threshold = m.threshold.clamp(0.0, 1_000_000.0);
            }
            if let Some(ref mut crit) = m.critical
                && (*crit < 0.0 || *crit > 1_000_000.0)
            {
                tracing::warn!("network.critical {} out of range [0,1000000], clamped", crit);
                *crit = crit.clamp(0.0, 1_000_000.0);
            }
            if m.cooldown_secs < MIN_COOLDOWN {
                tracing::warn!(
                    "network.cooldown_secs {} < {MIN_COOLDOWN}, set to {MIN_COOLDOWN}",
                    m.cooldown_secs
                );
                m.cooldown_secs = MIN_COOLDOWN;
            }
        }
        if self.time.cooldown_secs < MIN_COOLDOWN {
            tracing::warn!(
                "time.cooldown_secs {} < {MIN_COOLDOWN}, set to {MIN_COOLDOWN}",
                self.time.cooldown_secs
            );
            self.time.cooldown_secs = MIN_COOLDOWN;
        }
    }

    fn validate_metric(name: &str, m: &mut MetricConfig, lo: f64, hi: f64) {
        if m.threshold < lo || m.threshold > hi {
            tracing::warn!("{name}.threshold {} out of range [{lo},{hi}], clamped", m.threshold);
            m.threshold = m.threshold.clamp(lo, hi);
        }
        if let Some(crit) = &mut m.critical
            && (*crit < lo || *crit > hi)
        {
            tracing::warn!("{name}.critical {} out of range [{lo},{hi}], clamped", crit);
            *crit = crit.clamp(lo, hi);
        }
        if m.cooldown_secs < MIN_COOLDOWN {
            tracing::warn!(
                "{name}.cooldown_secs {} < {MIN_COOLDOWN}, set to {MIN_COOLDOWN}",
                m.cooldown_secs
            );
            m.cooldown_secs = MIN_COOLDOWN;
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut cfg = Self {
            check_interval_secs: default_check_interval(),
            disk: default_disk(),
            cpu: default_cpu(),
            memory: default_memory(),
            swap: default_swap(),
            battery: default_battery(),
            time: TimeConfig::default(),
            temperature: default_temperature(),
            network: NetworkConfig::default(),
            disk_io: default_disk_io(),
            log: LogConfig::default(),
        };
        cfg.validate();
        cfg
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    check_interval_secs: Option<u64>,
    disk: Option<MetricConfig>,
    cpu: Option<MetricConfig>,
    memory: Option<MetricConfig>,
    swap: Option<MetricConfig>,
    battery: Option<MetricConfig>,
    time: Option<TimeConfig>,
    temperature: Option<MetricConfig>,
    network: Option<NetworkConfig>,
    disk_io: Option<MetricConfig>,
    log: Option<LogConfig>,
}

impl From<ConfigFile> for Config {
    fn from(raw: ConfigFile) -> Self {
        let mut cfg = Config {
            check_interval_secs: raw.check_interval_secs.unwrap_or_else(default_check_interval),
            disk: raw.disk.unwrap_or_else(default_disk),
            cpu: raw.cpu.unwrap_or_else(default_cpu),
            memory: raw.memory.unwrap_or_else(default_memory),
            swap: raw.swap.unwrap_or_else(default_swap),
            battery: raw.battery.unwrap_or_else(default_battery),
            time: raw.time.unwrap_or_else(default_time),
            temperature: raw.temperature.unwrap_or_else(default_temperature),
            network: raw.network.unwrap_or_default(),
            disk_io: raw.disk_io.unwrap_or_else(default_disk_io),
            log: raw.log.unwrap_or_else(default_log),
        };
        cfg.validate();
        cfg
    }
}

pub fn config_path() -> PathBuf {
    let mut p = dirs_next_config_dir().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| {
            tracing::warn!("HOME not set, using current directory for config");
            ".".to_string()
        });
        PathBuf::from(home).join(".config")
    });
    // Resolve symlinks to prevent XDG env var attacks
    if p.exists() {
        p = p.canonicalize().unwrap_or(p);
    }
    p.push("sema");
    p.push("config.toml");
    p
}

pub fn generate_default_config(path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        if let Err(e) = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)) {
            tracing::warn!("cannot set config directory permissions: {e}");
        }
    }
    // Use O_NOFOLLOW to prevent writing through a symlink
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(
        concat!(
            "# sema configuration\n",
            "# See https://github.com/codethare/sema for documentation\n",
            "\n",
            "check_interval_secs = 10\n",
            "\n",
            "[disk]\n",
            "enabled = true\n",
            "threshold = 90.0\n",
            "critical = 95.0\n",
            "cooldown_secs = 60\n",
            "\n",
            "[cpu]\n",
            "enabled = true\n",
            "threshold = 50.0\n",
            "cooldown_secs = 60\n",
            "\n",
            "[memory]\n",
            "enabled = true\n",
            "threshold = 50.0\n",
            "cooldown_secs = 60\n",
            "\n",
            "[swap]\n",
            "enabled = true\n",
            "threshold = 80.0\n",
            "cooldown_secs = 60\n",
            "\n",
            "[battery]\n",
            "enabled = true\n",
            "threshold = 84.0\n",
            "cooldown_secs = 60\n",
            "\n",
            "[time]\n",
            "enabled = true\n",
            "cooldown_secs = 60\n",
            "\n",
            "[temperature]\n",
            "enabled = true\n",
            "threshold = 80.0\n",
            "critical = 95.0\n",
            "cooldown_secs = 60\n",
            "\n",
            "[network]\n",
            "enabled = true\n",
            "threshold = 100.0\n",
            "critical = 500.0\n",
            "cooldown_secs = 60\n",
            "exclude_loopback = true\n",
            "\n",
            "[disk_io]\n",
            "enabled = true\n",
            "threshold = 30.0\n",
            "critical = 80.0\n",
            "cooldown_secs = 60\n",
            "\n",
            "[log]\n",
            "enabled = true\n",
        )
        .as_bytes(),
    )?;
    drop(file);
    // Restrict config file to owner read/write only
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        tracing::warn!("cannot set config permissions: {e}");
    }
    Ok(())
}

fn dirs_next_config_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_warning_for_normal_value() {
        let cfg = MetricConfig { threshold: 50.0, critical: None, ..Default::default() };
        assert_eq!(cfg.severity(30.0, false), Severity::Warning);
    }

    #[test]
    fn severity_critical_when_critical_threshold_exceeded() {
        let cfg = MetricConfig { threshold: 50.0, critical: Some(80.0), ..Default::default() };
        assert_eq!(cfg.severity(90.0, false), Severity::Critical);
    }

    #[test]
    fn severity_warning_below_critical_threshold() {
        let cfg = MetricConfig { threshold: 50.0, critical: Some(80.0), ..Default::default() };
        assert_eq!(cfg.severity(70.0, false), Severity::Warning);
    }

    #[test]
    fn severity_inverted_lower_is_more_severe() {
        let cfg = MetricConfig { threshold: 84.0, critical: Some(20.0), ..Default::default() };
        assert_eq!(cfg.severity(10.0, true), Severity::Critical);
        assert_eq!(cfg.severity(50.0, true), Severity::Warning);
    }

    #[test]
    fn severity_no_critical_falls_back_to_warning() {
        let cfg = MetricConfig { threshold: 50.0, critical: None, ..Default::default() };
        // Even very high values only get Warning if no critical is set
        assert_eq!(cfg.severity(99.0, false), Severity::Warning);
    }

    #[test]
    fn validate_clamps_check_interval() {
        let mut cfg = Config { check_interval_secs: 9999, ..Config::default() };
        cfg.validate();
        assert_eq!(cfg.check_interval_secs, MAX_INTERVAL);
    }

    #[test]
    fn validate_clamps_min_check_interval() {
        let mut cfg = Config { check_interval_secs: 0, ..Config::default() };
        cfg.validate();
        assert_eq!(cfg.check_interval_secs, MIN_INTERVAL);
    }

    #[test]
    fn validate_clamps_metric_threshold() {
        let mut m = MetricConfig { threshold: 999.0, ..Default::default() };
        Config::validate_metric("test", &mut m, 0.0, 100.0);
        assert_eq!(m.threshold, 100.0);
    }

    #[test]
    fn validate_clamps_critical_threshold() {
        let mut m = MetricConfig { threshold: 50.0, critical: Some(999.0), ..Default::default() };
        Config::validate_metric("test", &mut m, 0.0, 100.0);
        assert_eq!(m.critical, Some(100.0));
    }

    #[test]
    fn validate_clamps_cooldown_below_min() {
        let mut m = MetricConfig { cooldown_secs: 0, ..Default::default() };
        Config::validate_metric("test", &mut m, 0.0, 100.0);
        assert_eq!(m.cooldown_secs, MIN_COOLDOWN);
    }

    #[test]
    fn config_default_values() {
        let cfg = Config::default();
        assert_eq!(cfg.check_interval_secs, 10);
        assert!(cfg.cpu.enabled);
        assert_eq!(cfg.cpu.threshold, 50.0);
        assert_eq!(cfg.cpu.cooldown_secs, 60);
        assert!(cfg.log.enabled);
        assert_eq!(cfg.time.cooldown_secs, 60);
    }

    #[test]
    fn severity_emoji_mapping() {
        assert_eq!(Severity::Warning.emoji(), "⚠️");
        assert_eq!(Severity::Critical.emoji(), "🔴");
    }

    #[test]
    fn metric_config_default_is_enabled() {
        let m = MetricConfig::default();
        assert!(m.enabled);
        assert_eq!(m.threshold, 50.0);
        assert_eq!(m.cooldown_secs, 60);
        assert!(m.critical.is_none());
    }

    #[test]
    fn temperature_default_has_critical() {
        let t = default_temperature();
        assert_eq!(t.critical, Some(95.0));
    }

    #[test]
    fn network_default_has_critical() {
        let n = NetworkConfig::default();
        assert_eq!(n.critical, Some(500.0));
        assert!(n.exclude_loopback);
        assert!(n.include_interfaces.is_none());
    }
}
