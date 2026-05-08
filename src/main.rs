mod config;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use chrono::{Local, Timelike};
use notify_rust::Notification;
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, RefreshKind, System,
};

use config::{Config, MetricConfig, TimeConfig};

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        let mut p = PathBuf::from(dir);
        p.push("sema");
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".local").join("share").join("sema")
}

fn log_path() -> PathBuf {
    let mut p = data_dir();
    p.push("sema.log");
    p
}

fn write_log(summary: &str, body: &str) {
    let path = log_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        writeln!(f, "[{ts}] {summary} | {body}").ok();
    }
}

struct Monitor {
    sys: System,
    cfg: Config,
    last_notified: HashMap<String, Instant>,
    dry_run: bool,
}

impl Monitor {
    fn new(cfg: Config, dry_run: bool) -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        Self { sys, cfg, last_notified: HashMap::new(), dry_run }
    }

    // ── 通知 ──

    fn can_notify(&self, key: &str, cooldown_secs: u64) -> bool {
        self.last_notified
            .get(key)
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(cooldown_secs))
    }

    fn send_notification(&mut self, key: &str, cooldown_secs: u64, summary: &str, body: &str) {
        if self.dry_run {
            println!("    └─ {summary}");
            println!("       {body}");
            return;
        }
        if !self.can_notify(key, cooldown_secs) {
            return;
        }
        let ok = Notification::new()
            .summary(summary)
            .body(body)
            .appname("sema")
            .show()
            .is_ok();
        if ok {
            write_log(summary, body);
            self.last_notified.insert(key.to_string(), Instant::now());
        }
    }

    // ── CPU ──

    fn check_cpu(&mut self, cfg: &MetricConfig) {
        self.sys.refresh_cpu_usage();
        let usage = self.sys.global_cpu_usage() as f64;

        if self.dry_run {
            let flag = if usage > cfg.threshold { "⚠️ 超过阈值" } else { "✓" };
            println!("  CPU     {:>6.1}%  阈值: {:>5.1}%  {flag}", usage, cfg.threshold);
            return;
        }
        if !cfg.enabled || usage <= cfg.threshold {
            return;
        }
        self.send_notification("cpu", cfg.cooldown_secs,
            "⚠️ CPU 负载过高",
            &format!("当前 CPU 使用率: {usage:.1}%（阈值: {thr}%）", thr = cfg.threshold));
    }

    // ── 内存 ──

    fn check_memory(&mut self, cfg: &MetricConfig) {
        self.sys.refresh_memory();
        let total = self.sys.total_memory();
        let used = self.sys.used_memory();
        let usage = used as f64 / total as f64 * 100.0;

        if self.dry_run {
            let used_mb = used / (1024 * 1024);
            let total_mb = total / (1024 * 1024);
            let flag = if usage > cfg.threshold { "⚠️ 超过阈值" } else { "✓" };
            println!("  内存   {:>6.1}%  阈值: {:>5.1}%  {flag}  ({used_mb}MB / {total_mb}MB)", usage, cfg.threshold);
            return;
        }
        if !cfg.enabled || usage <= cfg.threshold {
            return;
        }
        let used_mb = used / (1024 * 1024);
        let total_mb = total / (1024 * 1024);
        self.send_notification("memory", cfg.cooldown_secs,
            "⚠️ 内存使用过高",
            &format!("当前内存: {used_mb}MB / {total_mb}MB ({usage:.1}%，阈值: {thr}%)", thr = cfg.threshold));
    }

    // ── Swap ──

    fn check_swap(&mut self, cfg: &MetricConfig) {
        self.sys.refresh_memory();
        let total = self.sys.total_swap();
        if total == 0 {
            if self.dry_run {
                println!("  Swap    {:>6}   阈值: {:>5.1}%  -  (未启用)", "N/A", cfg.threshold);
            }
            return;
        }
        let used = self.sys.used_swap();
        let usage = used as f64 / total as f64 * 100.0;

        if self.dry_run {
            let used_mb = used / (1024 * 1024);
            let total_mb = total / (1024 * 1024);
            let flag = if usage > cfg.threshold { "⚠️ 超过阈值" } else { "✓" };
            println!("  Swap    {:>6.1}%  阈值: {:>5.1}%  {flag}  ({used_mb}MB / {total_mb}MB)", usage, cfg.threshold);
            return;
        }
        if !cfg.enabled || usage <= cfg.threshold {
            return;
        }
        let used_mb = used / (1024 * 1024);
        let total_mb = total / (1024 * 1024);
        self.send_notification("swap", cfg.cooldown_secs,
            "⚠️ Swap 使用过高",
            &format!("当前 Swap: {used_mb}MB / {total_mb}MB ({usage:.1}%，阈值: {thr}%)", thr = cfg.threshold));
    }

    // ── 电池 ──

    fn check_battery(&mut self, cfg: &MetricConfig) {
        if self.dry_run {
            match Self::read_battery_capacity() {
                Some(cap) => {
                    let flag = if (cap as f64) < cfg.threshold { "⚠️ 低于阈值（将触发通知）" } else { "✓" };
                    println!("  电池   {:>6}%  阈值: {:>5.1}%  {flag}", cap, cfg.threshold);
                }
                None => println!("  电池   {:>6}   阈值: {:>5.1}%  -  (未检测到)", "N/A", cfg.threshold),
            }
            return;
        }
        if !cfg.enabled {
            return;
        }
        if let Some(capacity) = Self::read_battery_capacity()
            .filter(|&c| (c as f64) < cfg.threshold)
        {
            self.send_notification("battery", cfg.cooldown_secs,
                "🔋 电池电量不足",
                &format!("当前电量: {capacity}%（阈值: {thr}%）", thr = cfg.threshold));
        }
    }

    /// 从 sysfs 读取电池容量
    /// Linux 内核通过 /sys/class/power_supply/BAT*/capacity 暴露电池信息，
    /// 这是最直接的读取方式，与 acpi/upower 同源，零额外依赖。
