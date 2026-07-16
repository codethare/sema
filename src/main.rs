mod checkers;
mod config;
mod log;
mod sink;

use std::io::{self, Write as IoWrite};
use std::os::unix::fs::OpenOptionsExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tracing_subscriber::EnvFilter;

use checkers::{Checker, CheckerError, Severity, SysSnapshot};
use config::Config;
use sink::{PendingAlert, Sink, send_notification};

#[derive(Parser)]
#[command(
    name = "sema",
    version,
    about = "Linux system resource monitor",
    after_help = "Config: ~/.config/sema/config.toml\nLog:    ~/.local/share/sema/sema.log"
)]
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

    /// Send a test notification to verify notifications work
    #[arg(long = "test")]
    test: bool,

    /// Generate shell completion script
    #[arg(long = "completions", value_enum)]
    completions: Option<Shell>,
}

static HUP_RECEIVED: AtomicBool = AtomicBool::new(false);
static TERM_RECEIVED: AtomicBool = AtomicBool::new(false);

fn sd_notify(state: &str) {
    use std::os::unix::net::UnixDatagram;

    // Check NOTIFY_SOCKET first before allocating socket (order optimization)
    static PATH: LazyLock<Option<String>> = LazyLock::new(|| std::env::var("NOTIFY_SOCKET").ok());
    let path = match PATH.as_ref() {
        Some(p) => p,
        None => return, // not running under systemd, no-op
    };
    // Only accept abstract sockets (@...) or paths under /run/
    if !path.starts_with('@') && !path.starts_with("/run/") {
        tracing::warn!("sd_notify: invalid NOTIFY_SOCKET '{path}', ignoring");
        return;
    }

    static SOCK: LazyLock<Option<UnixDatagram>> = LazyLock::new(|| UnixDatagram::unbound().ok());
    let sock = match SOCK.as_ref() {
        Some(s) => s,
        None => {
            tracing::warn!("sd_notify: no UnixDatagram socket available");
            return;
        }
    };
    let pid = std::process::id();
    let msg = format!("{state}\nMAINPID={pid}");
    if let Err(e) = sock.send_to(msg.as_bytes(), path) {
        tracing::warn!("sd_notify failed: {e}");
    }
}

struct Monitor {
    sys: System,
    sink: Sink,
    checkers: Vec<Box<dyn Checker>>,
    check_interval: Duration,
    config_path: Option<std::path::PathBuf>,
    checker_failures: std::collections::HashMap<&'static str, u32>,
    wake_rx: std::sync::mpsc::Receiver<libc::c_int>,
}

