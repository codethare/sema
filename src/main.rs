mod checkers;
mod config;
mod sink;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

use clap::Parser;
use notify_rust::Notification;
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, RefreshKind, System,
};

use checkers::Checker;
use config::Config;
use sink::Sink;

#[derive(Parser)]
#[command(name = "sema", version, about = "Linux system resource monitor",
    after_help = "Config: ~/.config/sema/config.toml\nLog:    ~/.local/share/sema/sema.log")]
struct Cli {
    /// Print system status and exit (no notifications)
    #[arg(short = 'n', long = "dry-run")]
    dry_run: bool,

    /// Path to config file
    #[arg(short = 'c', long = "config")]
    config: Option<PathBuf>,

    /// Generate default config file and exit
    #[arg(long = "init")]
    init: bool,

    /// Overwrite existing config with --init
    #[arg(long = "force")]
    force: bool,
}

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
    config_path: Option<std::path::PathBuf>,
}

impl Monitor {
    fn new(cfg: Config, dry_run: bool, config_path: Option<std::path::PathBuf>) -> Self {
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
        Self { sys, sink, checkers, check_interval, config_path }
    }

    fn reload(&mut self) {
        let cfg = Config::load_from(self.config_path.as_deref());
        let log_enabled = cfg.log.enabled;
        self.check_interval = Duration::from_secs(cfg.check_interval_secs);
        self.checkers = cfg.into_checkers();
        self.sink.set_log_enabled(log_enabled);
        eprintln!("[sema] config reloaded ({} checkers)", self.checkers.len());
    }

    fn print_banner(&self) {
        let mode = if self.sink.dry_run { " (dry-run)" } else { "" };
        let config_display = self.config_path.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| config::config_path().display().to_string());
        println!(
            "🔍 sema {version}{mode}\n\
             ──────────────────────────────────\n\
             config:  {config_display}\n\
             log:     {log_path}\n\
             interval: {interval}s\n",
            version = env!("CARGO_PKG_VERSION"),
            mode = mode,
            config_display = config_display,
            log_path = Sink::log_path_display(),
            interval = self.check_interval.as_secs(),
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

            // Refresh process list so CPU/memory TOP 3 are current
            self.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

            // Phase 1: run all checks, collect alerts
            struct PendingAlert<'a> {
                key: &'a str,
                cooldown: u64,
                alert: checkers::Alert,
            }
            let mut pending: Vec<PendingAlert> = Vec::new();
            let mut crash_keys: Vec<&str> = Vec::new();

            let sys = &self.sys;
            let sink = &mut self.sink;
            for c in &mut self.checkers {
                let key = c.key();
                let cooldown = c.cooldown_secs();

                let result = catch_unwind(AssertUnwindSafe(|| c.check(sys)));
                match result {
                    Ok(Some(alert)) => {
                        sink.note_active(key, true);
                        pending.push(PendingAlert { key, cooldown, alert });
                    }
                    Ok(None) => {
                        if sink.note_active(key, false) {
                            pending.push(PendingAlert {
                                key, cooldown: 0,
                                alert: checkers::Alert {
                                    summary: format!("✅ {key} back to normal"),
                                    body: String::new(),
                                },
                            });
                        }
                    }
                    Err(_) => {
                        eprintln!("[sema] checker '{key}' panicked, continuing");
                        crash_keys.push(key);
                    }
                }
            }

            for k in crash_keys {
                let _ = Notification::new()
                    .summary("⚠️ sema: checker crashed")
                    .body(&format!("'{k}' panicked, sema is still running"))
                    .appname("sema")
                    .show();
            }

            // Phase 2: send grouped or individual notifications
            if pending.len() == 1 {
                let p = &pending[0];
                sink.notify(p.key, p.cooldown, &p.alert);
            } else if !pending.is_empty() {
                let min_cooldown = pending.iter().map(|p| p.cooldown).min().unwrap_or(60);
                let body = pending.iter()
                    .map(|a| format!("{}: {}", a.alert.summary, a.alert.body))
                    .collect::<Vec<_>>()
                    .join(" | ");
                let composite = checkers::Alert {
                    summary: format!("{} alerts", pending.len()),
                    body,
                };
                sink.notify("group", min_cooldown, &composite);
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
    let cli = Cli::parse();

    if cli.init {
        let path = cli.config.clone().unwrap_or_else(config::config_path);
        if path.exists() && !cli.force {
            eprintln!("Error: config already exists at {}\nUse --force to overwrite", path.display());
            std::process::exit(1);
        }
        match config::generate_default_config(&path) {
            Ok(()) => println!("Default config written to {}\nEdit it and run sema", path.display()),
            Err(e) => eprintln!("Error: cannot write config to {}: {e}", path.display()),
        }
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

    let dry_run = cli.dry_run;

    let config_path = cli.config;

    let cfg = match &config_path {
        Some(p) => Config::load_from(Some(p)),
        None => Config::load(),
    };
    let mut monitor = Monitor::new(cfg, dry_run, config_path);

    if dry_run {
        monitor.dry_run();
    } else {
        monitor.run();
    }
}
