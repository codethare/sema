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
    /// 是否启用
    fn enabled(&self) -> bool;
    /// 正常检测：超阈值返回 Some(Alert)，否则 None
    fn check(&mut self, sys: &System) -> Option<Alert>;
    /// dry-run 一行状态文本
    fn report(&self, sys: &System) -> String;
}

/// 从 Config 消费创建全部检测器
pub fn all_checkers(cfg: Config) -> Vec<Box<dyn Checker>> {
    vec![
        Box::new(cpu::Cpu::new(cfg.cpu)),
        Box::new(memory::Memory::new(cfg.memory)),
        Box::new(swap::Swap::new(cfg.swap)),
        Box::new(battery::Battery::new(cfg.battery)),
        Box::new(time::TimeChecker::new(cfg.time)),
        Box::new(temperature::Temperature::new(cfg.temperature)),
        Box::new(network::Network::new(cfg.network)),
    ]
}
