use std::path::PathBuf;

use serde::Deserialize;

const MIN_INTERVAL: u64 = 1;
const MAX_INTERVAL: u64 = 3600;
const MIN_COOLDOWN: u64 = 1;

/// sema 顶层配置
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,

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
    pub network: MetricConfig,

    #[serde(default)]
    pub log: LogConfig,
}

/// 通用指标配置（CPU / 内存 / Swap / 电池 / 温度 / 网络）
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct MetricConfig {
    pub enabled: bool,
    pub threshold: f64,
    pub cooldown_secs: u64,
}

impl Default for MetricConfig {
    fn default() -> Self {
        Self { enabled: true, threshold: 0.0, cooldown_secs: 60 }
    }
}

/// 时间提醒配置
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TimeConfig {
    pub enabled: bool,
    pub cooldown_secs: u64,
}

impl Default for TimeConfig {
    fn default() -> Self {
        Self { enabled: true, cooldown_secs: 60 }
    }
}

/// 日志配置
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
            #[allow(dead_code)]
            pub fn $name() -> MetricConfig {
                MetricConfig {
                    enabled: true,
                    threshold: $threshold,
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

pub fn default_temperature() -> MetricConfig {
    MetricConfig { enabled: true, threshold: 80.0, cooldown_secs: 60 }
}

pub fn default_network() -> MetricConfig {
    MetricConfig { enabled: true, threshold: 100.0, cooldown_secs: 60 }
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
        let path = config_path();
        if !path.exists() {
            return Config::default();
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[sema] warning: cannot read config {path:?}: {e}, using defaults");
                return Config::default();
            }
        };
        match toml::from_str::<ConfigRaw>(&content) {
            Ok(raw) => {
                let mut cfg = Config::from_raw(raw);
                cfg.validate();
                cfg
            }
            Err(e) => {
                eprintln!("[sema] warning: config parse failed: {e}\n       using defaults");
                Config::default()
            }
        }
    }

    fn from_raw(raw: ConfigRaw) -> Self {
        Config {
            check_interval_secs: raw.check_interval_secs.unwrap_or_else(default_check_interval),
            cpu: raw.cpu.unwrap_or_else(default_cpu),
            memory: raw.memory.unwrap_or_else(default_memory),
            swap: raw.swap.unwrap_or_else(default_swap),
            battery: raw.battery.unwrap_or_else(default_battery),
            time: raw.time.unwrap_or_else(default_time),
            temperature: raw.temperature.unwrap_or_else(default_temperature),
            network: raw.network.unwrap_or_else(default_network),
            log: raw.log.unwrap_or_else(default_log),
        }
    }

    fn validate(&mut self) {
        if self.check_interval_secs < MIN_INTERVAL || self.check_interval_secs > MAX_INTERVAL {
            eprintln!("[sema] warning: check_interval_secs {} out of range [{MIN_INTERVAL},{MAX_INTERVAL}], clamped",
                self.check_interval_secs);
            self.check_interval_secs = self.check_interval_secs.clamp(MIN_INTERVAL, MAX_INTERVAL);
        }
        Self::validate_metric("cpu", &mut self.cpu, 0.0, 100.0);
        Self::validate_metric("memory", &mut self.memory, 0.0, 100.0);
        Self::validate_metric("swap", &mut self.swap, 0.0, 100.0);
        Self::validate_metric("battery", &mut self.battery, 0.0, 100.0);
        Self::validate_metric("temperature", &mut self.temperature, 0.0, 150.0);
        Self::validate_metric("network", &mut self.network, 0.0, 1_000_000.0);
        if self.time.cooldown_secs < MIN_COOLDOWN {
            eprintln!("[sema] warning: time.cooldown_secs {} < {MIN_COOLDOWN}, set to {MIN_COOLDOWN}",
                self.time.cooldown_secs);
            self.time.cooldown_secs = MIN_COOLDOWN;
        }
    }

    fn validate_metric(name: &str, m: &mut MetricConfig, lo: f64, hi: f64) {
        if m.threshold < lo || m.threshold > hi {
            eprintln!("[sema] warning: {name}.threshold {} out of range [{lo},{hi}], clamped", m.threshold);
            m.threshold = m.threshold.clamp(lo, hi);
        }
        if m.cooldown_secs < MIN_COOLDOWN {
            eprintln!("[sema] warning: {name}.cooldown_secs {} < {MIN_COOLDOWN}, set to {MIN_COOLDOWN}",
                m.cooldown_secs);
            m.cooldown_secs = MIN_COOLDOWN;
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut cfg = Self {
            check_interval_secs: default_check_interval(),
            cpu: default_cpu(),
            memory: default_memory(),
            swap: default_swap(),
            battery: default_battery(),
            time: TimeConfig::default(),
            temperature: default_temperature(),
            network: default_network(),
            log: LogConfig::default(),
        };
        cfg.validate();
        cfg
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigRaw {
    check_interval_secs: Option<u64>,
    cpu: Option<MetricConfig>,
    memory: Option<MetricConfig>,
    swap: Option<MetricConfig>,
    battery: Option<MetricConfig>,
    time: Option<TimeConfig>,
    temperature: Option<MetricConfig>,
    network: Option<MetricConfig>,
    log: Option<LogConfig>,
}

pub fn config_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        let mut p = PathBuf::from(dir);
        p.push("sema");
        p.push("config.toml");
        return p;
    }
    let mut p = dirs_next_config_dir().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(".config")
    });
    p.push("sema");
    p.push("config.toml");
    p
}

fn dirs_next_config_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config"))
}
