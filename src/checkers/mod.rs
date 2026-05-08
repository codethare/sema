pub mod battery;
pub mod cpu;
pub mod memory;
pub mod network;
pub mod swap;
pub mod temperature;
pub mod time;

use sysinfo::System;

use crate::config::Config;

/// 检测结果
pub struct Alert {
    pub summary: String,
    pub body: String,
}

/// 检测器 trait — 每个指标一个实现
pub trait Checker {
    /// 指标标识（也用作通知冷却 key）
    fn key(&self) -> &'static str;
    /// 通知冷却秒数
    fn cooldown_secs(&self) -> u64;
    /// 正常检测：超阈值返回 Some(Alert)，否则 None
    fn check(&mut self, sys: &System) -> Option<Alert>;
    /// dry-run 一行状态文本
    fn report(&self, sys: &System) -> String;
}

/// 从 Config 消费创建启用的检测器（enabled = true 的才会构建）
pub fn all_checkers(cfg: Config) -> Vec<Box<dyn Checker>> {
    let mut v: Vec<Box<dyn Checker>> = Vec::new();
    if cfg.cpu.enabled { v.push(Box::new(cpu::Cpu::new(cfg.cpu))); }
    if cfg.memory.enabled { v.push(Box::new(memory::Memory::new(cfg.memory))); }
    if cfg.swap.enabled { v.push(Box::new(swap::Swap::new(cfg.swap))); }
    if cfg.battery.enabled { v.push(Box::new(battery::Battery::new(cfg.battery))); }
    if cfg.time.enabled { v.push(Box::new(time::TimeChecker::new(cfg.time))); }
    if cfg.temperature.enabled { v.push(Box::new(temperature::Temperature::new(cfg.temperature))); }
    if cfg.network.enabled { v.push(Box::new(network::Network::new(cfg.network))); }
    v
}