impl Monitor {
    fn new(
        cfg: Config,
        dry_run: bool,
        config_path: Option<std::path::PathBuf>,
        wake_rx: std::sync::mpsc::Receiver<libc::c_int>,
    ) -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
                .with_memory(MemoryRefreshKind::everything()),
        );
        let check_interval = Duration::from_secs(cfg.check_interval_secs);
        let log_enabled = cfg.log.enabled;
        let recovery_cooldown_secs = cfg.recovery_cooldown_secs;
        let checkers = cfg.into_checkers();
        let sink = Sink::new(dry_run, log_enabled, recovery_cooldown_secs);
        Self {
            sys,
            sink,
            checkers,
            check_interval,
            config_path,
            checker_failures: std::collections::HashMap::new(),
            wake_rx,
        }
    }

    fn sys_snapshot(&self) -> SysSnapshot {
        let load = System::load_average();
        SysSnapshot {
            cpu_usage: self.sys.global_cpu_usage() as f64,
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            mem_available: self.sys.available_memory(),
            swap_used: self.sys.used_swap(),
            swap_total: self.sys.total_swap(),
            load_one: load.one,
            load_five: load.five,
            load_fifteen: load.fifteen,
        }
    }

    fn reload(&mut self) {
        let cfg = match Config::load_from_strict(self.config_path.as_deref()) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::error!("config reload failed: {e}, keeping current config");
                return;
            }
        };
        let log_enabled = cfg.log.enabled;
        let recovery_cooldown_secs = cfg.recovery_cooldown_secs;
        self.check_interval = Duration::from_secs(cfg.check_interval_secs);
        self.checkers = cfg.into_checkers();
        self.sink.set_log_enabled(log_enabled);
        self.sink.set_recovery_cooldown_secs(recovery_cooldown_secs);
        self.checker_failures.clear();
        tracing::info!("config reloaded ({} checkers)", self.checkers.len());
    }

    fn print_banner(&self) {
        let mode = if self.sink.dry_run { " (dry-run)" } else { "" };
        let config_display = self
            .config_path
            .as_ref()
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
            log_path = log::log_path_display(),
            interval = self.check_interval.as_secs(),
        );
    }

    fn run(&mut self) {
        static HAS_SD: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let sd = || *HAS_SD.get_or_init(|| std::env::var("NOTIFY_SOCKET").is_ok());

        self.print_banner();
        if sd() {
            sd_notify("READY=1\nSTATUS=Monitoring...");
        }

        loop {
            let loop_start = Instant::now();
            if TERM_RECEIVED.load(Ordering::SeqCst) {
                println!("\nsema shutting down");
                if sd() {
                    sd_notify("STOPPING=1\nSTATUS=Shutting down...");
                }
                break;
            }

            if HUP_RECEIVED.swap(false, Ordering::SeqCst) {
                self.reload();
                self.print_banner();
                if sd() {
                    sd_notify("RELOADING=1\nSTATUS=Config reloaded");
                }
                if sd() {
                    sd_notify("READY=1\nSTATUS=Monitoring...");
                }
            } else if sd() {
                sd_notify("WATCHDOG=1\nSTATUS=Monitoring...");
            }

            // Refresh system data before running checks
            self.sys.refresh_cpu_usage();
            self.sys.refresh_memory_specifics(MemoryRefreshKind::everything());

            let snap = self.sys_snapshot();
            // Phase 1: run all checks, collect alerts
            let sys = &snap;
            let sink = &mut self.sink;
            let failures = &mut self.checker_failures;
            let checkers = std::mem::take(&mut self.checkers);
            const MAX_CHECKER_FAILURES: u32 = 3;

            let mut pending: Vec<PendingAlert> = Vec::new();
            let mut crashed_checkers: Vec<&str> = Vec::new();
            let mut keep_checkers: Vec<Box<dyn Checker>> = Vec::new();

            for mut c in checkers {
                let key = c.key();
                let cooldown = c.cooldown_secs();
                // SAFETY: if check() panics, the checker is dropped (not added to
                // keep_checkers), so no inconsistent state survives the next iteration.
                let result = catch_unwind(AssertUnwindSafe(|| c.check(sys)));
                match result {
                    Ok(Ok(Some(alert))) => {
                        failures.remove(key);
                        sink.note_active(key, true);
                        pending.push(PendingAlert {
                            key,
                            cooldown,
                            recovery: false,
                            alert,
                        });
                        keep_checkers.push(c);
                    }
                    Ok(Ok(None)) => {
                        failures.remove(key);
                        if sink.note_active(key, false) {
                            pending.push(PendingAlert {
                                key,
                                cooldown: 0,
                                recovery: true,
                                alert: checkers::Alert {
                                    severity: Severity::Warning,
                                    summary: format!("✅ {key} back to normal"),
                                    body: String::new(),
                                },
                            });
                        }
                        keep_checkers.push(c);
                    }
                    Ok(Err(CheckerError::Io(e))) => {
                        tracing::error!("checker '{key}' I/O error: {e}");
                        *failures.entry(key).or_insert(0) += 1;
                        if failures.get(key).copied().unwrap_or(0) >= MAX_CHECKER_FAILURES {
                            crashed_checkers.push(key);
                            failures.remove(key);
                        } else {
                            keep_checkers.push(c);
                        }
                    }
                    Ok(Err(CheckerError::InvalidData(e))) => {
                        tracing::error!("checker '{key}' invalid data: {e}");
                        *failures.entry(key).or_insert(0) += 1;
                        if failures.get(key).copied().unwrap_or(0) >= MAX_CHECKER_FAILURES {
                            crashed_checkers.push(key);
                            failures.remove(key);
                        } else {
                            keep_checkers.push(c);
                        }
                    }
                    Err(panic) => {
                        let msg = if let Some(s) = panic.as_ref().downcast_ref::<&str>() {
                            s
                        } else if let Some(s) = panic.as_ref().downcast_ref::<String>() {
                            s.as_str()
                        } else {
                            "<unknown>"
                        };
                        tracing::error!("checker '{key}' panicked: {msg}");
                        failures.remove(key);
                        crashed_checkers.push(key);
                    }
                }
            }
            self.checkers = keep_checkers;

            for k in crashed_checkers {
                // Leak a stable key so the per-checker crash cooldown survives this loop iteration.
                // Crashes are rare, so this tiny leak is acceptable.
                let key: &'static str = Box::leak(format!("checker_crash:{k}").into_boxed_str());
                sink.notify(
                    key,
                    300,
                    &checkers::Alert {
                        severity: Severity::Critical,
                        summary: "⚠️ sema: checker crashed".to_string(),
                        body: format!("'{k}' crashed, sema is still running"),
                    },
                );
            }

            sink.notify_pending(pending);

            // Block for the interval (minus work time), waking early on signal.
            let elapsed = loop_start.elapsed();
            if let Some(remaining) = self.check_interval.checked_sub(elapsed) {
                // Event-driven: one wake per interval, immediate wake on HUP/TERM.
                if let Err(std::sync::mpsc::RecvTimeoutError::Disconnected) = self.wake_rx.recv_timeout(remaining) {
                    // signal thread unavailable; fall back to plain sleep
                    sleep(remaining);
                }
                // Ok(signal) or Timeout: loop-top handles HUP/TERM flags
            }
        }
    }

    fn dry_run(&mut self) {
        self.print_banner();
        println!("System state:\n");
        let snap = self.sys_snapshot();
        for c in &self.checkers {
            println!("{}", c.report(&snap));
        }
        println!();
    }
}

