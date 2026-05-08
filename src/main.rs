use std::collections::HashMap;
use std::fs;
use std::thread::sleep;
use std::time::{Duration, Instant};

use chrono::{Local, Timelike};
use notify_rust::Notification;
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, RefreshKind, System, MINIMUM_CPU_UPDATE_INTERVAL,
};

const COOLDOWN_SECS: u64 = 60;
const CPU_THRESHOLD: f32 = 50.0;
const MEM_THRESHOLD: f64 = 50.0;
const SWAP_THRESHOLD: f64 = 80.0;
const BATT_THRESHOLD: f64 = 84.0;
const CHECK_INTERVAL: Duration = Duration::from_secs(10);

struct ArchMonitor {
    sys: System,
    last_notified: HashMap<String, Instant>,
}

impl ArchMonitor {
    fn new() -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        Self { sys, last_notified: HashMap::new() }
    }

    fn can_notify(&self, key: &str) -> bool {
        self.last_notified
            .get(key)
            .map_or(true, |t| t.elapsed() >= Duration::from_secs(COOLDOWN_SECS))
    }

    fn send_notification(&mut self, key: &str, summary: &str, body: &str) {
        if !self.can_notify(key) {
            return;
        }
        if let Err(e) = Notification::new()
            .summary(summary)
            .body(body)
            .appname("sema")
            .show()
        {
            eprintln!("[sema] 通知发送失败: {e}");
        }
        self.last_notified.insert(key.to_string(), Instant::now());
    }

    fn check_cpu(&mut self) {
        self.sys.refresh_cpu_usage();
        sleep(MINIMUM_CPU_UPDATE_INTERVAL);
        self.sys.refresh_cpu_usage();

        let usage = self.sys.global_cpu_usage();
        if usage > CPU_THRESHOLD {
            self.send_notification(
                "cpu",
                "⚠️ CPU 负载过高",
                &format!("当前 CPU 使用率: {usage:.1}%（阈值: {CPU_THRESHOLD:.0}%）"),
            );
        }
    }

    fn check_memory(&mut self) {
        self.sys.refresh_memory();
        let total = self.sys.total_memory();
        let used = self.sys.used_memory();
        let usage = used as f64 / total as f64 * 100.0;

        if usage > MEM_THRESHOLD {
            let used_mb = used / (1024 * 1024);
            let total_mb = total / (1024 * 1024);
            self.send_notification(
                "memory",
                "⚠️ 内存使用过高",
                &format!(
                    "当前内存: {used_mb}MB / {total_mb}MB ({usage:.1}%，阈值: {MEM_THRESHOLD}%)"
                ),
            );
        }
    }

    fn check_swap(&mut self) {
        self.sys.refresh_memory();
        let total = self.sys.total_swap();
        if total == 0 {
            return;
        }
        let used = self.sys.used_swap();
        let usage = used as f64 / total as f64 * 100.0;

        if usage > SWAP_THRESHOLD {
            let used_mb = used / (1024 * 1024);
            let total_mb = total / (1024 * 1024);
            self.send_notification(
                "swap",
                "⚠️ Swap 使用过高",
                &format!(
                    "当前 Swap: {used_mb}MB / {total_mb}MB ({usage:.1}%，阈值: {SWAP_THRESHOLD}%)"
                ),
            );
        }
    }

    fn check_battery(&mut self) {
        if let Some(capacity) = Self::read_battery_capacity() {
            if (capacity as f64) < BATT_THRESHOLD {
                self.send_notification(
                    "battery",
                    "🔋 电池电量不足",
                    &format!("当前电量: {capacity}%（阈值: {BATT_THRESHOLD}%）"),
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

    fn check_time(&mut self) {
        let now = Local::now();
        let hour = now.hour();
        let minute = now.minute();

        if minute == 0 || minute == 30 {
            self.send_notification(
                "time",
                "⏰ 时间提醒",
                &format!("现在是 {hour}:{minute:02}"),
            );
        }
    }

    fn run(&mut self) {
        println!(
            "🔍 sema 已启动，每 {}s 检测一次\n\
             CPU 阈值: {CPU_THRESHOLD}% | 内存阈值: {MEM_THRESHOLD}% | \
             Swap 阈值: {SWAP_THRESHOLD}% | 电池阈值: {BATT_THRESHOLD}%\n\
             整点/半点时间提醒 · 通知冷却 {COOLDOWN_SECS}s\n",
            CHECK_INTERVAL.as_secs(),
        );

        loop {
            self.check_cpu();
            self.check_memory();
            self.check_swap();
            self.check_battery();
            self.check_time();
            sleep(CHECK_INTERVAL);
        }
    }
}

fn main() {
    let mut monitor = ArchMonitor::new();
    monitor.run();
}

