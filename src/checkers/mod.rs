pub mod battery;
pub mod cpu;
pub mod disk;
pub mod disk_io;
pub mod memory;
pub mod network;
pub mod swap;
pub mod temperature;
pub mod time;

use std::fmt;

use crate::config::Config;
pub use crate::config::Severity;

/// Snapshot of sysinfo data needed by checkers
pub struct SysSnapshot {
    pub cpu_usage: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub mem_available: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub load_one: f64,
    pub load_five: f64,
    pub load_fifteen: f64,
}

impl fmt::Display for SysSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CPU: {:.1}% | Mem: {:.0}% | Load: {:.2} {:.2} {:.2}",
            self.cpu_usage,
            if self.mem_total > 0 {
                self.mem_used as f64 / self.mem_total as f64 * 100.0
            } else {
                0.0
            },
            self.load_one,
            self.load_five,
            self.load_fifteen,
        )
    }
}

/// Check result
pub struct Alert {
    pub severity: Severity,
    pub summary: String,
    pub body: String,
}

/// Checker error
#[derive(Debug, thiserror::Error)]
pub enum CheckerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid data: {0}")]
    InvalidData(String),
}

/// Checker trait — one implementation per metric
pub trait Checker: Send {
    /// Metric identifier (also used as notification cooldown key)
    fn key(&self) -> &'static str;
    /// Notification cooldown in seconds
    fn cooldown_secs(&self) -> u64;
    /// Check: returns Some(Alert) if threshold exceeded, None otherwise
    fn check(&mut self, sys: &SysSnapshot) -> Result<Option<Alert>, CheckerError>;
    /// Dry-run one-line status text
    fn report(&self, sys: &SysSnapshot) -> String;
}

/// Consume Config and create enabled checkers (only those with enabled = true)
pub fn all_checkers(cfg: Config) -> Vec<Box<dyn Checker>> {
    let mut v: Vec<Box<dyn Checker>> = Vec::new();
    if cfg.disk.enabled {
        v.push(Box::new(disk::Disk::new(cfg.disk)));
    }
    if cfg.cpu.enabled {
        v.push(Box::new(cpu::Cpu::new(cfg.cpu)));
    }
    if cfg.memory.enabled {
        v.push(Box::new(memory::Memory::new(cfg.memory)));
    }
    if cfg.swap.enabled {
        v.push(Box::new(swap::Swap::new(cfg.swap)));
    }
    if cfg.battery.enabled {
        v.push(Box::new(battery::Battery::new(cfg.battery)));
    }
    if cfg.time.enabled {
        v.push(Box::new(time::TimeChecker::new(cfg.time)));
    }
    if cfg.temperature.enabled {
        v.push(Box::new(temperature::Temperature::new(cfg.temperature)));
    }
    if cfg.network.enabled {
        v.push(Box::new(network::Network::new(cfg.network)));
    }
    if cfg.disk_io.enabled {
        v.push(Box::new(disk_io::DiskIo::new(cfg.disk_io)));
    }
    v
}
