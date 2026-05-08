use std::path::PathBuf;

use serde::Deserialize;

/// sema 顶层配置
///
/// 从 `~/.config/sema/config.toml` (或 `$XDG_CONFIG_HOME/sema/config.toml`) 读取。
/// 所有字段都有默认值，缺失字段自动使用内置默认值。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// 全局检测间隔（秒）。默认 10。
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
}

/// 通用指标配置（CPU / 内存 / Swap / 电池）
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct MetricConfig {
    pub enabled: bool,
    /// 告警阈值（%）
    pub threshold: f64,
    /// 单指标通知冷却时间（秒）
    pub cooldown_secs: u64,
}

impl Default for MetricConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.0,
            cooldown_secs: 60,
        }
    }
}

/// 时间提醒配置
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TimeConfig {
    pub enabled: bool,
    /// 通知冷却时间（秒）
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

pub fn default_time() -> TimeConfig {
    TimeConfig::default()
}

impl Config {
    /// 加载配置。优先读取 XDG 配置路径，文件不存在时返回全默认值。
    pub fn load() -> Self {
        let path = config_path();
        if !path.exists() {
            return Config::default();
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[sema] 警告: 读取配置文件失败 {path:?}: {e}");
                return Config::default();
            }
        };

        match toml::from_str::<ConfigRaw>(&content) {
            Ok(raw) => Config::from_raw(raw),
            Err(e) => {
                eprintln!("[sema] 警告: 配置文件解析失败: {e}\n      使用默认配置运行");
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
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            check_interval_secs: default_check_interval(),
            cpu: default_cpu(),
            memory: default_memory(),
            swap: default_swap(),
            battery: default_battery(),
            time: TimeConfig::default(),
        }
    }
}

/// 中间层配置（全部 Option），用于区分"未设置"和"设置但为默认值"。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigRaw {
    check_interval_secs: Option<u64>,
    cpu: Option<MetricConfig>,
    memory: Option<MetricConfig>,
    swap: Option<MetricConfig>,
    battery: Option<MetricConfig>,
    time: Option<TimeConfig>,
}

/// 获取配置路径：`$XDG_CONFIG_HOME/sema/config.toml` 或 `~/.config/sema/config.toml`
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