fn read_battery_capacity() -> Option<u8> {
    (0..4).find_map(|i| {
        let path = format!("/sys/class/power_supply/BAT{i}/capacity");
        let content = fs::read_to_string(&path).ok()?;
        content.trim().parse::<u8>().ok()
    })
}

    // ── 时间 ──

    fn check_time(&mut self, cfg: &TimeConfig) {
        let now = Local::now();
        let hour = now.hour();
        let minute = now.minute();
        let is_time = minute == 0 || minute == 30;

        if self.dry_run {
            let status = if is_time { "当前时间，将触发提醒" } else { "—" };
            println!("  时间   {:>2}:{:02}       {:>8}    {status}", hour, minute, "整点/半点");
            return;
        }
        if !cfg.enabled || !is_time {
            return;
        }
        self.send_notification("time", cfg.cooldown_secs,
            "⏰ 时间提醒",
            &format!("现在是 {hour}:{minute:02}"));
    }

    // ── 启动信息 ──

    fn print_banner(&self) {
        let c = &self.cfg;
        let mode = if self.dry_run { " (dry-run 模式)" } else { "" };
        println!(
            "🔍 sema {}{}\n\
             ──────────────────────────────────\n\
             配置文件: {}\n\
             日志路径: {}\n\
             检测间隔: {}s\n",
            env!("CARGO_PKG_VERSION"),
            mode,
            config::config_path().display(),
            log_path().display(),
            c.check_interval_secs,
        );
    }

    // ── 正常运行 ──

    fn run(&mut self) {
        self.print_banner();
        let c = self.cfg.clone();
        loop {
            self.check_cpu(&c.cpu);
            self.check_memory(&c.memory);
            self.check_swap(&c.swap);
            self.check_battery(&c.battery);
            self.check_time(&c.time);
            sleep(Duration::from_secs(c.check_interval_secs));
        }
    }

    // ── Dry-run ──

    fn dry_run(&mut self) {
        self.print_banner();
        println!("系统状态:\n");
        let c = self.cfg.clone();
        self.check_cpu(&c.cpu);
        self.check_memory(&c.memory);
        self.check_swap(&c.swap);
        self.check_battery(&c.battery);
        self.check_time(&c.time);
        println!();
    }
}

// ─── 入口 ───

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("sema {} — 系统资源监控守护工具", env!("CARGO_PKG_VERSION"));
        println!();
        println!("用法: sema [选项]");
        println!();
        println!("选项:");
        println!("  -n, --dry-run    打印系统状态概览并退出（不发送通知）");
        println!("  -h, --help       显示此帮助信息");
        println!();
        println!("配置文件: ~/.config/sema/config.toml");
        println!("日志文件: ~/.local/share/sema/sema.log");
        return;
    }

    let dry_run = args.iter().any(|a| a == "--dry-run" || a == "-n");
    let cfg = Config::load();
    let mut monitor = Monitor::new(cfg, dry_run);

    if dry_run {
        monitor.dry_run();
    } else {
        monitor.run();
    }
}
