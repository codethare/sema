mod checkers;
mod config;
mod sink;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

use notify_rust::Notification;
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, RefreshKind, System,
};

use checkers::Checker;
use config::Config;
use sink::Sink;

static HUP_RECEIVED: AtomicBool = AtomicBool::new(false);
static TERM_RECEIVED: AtomicBool = AtomicBool::new(false);

fn handle_sighup() {
    HUP_RECEIVED.store(true, Ordering::SeqCst);
}

fn handle_sigterm() {
    TERM_RECEIVED.store(true, Ordering::SeqCst);
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
        let log_enabled = cfg.log.enabled;
        let checkers = cfg.into_checkers();
        let sink = Sink::new(dry_run, log_enabled);
        Self { sys, sink, checkers, check_interval }
    }

    fn reload(&mut self) {
        let cfg = Config::load();
        self.check_interval = Duration::from_secs(cfg.check_interval_secs);
        self.checkers = cfg.into_checkers();
        eprintln!("[sema] config reloaded ({} checkers)", self.checkers.len());
    }

    fn print_banner(&self) {
        let mode = if self.sink.dry_run { " (dry-run)" } else { "" };
        println!(
            "🔍 sema {}{}\n\
             ──────────────────────────────────\n\
             config:  {}\n\
             log:     {}\n\
             interval: {}s\n",
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
            if TERM_RECEIVED.load(Ordering::SeqCst) {
                println!("\nsema shutting down");
                sd_notify("STOPPING=1\nSTATUS=Shutting down...\nMAINPID=1");
                break;
            }

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
                let key = c.key();
                let cooldown = c.cooldown_secs();

                let result = catch_unwind(AssertUnwindSafe(|| c.check(sys)));
                match result {
                    Ok(Some(alert)) => {
                        sink.note_active(key, true);
                        sink.notify(key, cooldown, &alert);
                    }
                    Ok(None) => {
                        sink.note_active(key, false);
                    }
                    Err(_) => {
                        eprintln!("[sema] checker '{key}' panicked, continuing");
                        let _ = Notification::new()
                            .summary("⚠️ sema: checker crashed")
                            .body(&format!("'{key}' panicked, sema is still running"))
                            .appname("sema")
                            .show();
                    }
                }
            }
            sleep(self.check_interval);
        }
    }

    fn dry_run(&mut self) {
        self.print_banner();
        println!("System state:\n");
        for c in &self.checkers {
            println!("{}", c.report(&self.sys));
        }
        println!();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("sema {} — Linux system resource monitor", env!("CARGO_PKG_VERSION"));
        println!();
        println!("Usage: sema [OPTIONS]");
        println!();
        println!("Options:");
        println!("  -n, --dry-run    Print system status and exit (no notifications)");
        println!("  -V, --version    Print version and exit");
        println!("  -h, --help       Show this help message");
        println!();
        println!("Config: ~/.config/sema/config.toml");
        println!("Log:    ~/.local/share/sema/sema.log");
        return;
    }

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("sema {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if let Err(e) = unsafe { signal_hook::low_level::register(signal_hook::consts::SIGHUP, handle_sighup) } {
        eprintln!("[sema] warning: cannot register SIGHUP handler: {e}");
    }
    if let Err(e) = unsafe { signal_hook::low_level::register(signal_hook::consts::SIGTERM, handle_sigterm) } {
        eprintln!("[sema] warning: cannot register SIGTERM handler: {e}");
    }
    if let Err(e) = unsafe { signal_hook::low_level::register(signal_hook::consts::SIGINT, handle_sigterm) } {
        eprintln!("[sema] warning: cannot register SIGINT handler: {e}");
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