struct TraceLog;

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for TraceLog {
    type Writer = Box<dyn IoWrite + Send>;

    fn make_writer(&'a self) -> Self::Writer {
        let path = crate::log::trace_log_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
        {
            Ok(f) => Box::new(f),
            Err(_) => Box::new(io::sink()),
        }
    }
}

fn init_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "unknown error"
        };
        let location = info.location().map(|l| l.to_string()).unwrap_or_default();
        eprintln!(
            "\nsema: internal error — {msg}\n  at {location}\n  Please report: https://github.com/codethare/sema/issues"
        );
        prev(info);
    }));
}

fn init_tracing() {
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    let stderr_layer = fmt::layer().with_writer(io::stderr).with_target(false).without_time();

    let file_layer = fmt::layer().with_writer(TraceLog).with_target(true);

    tracing_subscriber::registry()
        .with(stderr_layer.with_filter(filter.clone()))
        .with(file_layer.with_filter(filter))
        .init();
}

fn main() {
    init_tracing();
    init_panic_hook();

    let cli = Cli::parse();

    let _lock: Option<single_instance::SingleInstance> =
        if cli.init || cli.test || cli.dry_run || cli.completions.is_some() {
            None
        } else {
            let instance = match single_instance::SingleInstance::new("sema") {
                Ok(inst) => inst,
                Err(e) => {
                    eprintln!("Error: single instance check failed ({e})");
                    std::process::exit(1);
                }
            };
            if !instance.is_single() {
                eprintln!("Error: another sema instance is already running");
                std::process::exit(1);
            }
            Some(instance)
        };

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

    if cli.test {
        if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
            eprintln!(
                "Error: D-Bus session bus not available (DBUS_SESSION_BUS_ADDRESS not set). Is a desktop session running?"
            );
            std::process::exit(1);
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let ok = send_notification(
                "🔍 sema test notification",
                "If you can read this, desktop notifications are working correctly.",
                Severity::Warning,
            );
            let _ = tx.send(ok);
        });
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(true) => println!("Test notification sent successfully."),
            Ok(false) => {
                eprintln!("Error: notification daemon refused the request.");
                std::process::exit(1);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                eprintln!("Error: notification daemon did not respond within 5s.");
                std::process::exit(1);
            }
            Err(_) => {
                eprintln!("Error: notification thread panicked.");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Some(shell) = cli.completions {
        generate(shell, &mut Cli::command(), "sema", &mut std::io::stdout());
        return;
    }

    // Dedicated signal thread: drives the atomics and wakes the main loop via the
    // channel so it blocks event-driven instead of polling every 100ms.
    let (wake_tx, wake_rx) = std::sync::mpsc::channel::<libc::c_int>();
    std::thread::spawn(move || {
        let mut sigs = match signal_hook::iterator::Signals::new([
            signal_hook::consts::SIGHUP,
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGINT,
        ]) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("cannot register signal handlers: {e}");
                return;
            }
        };
        for sig in sigs.forever() {
            match sig {
                signal_hook::consts::SIGHUP => HUP_RECEIVED.store(true, Ordering::SeqCst),
                _ => TERM_RECEIVED.store(true, Ordering::SeqCst),
            }
            let _ = wake_tx.send(sig);
        }
    });

    let dry_run = cli.dry_run;

    let config_path = cli.config;

    let cfg = match &config_path {
        Some(p) => Config::load_from_strict(Some(p)),
        None => Config::load_from_strict(None),
    };
    let cfg = match cfg {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };
    let mut monitor = Monitor::new(cfg, dry_run, config_path, wake_rx);

    if dry_run {
        monitor.dry_run();
    } else {
        monitor.run();
    }
}
