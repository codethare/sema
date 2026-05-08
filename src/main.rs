mod checkers;
mod config;
mod sink;

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, RefreshKind, System,
};

use checkers::Checker;
use config::Config;
use sink::Sink;

static HUP_RECEIVED: AtomicBool = AtomicBool::new(false);

fn handle_sighup() {
    HUP_RECEIVED.store(true, Ordering::SeqCst);
}

fn sd_notify(state: &str) {
    let socket_path = match std::env::var("NOTIFY_SOCKET") {
        Ok(p) => p,
        Err(_) => return,
    };
    let sock = match std::os::unix::net::UnixDatagram::unbound() {
        Ok(s) => s,
        Err(_) => return,
    };
    let _ = sock.send_to(state.as_bytes(), &socket_path);
}

struct Monitor {
    sys: System,
    sink: Sink,
    checkers: Vec<Box<dyn Checker>>,
    check_interval: Duration,
}

impl Monitor {
    fn new(cfg: Config, dry_run: bool) -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything())
                .with_processes(ProcessRefreshKind::everything()),
        );
        let check_interval = Duration::from_secs(cfg.check_interval_secs);
        let checkers = cfg.into_checkers();
        let sink = Sink::new(dry_run);
        Self { sys, sink, checkers, check_interval }
    }

    fn reload(&mut self) {
        let cfg = Config::load();
        self.check_interval = Duration::from_secs(cfg.check_interval_secs);
        self.checkers = cfg.into_checkers();
        eprintln!("[sema] 配置已重新加载 ({} checkers)", self.checkers.len());
    }

    fn print_banner(&self) {
        let mode = if self.sink.dry_run { " (dry-run)" } else { "" };
        println!(
            "🔍 sema {}{}\n\
             ──────────────────────────────────\n\
             配置文件: {}\n\
             日志路径: {}\n\
             检测间隔: {}s\n",
            env!("CARGO_PKG_VERSION"),
            mode,
            config::config_path().display(),
            Sink::log_path_display(),
            self.check_interval.as_secs(),
        );
    }

    fn run(&mut self) {
        self.print_banner();
        sd_notify("READY=1\nSTATUS=Monitoring...\nMAINPID=1");

        loop {
            if HUP_RECEIVED.swap(false, Ordering::SeqCst) {
                self.reload();
                sd_notify("RELOADING=1\nSTATUS=Config reloaded\nMAINPID=1");
                sd_notify("READY=1\nSTATUS=Monitoring...\nMAINPID=1");
            } else {
                sd_notify("WATCHDOG=1\nSTATUS=Monitoring...\nMAINPID=1");
            }

            let sys = &self.sys;
            let sink = &mut self.sink;
            for c in &mut self.checkers {
                if let Some(alert) = c.check(sys) {
                    let key = c.key();
                    sink.note_active(key, true);
                    sink.notify(key, c.cooldown_secs(), &alert);
                } else {
                    sink.note_active(c.key(), false);
                }
            }
            sleep(self.check_interval);
        }
    }

    fn dry_run(&mut self) {
        self.print_banner();
        println!("系统状态:\n");
        for c in &self.checkers {
            println!("{}", c.report(&self.sys));
        }
        println!();
    }
}

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

    if let Err(e) = unsafe { signal_hook::low_level::register(signal_hook::consts::SIGHUP, handle_sighup) } {
        eprintln!("[sema] 警告: 无法注册 SIGHUP 处理器: {e}");
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
