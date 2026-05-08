mod config;

use std::collections::HashMap;
use std::fs;
use std::thread::sleep;
use std::time::{Duration, Instant};

use chrono::{Local, Timelike};
use notify_rust::Notification;
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, RefreshKind, System,
};

use config::{Config, MetricConfig, TimeConfig};

struct Monitor {
    sys: System,
    cfg: Config,
    last_notified: HashMap<String, Instant>,
}

impl Monitor {
    fn new(cfg: Config) -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        Self { sys, cfg, last_notified: HashMap::new() }
    }

    fn can_notify(&self, key: &str, cooldown_secs: u64) -> bool {
        self.last_notified
            .get(key)
            .map_or(true, |t| t.elapsed() >= Duration::from_secs(cooldown_secs))
    }

    fn send_notification(
        &mut self,
        key: &str,
        cooldown_secs: u64,
        summary: &str,
        body: &str,
    ) {
        if !self.can_notify(key, cooldown_secs) {
            return;
        }
        if let Err(e) = Notification::new()
            .summary(summary)
            .body(body)
            .appname("sema")
            .show()
        {
            eprintln!("[sema] {e}");
        } else {
            self.last_notified.insert(key.to_string(), Instant::now());
        }
    }

    fn check_cpu(&mut self, cfg: &MetricConfig) {
        if !cfg.enabled {
            return;
        }
        // refresh_cpu_usage() 每次调用都会与上次数据比较得出实际使用率。
        // 主循环间隔 10s >> sysinfo 所需的最小间隔，无需额外 sleep。
        self.sys.refresh_cpu_usage();
        let usage = self.sys.global_cpu_usage() as f64;
        if usage > cfg.threshold {
            self.send_notification(
                "cpu",
                cfg.cooldown_secs,
                "⚠️ CPU 负载过高",
                &format!("当前 CPU 使用率: {usage:.1}%（阈值: {thr}%）",
                    thr = cfg.threshold),
            );
        }
    }

    fn check_memory(&mut self, cfg: &MetricConfig) {
        if !cfg.enabled {
            return;
        }
        self.sys.refresh_memory();
        let total = self.sys.total_memory();
        let used = self.sys.used_memory();
        let usage = used as f64 / total as f64 * 100.0;

        if usage > cfg.threshold {
            let used_mb = used / (1024 * 1024);
            let total_mb = total / (1024 * 1024);
            self.send_notification(
                "memory",
                cfg.cooldown_secs,
                "⚠️ 内存使用过高",
                &format!("当前内存: {used_mb}MB / {total_mb}MB ({usage:.1}%，阈值: {thr}%)",
                    thr = cfg.threshold),
            );
        }
    }

    fn check_swap(&mut self, cfg: &MetricConfig) {
        if !cfg.enabled {
            return;
        }
        self.sys.refresh_memory();
        let total = self.sys.total_swap();
        if total == 0 {
            return;
        }
        let used = self.sys.used_swap();
        let usage = used as f64 / total as f64 * 100.0;

        if usage > cfg.threshold {
            let used_mb = used / (1024 * 1024);
            let total_mb = total / (1024 * 1024);
            self.send_notification(
                "swap",
                cfg.cooldown_secs,
                "⚠️ Swap 使用过高",
                &format!("当前 Swap: {used_mb}MB / {total_mb}MB ({usage:.1}%，阈值: {thr}%)",
                    thr = cfg.threshold),
            );
        }
    }

    fn check_battery(&mut self, cfg: &MetricConfig) {
        if !cfg.enabled {
            return;
        }
        if let Some(capacity) = Self::read_battery_capacity() {
            if (capacity as f64) < cfg.threshold {
                self.send_notification(
                    "battery",
                    cfg.cooldown_secs,
                    "🔋 电池电量不足",
                    &format!("当前电量: {capacity}%（阈值: {thr}%）",
                        thr = cfg.threshold),
                );
            }
        }
    }

    /// 从 sysfs 读取电池容量
    /// Linux 内核通过 /sys/class/power_supply/BAT*/capacity 暴露电池信息，
    /// 这是最直接的读取方式，与 acpi/upower 同源，零额外依赖。
    fn read_battery_capacity() -> Option<u8> {
        for i in 0..4 {
            let path = format!("/sys/class/power_supply/BAT{i}/capacity");
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(val) = content.trim().parse::<u8>() {
                    return Some(val);
                }
            }
        }
        None
    }

    fn check_time(&mut self, cfg: &TimeConfig) {
        if !cfg.enabled {
            return;
        }
        let now = Local::now();
        let hour = now.hour();
        let minute = now.minute();

        if minute == 0 || minute == 30 {
            self.send_notification(
                "time",
                cfg.cooldown_secs,
                "⏰ 时间提醒",
                &format!("现在是 {hour}:{minute:02}"),
            );
        }
    }

    fn run(&mut self) {
        let c = self.cfg.clone();
        println!(
            "🔍 sema {}\n\
             检测间隔: {}s\n\
             CPU:     {} {:>4}%  | 内存: {} {:>4}%  | Swap: {} {:>4}%  | 电池: {} {:>4}%  | 时间: {}\n",
            env!("CARGO_PKG_VERSION"),
            c.check_interval_secs,
            if c.cpu.enabled { "✔" } else { "✗" }, c.cpu.threshold,
            if c.memory.enabled { "✔" } else { "✗" }, c.memory.threshold,
            if c.swap.enabled { "✔" } else { "✗" }, c.swap.threshold,
            if c.battery.enabled { "✔" } else { "✗" }, c.battery.threshold,
            if c.time.enabled { "✔ 整点/半点" } else { "✗" },
        );

        loop {
            self.check_cpu(&c.cpu);
            self.check_memory(&c.memory);
            self.check_swap(&c.swap);
            self.check_battery(&c.battery);
            self.check_time(&c.time);
            sleep(Duration::from_secs(c.check_interval_secs));
        }
    }
}

fn main() {
    let cfg = Config::load();
    let mut monitor = Monitor::new(cfg);
    monitor.run();
}
